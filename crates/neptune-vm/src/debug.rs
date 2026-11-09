//! Debugger support: breakpoints, stepping and looking at a stopped program.
//!
//! Nothing here runs unless [`Vm::debug_enable`] was called. The VM then
//! checks each statement it reaches and returns [`Status::Paused`] from
//! `run` when it should stop. The host calls [`DebugClient::service`], which
//! talks to the debugger front end and decides when the program goes on.
//!
//! [`Status::Paused`]: crate::vm::Status::Paused

use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::value::Value;
use crate::vm::Vm;

/// Most children shown for one array or object.
const MAX_CHILDREN: usize = 1000;

/// Why the program stopped.
#[derive(Clone, Debug, PartialEq)]
pub enum StopReason {
    Entry,
    Breakpoint,
    Step,
    Pause,
    /// A `Stop` statement.
    Stop,
    /// An error nothing caught; the program ends when it continues.
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    In,
    Over,
    Out,
}

/// What the host does after [`DebugClient::service`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Run the program.
    Run,
    /// The program stays stopped: keep the window alive and ask again.
    Hold,
    /// End the program.
    Quit,
}

/// The debugger front end, called by the host between runs of the VM.
pub trait DebugClient {
    /// Answers the front end's requests. With `wait`, may block until the
    /// program should go on (only console programs do: a window must keep
    /// running, so it never waits and gets [`Control::Hold`] instead).
    fn service(&mut self, vm: &mut Vm, wait: bool) -> Control;
}

#[derive(Clone, Copy)]
struct Point {
    file: u32,
    line: u32,
    /// The first statement of its line in this function: only these take
    /// breakpoints, so a line with several statements stops once.
    first: bool,
}

#[derive(Clone, Copy)]
struct Step {
    kind: StepKind,
    depth: usize,
    file: u32,
    line: u32,
    pc: u32,
}

pub(crate) struct DebugState {
    /// By function, then instruction.
    points: Vec<Vec<Option<Point>>>,
    /// `(file, line)` of every place a breakpoint can go.
    lines: BTreeSet<(u32, u32)>,
    breakpoints: HashSet<(u32, u32)>,
    step: Option<Step>,
    pause: Option<StopReason>,
    /// `(depth, pc)` of the statement to run without stopping again.
    pub(crate) skip: Option<(usize, u32)>,
    /// Set by another thread when the front end has something to say.
    attention: Arc<AtomicBool>,
    pub(crate) stop: Option<StopReason>,
    /// The uncaught error being shown.
    pub(crate) error: Option<String>,
}

impl DebugState {
    /// Called for every instruction; true means stop here.
    #[inline]
    pub(crate) fn check(&mut self, func: u32, depth: usize, pc: u32) -> bool {
        match self.points[func as usize][pc as usize] {
            Some(p) => self.at_statement(p, depth, pc),
            None => false,
        }
    }

    fn at_statement(&mut self, p: Point, depth: usize, pc: u32) -> bool {
        let here = (depth, pc);
        let skipped = self.skip.take() == Some(here);
        if self.attention.swap(false, Ordering::Relaxed) {
            // not a stop: the client looks at its messages, then this
            // statement is checked again
            self.skip = skipped.then_some(here);
            return true;
        }
        if skipped {
            return false;
        }
        let reason = if let Some(r) = self.pause.take() {
            Some(r)
        } else if p.first && self.breakpoints.contains(&(p.file, p.line)) {
            Some(StopReason::Breakpoint)
        } else if self.step.is_some_and(|s| step_done(s, p, depth, pc)) {
            Some(StopReason::Step)
        } else {
            None
        };
        let Some(reason) = reason else { return false };
        self.step = None;
        self.stop = Some(reason);
        self.skip = Some(here);
        true
    }
}

fn step_done(s: Step, p: Point, depth: usize, pc: u32) -> bool {
    // a jump back to the same line is the next turn of a one-line loop
    let moved = (p.file, p.line) != (s.file, s.line) || pc <= s.pc;
    match s.kind {
        StepKind::In => depth != s.depth || moved,
        StepKind::Over => depth < s.depth || (depth == s.depth && moved),
        StepKind::Out => depth < s.depth,
    }
}

