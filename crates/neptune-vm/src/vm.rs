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
use crate::value::{Heap, ObjRef, Value, NO_OBJECT};

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
    gosub: Vec<u32>,
}

struct Handler {
    frame: usize,
    stack_len: usize,
    pc: u32,
}

pub(crate) struct DimArray {
    pub(crate) dims: Vec<usize>,
    pub(crate) data: Vec<Value>,
}

const MAX_FRAMES: usize = 100_000;
const MAX_ARRAY: usize = 1 << 28;

pub struct Vm {
    pub(crate) prog: Rc<Program>,
    pub(crate) stack: Vec<Value>,
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
            stack: Vec::with_capacity(1024),
            frames: Vec::new(),
            globals: prog.globals.iter().map(|(_, k)| Value::default_for(*k)).collect(),
            arrays: prog.arrays.iter().map(|_| DimArray { dims: vec![], data: vec![] }).collect(),
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
        let base = self.stack.len() - n_args;
        for k in &f.locals[f.n_params as usize..] {
            self.stack.push(Value::default_for(*k));
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
                        self.stack.truncate(h.stack_len);
                        self.stack.push(Value::Str(msg.into()));
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
                std::iter::once((fr.func, pc)).chain(fr.gosub.iter().rev().map(move |r| (fr.func, r.saturating_sub(1))))
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

    #[inline]
    fn pop(&mut self) -> Value {
        self.stack.pop().expect("stack underflow")
    }
    #[inline]
    fn pop_i(&mut self) -> i32 {
        match self.stack.pop() {
            Some(Value::Int(v)) => v,
            Some(v) => v.int(),
            None => panic!("stack underflow"),
        }
    }
    #[inline]
    fn pop_f(&mut self) -> f32 {
        match self.stack.pop() {
            Some(Value::Float(v)) => v,
            Some(v) => v.float(),
            None => panic!("stack underflow"),
        }
    }
    #[inline]
    fn pop_s(&mut self) -> Rc<str> {
        match self.stack.pop() {
            Some(Value::Str(s)) => s,
            Some(v) => v.str(),
            None => panic!("stack underflow"),
        }
    }
    #[inline]
    fn top(&mut self) -> &mut Value {
        self.stack.last_mut().expect("stack underflow")
    }

    fn pop_obj(&mut self) -> Result<ObjRef, String> {
        let v = self.pop();
        self.heap.live_ref(&v).ok_or_else(|| NO_OBJECT.to_string())
    }

    fn array_index(&mut self, array: u32, dims: u8) -> Result<usize, String> {
        let mut idx = [0i32; 8];
        let n = dims as usize;
        for k in (0..n).rev() {
            idx[k.min(7)] = self.pop_i();
        }
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

    fn exec<const DEBUG: bool>(&mut self, host: &mut dyn Host, prog: &Program) -> Result<Status, String> {
        let mut fi = self.frames.len() - 1;
        let mut code: &[Op] = &prog.functions[self.frames[fi].func as usize].code;
        let mut pc = self.frames[fi].pc as usize;
        let mut base = self.frames[fi].base as usize;

        macro_rules! fail {
            ($e:expr) => {{
                self.frames[fi].pc = (pc - 1) as u32;
                return Err($e.into());
            }};
        }
        macro_rules! tri {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => fail!(e),
                }
            };
        }
        macro_rules! int_bin {
            (|$a:ident, $b:ident| $body:expr) => {{
                let $b = self.pop_i();
                match self.top() {
                    Value::Int($a) => {
                        let $a = *$a;
                        *self.top() = Value::Int($body);
                    }
                    _ => unreachable!("typed int op on non-int"),
                }
            }};
        }
        macro_rules! float_bin {
            (|$a:ident, $b:ident| $body:expr) => {{
                let $b = self.pop_f();
                let $a = match self.top() {
                    Value::Float(v) => *v,
                    v => v.float(),
                };
                *self.top() = Value::Float($body);
            }};
        }

        loop {
            if DEBUG {
                let func = self.frames[fi].func;
                if self.debug.as_mut().expect("debugger").check(func, fi + 1, pc as u32) {
                    self.frames[fi].pc = pc as u32;
                    return Ok(Status::Paused);
                }
            }
            let op = code[pc];
            pc += 1;
            match op {
                Op::PushInt(v) => self.stack.push(Value::Int(v)),
                Op::PushFloat(v) => self.stack.push(Value::Float(v)),
                Op::PushStr(i) => self.stack.push(Value::Str(prog.strings[i as usize].clone())),
                Op::PushNull => self.stack.push(Value::Null),
                Op::Pop => {
                    self.stack.pop();
                }
                Op::Dup => {
                    let v = self.top().clone();
                    self.stack.push(v);
                }
                Op::LoadLocal(s) => {
                    let v = self.stack[base + s as usize].clone();
                    self.stack.push(v);
                }
                Op::StoreLocal(s) => {
                    let v = self.pop();
                    self.stack[base + s as usize] = v;
                }
                Op::LoadGlobal(g) => {
                    let v = self.globals[g as usize].clone();
                    self.stack.push(v);
                }
                Op::StoreGlobal(g) => {
                    let v = self.pop();
                    self.globals[g as usize] = v;
                }
                Op::IncLocalI(s, d) => {
                    if let Value::Int(v) = &mut self.stack[base + s as usize] {
                        *v = v.wrapping_add(d);
                    }
                }
                Op::LoadField(i) => {
                    let r = tri!(self.pop_obj());
                    let v = self.heap.field(r, i).clone();
                    self.stack.push(v);
                }
                Op::StoreField(i) => {
                    let v = self.pop();
                    let r = tri!(self.pop_obj());
                    *self.heap.field_mut(r, i) = v;
                }
                Op::DimArray { array, dims } => {
                    let mut sizes = vec![0usize; dims as usize];
                    for k in (0..dims as usize).rev() {
                        let n = self.pop_i();
                        if n < 0 {
                            fail!(format!("negative array size {n}"));
                        }
                        sizes[k] = n as usize + 1;
                    }
                    let total = sizes.iter().try_fold(1usize, |a, &d| a.checked_mul(d)).filter(|&t| t <= MAX_ARRAY);
                    let Some(total) = total else { fail!("array too large") };
                    let k = prog.arrays[array as usize].elem;
                    self.arrays[array as usize] = DimArray { dims: sizes, data: vec![Value::default_for(k); total] };
                }
                Op::LoadArray { array, dims } => {
                    let i = tri!(self.array_index(array, dims));
                    let v = self.arrays[array as usize].data[i].clone();
                    self.stack.push(v);
                }
                Op::StoreArray { array, dims } => {
                    let v = self.pop();
                    let i = tri!(self.array_index(array, dims));
                    self.arrays[array as usize].data[i] = v;
                }
                Op::NewVector { len, elem } => {
                    self.stack.push(Value::Vector(Rc::new(RefCell::new(vec![Value::default_for(elem); len as usize]))));
                }
                Op::VectorLen => {
                    let v = self.pop();
                    let n = match &v {
                        Value::Vector(vec) => vec.borrow().len() as i32,
                        _ => 0,
                    };
                    self.stack.push(Value::Int(n));
                }
                Op::LoadVector => {
                    let i = self.pop_i();
                    let v = self.pop();
                    let (vec, i) = tri!(Self::vector_index(&v, i));
                    let x = vec.borrow()[i].clone();
                    self.stack.push(x);
                }
                Op::StoreVector => {
                    let x = self.pop();
                    let i = self.pop_i();
                    let v = self.pop();
                    let (vec, i) = tri!(Self::vector_index(&v, i));
                    vec.borrow_mut()[i] = x;
                }

                Op::AddI => int_bin!(|a, b| a.wrapping_add(b)),
                Op::SubI => int_bin!(|a, b| a.wrapping_sub(b)),
                Op::MulI => int_bin!(|a, b| a.wrapping_mul(b)),
                Op::DivI => {
                    if matches!(self.stack.last(), Some(Value::Int(0))) {
                        fail!("Integer divide by zero");
                    }
                    int_bin!(|a, b| a.wrapping_div(b))
                }
                Op::ModI => {
                    if matches!(self.stack.last(), Some(Value::Int(0))) {
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
                    if let Value::Int(v) = self.top() {
                        *v = v.wrapping_neg();
                    }
                }
                Op::BitNotI => {
                    if let Value::Int(v) = self.top() {
                        *v = !*v;
                    }
                }
                Op::AbsI => {
                    if let Value::Int(v) = self.top() {
                        *v = v.wrapping_abs();
                    }
                }
                Op::SgnI => {
                    if let Value::Int(v) = self.top() {
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
                    if let Value::Float(v) = self.top() {
                        *v = -*v;
                    }
                }
                Op::AbsF => {
                    if let Value::Float(v) = self.top() {
                        *v = v.abs();
                    }
                }
                Op::SgnF => {
                    if let Value::Float(v) = self.top() {
                        *v = if *v > 0.0 { 1.0 } else if *v < 0.0 { -1.0 } else { 0.0 };
                    }
                }
                Op::Concat => {
                    let b = self.pop_s();
                    let a = self.pop_s();
                    let mut s = String::with_capacity(a.len() + b.len());
                    s.push_str(&a);
                    s.push_str(&b);
                    self.stack.push(Value::Str(s.into()));
                }
                Op::NotI => {
                    let v = self.pop_i();
                    self.stack.push(Value::Int((v == 0) as i32));
                }
                Op::NotF => {
                    let v = self.pop_f();
                    self.stack.push(Value::Int((v == 0.0) as i32));
                }
                Op::NotS => {
                    let v = self.pop_s();
                    self.stack.push(Value::Int(v.is_empty() as i32));
                }
                Op::NotO => {
                    let v = self.pop();
                    let dead = self.heap.live_ref(&v).is_none();
                    self.stack.push(Value::Int(dead as i32));
                }
                Op::NotV => {
                    let v = self.pop();
                    let z = match &v {
                        Value::Int(i) => *i == 0,
                        Value::Float(f) => *f == 0.0,
                        Value::Str(s) => s.is_empty(),
                        Value::Obj(_) => self.heap.live_ref(&v).is_none(),
                        Value::Null => true,
                        Value::Vector(_) => false,
                    };
                    self.stack.push(Value::Int(z as i32));
                }
                Op::CmpI(c) => {
                    let b = self.pop_i();
                    let a = self.pop_i();
                    self.stack.push(Value::Int(cmp(c, &a, &b) as i32));
                }
                Op::CmpF(c) => {
                    let b = self.pop_f();
                    let a = self.pop_f();
                    self.stack.push(Value::Int(cmp(c, &a, &b) as i32));
                }
                Op::CmpS(c) => {
                    let b = self.pop_s();
                    let a = self.pop_s();
                    self.stack.push(Value::Int(cmp(c, &*a, &*b) as i32));
                }
                Op::CmpO(eq) => {
                    let b = self.pop();
                    let a = self.pop();
                    let same = self.heap.live_ref(&a) == self.heap.live_ref(&b);
                    self.stack.push(Value::Int((same == eq) as i32));
                }
                Op::BinV(o) => {
                    let b = self.pop();
                    let a = self.pop();
                    let r = tri!(variant_bin(o, &a, &b));
                    self.stack.push(r);
                }
                Op::NegV => {
                    let v = self.pop();
                    let r = match v {
                        Value::Int(i) => Value::Int(i.wrapping_neg()),
                        Value::Float(f) => Value::Float(-f),
                        Value::Str(s) => Value::Float(-str_to_float(&s)),
                        _ => fail!("cannot negate a non-numeric value"),
                    };
                    self.stack.push(r);
                }
                Op::CmpV(c) => {
                    let b = self.pop();
                    let a = self.pop();
                    let r = tri!(variant_cmp(c, &a, &b, &self.heap));
                    self.stack.push(Value::Int(r as i32));
                }
                Op::IToF => {
                    let v = self.pop_i();
                    self.stack.push(Value::Float(v as f32));
                }
                Op::FToI => {
                    let v = self.pop_f();
                    self.stack.push(Value::Int(float_to_int(v)));
                }
                Op::IToS => {
                    let v = self.pop_i();
                    self.stack.push(Value::Str(v.to_string().into()));
                }
                Op::FToS => {
                    let v = self.pop_f();
                    self.stack.push(Value::Str(format_float(v).into()));
                }
                Op::SToI => {
                    let v = self.pop_s();
                    self.stack.push(Value::Int(str_to_int(&v)));
                }
                Op::SToF => {
                    let v = self.pop_s();
                    self.stack.push(Value::Float(str_to_float(&v)));
                }
                Op::FromVariant(k) => {
                    let v = self.pop();
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
                    self.stack.push(r);
                }

                Op::Jump(t) => pc = t as usize,
                Op::JumpIfZero(t) => {
                    if self.pop_i() == 0 {
                        pc = t as usize;
                    }
                }
                Op::JumpIfNotZero(t) => {
                    if self.pop_i() != 0 {
                        pc = t as usize;
                    }
                }
                Op::Call(f) => {
                    if self.frames.len() >= MAX_FRAMES {
                        fail!("stack overflow (too much recursion)");
                    }
                    self.frames[fi].pc = pc as u32;
                    let n = prog.functions[f as usize].n_params as usize;
                    self.enter(f, n);
                    fi = self.frames.len() - 1;
                    code = &prog.functions[f as usize].code;
                    pc = 0;
                    base = self.frames[fi].base as usize;
                }
                Op::Return => {
                    let v = self.pop();
                    self.stack.truncate(base);
                    self.frames.pop();
                    while self.handlers.last().is_some_and(|h| h.frame >= fi) {
                        self.handlers.pop();
                    }
                    if self.frames.is_empty() {
                        return Ok(Status::Finished);
                    }
                    fi -= 1;
                    let fr = &self.frames[fi];
                    code = &prog.functions[fr.func as usize].code;
                    pc = fr.pc as usize;
                    base = fr.base as usize;
                    self.stack.push(v);
                }
                Op::CallNative { id, argc } => {
                    let start = self.stack.len() - argc as usize;
                    let mut ctx = NativeCtx::default();
                    let r = if id < self.n_core {
                        stdlib::call(id, &self.stack[start..], &mut self.core, &mut ctx)
                    } else {
                        host.call(id - self.n_core, &self.stack[start..], &mut ctx)
                    };
                    if ctx.retry_requested {
                        // leave the arguments on the stack and run this call again
                        self.frames[fi].pc = (pc - 1) as u32;
                        if DEBUG {
                            self.debug.as_mut().expect("debugger").skip = Some((fi + 1, (pc - 1) as u32));
                        }
                        return Ok(Status::Yielded);
                    }
                    let v = tri!(r);
                    self.stack.truncate(start);
                    if self.native_returns[id as usize] {
                        self.stack.push(v);
                    }
                    if ctx.end_requested {
                        self.frames[fi].pc = pc as u32;
                        return Ok(Status::Finished);
                    }
                    if ctx.yield_requested {
                        self.frames[fi].pc = pc as u32;
                        return Ok(Status::Yielded);
                    }
                }
                Op::Gosub(t) => {
                    self.frames[fi].gosub.push(pc as u32);
                    pc = t as usize;
                }
                Op::GosubReturn => match self.frames[fi].gosub.pop() {
                    Some(r) => pc = r as usize,
                    None => return Ok(Status::Finished),
                },

                Op::New(t) => {
                    let r = self.heap.alloc(t, &prog.types[t as usize]);
                    self.stack.push(Value::Obj(r));
                }
                Op::Delete => {
                    let v = self.pop();
                    if let Some(r) = self.heap.live_ref(&v) {
                        self.heap.delete(r);
                    }
                }
                Op::DeleteEach(t) => self.heap.delete_each(t),
                Op::First(t) => {
                    let v = self.heap.first(t).map_or(Value::Null, Value::Obj);
                    self.stack.push(v);
                }
                Op::Last(t) => {
                    let v = self.heap.last(t).map_or(Value::Null, Value::Obj);
                    self.stack.push(v);
                }
                Op::After => {
                    let r = tri!(self.pop_obj());
                    let v = self.heap.after(r).map_or(Value::Null, Value::Obj);
                    self.stack.push(v);
                }
                Op::Before => {
                    let r = tri!(self.pop_obj());
                    let v = self.heap.before(r).map_or(Value::Null, Value::Obj);
                    self.stack.push(v);
                }
                Op::Insert { before } => {
                    let t = tri!(self.pop_obj());
                    let o = tri!(self.pop_obj());
                    tri!(self.heap.insert(o, t, before));
                }
                Op::ToHandle => {
                    let v = self.pop();
                    let h = match self.heap.live_ref(&v) {
                        Some(r) => self.heap.handle(r),
                        None => 0,
                    };
                    self.stack.push(Value::Int(h));
                }
                Op::FromHandle(t) => {
                    let h = self.pop_i();
                    let v = self.heap.from_handle(h, t).map_or(Value::Null, Value::Obj);
                    self.stack.push(v);
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
                    self.stack.push(v);
                }
                Op::Restore(i) => self.data_ptr = i as usize,
                Op::TryBegin { handler } => {
                    self.handlers.push(Handler { frame: fi, stack_len: self.stack.len(), pc: handler });
                }
                Op::TryEnd => {
                    self.handlers.pop();
                }
                Op::Throw => {
                    let m = self.pop_s();
                    fail!(m.to_string());
                }
                Op::Stop if DEBUG => {
                    self.frames[fi].pc = pc as u32;
                    self.debug.as_mut().expect("debugger").stop = Some(StopReason::Stop);
                    return Ok(Status::Paused);
                }
                Op::End | Op::Stop => {
                    self.frames[fi].pc = (pc - 1) as u32;
                    return Ok(Status::Finished);
                }
            }
        }
    }
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

