//! `neptune dap`: the debugger for editors, speaking the Debug Adapter
//! Protocol over a local TCP connection.
//!
//! It prints the port it listens on as the first line of stdout, accepts one
//! connection and runs the program in this process. A socket rather than
//! stdin/stdout keeps the program's own output (and the engine's log) from
//! mixing with the protocol.

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use neptune_lang::FsLoader;
use neptune_vm::debug::{BreakpointSpec, Control, DebugClient, StepKind, StopReason};
use neptune_vm::{Value, Vm};

use crate::json::{n, obj, parse, s, Json};

const THREAD: u32 = 1;

struct Conn {
    out: TcpStream,
    seq: u32,
    rx: Receiver<Json>,
}

impl Conn {
    fn send(&mut self, mut msg: Json) {
        self.seq += 1;
        if let Json::Obj(m) = &mut msg {
            m.insert("seq".into(), n(self.seq));
        }
        let body = msg.to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        // a closed connection shows up in the reader thread
        let _ = self.out.write_all(frame.as_bytes());
    }

    fn event(&mut self, name: &str, body: Json) {
        self.send(obj([("type", s("event")), ("event", s(name)), ("body", body)]));
    }

    fn respond(&mut self, req: &Json, body: Json) {
        self.reply(req, true, None, body);
    }

    fn fail(&mut self, req: &Json, message: &str) {
        self.reply(req, false, Some(message), Json::Null);
    }

    fn reply(&mut self, req: &Json, success: bool, message: Option<&str>, body: Json) {
        let seq = req.get("seq").num().unwrap_or(0.0);
        let command = req.get("command").clone();
        let mut r = obj([("type", s("response")), ("request_seq", Json::Num(seq)), ("success", Json::Bool(success)), ("command", command), ("body", body)]);
        if let (Json::Obj(m), Some(text)) = (&mut r, message) {
            m.insert("message".into(), s(text));
        }
        self.send(r);
    }

    fn output(&mut self, category: &str, text: &str) {
        self.event("output", obj([("category", s(category)), ("output", s(text))]));
    }
}

/// What a `variablesReference` stands for; valid until the program moves.
enum VarRef {
    Locals(usize),
    Globals,
    Arrays,
    Array(usize),
    Value(Value),
}

struct Session {
    conn: Rc<RefCell<Conn>>,
    /// Canonical path of each source file, by file id.
    files: Vec<PathBuf>,
    vars: Vec<VarRef>,
    announced: bool,
    no_debug: bool,
}

