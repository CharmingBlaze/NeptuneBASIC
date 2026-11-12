//! Child processes: the language server, the debugger and running programs.
//! All pipes are served by threads that post messages, so a busy compiler or
//! a hung game can never stall the editor.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Sender};

use eframe::egui;

use crate::json::{n, obj, parse, s, Json};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunKind {
    Run,
    Check,
    Build,
}

pub enum Msg {
    Output { text: String, error: bool },
    Finished { run: u64, kind: RunKind },
    Lsp(Json),
    LspStopped,
    DapConnected(u64, TcpStream),
    Dap(u64, Json),
    DapStopped(u64),
    FoundFiles(Vec<PathBuf>),
    SearchHit { path: PathBuf, line: usize, col: usize, len: usize, text: String },
    SearchDone { generation: u64 },
}

#[derive(Clone)]
pub struct Poster {
    pub tx: Sender<Msg>,
    pub ctx: egui::Context,
}

impl Poster {
    pub fn post(&self, m: Msg) {
        let _ = self.tx.send(m);
        self.ctx.request_repaint();
    }
}

/// A console window must not flash up for each tool the IDE starts.
fn quiet(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Finds the `neptune` program: the configured one, next to the IDE, or on the PATH.
pub fn find_neptune(configured: &str) -> PathBuf {
    if !configured.trim().is_empty() {
        return PathBuf::from(configured.trim());
    }
    beside_ide("neptune")
}

/// The Blender add-on (`tools/blender`), built into the IDE so nobody has to
/// install it: Blender runs it with its own Python when the IDE starts Blender.
const BLENDER_ADDON: &str = include_str!("../../../tools/blender/neptune_levels.py");

/// Finds Blender: the configured program, on the PATH, or where its installer
/// puts it (the newest version on Windows).
pub fn find_blender(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        return Some(PathBuf::from(configured));
    }
    let name = format!("blender{}", std::env::consts::EXE_SUFFIX);
    if let Some(found) = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(&name)).find(|f| f.is_file())) {
        return Some(found);
    }
    let mut places: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for root in ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os) {
            let foundation = PathBuf::from(root).join("Blender Foundation");
            let mut versions: Vec<PathBuf> = std::fs::read_dir(&foundation).into_iter().flatten().flatten().map(|e| e.path()).collect();
            versions.sort_by_key(|p| version_key(p));
            places.extend(versions.into_iter().rev().map(|v| v.join("blender.exe")));
        }
    } else if cfg!(target_os = "macos") {
        places.push("/Applications/Blender.app/Contents/MacOS/Blender".into());
        if let Some(home) = std::env::var_os("HOME") {
            places.push(PathBuf::from(home).join("Applications/Blender.app/Contents/MacOS/Blender"));
        }
    } else {
        places.extend(["/usr/bin/blender", "/usr/local/bin/blender", "/snap/bin/blender"].map(PathBuf::from));
    }
    places.into_iter().find(|p| p.is_file())
}

/// "Blender 4.10" sorts after "Blender 4.2": the numbers in the folder name.
fn version_key(path: &Path) -> Vec<u32> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

/// The `.blend` file changed most recently in the game folder or the folders
/// just inside it (such as `levels`).
pub fn newest_blend(folder: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 2 {
                dirs.push((path, depth + 1));
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("blend")) {
                let changed = entry.metadata().and_then(|m| m.modified()).ok();
                found.push((changed, path));
            }
        }
    }
    found.into_iter().max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1))).map(|(_, p)| p)
}

/// Starts Blender on `blend` (or a new file) in the game folder, with the
/// Neptune tab loaded. Blender goes on running when the IDE closes.
pub fn open_blender(exe: &Path, folder: &Path, blend: Option<&Path>) -> Result<(), String> {
    let addon = std::env::temp_dir().join("neptune_levels.py");
    std::fs::write(&addon, BLENDER_ADDON).map_err(|e| format!("Cannot write {}: {e}", addon.display()))?;
    let mut cmd = Command::new(exe);
    if let Some(blend) = blend {
        cmd.arg(blend);
    }
    cmd.arg("--python").arg(&addon);
    cmd.current_dir(folder).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    quiet(&mut cmd);
    cmd.spawn().map(drop).map_err(|e| format!("Cannot start {}: {e}", exe.display()))
}