/// One entry of the call stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    pub function: String,
    pub file: u32,
    pub line: u32,
    pub column: u32,
}

/// A `Dim` array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayView {
    pub index: usize,
    pub name: String,
    pub dims: Vec<usize>,
}

impl Vm {
    /// Turns the debugger on. Setting `attention` from any thread makes the
    /// VM return to the client at the next statement (even from a loop that
    /// never yields).
    pub fn debug_enable(&mut self, attention: Arc<AtomicBool>) {
        let prog = self.prog.clone();
        let mut points = Vec::with_capacity(prog.functions.len());
        let mut lines = BTreeSet::new();
        for f in &prog.functions {
            let mut at = vec![None; f.code.len()];
            let mut seen = HashSet::new();
            for &pc in &f.stmts {
                let span = f.span_at(pc);
                let Some(file) = prog.sources.files.get(span.file as usize) else { continue };
                let line = file.line_col(span.start).0 as u32;
                let first = seen.insert((span.file, line));
                if first {
                    lines.insert((span.file, line));
                }
                if let Some(slot) = at.get_mut(pc as usize) {
                    *slot = Some(Point { file: span.file, line, first });
                }
            }
            points.push(at);
        }
        self.debug = Some(Box::new(DebugState {
            points,
            lines,
            breakpoints: HashSet::new(),
            step: None,
            pause: None,
            skip: None,
            attention,
            stop: None,
            error: None,
        }));
    }

    /// Why the program is stopped (`None` while it runs).
    pub fn debug_stop(&self) -> Option<&StopReason> {
        self.debug.as_ref()?.stop.as_ref()
    }

    /// Stops at the next statement.
    pub fn debug_pause(&mut self, reason: StopReason) {
        if let Some(d) = self.debug.as_mut() {
            d.pause = Some(reason);
        }
    }

    /// Replaces a file's breakpoints. Returns the line each one ended up on:
    /// the next line with a statement, or `None` if the file has none below.
    pub fn debug_set_breakpoints(&mut self, file: u32, lines: &[u32]) -> Vec<Option<u32>> {
        let Some(d) = self.debug.as_mut() else { return vec![None; lines.len()] };
        d.breakpoints.retain(|b| b.0 != file);
        let mut placed = Vec::with_capacity(lines.len());
        for &line in lines {
            let found = d.lines.range((file, line)..=(file, u32::MAX)).next().map(|l| l.1);
            if let Some(l) = found {
                d.breakpoints.insert((file, l));
            }
            placed.push(found);
        }
        placed
    }

    /// Lets a stopped program go on.
    pub fn debug_continue(&mut self) {
        if let Some(d) = self.debug.as_mut() {
            d.stop = None;
            d.step = None;
        }
    }

    /// Lets a stopped program run one step.
    pub fn debug_step(&mut self, kind: StepKind) {
        let Some(top) = self.frames.last() else { return };
        let func = &self.prog.functions[top.func as usize];
        let (depth, pc) = (self.frames.len(), top.pc);
        let span = func.span_at(pc);
        let line = self.prog.sources.files.get(span.file as usize).map_or(0, |f| f.line_col(span.start).0 as u32);
        if let Some(d) = self.debug.as_mut() {
            d.stop = None;
            d.step = Some(Step { kind, depth, file: span.file, line, pc });
        }
    }

    /// The call stack, innermost first.
    pub fn debug_frames(&self) -> Vec<FrameInfo> {
        let after_stop = self.debug_stop() == Some(&StopReason::Stop);
        let top = self.frames.len().saturating_sub(1);
        self.frames
            .iter()
            .enumerate()
            .rev()
            .map(|(i, fr)| {
                let func = &self.prog.functions[fr.func as usize];
                // callers hold their return address, the instruction after the call
                let pc = if i < top || after_stop { fr.pc.saturating_sub(1) } else { fr.pc };
                let span = func.span_at(pc);
                let (line, column) = self.prog.sources.files.get(span.file as usize).map_or((1, 1), |f| f.line_col(span.start));
                FrameInfo { function: func.name.clone(), file: span.file, line: line as u32, column: column as u32 }
            })
            .collect()
    }