/// Path as editors write it (Windows canonical paths start with `\\?\`).
fn editor_path(p: &Path) -> String {
    let t = p.to_string_lossy();
    match t.strip_prefix(r"\\?\") {
        Some(rest) => match rest.strip_prefix(r"UNC\") {
            Some(unc) => format!(r"\\{unc}"),
            None => rest.to_string(),
        },
        None => t.into_owned(),
    }
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

impl Session {
    fn reference(&mut self, v: VarRef) -> u32 {
        self.vars.push(v);
        self.vars.len() as u32
    }

    /// One row of the variables view.
    fn variable(&mut self, vm: &Vm, name: String, v: Value) -> Json {
        let value = vm.debug_describe(&v);
        let child = if vm.debug_has_children(&v) { self.reference(VarRef::Value(v)) } else { 0 };
        obj([("name", s(name)), ("value", s(value)), ("variablesReference", n(child))])
    }

    fn announce(&mut self, reason: &StopReason) {
        let (name, text) = match reason {
            StopReason::Entry => ("entry", None),
            StopReason::Breakpoint => ("breakpoint", None),
            StopReason::Step => ("step", None),
            StopReason::Pause => ("pause", None),
            StopReason::Stop => ("pause", Some("Stop statement".to_string())),
            StopReason::Error(m) => ("exception", Some(m.clone())),
            StopReason::Condition(m) => ("breakpoint", Some(format!("Breakpoint condition: {m}"))),
        };
        let mut body = obj([("reason", s(name)), ("threadId", n(THREAD)), ("allThreadsStopped", Json::Bool(true))]);
        if let (Json::Obj(m), Some(t)) = (&mut body, text) {
            m.insert("description".into(), s(t.clone()));
            m.insert("text".into(), s(t));
        }
        self.conn.borrow_mut().event("stopped", body);
    }

    /// Handles a request; `Some` when the host should act on it now.
    fn request(&mut self, vm: &mut Vm, req: &Json) -> Option<Control> {
        let command = req.get("command").str().unwrap_or("");
        let args = req.get("arguments");
        let stopped = vm.debug_stop().is_some();
        match command {
            "threads" => {
                let list = Json::Arr(vec![obj([("id", n(THREAD)), ("name", s("main"))])]);
                self.conn.borrow_mut().respond(req, obj([("threads", list)]));
            }
            "setBreakpoints" => self.set_breakpoints(vm, req),
            "setExceptionBreakpoints" | "configurationDone" => self.conn.borrow_mut().respond(req, Json::Null),
            "stackTrace" => self.stack_trace(vm, req),
            "scopes" => {
                let frame = (args.get("frameId").num().unwrap_or(1.0) as usize).saturating_sub(1);
                let locals = self.reference(VarRef::Locals(frame));
                let globals = self.reference(VarRef::Globals);
                let scope = |name: &str, r: u32, expensive: bool| obj([("name", s(name)), ("variablesReference", n(r)), ("expensive", Json::Bool(expensive))]);
                let mut list = vec![scope("Locals", locals, false), scope("Globals", globals, false)];
                if !vm.debug_arrays().is_empty() {
                    let arrays = self.reference(VarRef::Arrays);
                    list.push(scope("Arrays", arrays, false));
                }
                self.conn.borrow_mut().respond(req, obj([("scopes", Json::Arr(list))]));
            }
            "variables" => self.variables(vm, req),
            "evaluate" => {
                if !stopped {
                    self.conn.borrow_mut().fail(req, "the program is running");
                    return None;
                }
                let frame = (args.get("frameId").num().unwrap_or(1.0) as usize).saturating_sub(1);
                match vm.debug_eval(frame, args.get("expression").str().unwrap_or("")) {
                    Ok(v) => {
                        let result = vm.debug_describe(&v);
                        let child = if vm.debug_has_children(&v) { self.reference(VarRef::Value(v)) } else { 0 };
                        self.conn.borrow_mut().respond(req, obj([("result", s(result)), ("variablesReference", n(child))]));
                    }
                    Err(e) => self.conn.borrow_mut().fail(req, &e),
                }
            }
            "continue" | "next" | "stepIn" | "stepOut" => {
                if !stopped {
                    self.conn.borrow_mut().fail(req, "the program is running");
                    return None;
                }
                match command {
                    "next" => vm.debug_step(StepKind::Over),
                    "stepIn" => vm.debug_step(StepKind::In),
                    "stepOut" => vm.debug_step(StepKind::Out),
                    _ => vm.debug_continue(),
                }
                self.announced = false;
                self.conn.borrow_mut().respond(req, obj([("allThreadsContinued", Json::Bool(true))]));
                return Some(Control::Run);
            }
            "pause" => {
                if !stopped {
                    vm.debug_pause(StopReason::Pause);
                }
                self.conn.borrow_mut().respond(req, Json::Null);
            }
            "disconnect" | "terminate" => {
                self.conn.borrow_mut().respond(req, Json::Null);
                return Some(Control::Quit);
            }
            other => self.conn.borrow_mut().fail(req, &format!("{other} is not supported")),
        }
        None
    }

    fn set_breakpoints(&mut self, vm: &mut Vm, req: &Json) {
        let args = req.get("arguments");
        let path = canonical(Path::new(args.at("source.path").str().unwrap_or("")));
        let text = |b: &Json, key: &str| b.get(key).str().map(String::from);
        let wanted: Vec<BreakpointSpec> = args
            .get("breakpoints")
            .arr()
            .iter()
            .map(|b| BreakpointSpec { line: b.get("line").num().unwrap_or(0.0) as u32, condition: text(b, "condition"), hit_condition: text(b, "hitCondition") })
            .collect();
        let file = self.files.iter().position(|f| *f == path).filter(|_| !self.no_debug);
        let placed = match file {
            Some(f) => vm.debug_set_breakpoint_specs(f as u32, &wanted),
            None => vec![Err("no code here in the running program".to_string()); wanted.len()],
        };
        let list = wanted
            .iter()
            .zip(placed)
            .map(|(asked, got)| match got {
                Ok(line) => obj([("verified", Json::Bool(true)), ("line", n(line))]),
                Err(why) => obj([("verified", Json::Bool(false)), ("line", n(asked.line)), ("message", s(why))]),
            })
            .collect();
        self.conn.borrow_mut().respond(req, obj([("breakpoints", Json::Arr(list))]));
    }

    fn stack_trace(&mut self, vm: &Vm, req: &Json) {
        if vm.debug_stop().is_none() {
            self.conn.borrow_mut().fail(req, "the program is running");
            return;
        }
        let args = req.get("arguments");
        let frames = vm.debug_frames();
        let start = args.get("startFrame").num().unwrap_or(0.0) as usize;
        let levels = args.get("levels").num().filter(|l| *l > 0.0).map_or(frames.len(), |l| l as usize);
        let list = frames
            .iter()
            .enumerate()
            .skip(start)
            .take(levels)
            .map(|(i, f)| {
                let path = self.files.get(f.file as usize).map(|p| editor_path(p)).unwrap_or_default();
                let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                obj([
                    ("id", n(i as u32 + 1)),
                    ("name", s(f.function.clone())),
                    ("source", obj([("name", s(name)), ("path", s(path))])),
                    ("line", n(f.line)),
                    ("column", n(f.column)),
                ])
            })
            .collect();
        self.conn.borrow_mut().respond(req, obj([("stackFrames", Json::Arr(list)), ("totalFrames", n(frames.len() as u32))]));
    }

    fn variables(&mut self, vm: &Vm, req: &Json) {
        let id = req.at("arguments.variablesReference").num().unwrap_or(0.0) as usize;
        let rows: Vec<Json> = match id.checked_sub(1).and_then(|i| self.vars.get(i)) {
            Some(VarRef::Arrays) => {
                let arrays = vm.debug_arrays();
                let mut rows = Vec::with_capacity(arrays.len());
                for a in arrays {
                    let dims: Vec<String> = a.dims.iter().map(|d| (d.saturating_sub(1)).to_string()).collect();
                    let child = self.reference(VarRef::Array(a.index));
                    rows.push(obj([("name", s(a.name)), ("value", s(format!("Dim ({})", dims.join(", ")))), ("variablesReference", n(child))]));
                }
                rows
            }
            Some(r) => {
                let items = match r {
                    VarRef::Locals(frame) => vm.debug_locals(*frame),
                    VarRef::Globals => vm.debug_globals(),
                    VarRef::Array(i) => vm.debug_array_items(*i),
                    VarRef::Value(v) => vm.debug_children(v),
                    VarRef::Arrays => unreachable!(),
                };
                items.into_iter().map(|(name, v)| self.variable(vm, name, v)).collect()
            }
            None => Vec::new(),
        };
        self.conn.borrow_mut().respond(req, obj([("variables", Json::Arr(rows))]));
    }
}

impl DebugClient for Session {
    fn service(&mut self, vm: &mut Vm, wait: bool) -> Control {
        if !self.announced {
            if let Some(reason) = vm.debug_stop().cloned() {
                self.vars.clear();
                self.announced = true;
                self.announce(&reason);
            }
        }
        loop {
            let stopped = vm.debug_stop().is_some();
            let next = {
                let conn = self.conn.borrow();
                if wait && stopped {
                    conn.rx.recv().map_err(|_| TryRecvError::Disconnected)
                } else {
                    conn.rx.try_recv()
                }
            };
            match next {
                Ok(req) => {
                    if let Some(c) = self.request(vm, &req) {
                        return c;
                    }
                }
                Err(TryRecvError::Empty) => return if stopped { Control::Hold } else { Control::Run },
                Err(TryRecvError::Disconnected) => return Control::Quit,
            }
        }
    }
}

/// Reads protocol messages on its own thread, so a running program can be
/// interrupted: `attention` makes the VM look up at the next statement.
fn read_messages(stream: TcpStream, tx: std::sync::mpsc::Sender<Json>, attention: Arc<AtomicBool>) {
    let mut input = BufReader::new(stream);
    'messages: loop {
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            if !matches!(input.read_line(&mut header), Ok(k) if k > 0) {
                break 'messages;
            }
            let header = header.trim();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
        }
        let mut body = vec![0; length];
        if input.read_exact(&mut body).is_err() {
            break;
        }
        let Ok(msg) = parse(&String::from_utf8_lossy(&body)) else { continue };
        if tx.send(msg).is_err() {
            return;
        }
        attention.store(true, Ordering::Relaxed);
    }
    // the editor went away: dropping `tx` tells the session
    drop(tx);
    attention.store(true, Ordering::Relaxed);
}