/// `program` next to the IDE's own executable, or just its name (found on the PATH).
fn beside_ide(program: &str) -> PathBuf {
    let name = format!("{program}{}", std::env::consts::EXE_SUFFIX);
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        let next = dir.join(&name);
        if next.exists() {
            return next;
        }
    }
    PathBuf::from(name)
}

fn pump(mut from: impl Read + Send + 'static, error: bool, post: Poster) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut hold: Vec<u8> = Vec::new();
        while let Ok(k) = from.read(&mut buf) {
            if k == 0 {
                break;
            }
            hold.extend_from_slice(&buf[..k]);
            // never split a character between two reads
            let good = match std::str::from_utf8(&hold) {
                Ok(_) => hold.len(),
                Err(e) if e.error_len().is_none() => e.valid_up_to(),
                Err(_) => hold.len(),
            };
            if good > 0 {
                let text = String::from_utf8_lossy(&hold[..good]).into_owned();
                hold.drain(..good);
                post.post(Msg::Output { text, error });
            }
        }
    })
}

/// A program the IDE started and can stop.
pub struct Running {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
}

impl Running {
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    pub fn send_line(&mut self, line: &str) {
        if let Some(w) = &mut self.stdin {
            let _ = writeln!(w, "{line}");
            let _ = w.flush();
        }
    }
}

pub fn start(exe: &Path, args: &[String], cwd: Option<&Path>, kind: RunKind, run: u64, post: Poster) -> Result<Running, String> {
    let mut cmd = Command::new(exe);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    quiet(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("Cannot start {}: {e}", exe.display()))?;
    let out = pump(child.stdout.take().unwrap(), false, post.clone());
    let err = pump(child.stderr.take().unwrap(), true, post.clone());
    let stdin = child.stdin.take();
    std::thread::spawn(move || {
        let _ = out.join();
        let _ = err.join();
        post.post(Msg::Finished { run, kind });
    });
    Ok(Running { child, stdin })
}

fn read_frame(input: &mut impl BufRead) -> Option<Json> {
    let mut len = 0usize;
    loop {
        let mut header = String::new();
        if input.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().ok()?;
            }
        }
    }
    let mut body = vec![0u8; len];
    input.read_exact(&mut body).ok()?;
    parse(&String::from_utf8_lossy(&body)).ok()
}

fn frame(msg: &Json) -> Vec<u8> {
    let body = msg.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// A pipe writer on its own thread: the UI only ever queues bytes.
fn writer(mut to: impl Write + Send + 'static) -> Sender<Vec<u8>> {
    let (tx, rx) = channel::<Vec<u8>>();
    std::thread::spawn(move || {
        while let Ok(bytes) = rx.recv() {
            if to.write_all(&bytes).is_err() || to.flush().is_err() {
                break;
            }
        }
    });
    tx
}

pub fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    let p = path.replace('\\', "/");
    if !p.starts_with('/') {
        out.push('/');
    }
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The client side of `neptune lsp`.
pub struct Lsp {
    child: Child,
    tx: Sender<Vec<u8>>,
    next: u32,
}

impl Lsp {
    pub fn start(exe: &Path, post: Poster) -> Result<Lsp, String> {
        let mut cmd = Command::new(exe);
        cmd.arg("lsp").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        quiet(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("Cannot start {}: {e}", exe.display()))?;
        let tx = writer(child.stdin.take().unwrap());
        let out = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            let mut input = BufReader::new(out);
            while let Some(m) = read_frame(&mut input) {
                post.post(Msg::Lsp(m));
            }
            post.post(Msg::LspStopped);
        });
        let mut lsp = Lsp { child, tx, next: 1 };
        lsp.request("initialize", obj([("capabilities", obj::<0>([]))]));
        // the full list of commands and constants is fetched once
        lsp.request(
            "textDocument/completion",
            obj([("textDocument", obj([("uri", s("file:///neptune-ide-startup.nb"))])), ("position", obj([("line", n(0)), ("character", n(0))]))]),
        );
        Ok(lsp)
    }

    pub fn request(&mut self, method: &str, params: Json) -> u32 {
        let id = self.next;
        self.next += 1;
        let _ = self.tx.send(frame(&obj([("jsonrpc", s("2.0")), ("id", n(id)), ("method", s(method)), ("params", params)])));
        id
    }

    pub fn notify(&self, method: &str, params: Json) {
        let _ = self.tx.send(frame(&obj([("jsonrpc", s("2.0")), ("method", s(method)), ("params", params)])));
    }

    pub fn did_open(&self, uri: &str, text: String) {
        self.notify("textDocument/didOpen", obj([("textDocument", obj([("uri", s(uri)), ("languageId", s("neptunebasic")), ("version", n(1)), ("text", s(text))]))]));
    }

    pub fn did_change(&self, uri: &str, text: String) {
        self.notify("textDocument/didChange", obj([("textDocument", obj([("uri", s(uri))])), ("contentChanges", Json::Arr(vec![obj([("text", s(text))])]))]));
    }

    pub fn did_close(&self, uri: &str) {
        self.notify("textDocument/didClose", obj([("textDocument", obj([("uri", s(uri))]))]));
    }
}

