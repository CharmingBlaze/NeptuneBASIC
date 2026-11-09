//! Driving `neptune dap`: breakpoints, stepping, the call stack and variables.

use std::path::Path;

use crate::app::{App, DockTab, Frame, Pending, Var, Watch};
use crate::buffer::Pos;
use crate::json::{n, obj, s, Json};
use crate::procs;

const THREAD: u32 = 1;

impl App {
    fn dap_request(&mut self, command: &str, args: Json, what: Pending) {
        if let Some(d) = &mut self.debug.session {
            let seq = d.request(command, args);
            self.debug.pending.insert(seq, what);
        }
    }

    pub fn begin_debug(&mut self, program: &Path) {
        let cwd = program.parent().map_or(String::new(), |p| p.display().to_string());
        self.dap_request("initialize", obj([("adapterID", s("neptune"))]), Pending::Other);
        let args = Json::Arr(procs::split_args(&self.command_line).into_iter().map(s).collect());
        let mut launch = obj([("program", s(program.display().to_string())), ("cwd", s(cwd)), ("args", args)]);
        if !self.st.quality.is_empty() {
            if let Json::Obj(m) = &mut launch {
                m.insert("quality".into(), s(self.st.quality.clone()));
            }
        }
        self.dap_request("launch", launch, Pending::Launch);
    }

    /// Tells a running debugger about every open file's breakpoints.
    pub fn send_breakpoints(&mut self) {
        if self.debug.session.is_none() {
            return;
        }
        let all: Vec<(String, Vec<usize>)> = self.docs.iter().filter_map(|d| d.path.as_ref().map(|p| (p.display().to_string(), d.breakpoints.iter().copied().collect()))).collect();
        for (path, lines) in all {
            let list = Json::Arr(lines.iter().map(|l| obj([("line", n(*l as u32 + 1))])).collect());
            self.dap_request("setBreakpoints", obj([("source", obj([("path", s(path))])), ("breakpoints", list)]), Pending::Other);
        }
    }

    pub fn dap_message(&mut self, j: Json) {
        match j.get("type").str() {
            Some("event") => self.dap_event(&j),
            Some("response") => self.dap_response(&j),
            _ => {}
        }
    }

    fn dap_event(&mut self, j: &Json) {
        let body = j.get("body");
        match j.get("event").str().unwrap_or("") {
            "initialized" => {
                self.send_breakpoints();
                self.dap_request("configurationDone", Json::Null, Pending::Other);
            }
            "stopped" => {
                self.debug.paused = true;
                self.debug.reason = body.get("reason").str().unwrap_or("").to_string();
                self.debug.exception = if self.debug.reason == "exception" { body.get("text").str().map(str::to_string) } else { None };
                self.debug.vars.clear();
                self.debug.hover.clear();
                self.debug.frame = 0;
                self.dock = DockTab::Debug;
                self.show_dock = true;
                self.dap_request("stackTrace", obj([("threadId", n(THREAD))]), Pending::Stack);
            }
            "output" => {
                let error = body.get("category").str() == Some("stderr");
                let text = body.get("output").str().unwrap_or("").to_string();
                self.push_output(&text, error);
            }
            "terminated" => {
                self.debug_ended();
            }
            _ => {}
        }
    }

    fn dap_response(&mut self, j: &Json) {
        let seq = j.get("request_seq").num().unwrap_or(0.0) as u32;
        let Some(what) = self.debug.pending.remove(&seq) else { return };
        let ok = j.get("success") == &Json::Bool(true);
        let body = j.get("body");
        match what {
            Pending::Launch => {
                if !ok {
                    let msg = j.get("message").str().unwrap_or("").to_string();
                    self.say(msg, true);
                }
            }
            Pending::Stack if ok => {
                self.debug.frames = body
                    .get("stackFrames")
                    .arr()
                    .iter()
                    .map(|f| Frame {
                        name: f.get("name").str().unwrap_or("").to_string(),
                        path: f.get("source").get("path").str().unwrap_or("").to_string(),
                        line: f.get("line").num().unwrap_or(1.0) as usize,
                        col: f.get("column").num().unwrap_or(1.0) as usize,
                    })
                    .collect();
                self.select_frame(0);
            }
            Pending::Scopes if ok => {
                self.debug.scopes = body.get("scopes").arr().iter().map(|x| (x.get("name").str().unwrap_or("").to_string(), x.get("variablesReference").num().unwrap_or(0.0) as u32)).collect();
                for r in self.debug.scopes.iter().map(|x| x.1).collect::<Vec<_>>() {
                    self.request_vars(r);
                }
            }
            Pending::Vars(r) if ok => {
                let list = body
                    .get("variables")
                    .arr()
                    .iter()
                    .map(|v| Var { name: v.get("name").str().unwrap_or("").to_string(), value: v.get("value").str().unwrap_or("").to_string(), reference: v.get("variablesReference").num().unwrap_or(0.0) as u32 })
                    .collect();
                self.debug.vars.insert(r, list);
            }
            Pending::Watch(i) => {
                if let Some(w) = self.debug.watches.get_mut(i) {
                    w.failed = !ok;
                    if ok {
                        w.value = body.get("result").str().unwrap_or("").to_string();
                        w.reference = body.get("variablesReference").num().unwrap_or(0.0) as u32;
                    } else {
                        w.value = j.get("message").str().unwrap_or("not available").to_string();
                        w.reference = 0;
                    }
                }
            }
            Pending::Hover(word) if ok => {
                let v = body.get("result").str().unwrap_or("").to_string();
                self.debug.hover.insert(word, v);
            }
            Pending::Hover(word) => {
                // an unknown name is not worth asking about again
                self.debug.hover.insert(word, "not available".into());
            }
            _ => {}
        }
    }