/// Compiles the program named by a `launch` request and prepares it to run.
fn launch(args: &Json, attention: &Arc<AtomicBool>) -> Result<(Vm, Vec<PathBuf>), String> {
    let program = args.get("program").str().ok_or("launch: no program given")?;
    if let Some(cwd) = args.get("cwd").str().filter(|c| !c.is_empty()) {
        std::env::set_current_dir(cwd).map_err(|e| format!("cannot use folder '{cwd}': {e}"))?;
    }
    if let Some(level) = args.get("quality").str() {
        if neptune_runtime::quality::parse_level(level).is_none() {
            return Err("quality must be low, medium, high or ultra".into());
        }
        std::env::set_var("NEPTUNE_QUALITY", level);
    }
    let words = args.get("args").arr().iter().filter_map(|a| a.str().map(str::to_string)).collect();
    neptune_vm::stdlib::set_program_args(words);
    let src = std::fs::read_to_string(program).map_err(|e| format!("cannot read '{program}': {e}"))?;
    let prog = neptune_runtime::compile_source(program, &src, &mut FsLoader).map_err(|e| e.to_string())?;
    let files = prog.sources.files.iter().map(|f| canonical(Path::new(&f.name))).collect();
    let mut vm = Vm::new(prog);
    vm.debug_enable(attention.clone());
    Ok((vm, files))
}

