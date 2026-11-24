//! Debugger support: breakpoints, stepping and looking at a stopped program.
//!
//! Nothing here runs unless [`Vm::debug_enable`] was called. The VM then
//! checks each statement it reaches and returns [`Status::Paused`] from
//! `run` when it should stop. The host calls [`DebugClient::service`], which
//! talks to the debugger front end and decides when the program goes on.
//!
//! [`Status::Paused`]: crate::vm::Status::Paused

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::value::{NbStr, Value};
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
    /// A breakpoint's condition could not be worked out here.
    Condition(String),
}

/// A breakpoint asked for by the front end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BreakpointSpec {
    pub line: u32,
    /// Stop only when this is true, such as `lives < 2 And player\x > 10`.
    pub condition: Option<String>,
    /// Stop only on certain hits: `5`, `>5`, `>=5`, `<5`, `<=5` or `%5`.
    pub hit_condition: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HitRule {
    Equal(u32),
    Above(u32),
    AtLeast(u32),
    Below(u32),
    AtMost(u32),
    Every(u32),
}

impl HitRule {
    fn parse(text: &str) -> Option<HitRule> {
        let t = text.trim();
        let number = |s: &str| s.trim().parse::<u32>().ok();
        if let Some(r) = t.strip_prefix(">=") {
            number(r).map(HitRule::AtLeast)
        } else if let Some(r) = t.strip_prefix("<=") {
            number(r).map(HitRule::AtMost)
        } else if let Some(r) = t.strip_prefix("==") {
            number(r).map(HitRule::Equal)
        } else if let Some(r) = t.strip_prefix('>') {
            number(r).map(HitRule::Above)
        } else if let Some(r) = t.strip_prefix('<') {
            number(r).map(HitRule::Below)
        } else if let Some(r) = t.strip_prefix('=') {
            number(r).map(HitRule::Equal)
        } else if let Some(r) = t.strip_prefix('%') {
            number(r).filter(|n| *n > 0).map(HitRule::Every)
        } else {
            number(t).map(HitRule::Equal)
        }
    }