    pub fn request_vars(&mut self, reference: u32) {
        if reference != 0 && !self.debug.vars.contains_key(&reference) {
            self.dap_request("variables", obj([("variablesReference", n(reference))]), Pending::Vars(reference));
        }
    }

    pub fn select_frame(&mut self, i: usize) {
        self.debug.frame = i;
        let Some(f) = self.debug.frames.get(i) else { return };
        let (path, line, col) = (f.path.clone(), f.line, f.col);
        if !path.is_empty() {
            self.goto_file(Path::new(&path), line.saturating_sub(1), col.saturating_sub(1));
        }
        self.dap_request("scopes", obj([("frameId", n(i as u32 + 1))]), Pending::Scopes);
        self.refresh_watches();
    }

    pub fn refresh_watches(&mut self) {
        if !self.debug.paused {
            return;
        }
        let frame = self.debug.frame as u32 + 1;
        for (i, expr) in self.debug.watches.iter().map(|w| w.expr.clone()).enumerate().collect::<Vec<_>>() {
            self.dap_request("evaluate", obj([("expression", s(expr)), ("frameId", n(frame)), ("context", s("watch"))]), Pending::Watch(i));
        }
    }

    pub fn add_watch(&mut self, expr: String) {
        self.debug.watches.push(Watch { expr, value: String::new(), reference: 0, failed: false });
        self.refresh_watches();
    }

    pub fn debug_hover(&mut self, word: String) {
        if !self.debug.paused || self.debug.hover.contains_key(&word) || self.debug.pending.values().any(|p| matches!(p, Pending::Hover(w) if *w == word)) {
            return;
        }
        let frame = self.debug.frame as u32 + 1;
        self.dap_request("evaluate", obj([("expression", s(word.clone())), ("frameId", n(frame)), ("context", s("hover"))]), Pending::Hover(word));
    }

    fn resume(&mut self) {
        self.debug.paused = false;
        self.debug.exception = None;
        self.debug.frames.clear();
        self.debug.scopes.clear();
    }

    pub fn debug_continue(&mut self) {
        if self.debug.paused {
            self.dap_request("continue", obj([("threadId", n(THREAD))]), Pending::Other);
            self.resume();
        }
    }

    pub fn debug_step(&mut self, command: &str) {
        if self.debug.paused {
            self.dap_request(command, obj([("threadId", n(THREAD))]), Pending::Other);
            self.resume();
        }
    }

    pub fn debug_request_simple(&mut self, command: &str) {
        if self.debug.session.is_some() && !self.debug.paused {
            self.dap_request(command, obj([("threadId", n(THREAD))]), Pending::Other);
        }
    }

    pub fn debug_ended(&mut self) {
        if self.debug.session.is_none() {
            return;
        }
        self.debug.session = None;
        self.resume();
        self.debug.pending.clear();
        self.debug.vars.clear();
        self.out_open = false;
        let secs = self.run_started.elapsed().as_secs_f32();
        let verb = if std::mem::take(&mut self.stop_asked) { "stopped" } else { "finished" };
        self.push_output(&format!("\nProgram {verb} after {secs:.1}s\n"), false);
        self.say(format!("Program {verb} after {secs:.1}s"), false);
        if self.dock == DockTab::Debug {
            self.dock = DockTab::Output;
        }
    }

    /// The line the program is stopped at in a document, if any.
    pub fn exec_line(&self, doc: u64) -> Option<usize> {
        if !self.debug.paused {
            return None;
        }
        let f = self.debug.frames.get(self.debug.frame)?;
        let d = self.doc(doc)?;
        let p = d.path.as_ref()?;
        let same = p.canonicalize().ok().zip(Path::new(&f.path).canonicalize().ok()).is_some_and(|(a, b)| a == b);
        same.then_some(f.line.saturating_sub(1))
    }

    pub fn goto_pos_in_active(&mut self, line: usize, col: usize) {
        if let Some(id) = self.active_id() {
            self.goto(id, Pos::new(line, col), None);
        }
    }
}