impl Drop for Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// The client side of `neptune dap`.
pub struct Dap {
    child: Child,
    tx: Option<Sender<Vec<u8>>>,
    seq: u32,
    queued: Vec<Vec<u8>>,
}

impl Dap {
    pub fn start(exe: &Path, cwd: Option<&Path>, run: u64, post: Poster) -> Result<Dap, String> {
        let mut cmd = Command::new(exe);
        cmd.arg("dap").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        quiet(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("Cannot start {}: {e}", exe.display()))?;
        pump(child.stderr.take().unwrap(), true, post.clone());
        let out = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            let mut input = BufReader::new(out);
            // the first line names the port
            let mut first = String::new();
            let port = input.read_line(&mut first).ok().and_then(|_| first.trim().parse::<u16>().ok());
            if let Some(port) = port {
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(stream) => {
                        let _ = stream.set_nodelay(true);
                        if let Ok(reader) = stream.try_clone() {
                            post.post(Msg::DapConnected(run, stream));
                            let reader_post = post.clone();
                            std::thread::spawn(move || {
                                let mut input = BufReader::new(reader);
                                while let Some(m) = read_frame(&mut input) {
                                    reader_post.post(Msg::Dap(run, m));
                                }
                                reader_post.post(Msg::DapStopped(run));
                            });
                        }
                    }
                    Err(e) => post.post(Msg::Output { text: format!("Cannot reach the debugger: {e}\n"), error: true }),
                }
            } else {
                post.post(Msg::Output { text: "The debugger did not start.\n".into(), error: true });
                post.post(Msg::DapStopped(run));
            }
            // from here on it is the running program's own output
            pump(input.into_inner(), false, post);
        });
        Ok(Dap { child, tx: None, seq: 0, queued: Vec::new() })
    }

    pub fn connected(&mut self, stream: TcpStream) {
        let tx = writer(stream);
        for bytes in self.queued.drain(..) {
            let _ = tx.send(bytes);
        }
        self.tx = Some(tx);
    }

    /// Sends a request; the reply carries the returned number.
    pub fn request(&mut self, command: &str, args: Json) -> u32 {
        self.seq += 1;
        let bytes = frame(&obj([("seq", n(self.seq)), ("type", s("request")), ("command", s(command)), ("arguments", args)]));
        match &self.tx {
            Some(tx) => {
                let _ = tx.send(bytes);
            }
            None => self.queued.push(bytes),
        }
        self.seq
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for Dap {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Splits a program command line into words, honouring double quotes.
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "dist")
}

pub fn is_source(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("nb") || e.eq_ignore_ascii_case("bb") || e.eq_ignore_ascii_case("bbi"))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 24 || out.len() > 50_000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                if !skip_dir(&name) {
                    walk(&p, out, depth + 1);
                }
            }
            Ok(t) if t.is_file() => out.push(p),
            _ => {}
        }
    }
}

/// Lists a project's files in the background.
pub fn index_files(root: PathBuf, post: Poster) {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        walk(&root, &mut out, 0);
        out.sort();
        post.post(Msg::FoundFiles(out));
    });
}