    fn passes(self, hits: u32) -> bool {
        match self {
            HitRule::Equal(n) => hits == n,
            HitRule::Above(n) => hits > n,
            HitRule::AtLeast(n) => hits >= n,
            HitRule::Below(n) => hits < n,
            HitRule::AtMost(n) => hits <= n,
            HitRule::Every(n) => hits.is_multiple_of(n),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Breakpoint {
    condition: Option<String>,
    rule: Option<HitRule>,
    /// Times it was reached with its condition true.
    hits: u32,
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
    breakpoints: HashMap<(u32, u32), Breakpoint>,
    /// A breakpoint reached whose condition the VM must now work out.
    pending: Option<Point>,
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
        } else if let Some(b) = self.breakpoints.get(&(p.file, p.line)).filter(|_| p.first) {
            if b.condition.is_some() || b.rule.is_some() {
                // needs the program's variables: see `Vm::debug_decide`
                self.pending = Some(p);
                return true;
            }
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
            breakpoints: HashMap::new(),
            pending: None,
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
        let specs: Vec<BreakpointSpec> = lines.iter().map(|&line| BreakpointSpec { line, ..Default::default() }).collect();
        self.debug_set_breakpoint_specs(file, &specs).into_iter().map(Result::ok).collect()
    }

    /// Replaces a file's breakpoints, which may have conditions. Gives the
    /// line each one ended up on, or why it could not be placed. A breakpoint
    /// that is sent again unchanged keeps its hit count.
    pub fn debug_set_breakpoint_specs(&mut self, file: u32, specs: &[BreakpointSpec]) -> Vec<Result<u32, String>> {
        let Some(d) = self.debug.as_mut() else { return vec![Err("the debugger is off".into()); specs.len()] };
        let mut old = HashMap::new();
        d.breakpoints.retain(|k, b| {
            if k.0 == file {
                old.insert(k.1, b.clone());
            }
            k.0 != file
        });
        let mut placed = Vec::with_capacity(specs.len());
        for spec in specs {
            let rule = match spec.hit_condition.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                Some(t) => match HitRule::parse(t) {
                    Some(r) => Some(r),
                    None => {
                        placed.push(Err(format!("bad hit count '{t}': use 5, >5, >=5, <5, <=5 or %5")));
                        continue;
                    }
                },
                None => None,
            };
            let Some(line) = d.lines.range((file, spec.line)..=(file, u32::MAX)).next().map(|l| l.1) else {
                placed.push(Err("no code here in the running program".into()));
                continue;
            };
            let condition = spec.condition.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(String::from);
            let hits = old.get(&line).filter(|b| b.condition == condition && b.rule == rule).map_or(0, |b| b.hits);
            d.breakpoints.insert((file, line), Breakpoint { condition, rule, hits });
            placed.push(Ok(line));
        }
        placed
    }

    /// Called when a statement check stopped the interpreter. True if the
    /// program really stops; false if a breakpoint's condition says go on.
    pub(crate) fn debug_decide(&mut self, depth: usize, pc: u32) -> bool {
        let d = self.debug.as_mut().expect("debugger");
        let Some(p) = d.pending.take() else { return true };
        let key = (p.file, p.line);
        let condition = d.breakpoints.get(&key).and_then(|b| b.condition.clone());
        let outcome = condition.map_or(Ok(true), |c| self.debug_condition(0, &c));
        let d = self.debug.as_mut().expect("debugger");
        let reason = match outcome {
            Err(e) => Some(StopReason::Condition(e)),
            Ok(false) => None,
            Ok(true) => d.breakpoints.get_mut(&key).and_then(|b| {
                b.hits += 1;
                b.rule.is_none_or(|r| r.passes(b.hits)).then_some(StopReason::Breakpoint)
            }),
        };
        // a step can still end on a line whose breakpoint did not fire
        let reason = reason.or_else(|| d.step.is_some_and(|s| step_done(s, p, depth, pc)).then_some(StopReason::Step));
        let Some(reason) = reason else { return false };
        d.step = None;
        d.stop = Some(reason);
        d.skip = Some((depth, pc));
        true
    }

    /// Works out a breakpoint condition in a frame (0 = innermost).
    pub fn debug_condition(&self, frame: usize, expr: &str) -> Result<bool, String> {
        let mut e = Eval { s: expr.as_bytes(), i: 0, vm: self, frame, dead: false };
        let v = e.logic()?;
        e.ws();
        if e.i != e.s.len() {
            return Err(format!("could not understand '{}'", String::from_utf8_lossy(&e.s[e.i..])));
        }
        Ok(e.truth(&v))
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
        for flat in 0..a.data.len().min(MAX_CHILDREN) {
            let mut idx = vec![0; a.dims.len()];
            let mut rest = flat;
            for (k, &d) in a.dims.iter().enumerate().rev() {
                idx[k] = rest % d.max(1);
                rest /= d.max(1);
            }
            let text: Vec<String> = idx.iter().map(usize::to_string).collect();
            items.push((format!("({})", text.join(", ")), a.data.get(flat)));
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
        let mut e = Eval { s: expr.as_bytes(), i: 0, vm: self, frame, dead: false };
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
    /// Reading the unused side of `AndAlso`/`OrElse`: nothing can fail.
    dead: bool,
}

impl Eval<'_> {
    /// True if the next word is `w`, which is then consumed.
    fn word(&mut self, w: &str) -> bool {
        self.ws();
        let end = self.i + w.len();
        let same = self.s.get(self.i..end).is_some_and(|t| t.eq_ignore_ascii_case(w.as_bytes()));
        let whole = !self.s.get(end).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
        if same && whole {
            self.i = end;
        }
        same && whole
    }

    fn truth(&self, v: &Value) -> bool {
        match v {
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Null => false,
            Value::Obj(_) => self.vm.heap.live_ref(v).is_some(),
            _ => true,
        }
    }

    /// `And`, `Or` and `Xor` work on bits as in programs; `AndAlso` and
    /// `OrElse` skip their right side when the left side decides.
    fn logic(&mut self) -> Result<Value, String> {
        let mut a = self.not()?;
        loop {
            if self.word("AndAlso") || self.word("OrElse") {
                let is_and = self.s[self.i - 1].eq_ignore_ascii_case(&b'o');
                let left = self.truth(&a);
                let decided = left != is_and;
                let was = self.dead;
                self.dead = was || decided;
                let b = self.not()?;
                self.dead = was;
                let result = if decided { left } else { self.truth(&b) };
                a = Value::Int(result as i32);
            } else if self.word("And") {
                let b = self.not()?;
                a = Value::Int(a.int() & b.int());
            } else if self.word("Or") {
                let b = self.not()?;
                a = Value::Int(a.int() | b.int());
            } else if self.word("Xor") {
                let b = self.not()?;
                a = Value::Int(a.int() ^ b.int());
            } else {
                return Ok(a);
            }
        }
    }

    fn not(&mut self) -> Result<Value, String> {
        if self.word("Not") {
            let v = self.not()?;
            return Ok(Value::Int(!self.truth(&v) as i32));
        }
        self.compare()
    }

    fn compare(&mut self) -> Result<Value, String> {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let mut a = self.sum()?;
        loop {
            self.ws();
            let rest = &self.s[self.i..];
            let (len, wanted): (usize, &[std::cmp::Ordering]) = if rest.starts_with(b"<>") || rest.starts_with(b"!=") {
                (2, &[Less, Greater])
            } else if rest.starts_with(b"<=") || rest.starts_with(b"=<") {
                (2, &[Less, Equal])
            } else if rest.starts_with(b">=") || rest.starts_with(b"=>") {
                (2, &[Greater, Equal])
            } else if rest.starts_with(b"==") {
                (2, &[Equal])
            } else if rest.starts_with(b"=") {
                (1, &[Equal])
            } else if rest.starts_with(b"<") {
                (1, &[Less])
            } else if rest.starts_with(b">") {
                (1, &[Greater])
            } else {
                return Ok(a);
            };
            self.i += len;
            let b = self.sum()?;
            a = Value::Int(self.order(&a, &b).is_some_and(|o| wanted.contains(&o)) as i32);
        }
    }

    /// How two values compare; `None` for objects that merely differ.
    fn order(&self, a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::Equal;
        let is_ref = |v: &Value| matches!(v, Value::Obj(_) | Value::Null | Value::Vector(_));
        if is_ref(a) || is_ref(b) {
            let heap = &self.vm.heap;
            return match (heap.live_ref(a), heap.live_ref(b)) {
                (None, None) => Some(Equal),
                (Some(x), Some(y)) if x == y => Some(Equal),
                _ => Some(std::cmp::Ordering::Less),
            };
        }
        if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
            return Some(a.str().cmp(&b.str()));
        }
        if matches!(a, Value::Float(_)) || matches!(b, Value::Float(_)) {
            return a.float().partial_cmp(&b.float());
        }
        Some(a.int().cmp(&b.int()))
    }

    fn sum(&mut self) -> Result<Value, String> {
        let mut a = self.product()?;
        loop {
            if self.eat(b'+') {
                let b = self.product()?;
                a = match (&a, &b) {
                    (Value::Str(_), _) | (_, Value::Str(_)) => Value::Str(NbStr::concat(a.text(), b.text())),
                    (Value::Float(_), _) | (_, Value::Float(_)) => Value::Float(a.float() + b.float()),
                    _ => Value::Int(a.int().wrapping_add(b.int())),
                };
            } else if self.eat(b'-') {
                let b = self.product()?;
                a = match (&a, &b) {
                    (Value::Float(_), _) | (_, Value::Float(_)) => Value::Float(a.float() - b.float()),
                    _ => Value::Int(a.int().wrapping_sub(b.int())),
                };
            } else {
                return Ok(a);
            }
        }
    }

    fn product(&mut self) -> Result<Value, String> {
        let mut a = self.power()?;
        loop {
            let is_float = |x: &Value, y: &Value| matches!(x, Value::Float(_)) || matches!(y, Value::Float(_));
            if self.eat(b'*') {
                let b = self.power()?;
                a = if is_float(&a, &b) { Value::Float(a.float() * b.float()) } else { Value::Int(a.int().wrapping_mul(b.int())) };
            } else if self.eat(b'/') || self.word("Mod") {
                let divide = self.s[self.i - 1] == b'/';
                let b = self.power()?;
                a = if is_float(&a, &b) {
                    Value::Float(if divide { a.float() / b.float() } else { a.float() % b.float() })
                } else if b.int() == 0 {
                    if !self.dead {
                        return Err("Integer divide by zero".into());
                    }
                    Value::Int(0)
                } else if divide {
                    Value::Int(a.int().wrapping_div(b.int()))
                } else {
                    Value::Int(a.int().wrapping_rem(b.int()))
                };
            } else {
                return Ok(a);
            }
        }
    }

    fn power(&mut self) -> Result<Value, String> {
        let a = self.unary()?;
        if self.eat(b'^') {
            let b = self.power()?;
            return Ok(Value::Float(a.float().powf(b.float())));
        }
        Ok(a)
    }

    fn unary(&mut self) -> Result<Value, String> {
        if self.eat(b'-') {
            return Ok(match self.unary()? {
                Value::Float(f) => Value::Float(-f),
                v => Value::Int(v.int().wrapping_neg()),
            });
        }
        if self.eat(b'+') {
            return self.unary();
        }
        if self.eat(b'(') {
            let v = self.logic()?;
            if !self.eat(b')') {
                return Err("expected ')'".into());
            }
            return Ok(v);
        }
        self.ws();
        match self.s.get(self.i) {
            Some(b'"') => return self.string(),
            Some(c) if c.is_ascii_digit() || *c == b'.' => return self.number(),
            Some(b'$') => return self.radix(16),
            Some(b'%') => return self.radix(2),
            _ => {}
        }
        if self.word("True") {
            return Ok(Value::Int(1));
        }
        if self.word("False") {
            return Ok(Value::Int(0));
        }
        if self.word("Null") {
            return Ok(Value::Null);
        }
        if self.word("Pi") {
            return Ok(Value::Float(std::f32::consts::PI));
        }
        self.path()
    }

    fn string(&mut self) -> Result<Value, String> {
        let start = self.i + 1;
        let Some(len) = self.s[start..].iter().position(|c| *c == b'"') else { return Err("the text has no closing quote".into()) };
        self.i = start + len + 1;
        Ok(Value::Str(String::from_utf8_lossy(&self.s[start..start + len]).into_owned().into()))
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        let digits = |e: &mut Self| {
            while e.s.get(e.i).is_some_and(u8::is_ascii_digit) {
                e.i += 1;
            }
        };
        digits(self);
        let mut float = false;
        if self.s.get(self.i) == Some(&b'.') {
            float = true;
            self.i += 1;
            digits(self);
        }
        if self.s.get(self.i).is_some_and(|c| c.eq_ignore_ascii_case(&b'e')) {
            float = true;
            self.i += 1;
            if self.s.get(self.i).is_some_and(|c| b"+-".contains(c)) {
                self.i += 1;
            }
            digits(self);
        }
        let text = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        if float {
            text.parse().map(Value::Float).map_err(|_| format!("bad number '{text}'"))
        } else {
            text.parse().map(Value::Int).map_err(|_| format!("bad number '{text}'"))
        }
    }

    /// `$ff` or `%101`.
    fn radix(&mut self, radix: u32) -> Result<Value, String> {
        self.i += 1;
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| (*c as char).is_digit(radix)) {
            self.i += 1;
        }
        let text = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        u32::from_str_radix(&text, radix).map(|n| Value::Int(n as i32)).map_err(|_| "bad number".to_string())
    }

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

    /// An index: a whole number or any sum, such as `i + 1`.
    fn index(&mut self) -> Result<i32, String> {
        Ok(self.sum()?.int())
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
            if self.dead { Value::Int(0) } else { self.array_element(&name, &idx)? }
        } else if self.dead {
            Value::Int(0)
        } else {
            self.variable(&name)?
        };
        loop {
            if self.eat(b'\\') {
                let field = self.ident()?;
                if !self.dead {
                    v = self.field(&v, &field)?;
                }
            } else if self.eat(b'[') {
                let idx = self.indices(b']')?;
                if self.dead {
                    continue;
                }
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
        Ok(a.data.get(flat))
    }

    fn field(&self, v: &Value, name: &str) -> Result<Value, String> {
        let vm = self.vm;
        let r = vm.heap.live_ref(v).ok_or(crate::value::NO_OBJECT)?;
        let info = &vm.prog.types[vm.heap.type_of(r) as usize];
        let i = info.field_names.iter().position(|n| n.eq_ignore_ascii_case(name)).ok_or_else(|| format!("{} has no field '{name}'", info.name))?;
        Ok(vm.heap.field(r, i as u32).clone())
    }
}