    /// Local variables of a frame (0 = innermost).
    pub fn debug_locals(&self, frame: usize) -> Vec<(String, Value)> {
        let Some(fr) = self.frame_at(frame) else { return Vec::new() };
        let func = &self.prog.functions[fr.func as usize];
        let base = fr.base as usize;
        func.local_names
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.starts_with('\0'))
            .filter_map(|(i, n)| Some((n.clone(), self.stack.get(base + i)?.clone())))
            .collect()
    }

    pub fn debug_globals(&self) -> Vec<(String, Value)> {
        self.prog.globals.iter().zip(&self.globals).map(|((n, _), v)| (n.clone(), v.clone())).collect()
    }

    /// The `Dim` arrays that have been allocated.
    pub fn debug_arrays(&self) -> Vec<ArrayView> {
        self.prog
            .arrays
            .iter()
            .zip(&self.arrays)
            .enumerate()
            .filter(|(_, (_, a))| !a.dims.is_empty())
            .map(|(index, (info, a))| ArrayView { index, name: info.name.clone(), dims: a.dims.clone() })
            .collect()
    }

    /// Elements of a `Dim` array named by their indices, such as `(1, 2)`.
    pub fn debug_array_items(&self, index: usize) -> Vec<(String, Value)> {
        let Some(a) = self.arrays.get(index) else { return Vec::new() };
        let mut items = Vec::new();
        for (flat, v) in a.data.iter().enumerate().take(MAX_CHILDREN) {
            let mut idx = vec![0; a.dims.len()];
            let mut rest = flat;
            for (k, &d) in a.dims.iter().enumerate().rev() {
                idx[k] = rest % d.max(1);
                rest /= d.max(1);
            }
            let text: Vec<String> = idx.iter().map(usize::to_string).collect();
            items.push((format!("({})", text.join(", ")), v.clone()));
        }
        Self::note_cut(&mut items, a.data.len());
        items
    }

    /// Fields of an object or elements of a vector.
    pub fn debug_children(&self, v: &Value) -> Vec<(String, Value)> {
        match v {
            Value::Obj(_) => {
                let Some(r) = self.heap.live_ref(v) else { return Vec::new() };
                let info = &self.prog.types[self.heap.type_of(r) as usize];
                info.field_names.iter().enumerate().map(|(i, n)| (n.clone(), self.heap.field(r, i as u32).clone())).collect()
            }
            Value::Vector(vec) => {
                let vec = vec.borrow();
                let mut items: Vec<_> = vec.iter().take(MAX_CHILDREN).enumerate().map(|(i, x)| (format!("[{i}]"), x.clone())).collect();
                Self::note_cut(&mut items, vec.len());
                items
            }
            _ => Vec::new(),
        }
    }

    fn note_cut(items: &mut Vec<(String, Value)>, total: usize) {
        if total > items.len() {
            let more = total - items.len();
            items.push(("...".into(), Value::Str(format!("{more} more").into())));
        }
    }

    /// True if a value has children to expand.
    pub fn debug_has_children(&self, v: &Value) -> bool {
        match v {
            Value::Obj(_) => self.heap.live_ref(v).is_some(),
            Value::Vector(_) => true,
            _ => false,
        }
    }

    /// Text shown for a value.
    pub fn debug_describe(&self, v: &Value) -> String {
        match v {
            Value::Str(s) => format!("{s:?}"),
            Value::Obj(_) => match self.heap.live_ref(v) {
                Some(r) => self.prog.types[self.heap.type_of(r) as usize].name.clone(),
                None => "Null".into(),
            },
            other => other.to_string(),
        }
    }

    /// Value of a variable, field or element: `score`, `player\pos\x`,
    /// `grid(2, i)` or `list[0]`, in a frame (0 = innermost).
    pub fn debug_eval(&self, frame: usize, expr: &str) -> Result<Value, String> {
        let mut e = Eval { s: expr.as_bytes(), i: 0, vm: self, frame };
        let v = e.path()?;
        e.ws();
        if e.i != e.s.len() {
            return Err("only variables, fields and elements can be shown".into());
        }
        Ok(v)
    }

    fn frame_at(&self, frame: usize) -> Option<&crate::vm::Frame> {
        self.frames.iter().rev().nth(frame)
    }
}

