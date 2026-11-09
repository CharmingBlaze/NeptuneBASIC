//! Drives `neptune dap` the way an editor does, over its TCP connection.

use neptune_runtime::json;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use json::{n, obj, parse, s, Json};

struct Editor {
    child: Child,
    stream: TcpStream,
    input: BufReader<TcpStream>,
    seq: u32,
    /// Events that arrived while waiting for something else.
    events: Vec<Json>,
    file: PathBuf,
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.file);
    }
}

impl Editor {
    /// Starts the adapter on a program saved under a name unique to the test.
    fn start(name: &str, src: &str) -> Editor {
        let file = std::env::temp_dir().join(format!("neptune_dap_{}_{name}.nb", std::process::id()));
        std::fs::write(&file, src).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_neptune"))
            .args(["dap", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut port = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut port).unwrap();
        let stream = TcpStream::connect(("127.0.0.1", port.trim().parse::<u16>().unwrap())).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        let input = BufReader::new(stream.try_clone().unwrap());
        Editor { child, stream, input, seq: 0, events: Vec::new(), file }
    }

    fn path(&self) -> String {
        self.file.to_string_lossy().into_owned()
    }

    fn receive(&mut self) -> Json {
        let mut length = 0;
        loop {
            let mut h = String::new();
            assert!(self.input.read_line(&mut h).unwrap() > 0, "adapter closed the connection");
            if h.trim().is_empty() {
                break;
            }
            length = h.trim().strip_prefix("Content-Length:").unwrap().trim().parse().unwrap();
        }
        let mut body = vec![0; length];
        self.input.read_exact(&mut body).unwrap();
        parse(std::str::from_utf8(&body).unwrap()).unwrap()
    }

    fn send(&mut self, command: &str, arguments: Json) -> u32 {
        self.seq += 1;
        let body = obj([("seq", n(self.seq)), ("type", s("request")), ("command", s(command)), ("arguments", arguments)]).to_string();
        self.stream.write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).unwrap();
        self.seq
    }

    /// Sends a request and returns its whole response.
    fn request(&mut self, command: &str, arguments: Json) -> Json {
        let seq = self.send(command, arguments);
        loop {
            let m = self.receive();
            if m.get("type").str() == Some("event") {
                self.events.push(m);
            } else if m.get("request_seq").num() == Some(seq as f64) {
                return m;
            }
        }
    }

    fn event(&mut self, name: &str) -> Json {
        if let Some(i) = self.events.iter().position(|e| e.get("event").str() == Some(name)) {
            return self.events.remove(i);
        }
        loop {
            let m = self.receive();
            if m.get("event").str() == Some(name) {
                return m;
            }
            if m.get("type").str() == Some("event") {
                self.events.push(m);
            }
        }
    }

    /// Launches and sets breakpoints, up to the moment the program starts.
    fn launch(&mut self, lines: &[u32], stop_on_entry: bool) -> Json {
        assert_eq!(self.request("initialize", obj([("adapterID", s("neptunebasic"))])).get("success"), &Json::Bool(true));
        let path = self.path();
        let r = self.request("launch", obj([("program", s(path.clone())), ("stopOnEntry", Json::Bool(stop_on_entry))]));
        assert_eq!(r.get("success"), &Json::Bool(true), "{r}");
        self.event("initialized");
        let bps = Json::Arr(lines.iter().map(|l| obj([("line", n(*l))])).collect());
        let set = self.request("setBreakpoints", obj([("source", obj([("path", s(path))])), ("breakpoints", bps)]));
        assert_eq!(self.request("configurationDone", Json::Null).get("success"), &Json::Bool(true));
        set
    }

    fn top_line(&mut self) -> u32 {
        let r = self.request("stackTrace", obj([("threadId", n(1))]));
        r.at("body.stackFrames").arr()[0].get("line").num().unwrap() as u32
    }

    fn eval(&mut self, expr: &str) -> String {
        let r = self.request("evaluate", obj([("expression", s(expr)), ("frameId", n(1))]));
        match r.at("body.result").str() {
            Some(t) => t.to_string(),
            None => r.get("message").str().unwrap_or("").to_string(),
        }
    }

    /// Finds a variable in a list returned by `variables`.
    fn variables(&mut self, reference: f64) -> Vec<Json> {
        let r = self.request("variables", obj([("variablesReference", Json::Num(reference))]));
        r.at("body.variables").arr().to_vec()
    }

    fn scope(&mut self, name: &str) -> f64 {
        let r = self.request("scopes", obj([("frameId", n(1))]));
        let list = r.at("body.scopes").arr().to_vec();
        list.iter().find(|x| x.get("name").str() == Some(name)).unwrap().get("variablesReference").num().unwrap()
    }
}

const PROGRAM: &str = "Type Ship
    Field name$
End Type
Global count = 0
Function Bump(by)
    count = count + by
    Return count
End Function
Local hero.Ship = New Ship
hero\\name = \"Neo\"
Dim scores(2)
scores(1) = 9
a = Bump(5)
Print a
";

