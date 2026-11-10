//! The bytecode interpreter.
//!
//! The VM is *resumable*: [`Vm::run`] executes until the program ends or a
//! native command asks to yield (e.g. `RenderWorld`), so a host engine can
//! drive the program one frame at a time from its own event loop.

use std::cell::RefCell;
use std::rc::Rc;

use neptune_lang::bytecode::{Cmp, Op, Program, ValKind, VarOp};
use neptune_lang::sema::{float_to_int, format_float, str_to_float, str_to_int};

use crate::debug::{DebugState, StopReason};
use crate::stdlib::{self, Core};
use crate::value::{Heap, Value, NO_OBJECT};

/// A vector's storage and an index checked against it.
type VectorSlot = (Rc<RefCell<Vec<Value>>>, usize);

/// Engine-provided native commands (ids are relative to the host's first command).
pub trait Host {
    fn call(&mut self, id: u32, args: &[Value], ctx: &mut NativeCtx) -> Result<Value, String>;
}

/// A host with no commands (console programs, tests).
pub struct NoHost;

impl Host for NoHost {
    fn call(&mut self, id: u32, _: &[Value], _: &mut NativeCtx) -> Result<Value, String> {
        Err(format!("native command {id} is not available"))
    }
}

/// Requests a native command can make of the VM.
#[derive(Default)]
pub struct NativeCtx {
    pub(crate) yield_requested: bool,
    pub(crate) end_requested: bool,
    pub(crate) retry_requested: bool,
}

impl NativeCtx {
    /// Suspend execution after this command returns (end of a frame).
    pub fn request_yield(&mut self) {
        self.yield_requested = true;
    }
    /// Wait a frame and then call this command again with the same
    /// arguments (`WaitKey`, `WaitMouse`, ...: commands that block in
    /// Blitz3D but must not block the engine loop). The return value of
    /// this call is discarded.
    pub fn request_retry(&mut self) {
        self.retry_requested = true;
    }
    /// Terminate the program after this command returns.
    pub fn request_end(&mut self) {
        self.end_requested = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// A command requested a yield; call `run` again to continue.
    Yielded,
    /// The program has ended.
    Finished,
    /// The debugger wants attention (see [`crate::debug`]); never returned
    /// unless the debugger is on. Call `run` again to continue.
    Paused,
}

/// Callers an error lists before and after the ones it leaves out.
const TRACE_HEAD: usize = 16;
const TRACE_TAIL: usize = 4;

/// A place in the program that was waiting on a call when an error happened.
#[derive(Clone, Debug)]
pub struct CallSite {
    /// `file:line:col`
    pub location: String,
    /// Function containing the call.
    pub function: String,
}

/// A runtime error with its source location and the calls that led to it.
#[derive(Clone, Debug)]
pub struct RuntimeError {
    pub message: String,
    /// `file:line:col`
    pub location: String,
    /// Function the error occurred in.
    pub function: String,
    /// The calls that led here, innermost first (`Gosub`s included).
    pub callers: Vec<CallSite>,
    /// Callers left out of the middle of `callers` when the stack is very deep.
    pub omitted: usize,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runtime error: {}\n  at {} (in {})", self.message, self.location, self.function)?;
        for (i, c) in self.callers.iter().enumerate() {
            if self.omitted > 0 && i == TRACE_HEAD {
                write!(f, "\n  ... {} more calls ...", self.omitted)?;
            }
            write!(f, "\n  called from {} (in {})", c.location, c.function)?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}

pub(crate) struct Frame {
    pub(crate) func: u32,
    pub(crate) pc: u32,
    pub(crate) base: u32,
    gosub: Vec<GosubCall>,
}

struct GosubCall {
    ret: u32,
    /// Error handlers active at the `Gosub`, restored by its `Return`.
    handlers: usize,
}

struct Handler {
    frame: usize,
    stack_len: usize,
    pc: u32,
    /// `Gosub`s active in `frame` at the `Try`, restored when it catches.
    gosubs: usize,
}

pub(crate) struct DimArray {
    pub(crate) dims: Vec<usize>,
    pub(crate) data: ArrayData,
}

/// Elements of a `Dim` array. Integer and float arrays hold plain numbers
/// (4 bytes each instead of a whole [`Value`]), which is what large arrays
/// such as maps and grids are made of.
pub(crate) enum ArrayData {
    Int(Vec<i32>),
    Float(Vec<f32>),
    Other(Vec<Value>),
}

impl ArrayData {
    fn new(kind: ValKind, len: usize) -> ArrayData {
        match kind {
            ValKind::Int => ArrayData::Int(vec![0; len]),
            ValKind::Float => ArrayData::Float(vec![0.0; len]),
            _ => ArrayData::Other(vec![Value::default_for(kind); len]),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            ArrayData::Int(d) => d.len(),
            ArrayData::Float(d) => d.len(),
            ArrayData::Other(d) => d.len(),
        }
    }