struct Eval<'a> {
    s: &'a [u8],
    i: usize,
    vm: &'a Vm,
    frame: usize,
}

impl Eval<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        let yes = self.s.get(self.i) == Some(&c);
        if yes {
            self.i += 1;
        }
        yes
    }

    fn ident(&mut self) -> Result<String, String> {
        self.ws();
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
            self.i += 1;
        }
        if start == self.i || self.s[start].is_ascii_digit() {
            return Err("only variables, fields and elements can be shown".into());
        }
        let name = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        // a type tag is part of how it was written, not of the name
        if self.s.get(self.i).is_some_and(|c| b"%#$".contains(c)) {
            self.i += 1;
        }
        Ok(name)
    }

    /// An index: a whole number or anything else that can be shown.
    fn index(&mut self) -> Result<i32, String> {
        self.ws();
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        let digits = self.i;
        while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        if self.i > digits {
            return std::str::from_utf8(&self.s[start..self.i]).ok().and_then(|t| t.parse().ok()).ok_or_else(|| "bad index".to_string());
        }
        self.i = start;
        Ok(self.path()?.int())
    }

    fn indices(&mut self, close: u8) -> Result<Vec<i32>, String> {
        let mut out = vec![self.index()?];
        while self.eat(b',') {
            out.push(self.index()?);
        }
        if !self.eat(close) {
            return Err(format!("expected '{}'", close as char));
        }
        Ok(out)
    }

    fn path(&mut self) -> Result<Value, String> {
        let name = self.ident()?;
        let mut v = if self.eat(b'(') {
            let idx = self.indices(b')')?;
            self.array_element(&name, &idx)?
        } else {
            self.variable(&name)?
        };
        loop {
            if self.eat(b'\\') {
                let field = self.ident()?;
                v = self.field(&v, &field)?;
            } else if self.eat(b'[') {
                let idx = self.indices(b']')?;
                let Value::Vector(vec) = &v else { return Err("not an array".into()) };
                let item = idx.first().and_then(|&i| vec.borrow().get(usize::try_from(i).ok()?).cloned());
                v = item.ok_or("index out of bounds")?;
            } else {
                return Ok(v);
            }
        }
    }

    fn variable(&self, name: &str) -> Result<Value, String> {
        let vm = self.vm;
        let same = |n: &str| n.eq_ignore_ascii_case(name);
        let local = vm.frame_at(self.frame).and_then(|fr| {
            let func = &vm.prog.functions[fr.func as usize];
            let slot = func.local_names.iter().position(|n| same(n))?;
            vm.stack.get(fr.base as usize + slot).cloned()
        });
        local
            .or_else(|| vm.prog.globals.iter().position(|(n, _)| same(n)).map(|i| vm.globals[i].clone()))
            .ok_or_else(|| format!("'{name}' is not a variable here"))
    }

    fn array_element(&self, name: &str, idx: &[i32]) -> Result<Value, String> {
        let vm = self.vm;
        let i = vm.prog.arrays.iter().position(|a| a.name.eq_ignore_ascii_case(name)).ok_or_else(|| format!("'{name}' is not an array"))?;
        let a = &vm.arrays[i];
        if a.dims.is_empty() || a.dims.len() != idx.len() {
            return Err(format!("'{name}' has {} dimensions", a.dims.len()));
        }
        let mut flat = 0usize;
        for (&d, &k) in a.dims.iter().zip(idx) {
            let k = usize::try_from(k).ok().filter(|k| *k < d).ok_or("index out of bounds")?;
            flat = flat * d + k;
        }
        Ok(a.data[flat].clone())
    }

    fn field(&self, v: &Value, name: &str) -> Result<Value, String> {
        let vm = self.vm;
        let r = vm.heap.live_ref(v).ok_or(crate::value::NO_OBJECT)?;
        let info = &vm.prog.types[vm.heap.type_of(r) as usize];
        let i = info.field_names.iter().position(|n| n.eq_ignore_ascii_case(name)).ok_or_else(|| format!("{} has no field '{name}'", info.name))?;
        Ok(vm.heap.field(r, i as u32).clone())
    }
}