#[test]
fn breakpoints_steps_variables_and_exit() {
    let mut e = Editor::start("full", PROGRAM);
    // line 13 is a statement, line 8 (End Function) moves to line 9
    let set = e.launch(&[13, 8], false);
    let lines: Vec<f64> = set.at("body.breakpoints").arr().iter().map(|b| b.get("line").num().unwrap()).collect();
    assert_eq!(lines, vec![13.0, 9.0]);

    let stop = e.event("stopped");
    assert_eq!(stop.at("body.reason").str(), Some("breakpoint"));
    assert_eq!(e.top_line(), 9);

    let r = e.request("continue", obj([("threadId", n(1))]));
    assert_eq!(r.get("success"), &Json::Bool(true));
    e.event("stopped");
    assert_eq!(e.top_line(), 13);

    // variables: a local object opens to its fields, arrays are listed
    let locals = e.scope("Locals");
    let rows = e.variables(locals);
    let hero = rows.iter().find(|v| v.get("name").str().is_some_and(|t| t.eq_ignore_ascii_case("hero"))).unwrap().clone();
    assert_eq!(hero.get("value").str().map(str::to_lowercase).as_deref(), Some("ship"));
    let fields = e.variables(hero.get("variablesReference").num().unwrap());
    assert_eq!(fields[0].get("value").str(), Some("\"Neo\""));

    let arrays = e.scope("Arrays");
    let rows = e.variables(arrays);
    assert_eq!(rows[0].get("value").str(), Some("Dim (2)"));
    let items = e.variables(rows[0].get("variablesReference").num().unwrap());
    assert_eq!(items[1].get("value").str(), Some("9"));

    assert_eq!(e.eval("hero\\name"), "\"Neo\"");
    assert_eq!(e.eval("scores(1)"), "9");
    assert_eq!(e.eval("nope"), "'nope' is not a variable here");

    // into the function, and the call stack shows both levels
    e.request("stepIn", obj([("threadId", n(1))]));
    assert_eq!(e.event("stopped").at("body.reason").str(), Some("step"));
    assert_eq!(e.top_line(), 6);
    assert_eq!(e.eval("by"), "5");
    let r = e.request("stackTrace", obj([("threadId", n(1))]));
    assert_eq!(r.at("body.totalFrames").num(), Some(2.0));
    let source = r.at("body.stackFrames").arr()[0].at("source.path").str().unwrap().to_string();
    assert!(!source.starts_with(r"\\?\"), "{source}");

    e.request("stepOut", obj([("threadId", n(1))]));
    e.event("stopped");
    assert_eq!(e.top_line(), 14);

    e.request("continue", obj([("threadId", n(1))]));
    assert_eq!(e.event("exited").at("body.exitCode").num(), Some(0.0));
    e.event("terminated");
    e.request("disconnect", Json::Null);
    assert!(e.child.wait().unwrap().success());
}

#[test]
fn pause_interrupts_a_running_loop_and_disconnect_ends_it() {
    let mut e = Editor::start("pause", "n = 0\nWhile 1\n    n = n + 1\nWend\n");
    e.launch(&[], false);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(e.request("pause", obj([("threadId", n(1))])).get("success"), &Json::Bool(true));
    assert_eq!(e.event("stopped").at("body.reason").str(), Some("pause"));
    assert!(e.eval("n").parse::<i32>().unwrap() > 0);
    e.request("disconnect", Json::Null);
    assert!(e.child.wait().unwrap().success());
}

#[test]
fn stop_on_entry_and_breakpoints_set_while_running() {
    let mut e = Editor::start("entry", "a = 1\nb = 2\nWhile 1\n    c = 3\nWend\n");
    e.launch(&[], true);
    assert_eq!(e.event("stopped").at("body.reason").str(), Some("entry"));
    assert_eq!(e.top_line(), 1);
    e.request("continue", obj([("threadId", n(1))]));
    // the program is in an endless loop now
    let path = e.path();
    let r = e.request("setBreakpoints", obj([("source", obj([("path", s(path))])), ("breakpoints", Json::Arr(vec![obj([("line", n(4))])]))]));
    assert_eq!(r.at("body.breakpoints").arr()[0].get("verified"), &Json::Bool(true));
    assert_eq!(e.event("stopped").at("body.reason").str(), Some("breakpoint"));
    assert_eq!(e.top_line(), 4);
}

#[test]
fn runtime_errors_stop_the_program_where_they_happen() {
    let mut e = Editor::start("error", "x = 0\nPrint 1 / x\n");
    e.launch(&[], false);
    let stop = e.event("stopped");
    assert_eq!(stop.at("body.reason").str(), Some("exception"));
    assert_eq!(stop.at("body.text").str(), Some("Integer divide by zero"));
    assert_eq!(e.top_line(), 2);
    assert_eq!(e.eval("x"), "0");
    e.request("continue", obj([("threadId", n(1))]));
    assert_eq!(e.event("exited").at("body.exitCode").num(), Some(1.0));
}

#[test]
fn compile_errors_are_reported_at_launch() {
    let mut e = Editor::start("bad", "Print (\n");
    e.request("initialize", Json::Null);
    let path = e.path();
    let r = e.request("launch", obj([("program", s(path))]));
    assert_eq!(r.get("success"), &Json::Bool(false));
    let out = e.event("output");
    assert!(out.at("body.output").str().unwrap().contains("error"));
    e.event("terminated");
}