pub struct SearchOptions {
    pub needle: String,
    pub case: bool,
    pub word: bool,
}

/// Where `needle` occurs in `line`, as (char col, char len).
pub fn find_in_line(line: &str, o: &SearchOptions) -> Vec<(usize, usize)> {
    let hay: Vec<char> = line.chars().collect();
    let need: Vec<char> = o.needle.chars().collect();
    let mut out = Vec::new();
    if need.is_empty() || need.len() > hay.len() {
        return out;
    }
    let same = |a: char, b: char| if o.case { a == b } else { a.to_lowercase().eq(b.to_lowercase()) };
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    while i + need.len() <= hay.len() {
        let hit = hay[i..i + need.len()].iter().zip(&need).all(|(a, b)| same(*a, *b));
        let edges = !o.word || ((i == 0 || !word(hay[i - 1])) && hay.get(i + need.len()).is_none_or(|c| !word(*c)));
        if hit && edges {
            out.push((i, need.len()));
            i += need.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Searches the project's source files in the background.
pub fn search_files(files: Vec<PathBuf>, o: SearchOptions, generation: u64, cancel: std::sync::Arc<std::sync::atomic::AtomicU64>, post: Poster) {
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || {
        let mut hits = 0;
        'files: for path in files.into_iter().filter(|p| is_source(p)) {
            if cancel.load(Ordering::Relaxed) != generation {
                return;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let (text, _) = crate::document::decode(&bytes);
            for (i, line) in text.lines().enumerate() {
                for (col, len) in find_in_line(line, &o) {
                    post.post(Msg::SearchHit { path: path.clone(), line: i, col, len, text: line.trim_end().to_string() });
                    hits += 1;
                    if hits >= 5000 {
                        break 'files;
                    }
                }
            }
        }
        post.post(Msg::SearchDone { generation });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_command_lines() {
        assert_eq!(split_args(r#"-a "two words" b"#), vec!["-a", "two words", "b"]);
        assert_eq!(split_args(r#""""#), vec![""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn makes_uris() {
        assert_eq!(file_uri("C:\\My Games\\a.nb"), "file:///C:/My%20Games/a.nb");
        assert_eq!(file_uri("/home/me/a.nb"), "file:///home/me/a.nb");
    }

    #[test]
    fn finds_text_in_lines() {
        let o = SearchOptions { needle: "foo".into(), case: false, word: false };
        assert_eq!(find_in_line("Foo food", &o), vec![(0, 3), (4, 3)]);
        let o = SearchOptions { needle: "foo".into(), case: false, word: true };
        assert_eq!(find_in_line("Foo food", &o), vec![(0, 3)]);
        let o = SearchOptions { needle: "foo".into(), case: true, word: false };
        assert_eq!(find_in_line("Foo food", &o), vec![(4, 3)]);
    }

    #[test]
    fn reads_framed_messages() {
        let bytes = frame(&obj([("a", n(1))]));
        let mut input = std::io::Cursor::new(bytes);
        let m = read_frame(&mut input).unwrap();
        assert_eq!(m.get("a").num(), Some(1.0));
        assert!(read_frame(&mut input).is_none());
    }

    #[test]
    fn blender_versions_sort_by_number() {
        let mut v = [PathBuf::from("Blender 4.10"), PathBuf::from("Blender 3.6"), PathBuf::from("Blender 4.2")];
        v.sort_by_key(|p| version_key(p));
        assert_eq!(v.last().unwrap(), &PathBuf::from("Blender 4.10"));
    }

    #[test]
    fn finds_the_newest_blend_in_the_game_folder() {
        let root = std::env::temp_dir().join(format!("nb-ide-blend-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("levels")).unwrap();
        assert_eq!(newest_blend(&root), None);
        std::fs::write(root.join("old.blend"), "").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(root.join("levels").join("castle.BLEND"), "").unwrap();
        std::fs::write(root.join("main.nb"), "").unwrap();
        assert_eq!(newest_blend(&root), Some(root.join("levels").join("castle.BLEND")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_blender_addon_registers_when_run_as_a_script() {
        assert!(BLENDER_ADDON.contains("if __name__ == \"__main__\":\n    register()"));
    }
}