    #[inline]
    pub(crate) fn get(&self, i: usize) -> Value {
        match self {
            ArrayData::Int(d) => Value::Int(d[i]),
            ArrayData::Float(d) => Value::Float(d[i]),
            ArrayData::Other(d) => d[i].clone(),
        }
    }
}

const MAX_FRAMES: usize = 100_000;
const MAX_ARRAY: usize = 1 << 28;

pub struct Vm {
    pub(crate) prog: Rc<Program>,
    /// Value stack. All of it is allocated; the values in use are the first
    /// `sp`, and the rest is room that instructions write into directly.
    pub(crate) stack: Vec<Value>,
    pub(crate) sp: usize,
    pub(crate) frames: Vec<Frame>,
    pub(crate) globals: Vec<Value>,
    pub(crate) arrays: Vec<DimArray>,
    pub heap: Heap,
    handlers: Vec<Handler>,
    data_ptr: usize,
    native_returns: Vec<bool>,
    n_core: u32,
    pub core: Core,
    finished: bool,
    pub(crate) debug: Option<Box<DebugState>>,
}

/// Same for every copy of a program, so `StartNetGame` only finds those.
/// Based on what the program does, not where its lines are.
fn program_id(prog: &Program) -> u64 {
    let code: Vec<&Vec<Op>> = prog.functions.iter().map(|f| &f.code).collect();
    format!("{code:?}{:?}", prog.strings).bytes().fold(0xcbf2_9ce4_8422_2325, |h: u64, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// Defines the interpreter's helper macros inside a function. They work on
/// the function's own variables (`sp`, `pc`, ...), so that these stay in
/// processor registers; `$d` stands for a dollar sign.
macro_rules! vm_macros {
    ($d:tt, $self:ident, $sp:ident, $pc:ident, $base:ident, $fi:ident) => {
        macro_rules! leave {
            ($d r:expr) => {{
                $self.$sp = $sp;
                return $d r;
            }};
        }
        macro_rules! fail {
            ($d e:expr) => {{
                $self.frames[$fi].$pc = ($pc - 1) as u32;
                $self.$sp = $sp;
                return Err($d e.into());
            }};
        }
        macro_rules! tri {
            ($d e:expr) => {
                match $d e {
                    Ok(v) => v,
                    Err(e) => fail!(e),
                }
            };
        }
        // writes a number into a slot, field or variable
        macro_rules! set_i {
            ($d dst:expr, $d v:expr) => {{
                let v: i32 = $d v;
                match &mut $d dst {
                    Value::Int(x) => *x = v,
                    x => *x = Value::Int(v),
                }
            }};
        }
        macro_rules! set_f {
            ($d dst:expr, $d v:expr) => {{
                let v: f32 = $d v;
                match &mut $d dst {
                    Value::Float(x) => *x = v,
                    x => *x = Value::Float(v),
                }
            }};
        }
        macro_rules! room {
            () => {
                if $sp >= $self.stack.len() {
                    $self.grow_stack($sp);
                }
            };
        }
        macro_rules! push {
            ($d v:expr) => {{
                let v: Value = $d v;
                room!();
                $self.stack[$sp] = v;
                $sp += 1;
            }};
        }
        macro_rules! push_i {
            ($d v:expr) => {{
                let v: i32 = $d v;
                room!();
                set_i!($self.stack[$sp], v);
                $sp += 1;
            }};
        }
        macro_rules! push_f {
            ($d v:expr) => {{
                let v: f32 = $d v;
                room!();
                set_f!($self.stack[$sp], v);
                $sp += 1;
            }};
        }
        // pushes a copy of the value behind a reference
        macro_rules! push_copy {
            ($d r:expr) => {
                match $d r {
                    Value::Int(v) => push_i!(*v),
                    Value::Float(v) => push_f!(*v),
                    v => push!(v.clone()),
                }
            };
        }
        macro_rules! pop {
            () => {{
                $sp -= 1;
                std::mem::take(&mut $self.stack[$sp])
            }};
        }
        macro_rules! pop_i {
            () => {{
                $sp -= 1;
                match &$self.stack[$sp] {
                    Value::Int(v) => *v,
                    _ => std::mem::take(&mut $self.stack[$sp]).int(),
                }
            }};
        }
        macro_rules! pop_f {
            () => {{
                $sp -= 1;
                match &$self.stack[$sp] {
                    Value::Float(v) => *v,
                    _ => std::mem::take(&mut $self.stack[$sp]).float(),
                }
            }};
        }
        macro_rules! pop_s {
            () => {{
                $sp -= 1;
                match std::mem::take(&mut $self.stack[$sp]) {
                    Value::Str(s) => s,
                    v => v.str(),
                }
            }};
        }
        macro_rules! pop_obj {
            () => {{
                $sp -= 1;
                $self.heap.live_ref(&$self.stack[$sp]).ok_or(NO_OBJECT)
            }};
        }
        // moves the value in stack slot `$d from` (just popped) to `$d dst`
        macro_rules! move_to {
            ($d from:expr, $d dst:expr) => {
                match &$self.stack[$d from] {
                    Value::Int(v) => set_i!($d dst, *v),
                    Value::Float(v) => set_f!($d dst, *v),
                    _ => {
                        let v = std::mem::take(&mut $self.stack[$d from]);
                        $d dst = v;
                    }
                }
            };
        }
        macro_rules! top {
            () => {
                $self.stack[$sp - 1]
            };
        }
        // pops the indices of an array access
        macro_rules! array_index {
            ($d array:expr, $d dims:expr) => {
                if $d dims == 1 {
                    let i = pop_i!();
                    $self.array_index1($d array, i)
                } else {
                    let mut idx = [0i32; 8];
                    for k in (0..$d dims as usize).rev() {
                        idx[k.min(7)] = pop_i!();
                    }
                    $self.array_index_n($d array, &idx)
                }
            };
        }
        macro_rules! push_elem {
            ($d array:expr, $d i:expr) => {
                match &$self.arrays[$d array as usize].data {
                    ArrayData::Int(d) => push_i!(d[$d i]),
                    ArrayData::Float(d) => push_f!(d[$d i]),
                    ArrayData::Other(d) => push!(d[$d i].clone()),
                }
            };
        }
        macro_rules! int_bin {
            (|$d a:ident, $d b:ident| $d body:expr) => {{
                let $d b = pop_i!();
                int_with!($d b, |$d a, $d b| $d body)
            }};
        }
        // the same with the right operand given instead of popped
        macro_rules! int_with {
            ($d b:expr, |$d a:ident, $d bb:ident| $d body:expr) => {{
                let $d bb: i32 = $d b;
                match &mut top!() {
                    Value::Int($d a) => {
                        *$d a = {
                            let $d a = *$d a;
                            $d body
                        }
                    }
                    _ => unreachable!("typed int op on non-int"),
                }
            }};
        }
        macro_rules! float_with {
            ($d b:expr, |$d a:ident, $d bb:ident| $d body:expr) => {{
                let $d bb: f32 = $d b;
                let $d a = match &top!() {
                    Value::Float(v) => *v,
                    v => v.float(),
                };
                set_f!(top!(), $d body);
            }};
        }
        macro_rules! local_i {
            ($d s:expr) => {
                match &$self.stack[$base + $d s as usize] {
                    Value::Int(v) => *v,
                    v => v.int(),
                }
            };
        }
        macro_rules! local_f {
            ($d s:expr) => {
                match &$self.stack[$base + $d s as usize] {
                    Value::Float(v) => *v,
                    v => v.float(),
                }
            };
        }
        macro_rules! float_bin {
            (|$d a:ident, $d b:ident| $d body:expr) => {{
                let $d b = pop_f!();
                float_with!($d b, |$d a, $d b| $d body)
            }};
        }
    };
}

/// How a rarely used instruction ended.
enum Cold {
    /// Carry on at this `pc` with this `sp`.
    Next(usize, usize),
    Leave(Status),
}

impl Vm {
    /// Creates a VM for a program compiled against [`stdlib::base_env`] (plus host commands).
    pub fn new(prog: Program) -> Vm {
        let n_core = stdlib::core_count() as u32;
        assert!(
            prog.natives.len() >= n_core as usize
                && prog.natives[..n_core as usize]
                    .iter()
                    .zip(stdlib::COMMANDS.iter())
                    .all(|(a, b)| neptune_lang::commands::parse_sig(b.0).name == a.name),
            "program was not compiled with the standard library environment first"
        );
        let prog = Rc::new(prog);
        let mut vm = Vm {
            stack: vec![Value::Null; 1024],
            sp: 0,
            frames: Vec::new(),
            globals: prog.globals.iter().map(|(_, k)| Value::default_for(*k)).collect(),
            arrays: prog.arrays.iter().map(|a| DimArray { dims: vec![], data: ArrayData::new(a.elem, 0) }).collect(),
            heap: Heap::new(prog.types.len()),
            handlers: Vec::new(),
            data_ptr: 0,
            native_returns: prog.natives.iter().map(|n| n.ret.is_some()).collect(),
            n_core,
            core: Core::new(),
            finished: false,
            debug: None,
            prog,
        };
        vm.core.program_id = program_id(&vm.prog);
        let main = vm.prog.main;
        vm.enter(main, 0);
        vm
    }

    pub fn program(&self) -> &Program {
        &self.prog
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn enter(&mut self, func: u32, n_args: usize) {
        let f = &self.prog.functions[func as usize];
        let base = self.sp - n_args;
        let locals = &f.locals[f.n_params as usize..];
        if self.sp + locals.len() > self.stack.len() {
            let need = (self.sp + locals.len()).max(self.stack.len() * 2);
            self.stack.resize(need, Value::Null);
        }
        for k in locals {
            let slot = &mut self.stack[self.sp];
            match (k, &mut *slot) {
                // the usual case, an integer over an old integer: one small write
                (ValKind::Int | ValKind::Variant, Value::Int(x)) => *x = 0,
                _ => *slot = Value::default_for(*k),
            }
            self.sp += 1;
        }
        self.frames.push(Frame { func, pc: 0, base: base as u32, gosub: Vec::new() });
    }

    /// Runs until the program yields or finishes.
    pub fn run(&mut self, host: &mut dyn Host) -> Result<Status, RuntimeError> {
        if self.finished {
            return Ok(Status::Finished);
        }
        let prog = self.prog.clone();
        // the error the debugger stopped on: the program ends now
        if let Some(msg) = self.debug.as_mut().and_then(|d| d.error.take()) {
            return Err(self.fatal(msg, &prog));
        }
        loop {
            // two copies of the interpreter, so that programs not being
            // debugged pay nothing for the checks
            let result = if self.debug.is_some() { self.exec::<true>(host, &prog) } else { self.exec::<false>(host, &prog) };
            match result {
                Ok(s) => {
                    if s == Status::Finished {
                        self.finished = true;
                    }
                    return Ok(s);
                }
                Err(msg) => {
                    if let Some(h) = self.handlers.pop() {
                        self.frames.truncate(h.frame + 1);
                        self.frames[h.frame].gosub.truncate(h.gosubs);
                        self.release(h.stack_len.min(self.sp), self.sp);
                        self.sp = h.stack_len;
                        if self.sp >= self.stack.len() {
                            self.grow_stack(self.sp);
                        }
                        self.stack[self.sp] = Value::Str(msg.into());
                        self.sp += 1;
                        self.frames[h.frame].pc = h.pc;
                        continue;
                    }
                    // under the debugger, stop where it went wrong first
                    if let Some(d) = self.debug.as_mut() {
                        d.stop = Some(StopReason::Error(msg.clone()));
                        d.error = Some(msg);
                        return Ok(Status::Paused);
                    }
                    return Err(self.fatal(msg, &prog));
                }
            }
        }
    }

    /// Ends the program with an error at the current location.
    fn fatal(&mut self, message: String, prog: &Program) -> RuntimeError {
        self.finished = true;
        // every active call, innermost first; a waiting call's saved pc is
        // the instruction after it
        let top = self.frames.len() - 1;
        let sites: Vec<(u32, u32)> = self
            .frames
            .iter()
            .enumerate()
            .rev()
            .flat_map(|(i, fr)| {
                let pc = if i == top { fr.pc } else { fr.pc.saturating_sub(1) };
                std::iter::once((fr.func, pc)).chain(fr.gosub.iter().rev().map(move |g| (fr.func, g.ret.saturating_sub(1))))
            })
            .collect();
        let site = |&(f, pc): &(u32, u32)| {
            let func = &prog.functions[f as usize];
            CallSite { location: prog.sources.location(func.span_at(pc)), function: func.name.clone() }
        };
        let CallSite { location, function } = site(&sites[0]);
        let rest = &sites[1..];
        let omitted = rest.len().saturating_sub(TRACE_HEAD + TRACE_TAIL);
        let callers = if omitted == 0 {
            rest.iter().map(site).collect()
        } else {
            rest[..TRACE_HEAD].iter().chain(&rest[rest.len() - TRACE_TAIL..]).map(site).collect()
        };
        RuntimeError { message, location, function, callers, omitted }
    }

    /// Makes room for another value at `sp`.
    #[cold]
    #[inline(never)]
    fn grow_stack(&mut self, sp: usize) {
        let need = (sp + 1).max(self.stack.len() * 2);
        self.stack.resize(need, Value::Null);
    }

    /// Lets go of the strings and arrays in stack slots `from..to`, which
    /// are no longer in use. Numbers are left: they hold nothing.
    #[inline]
    fn release(&mut self, from: usize, to: usize) {
        for v in &mut self.stack[from..to] {
            if matches!(v, Value::Str(_) | Value::Vector(_)) {
                *v = Value::Null;
            }
        }
    }

    /// Index into a one-dimensional array (by far the most common kind).
    #[inline]
    fn array_index1(&self, array: u32, i: i32) -> Result<usize, String> {
        let a = &self.arrays[array as usize];
        // a negative index is a huge unsigned one, so one test covers both ends
        if a.dims.len() == 1 && (i as u32 as usize) < a.dims[0] {
            return Ok(i as u32 as usize);
        }
        let mut idx = [0i32; 8];
        idx[0] = i;
        self.array_index_n(array, &idx)
    }

    #[cold]
    #[inline(never)]
    fn array_index_n(&self, array: u32, idx: &[i32; 8]) -> Result<usize, String> {
        let a = &self.arrays[array as usize];
        if a.dims.is_empty() {
            return Err(format!("array '{}' used before Dim", self.prog.arrays[array as usize].name));
        }
        let mut flat = 0usize;
        for (k, &d) in a.dims.iter().enumerate() {
            let i = idx[k.min(7)];
            if i < 0 || i as usize >= d {
                return Err(format!("array index out of bounds ({i} not in 0..{})", d - 1));
            }
            flat = flat * d + i as usize;
        }
        Ok(flat)
    }

    fn vector_index(v: &Value, i: i32) -> Result<VectorSlot, String> {
        match v {
            Value::Vector(vec) => {
                let len = vec.borrow().len();
                if i < 0 || i as usize >= len {
                    return Err(format!("array index out of bounds ({i} not in 0..{})", len.saturating_sub(1)));
                }
                Ok((vec.clone(), i as usize))
            }
            _ => Err("array is not allocated".into()),
        }
    }

    #[allow(unused_macros)]
    fn exec<const DEBUG: bool>(&mut self, host: &mut dyn Host, prog: &Program) -> Result<Status, String> {
        let mut fi = self.frames.len() - 1;
        let mut code: &[Op] = &prog.functions[self.frames[fi].func as usize].code;
        let mut pc = self.frames[fi].pc as usize;
        let mut base = self.frames[fi].base as usize;

        // The top of the value stack is kept in a local, and instructions read
        // and write stack slots in place. Numbers are read and written as
        // numbers (4 bytes) rather than moved as whole values: the processor
        // forwards a small write to a small read, but stalls on a whole-value
        // read of a slot that was just written in part.
        let mut sp = self.sp;
        vm_macros!($, self, sp, pc, base, fi);

        loop {
            if DEBUG {
                let func = self.frames[fi].func;
                if self.debug.as_mut().expect("debugger").check(func, fi + 1, pc as u32) {
                    self.frames[fi].pc = pc as u32;
                    self.sp = sp;
                    if self.debug_decide(fi + 1, pc as u32) {
                        leave!(Ok(Status::Paused));
                    }
                }
            }
            // matched in place: the operands are read straight from the code
            let op = &code[pc];
            pc += 1;
            match *op {
                Op::PushInt(v) => push_i!(v),
                Op::PushFloat(v) => push_f!(v),
                Op::PushStr(i) => push!(Value::Str(prog.strings[i as usize].clone())),
                Op::PushNull => push!(Value::Null),
                Op::Pop => {
                    sp -= 1;
                    self.release(sp, sp + 1);
                }
                Op::Dup => push_copy!(&self.stack[sp - 1]),
                Op::LoadLocal(s) => push_copy!(&self.stack[base + s as usize]),
                Op::StoreLocal(s) => {
                    sp -= 1;
                    move_to!(sp, self.stack[base + s as usize]);
                }
                Op::LoadGlobal(g) => push_copy!(&self.globals[g as usize]),
                Op::StoreGlobal(g) => {
                    sp -= 1;
                    move_to!(sp, self.globals[g as usize]);
                }
                Op::IncLocalI(s, d) => {
                    if let Value::Int(v) = &mut self.stack[base + s as usize] {
                        *v = v.wrapping_add(d);
                    }
                }
                Op::LoadLocal2(a, b) => {
                    push_copy!(&self.stack[base + a as usize]);
                    push_copy!(&self.stack[base + b as usize]);
                }
                Op::AddIL(s) => int_with!(local_i!(s), |a, b| a.wrapping_add(b)),
                Op::SubIL(s) => int_with!(local_i!(s), |a, b| a.wrapping_sub(b)),
                Op::MulIL(s) => int_with!(local_i!(s), |a, b| a.wrapping_mul(b)),
                Op::AddIK(k) => int_with!(k, |a, b| a.wrapping_add(b)),
                Op::MulIK(k) => int_with!(k, |a, b| a.wrapping_mul(b)),
                Op::DivIK(k) => int_with!(k, |a, b| a.wrapping_div(b)),
                Op::ModIK(k) => int_with!(k, |a, b| a.wrapping_rem(b)),
                Op::AddFL(s) => float_with!(local_f!(s), |a, b| a + b),
                Op::SubFL(s) => float_with!(local_f!(s), |a, b| a - b),
                Op::MulFL(s) => float_with!(local_f!(s), |a, b| a * b),
                Op::DivFL(s) => float_with!(local_f!(s), |a, b| a / b),
                Op::AddFK(k) => float_with!(k, |a, b| a + b),
                Op::MulFK(k) => float_with!(k, |a, b| a * b),
                Op::DivFK(k) => float_with!(k, |a, b| a / b),
                Op::LoadLocalField(s, i) => {
                    let Some(r) = self.heap.live_ref(&self.stack[base + s as usize]) else { fail!(NO_OBJECT) };
                    push_copy!(self.heap.field(r, i));
                }
                Op::LoadArrayL { array, slot } => {
                    let i = local_i!(slot);
                    let i = tri!(self.array_index1(array, i));
                    push_elem!(array, i);
                }
                Op::JumpCmpI(c, t) => {
                    let b = pop_i!();
                    let a = pop_i!();
                    if cmp_i(c, a, b) {
                        pc = t as usize;
                    }
                }
                Op::JumpCmpF(c, t) => {
                    let b = pop_f!();
                    let a = pop_f!();
                    if cmp(c, &a, &b) {
                        pc = t as usize;
                    }
                }
                Op::JumpNotCmpF(c, t) => {
                    let b = pop_f!();
                    let a = pop_f!();
                    if !cmp(c, &a, &b) {
                        pc = t as usize;
                    }
                }
                Op::JumpCmpIL { c, slot, target } => {
                    let b = local_i!(slot);
                    let a = pop_i!();
                    if cmp_i(c, a, b) {
                        pc = target as usize;
                    }
                }
                Op::JumpCmpIK { c, k, target } => {
                    let a = pop_i!();
                    if cmp_i(c, a, k) {
                        pc = target as usize;
                    }
                }
                Op::JumpCmpILL { c, a, b, target } => {
                    let (a, b) = (local_i!(a), local_i!(b));
                    if cmp_i(c, a, b) {
                        pc = target as usize;
                    }
                }
                Op::JumpCmpILK { c, slot, k, target } => {
                    let a = local_i!(slot);
                    if cmp_i(c, a, k) {
                        pc = target as usize;
                    }
                }
                Op::IncLocalJump { slot, d, target } => {
                    if let Value::Int(v) = &mut self.stack[base + slot as usize] {
                        *v = v.wrapping_add(d as i32);
                    }
                    pc = target as usize;
                }
                Op::LoadField(i) => {
                    let r = tri!(pop_obj!());
                    push_copy!(self.heap.field(r, i));
                }
                Op::StoreField(i) => {
                    // [obj, value]
                    sp -= 1;
                    let r = tri!(pop_obj!());
                    move_to!(sp + 1, *self.heap.field_mut(r, i));
                }
                Op::LoadArray { array, dims } => {
                    let i = tri!(array_index!(array, dims));
                    push_elem!(array, i);
                }
                Op::StoreArray { array, dims } => {
                    // [indices.., value]
                    sp -= 1;
                    let at = sp;
                    let i = tri!(array_index!(array, dims));
                    match &mut self.arrays[array as usize].data {
                        ArrayData::Int(d) => {
                            d[i] = match &self.stack[at] {
                                Value::Int(v) => *v,
                                v => v.int(),
                            }
                        }
                        ArrayData::Float(d) => {
                            d[i] = match &self.stack[at] {
                                Value::Float(v) => *v,
                                v => v.float(),
                            }
                        }
                        ArrayData::Other(d) => d[i] = std::mem::take(&mut self.stack[at]),
                    }
                }
                Op::AddI => int_bin!(|a, b| a.wrapping_add(b)),
                Op::SubI => int_bin!(|a, b| a.wrapping_sub(b)),
                Op::MulI => int_bin!(|a, b| a.wrapping_mul(b)),
                Op::DivI => {
                    if matches!(top!(), Value::Int(0)) {
                        fail!("Integer divide by zero");
                    }
                    int_bin!(|a, b| a.wrapping_div(b))
                }
                Op::ModI => {
                    if matches!(top!(), Value::Int(0)) {
                        fail!("Integer divide by zero");
                    }
                    int_bin!(|a, b| a.wrapping_rem(b))
                }
                Op::AndI => int_bin!(|a, b| a & b),
                Op::OrI => int_bin!(|a, b| a | b),
                Op::XorI => int_bin!(|a, b| a ^ b),
                Op::ShlI => int_bin!(|a, b| a.wrapping_shl(b as u32)),
                Op::ShrI => int_bin!(|a, b| (a as u32).wrapping_shr(b as u32) as i32),
                Op::SarI => int_bin!(|a, b| a.wrapping_shr(b as u32)),
                Op::NegI => {
                    if let Value::Int(v) = &mut top!() {
                        *v = v.wrapping_neg();
                    }
                }
                Op::BitNotI => {
                    if let Value::Int(v) = &mut top!() {
                        *v = !*v;
                    }
                }
                Op::AbsI => {
                    if let Value::Int(v) = &mut top!() {
                        *v = v.wrapping_abs();
                    }
                }
                Op::SgnI => {
                    if let Value::Int(v) = &mut top!() {
                        *v = v.signum();
                    }
                }
                Op::AddF => float_bin!(|a, b| a + b),
                Op::SubF => float_bin!(|a, b| a - b),
                Op::MulF => float_bin!(|a, b| a * b),
                Op::DivF => float_bin!(|a, b| a / b),
                Op::ModF => float_bin!(|a, b| a % b),
                Op::PowF => float_bin!(|a, b| a.powf(b)),
                Op::NegF => {
                    if let Value::Float(v) = &mut top!() {
                        *v = -*v;
                    }
                }
                Op::AbsF => {
                    if let Value::Float(v) = &mut top!() {
                        *v = v.abs();
                    }
                }
                Op::SgnF => {
                    if let Value::Float(v) = &mut top!() {
                        *v = if *v > 0.0 { 1.0 } else if *v < 0.0 { -1.0 } else { 0.0 };
                    }
                }
                Op::NotI => {
                    let v = pop_i!();
                    push_i!((v == 0) as i32);
                }
                Op::NotF => {
                    let v = pop_f!();
                    push_i!((v == 0.0) as i32);
                }
                Op::NotO => {
                    let v = pop!();
                    let dead = self.heap.live_ref(&v).is_none();
                    push_i!(dead as i32);
                }
                Op::CmpI(c) => {
                    let b = pop_i!();
                    let a = pop_i!();
                    push_i!(cmp_i(c, a, b) as i32);
                }
                Op::CmpF(c) => {
                    let b = pop_f!();
                    let a = pop_f!();
                    push_i!(cmp(c, &a, &b) as i32);
                }
                Op::CmpO(eq) => {
                    let b = pop!();
                    let a = pop!();
                    let same = self.heap.live_ref(&a) == self.heap.live_ref(&b);
                    push_i!((same == eq) as i32);
                }
                Op::IToF => {
                    let v = pop_i!();
                    push_f!(v as f32);
                }
                Op::FToI => {
                    let v = pop_f!();
                    push_i!(float_to_int(v));
                }
                Op::Jump(t) => pc = t as usize,
                Op::JumpIfZero(t) => {
                    if pop_i!() == 0 {
                        pc = t as usize;
                    }
                }
                Op::JumpIfNotZero(t) => {
                    if pop_i!() != 0 {
                        pc = t as usize;
                    }
                }
                Op::Call(f) => {
                    if self.frames.len() >= MAX_FRAMES {
                        fail!("stack overflow (too much recursion)");
                    }
                    self.frames[fi].pc = pc as u32;
                    let n = prog.functions[f as usize].n_params as usize;
                    self.sp = sp;
                    self.enter(f, n);
                    sp = self.sp;
                    fi = self.frames.len() - 1;
                    code = &prog.functions[f as usize].code;
                    pc = 0;
                    base = self.frames[fi].base as usize;
                }
                Op::Return => {
                    // the result moves down to where the frame began
                    sp -= 1;
                    let (result, frame) = (sp, base);
                    self.release(frame, result);
                    self.frames.pop();
                    while self.handlers.last().is_some_and(|h| h.frame >= fi) {
                        self.handlers.pop();
                    }
                    if self.frames.is_empty() {
                        leave!(Ok(Status::Finished));
                    }
                    fi -= 1;
                    let fr = &self.frames[fi];
                    code = &prog.functions[fr.func as usize].code;
                    pc = fr.pc as usize;
                    base = fr.base as usize;
                    if result != frame {
                        move_to!(result, self.stack[frame]);
                    }
                    sp = frame + 1;
                }
                Op::CallNative { id, argc } => {
                    let start = sp - argc as usize;
                    let mut ctx = NativeCtx::default();
                    let r = if id < self.n_core {
                        stdlib::call(id, &self.stack[start..sp], &mut self.core, &mut ctx)
                    } else {
                        host.call(id - self.n_core, &self.stack[start..sp], &mut ctx)
                    };
                    if ctx.retry_requested {
                        // leave the arguments on the stack and run this call again
                        self.frames[fi].pc = (pc - 1) as u32;
                        if DEBUG {
                            self.debug.as_mut().expect("debugger").skip = Some((fi + 1, (pc - 1) as u32));
                        }
                        leave!(Ok(Status::Yielded));
                    }
                    let v = tri!(r);
                    self.release(start, sp);
                    sp = start;
                    if self.native_returns[id as usize] {
                        push!(v);
                    }
                    if ctx.end_requested {
                        self.frames[fi].pc = pc as u32;
                        leave!(Ok(Status::Finished));
                    }
                    if ctx.yield_requested {
                        self.frames[fi].pc = pc as u32;
                        leave!(Ok(Status::Yielded));
                    }
                }
                Op::First(t) => {
                    let v = self.heap.first(t).map_or(Value::Null, Value::Obj);
                    push!(v);
                }
                Op::After => {
                    let r = tri!(pop_obj!());
                    let v = self.heap.after(r).map_or(Value::Null, Value::Obj);
                    push!(v);
                }
                // everything else is kept out of this function: the smaller
                // the loop, the more of its variables stay in registers
                _ => match self.exec_cold::<DEBUG>(*op, fi, pc, base, sp, prog)? {
                    Cold::Next(p, s) => {
                        pc = p;
                        sp = s;
                    }
                    Cold::Leave(status) => return Ok(status),
                },
            }
        }
    }

    /// The instructions that are not worth a place in the main loop.
    #[inline(never)]
    #[allow(unused_assignments, unused_mut, unused_variables, unused_macros)]
    fn exec_cold<const DEBUG: bool>(&mut self, op: Op, fi: usize, mut pc: usize, base: usize, mut sp: usize, prog: &Program) -> Result<Cold, String> {
        vm_macros!($, self, sp, pc, base, fi);
        match op {
            Op::DimArray { array, dims } => {
                let mut sizes = vec![0usize; dims as usize];
                for k in (0..dims as usize).rev() {
                    let n = pop_i!();
                    if n < 0 {
                        fail!(format!("negative array size {n}"));
                    }
                    sizes[k] = n as usize + 1;
                }
                let total = sizes.iter().try_fold(1usize, |a, &d| a.checked_mul(d)).filter(|&t| t <= MAX_ARRAY);
                let Some(total) = total else { fail!("array too large") };
                let k = prog.arrays[array as usize].elem;
                self.arrays[array as usize] = DimArray { dims: sizes, data: ArrayData::new(k, total) };
            }
            Op::NewVector { len, elem } => {
                push!(Value::Vector(Rc::new(RefCell::new(vec![Value::default_for(elem); len as usize]))));
            }
            Op::VectorLen => {
                let v = pop!();
                let n = match &v {
                    Value::Vector(vec) => vec.borrow().len() as i32,
                    _ => 0,
                };
                push_i!(n);
            }
            Op::LoadVector => {
                let i = pop_i!();
                let v = pop!();
                let (vec, i) = tri!(Self::vector_index(&v, i));
                let x = vec.borrow()[i].clone();
                push!(x);
            }
            Op::StoreVector => {
                let x = pop!();
                let i = pop_i!();
                let v = pop!();
                let (vec, i) = tri!(Self::vector_index(&v, i));
                vec.borrow_mut()[i] = x;
            }
            Op::Concat => {
                let b = pop_s!();
                let a = pop_s!();
                let mut s = String::with_capacity(a.len() + b.len());
                s.push_str(&a);
                s.push_str(&b);
                push!(Value::Str(s.into()));
            }
            Op::NotS => {
                let v = pop_s!();
                push_i!(v.is_empty() as i32);
            }
            Op::NotV => {
                let v = pop!();
                let z = match &v {
                    Value::Int(i) => *i == 0,
                    Value::Float(f) => *f == 0.0,
                    Value::Str(s) => s.is_empty(),
                    Value::Obj(_) => self.heap.live_ref(&v).is_none(),
                    Value::Null => true,
                    Value::Vector(_) => false,
                };
                push_i!(z as i32);
            }
            Op::CmpS(c) => {
                let b = pop_s!();
                let a = pop_s!();
                push_i!(cmp(c, &*a, &*b) as i32);
            }
            Op::BinV(o) => {
                let b = pop!();
                let a = pop!();
                let r = tri!(variant_bin(o, &a, &b));
                push!(r);
            }
            Op::NegV => {
                let v = pop!();
                let r = match v {
                    Value::Int(i) => Value::Int(i.wrapping_neg()),
                    Value::Float(f) => Value::Float(-f),
                    Value::Str(s) => Value::Float(-str_to_float(&s)),
                    _ => fail!("cannot negate a non-numeric value"),
                };
                push!(r);
            }
            Op::CmpV(c) => {
                let b = pop!();
                let a = pop!();
                let r = tri!(variant_cmp(c, &a, &b, &self.heap));
                push_i!(r as i32);
            }
            Op::IToS => {
                let v = pop_i!();
                push!(Value::Str(v.to_string().into()));
            }
            Op::FToS => {
                let v = pop_f!();
                push!(Value::Str(format_float(v).into()));
            }
            Op::SToI => {
                let v = pop_s!();
                push_i!(str_to_int(&v));
            }
            Op::SToF => {
                let v = pop_s!();
                push_f!(str_to_float(&v));
            }
            Op::FromVariant(k) => {
                let v = pop!();
                let r = match k {
                    ValKind::Int => match v {
                        Value::Int(_) => v,
                        Value::Float(_) | Value::Str(_) => Value::Int(v.int()),
                        _ => fail!(format!("cannot convert {} to Int", v.type_name())),
                    },
                    ValKind::Float => match v {
                        Value::Float(_) => v,
                        Value::Int(_) | Value::Str(_) => Value::Float(v.float()),
                        _ => fail!(format!("cannot convert {} to Float", v.type_name())),
                    },
                    ValKind::Str => match v {
                        Value::Str(_) => v,
                        Value::Int(_) | Value::Float(_) => Value::Str(v.str()),
                        _ => fail!(format!("cannot convert {} to String", v.type_name())),
                    },
                    _ => v,
                };
                push!(r);
            }
            Op::Gosub(t) => {
                self.frames[fi].gosub.push(GosubCall { ret: pc as u32, handlers: self.handlers.len() });
                pc = t as usize;
            }
            Op::GosubReturn => match self.frames[fi].gosub.pop() {
                Some(g) => {
                    self.handlers.truncate(g.handlers);
                    pc = g.ret as usize;
                }
                None => leave!(Ok(Cold::Leave(Status::Finished))),
            },
            Op::New(t) => {
                let r = self.heap.alloc(t, &prog.types[t as usize]);
                push!(Value::Obj(r));
            }
            Op::Delete => {
                let v = pop!();
                if let Some(r) = self.heap.live_ref(&v) {
                    self.heap.delete(r);
                }
            }
            Op::DeleteEach(t) => self.heap.delete_each(t),
            Op::Last(t) => {
                let v = self.heap.last(t).map_or(Value::Null, Value::Obj);
                push!(v);
            }
            Op::Before => {
                let r = tri!(pop_obj!());
                let v = self.heap.before(r).map_or(Value::Null, Value::Obj);
                push!(v);
            }
            Op::Insert { before } => {
                let t = tri!(pop_obj!());
                let o = tri!(pop_obj!());
                tri!(self.heap.insert(o, t, before));
            }
            Op::ToHandle => {
                let v = pop!();
                let h = match self.heap.live_ref(&v) {
                    Some(r) => self.heap.handle(r),
                    None => 0,
                };
                push_i!(h);
            }
            Op::FromHandle(t) => {
                let h = pop_i!();
                let v = self.heap.from_handle(h, t).map_or(Value::Null, Value::Obj);
                push!(v);
            }
            Op::Read(k) => {
                let Some(c) = prog.data.get(self.data_ptr) else { fail!("Out of data") };
                self.data_ptr += 1;
                let v = Value::from_const(c);
                let v = match k {
                    ValKind::Int => Value::Int(v.int()),
                    ValKind::Float => Value::Float(v.float()),
                    ValKind::Str => Value::Str(v.str()),
                    _ => v,
                };
                push!(v);
            }
            Op::Restore(i) => self.data_ptr = i as usize,
            Op::TryBegin { handler } => {
                self.handlers.push(Handler {
                    frame: fi,
                    stack_len: sp,
                    pc: handler,
                    gosubs: self.frames[fi].gosub.len(),
                });
            }
            Op::TryEnd => {
                self.handlers.pop();
            }
            Op::TryLeave(n) => {
                self.handlers.truncate(self.handlers.len().saturating_sub(n as usize));
            }
            Op::Throw => {
                let m = pop_s!();
                fail!(m.to_string());
            }
            Op::Stop if DEBUG => {
                self.frames[fi].pc = pc as u32;
                self.debug.as_mut().expect("debugger").stop = Some(StopReason::Stop);
                leave!(Ok(Cold::Leave(Status::Paused)));
            }
            Op::End | Op::Stop => {
                self.frames[fi].pc = (pc - 1) as u32;
                leave!(Ok(Cold::Leave(Status::Finished)));
            }
            _ => unreachable!("instruction handled by the main loop"),
        }
        Ok(Cold::Next(pc, sp))
    }
}

/// Integer comparison without a branch per kind of comparison: bit 0, 1 or 2
/// of the mask says whether it holds when `a` is greater, equal or less.
#[inline(always)]
fn cmp_i(c: Cmp, a: i32, b: i32) -> bool {
    const MASK: [u8; 6] = [0b010, 0b101, 0b100, 0b001, 0b110, 0b011];
    let state = ((a < b) as u8) << 1 | (a == b) as u8;
    MASK[c as usize] >> state & 1 != 0
}

#[inline]
fn cmp<T: PartialOrd + ?Sized>(c: Cmp, a: &T, b: &T) -> bool {
    match c {
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
        Cmp::Lt => a < b,
        Cmp::Gt => a > b,
        Cmp::Le => a <= b,
        Cmp::Ge => a >= b,
    }
}

fn variant_bin(o: VarOp, a: &Value, b: &Value) -> Result<Value, String> {
    let numeric = |v: &Value| matches!(v, Value::Int(_) | Value::Float(_));
    match (a, b) {
        (Value::Str(_), _) | (_, Value::Str(_)) if o == VarOp::Add => {
            if !matches!(a, Value::Str(_) | Value::Int(_) | Value::Float(_)) || !matches!(b, Value::Str(_) | Value::Int(_) | Value::Float(_)) {
                return Err(format!("cannot add {} and {}", a.type_name(), b.type_name()));
            }
            Ok(Value::Str(format!("{a}{b}").into()))
        }
        (Value::Int(x), Value::Int(y)) if o != VarOp::Pow => {
            let (x, y) = (*x, *y);
            Ok(Value::Int(match o {
                VarOp::Add => x.wrapping_add(y),
                VarOp::Sub => x.wrapping_sub(y),
                VarOp::Mul => x.wrapping_mul(y),
                VarOp::Div | VarOp::Mod if y == 0 => return Err("Integer divide by zero".into()),
                VarOp::Div => x.wrapping_div(y),
                VarOp::Mod => x.wrapping_rem(y),
                VarOp::Pow => unreachable!(),
            }))
        }
        _ if numeric(a) && numeric(b) => {
            let (x, y) = (a.float(), b.float());
            Ok(Value::Float(match o {
                VarOp::Add => x + y,
                VarOp::Sub => x - y,
                VarOp::Mul => x * y,
                VarOp::Div => x / y,
                VarOp::Mod => x % y,
                VarOp::Pow => x.powf(y),
            }))
        }
        _ => Err(format!("cannot apply {o:?} to {} and {}", a.type_name(), b.type_name())),
    }
}

fn variant_cmp(c: Cmp, a: &Value, b: &Value, heap: &Heap) -> Result<bool, String> {
    Ok(match (a, b) {
        (Value::Str(x), Value::Str(y)) => cmp(c, &**x, &**y),
        (Value::Str(_), _) | (_, Value::Str(_)) => cmp(c, &*a.str(), &*b.str()),
        (Value::Int(x), Value::Int(y)) => cmp(c, x, y),
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => cmp(c, &a.float(), &b.float()),
        (Value::Obj(_) | Value::Null, Value::Obj(_) | Value::Null) if matches!(c, Cmp::Eq | Cmp::Ne) => {
            let same = heap.live_ref(a) == heap.live_ref(b);
            if c == Cmp::Eq { same } else { !same }
        }
        _ => return Err(format!("cannot compare {} with {}", a.type_name(), b.type_name())),
    })
}