pub fn run(args: &[String]) -> ExitCode {
    let port: u16 = args.iter().position(|a| a == "--port").and_then(|i| args.get(i + 1)).and_then(|p| p.parse().ok()).unwrap_or(0);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot listen on port {port}: {e}");
            return ExitCode::from(1);
        }
    };
    let Ok(local) = listener.local_addr() else { return ExitCode::from(1) };
    println!("{}", local.port());
    let _ = std::io::stdout().flush();
    let Ok((stream, _)) = listener.accept() else { return ExitCode::from(1) };
    drop(listener);
    let _ = stream.set_nodelay(true);
    let Ok(reader) = stream.try_clone() else { return ExitCode::from(1) };

    let (tx, rx) = channel();
    let attention = Arc::new(AtomicBool::new(false));
    let flag = attention.clone();
    std::thread::spawn(move || read_messages(reader, tx, flag));
    let conn = Rc::new(RefCell::new(Conn { out: stream, seq: 0, rx }));
    let mut session = Session { conn: conn.clone(), files: Vec::new(), vars: Vec::new(), announced: false, no_debug: false };

    // set-up: until the editor has sent its breakpoints
    let mut vm: Option<Vm> = None;
    loop {
        let Ok(req) = conn.borrow().rx.recv() else { return ExitCode::SUCCESS };
        match req.get("command").str().unwrap_or("") {
            "initialize" => {
                let caps = obj([("supportsConfigurationDoneRequest", Json::Bool(true)), ("supportsEvaluateForHovers", Json::Bool(true)), ("supportsConditionalBreakpoints", Json::Bool(true)), ("supportsHitConditionalBreakpoints", Json::Bool(true)),("supportsTerminateRequest", Json::Bool(true))]);
                conn.borrow_mut().respond(&req, caps);
            }
            "launch" => {
                let args = req.get("arguments");
                match launch(args, &attention) {
                    Ok((mut v, files)) => {
                        session.files = files;
                        session.no_debug = args.get("noDebug") == &Json::Bool(true);
                        if args.get("stopOnEntry") == &Json::Bool(true) && !session.no_debug {
                            v.debug_pause(StopReason::Entry);
                        }
                        vm = Some(v);
                        let mut c = conn.borrow_mut();
                        c.respond(&req, Json::Null);
                        c.event("initialized", Json::Null);
                    }
                    Err(e) => {
                        let mut c = conn.borrow_mut();
                        c.output("stderr", &format!("{}\n", e.trim_end()));
                        c.fail(&req, "the program could not be started: see the Debug Console");
                        c.event("terminated", Json::Null);
                        return ExitCode::from(1);
                    }
                }
            }
            "configurationDone" => {
                conn.borrow_mut().respond(&req, Json::Null);
                break;
            }
            "disconnect" | "terminate" => {
                conn.borrow_mut().respond(&req, Json::Null);
                return ExitCode::SUCCESS;
            }
            _ => match vm.as_mut() {
                Some(v) => {
                    session.request(v, &req);
                }
                None => conn.borrow_mut().fail(&req, "launch first"),
            },
        }
    }
    let Some(vm) = vm else {
        conn.borrow_mut().event("terminated", Json::Null);
        return ExitCode::from(1);
    };

    let result = neptune_runtime::run_vm(vm, Some(Box::new(session)));
    let code = match result {
        Ok(()) => 0u32,
        Err(e) => {
            conn.borrow_mut().output("stderr", &format!("{}\n", e.to_string().trim_end()));
            1
        }
    };
    {
        let mut c = conn.borrow_mut();
        c.event("exited", obj([("exitCode", n(code))]));
        c.event("terminated", Json::Null);
    }
    // answer the editor's goodbye if it sends one
    loop {
        let next = conn.borrow().rx.recv_timeout(Duration::from_secs(2));
        match next {
            Ok(req) => {
                let goodbye = matches!(req.get("command").str(), Some("disconnect" | "terminate"));
                conn.borrow_mut().reply(&req, goodbye, None, Json::Null);
                if goodbye {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    ExitCode::from(code as u8)
}
