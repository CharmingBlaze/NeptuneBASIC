//! Engine-independent standard library: console, math, strings, files,
//! directories, banks, timers, networking, time and random numbers. Names and signatures follow Blitz3D's
//! `basic.cpp`, `bbmath.cpp`, `bbstring.cpp`, `bbfilesystem.cpp` and
//! `bbstream.cpp`; NeptuneBASIC additions are marked in [`COMMANDS`].

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, Write};
use std::rc::Rc;
use std::time::Instant;

use neptune_lang::commands::{parse_sig, ConstVal};
use neptune_lang::sema::Env;

use crate::netgame::Plan;
use crate::value::Value;
use crate::vm::NativeCtx;

/// Where `Print`/`Write` output goes.
pub enum Output {
    Stdout,
    /// Captured output (tests, embedding, IDE consoles).
    Buffer(String),
    /// Shared with a host, which draws it (the graphics runtime puts
    /// `Print` text on the screen like Blitz3D).
    Shared(Rc<std::cell::RefCell<String>>),
}

enum Stream {
    Read(BufReader<File>),
    Write(BufWriter<File>),
    ReadWrite(File),
    Tcp(crate::net::Tcp),
    Udp(crate::net::Udp),
}

/// A `CreateTimer` timer.
struct Timer {
    period: std::time::Duration,
    next: Instant,
}

/// State used by standard library commands.
pub struct Core {
    pub output: Output,
    start: Instant,
    rng: u64,
    files: HashMap<i32, Stream>,
    next_file: i32,
    seed: i32,
    banks: HashMap<i32, Vec<u8>>,
    timers: HashMap<i32, Timer>,
    dirs: HashMap<i32, std::collections::VecDeque<String>>,
    servers: HashMap<i32, crate::net::Server>,
    next_handle: i32,
    /// `TCPTimeouts` (read, accept) and `UDPTimeouts`, milliseconds.
    tcp_read_ms: u32,
    tcp_accept_ms: u32,
    udp_ms: u32,
    /// Results of the last `CountHostIPs`.
    host_ips: Vec<i32>,
    /// The multiplayer game, shared with the 3D runtime's entity sync.
    pub net: crate::netgame::SharedNet,
    /// Identifies this program, so `StartNetGame` only finds copies of itself.
    pub program_id: u64,
    /// Lines returned by `Input` when set (tests/embedding); otherwise stdin.
    pub input_lines: Option<std::collections::VecDeque<String>>,
    /// (ext) `SetSetting`, `SaveSettings`, ...
    settings: crate::settings::Settings,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    pub fn new() -> Core {
        Core {
            output: Output::Stdout,
            start: Instant::now(),
            rng: 0x2545_F491_4F6C_DD1D,
            files: HashMap::new(),
            next_file: 1,
            seed: 0,
            banks: HashMap::new(),
            timers: HashMap::new(),
            dirs: HashMap::new(),
            servers: HashMap::new(),
            next_handle: 1,
            tcp_read_ms: 10_000,
            tcp_accept_ms: 0,
            udp_ms: 0,
            host_ips: Vec::new(),
            net: Default::default(),
            program_id: 0,
            input_lines: None,
            settings: Default::default(),
        }
    }

    pub fn write_out(&mut self, s: &str) {
        match &mut self.output {
            Output::Stdout => {
                let mut o = std::io::stdout().lock();
                let _ = o.write_all(s.as_bytes());
                let _ = o.flush();
            }
            Output::Buffer(b) => b.push_str(s),
            Output::Shared(b) => b.borrow_mut().push_str(s),
        }
    }

    /// Takes captured output (when using [`Output::Buffer`]).
    pub fn take_output(&mut self) -> String {
        match &mut self.output {
            Output::Buffer(b) => std::mem::take(b),
            Output::Shared(b) => std::mem::take(&mut *b.borrow_mut()),
            Output::Stdout => String::new(),
        }
    }

    fn program_game_name(&self) -> String {
        format!("program-{:016x}", self.program_id)
    }

    fn next_rand(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn rnd01(&mut self) -> f32 {
        (self.next_rand() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn file(&mut self, h: i32) -> Result<&mut Stream, String> {
        self.files.get_mut(&h).ok_or_else(|| format!("stream {h} does not exist"))
    }
    fn handle(&mut self) -> i32 {
        let h = self.next_handle;
        self.next_handle += 1;
        h
    }
    fn bank(&mut self, b: i32) -> Result<&mut Vec<u8>, String> {
        self.banks.get_mut(&b).ok_or_else(|| format!("Bank {b} does not exist"))
    }
    fn add_stream(&mut self, s: Stream) -> i32 {
        let h = self.next_file;
        self.next_file += 1;
        self.files.insert(h, s);
        h
    }
}

type NativeFn = fn(&[Value], &mut Core, &mut NativeCtx) -> Result<Value, String>;

fn ok_int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}
fn ok_f(v: f32) -> Result<Value, String> {
    Ok(Value::Float(v))
}
fn ok_s(s: impl Into<Rc<str>>) -> Result<Value, String> {
    Ok(Value::Str(s.into()))
}
const VOID: Result<Value, String> = Ok(Value::Null);

fn deg(v: &Value) -> f32 {
    v.float().to_radians()
}

fn chars_left(s: &str, n: i32) -> String {
    s.chars().take(n.max(0) as usize).collect()
}

fn stream_read(s: &mut Stream, buf: &mut [u8]) -> std::io::Result<usize> {
    match s {
        Stream::Read(r) => r.read(buf),
        Stream::ReadWrite(f) => f.read(buf),
        Stream::Write(_) => Ok(0),
        Stream::Tcp(t) => t.read(buf),
        Stream::Udp(u) => Ok(u.read(buf)),
    }
}
fn stream_write(s: &mut Stream, buf: &[u8]) -> Result<(), String> {
    let r = match s {
        Stream::Write(w) => w.write_all(buf),
        Stream::ReadWrite(f) => f.write_all(buf),
        Stream::Read(_) => return Err("stream is not writable".into()),
        Stream::Tcp(t) => t.write(buf),
        Stream::Udp(u) => {
            u.wbuf.extend_from_slice(buf);
            Ok(())
        }
    };
    r.map_err(|e| e.to_string())
}
/// Bytes that can be read without waiting (`ReadAvail`).
fn stream_avail(s: &mut Stream) -> i32 {
    match s {
        Stream::Read(r) => {
            let buffered = r.buffer().len() as u64;
            let f = r.get_mut();
            let pos = f.stream_position().unwrap_or(0);
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            (len.saturating_sub(pos) + buffered).min(i32::MAX as u64) as i32
        }
        Stream::ReadWrite(f) => {
            let pos = f.stream_position().unwrap_or(0);
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            len.saturating_sub(pos).min(i32::MAX as u64) as i32
        }
        Stream::Write(_) => 0,
        Stream::Tcp(t) => t.available(),
        Stream::Udp(u) => u.available(),
    }
}
fn bank_range(bank: &[u8], offset: i32, len: usize) -> Result<std::ops::Range<usize>, String> {
    let o = usize::try_from(offset).map_err(|_| format!("bank offset {offset} is negative"))?;
    if o + len > bank.len() {
        return Err(format!("bank offset {offset} out of range (bank size {})", bank.len()));
    }
    Ok(o..o + len)
}
fn peek<const N: usize>(c: &mut Core, a: &[Value]) -> Result<[u8; N], String> {
    let b = c.bank(a[0].int())?;
    let r = bank_range(b, a[1].int(), N)?;
    Ok(b[r].try_into().expect("range has N bytes"))
}
fn poke(c: &mut Core, a: &[Value], bytes: &[u8]) -> Result<Value, String> {
    let b = c.bank(a[0].int())?;
    let r = bank_range(b, a[1].int(), bytes.len())?;
    b[r].copy_from_slice(bytes);
    VOID
}
/// The program's own command-line arguments, for `CommandLine`: set by
/// the CLI (the words after the source file, or after a packaged game's
/// name).
static PROGRAM_ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Sets what `CommandLine` returns. Only the first call has an effect.
pub fn set_program_args(args: Vec<String>) {
    let _ = PROGRAM_ARGS.set(args);
}

fn program_args() -> Vec<String> {
    PROGRAM_ARGS.get().cloned().unwrap_or_else(|| std::env::args().skip(1).collect())
}

/// `CallDLL`: calls `int proc(void* in, int in_size, void* out, int out_size)`
/// exported by a shared library (`.dll`, `.so` or `.dylib`), passing two
/// banks (Blitz3D's userlib-free interface). Plugins are the better way to
/// add commands; this stays for old programs.
fn call_dll(dll: &str, proc_name: &str, input: &mut [u8], output: &mut [u8]) -> Result<i32, String> {
    use std::ffi::c_void;
    use std::sync::{Mutex, OnceLock};
    // libraries stay loaded, like Blitz3D
    static LIBS: OnceLock<Mutex<Vec<crate::dynlib::Library>>> = OnceLock::new();
    let libs = LIBS.get_or_init(Default::default);
    let mut libs = libs.lock().unwrap();
    let idx = match libs.iter().position(|l| l.path == dll) {
        Some(i) => i,
        None => {
            libs.push(crate::dynlib::Library::open(dll)?);
            libs.len() - 1
        }
    };
    let f = libs[idx].symbol(proc_name).ok_or_else(|| format!("library '{dll}' has no procedure '{proc_name}'"))?;
    type Proc = unsafe extern "system" fn(*mut c_void, i32, *mut c_void, i32) -> i32;
    // SAFETY: the program asked for this procedure with Blitz3D's documented
    // signature; the bank pointers stay valid for the duration of the call.
    unsafe {
        let f: Proc = std::mem::transmute::<*mut c_void, Proc>(f);
        let (ip, il) = if input.is_empty() { (std::ptr::null_mut(), 0) } else { (input.as_mut_ptr().cast(), input.len() as i32) };
        let (op, ol) = if output.is_empty() { (std::ptr::null_mut(), 0) } else { (output.as_mut_ptr().cast(), output.len() as i32) };
        Ok(f(ip, il, op, ol))
    }
}

/// Opens a file or folder with the program the system associates with it.
fn exec_file(target: &str) -> bool {
    #[cfg(windows)]
    let r = std::process::Command::new("cmd").args(["/C", "start", "", target]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    r.is_ok()
}
fn read_n<const N: usize>(c: &mut Core, h: i32) -> Result<[u8; N], String> {
    let s = c.file(h)?;
    let mut b = [0u8; N];
    let mut got = 0;
    while got < N {
        match stream_read(s, &mut b[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(b)
}

/// The standard library command table: (Blitz signature, implementation).
/// Order is significant: it defines native ids.
pub static COMMANDS: &[(&str, NativeFn)] = &[
    // ---- console / system (basic.cpp, bbruntime.cpp)
    ("Print?value=\"\"", |a, c, _| {
        let s = format!("{}\n", a[0]);
        c.write_out(&s);
        VOID
    }),
    ("Write?value", |a, c, _| {
        let s = a[0].to_string();
        c.write_out(&s);
        VOID
    }),
    ("$Input$prompt=\"\"", |a, c, _| {
        let p = a[0].str();
        c.write_out(&p);
        let line = match &mut c.input_lines {
            Some(q) => q.pop_front().unwrap_or_default(),
            None => {
                let mut l = String::new();
                let _ = std::io::stdin().read_line(&mut l);
                l
            }
        };
        ok_s(line.trim_end_matches(['\r', '\n']))
    }),
    ("%MilliSecs", |_, c, _| ok_int(c.start.elapsed().as_millis() as i32)),
    ("Delay%millisecs", |a, _, _| {
        std::thread::sleep(std::time::Duration::from_millis(a[0].int().max(0) as u64));
        VOID
    }),
    ("RuntimeError$message", |a, _, _| Err(a[0].str().to_string())),
    ("DebugLog$message", |a, _, _| {
        eprintln!("{}", a[0]);
        VOID
    }),
    ("$CommandLine", |_, _, _| ok_s(program_args().join(" "))),
    ("$GetEnv$name", |a, _, _| ok_s(std::env::var(&*a[0].str()).unwrap_or_default())),
    ("$CurrentDate", |_, _, _| ok_s(civil_now().0)),
    ("$CurrentTime", |_, _, _| ok_s(civil_now().1)),
    // ---- random (bbmath.cpp)
    ("SeedRnd%seed", |a, c, _| {
        c.seed = a[0].int();
        c.rng = (a[0].int() as u32 as u64) ^ 0x9E37_79B9_7F4A_7C15;
        if c.rng == 0 {
            c.rng = 1;
        }
        VOID
    }),
    ("#Rnd#from#to=0", |a, c, _| {
        let (lo, hi) = if a.len() > 1 && a[1].float() != 0.0 { (a[0].float(), a[1].float()) } else { (0.0, a[0].float()) };
        ok_f(lo + c.rnd01() * (hi - lo))
    }),
    ("%Rand%from%to=1", |a, c, _| {
        let (mut lo, mut hi) = (a[0].int(), a[1].int());
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        let span = (hi as i64 - lo as i64 + 1) as u64;
        ok_int((lo as i64 + (c.next_rand() % span) as i64) as i32)
    }),
    // ---- math (bbmath.cpp) – angles in degrees like Blitz3D
    ("#Sqr#value", |a, _, _| ok_f(a[0].float().sqrt())),
    ("#Sin#degrees", |a, _, _| ok_f(deg(&a[0]).sin())),
    ("#Cos#degrees", |a, _, _| ok_f(deg(&a[0]).cos())),
    ("#Tan#degrees", |a, _, _| ok_f(deg(&a[0]).tan())),
    ("#ASin#value", |a, _, _| ok_f(a[0].float().asin().to_degrees())),
    ("#ACos#value", |a, _, _| ok_f(a[0].float().acos().to_degrees())),
    ("#ATan#value", |a, _, _| ok_f(a[0].float().atan().to_degrees())),
    ("#ATan2#y#x", |a, _, _| ok_f(a[0].float().atan2(a[1].float()).to_degrees())),
    ("#Exp#value", |a, _, _| ok_f(a[0].float().exp())),
    ("#Log#value", |a, _, _| ok_f(a[0].float().ln())),
    ("#Log10#value", |a, _, _| ok_f(a[0].float().log10())),
    ("#Floor#value", |a, _, _| ok_f(a[0].float().floor())),
    ("#Ceil#value", |a, _, _| ok_f(a[0].float().ceil())),
    // NeptuneBASIC additions
    ("#Min#a#b", |a, _, _| ok_f(a[0].float().min(a[1].float()))),
    ("#Max#a#b", |a, _, _| ok_f(a[0].float().max(a[1].float()))),
    ("#Clamp#value#low#high", |a, _, _| ok_f(a[0].float().max(a[1].float()).min(a[2].float()))),
    ("#Lerp#a#b#t", |a, _, _| {
        let (x, y, t) = (a[0].float(), a[1].float(), a[2].float());
        ok_f(x + (y - x) * t)
    }),
    // ---- strings (bbstring.cpp)
    ("$Left$string%count", |a, _, _| ok_s(chars_left(&a[0].str(), a[1].int()))),
    ("$Right$string%count", |a, _, _| {
        let s = a[0].str();
        let n = s.chars().count();
        let k = (a[1].int().max(0) as usize).min(n);
        ok_s(s.chars().skip(n - k).collect::<String>())
    }),
    ("$Mid$string%offset%count=-1", |a, _, _| {
        let s = a[0].str();
        let off = (a[1].int().max(1) - 1) as usize;
        let it = s.chars().skip(off);
        let r: String = if a[2].int() < 0 { it.collect() } else { it.take(a[2].int() as usize).collect() };
        ok_s(r)
    }),
    ("%Len$string", |a, _, _| ok_int(a[0].str().chars().count() as i32)),
    ("$Upper$string", |a, _, _| ok_s(a[0].str().to_uppercase())),
    ("$Lower$string", |a, _, _| ok_s(a[0].str().to_lowercase())),
    ("$Trim$string", |a, _, _| ok_s(a[0].str().trim())),
    ("$LSet$string%size", |a, _, _| {
        let s = a[0].str();
        let n = a[1].int().max(0) as usize;
        let mut r: String = s.chars().take(n).collect();
        while r.chars().count() < n {
            r.push(' ');
        }
        ok_s(r)
    }),
    ("$RSet$string%size", |a, _, _| {
        let s = a[0].str();
        let n = a[1].int().max(0) as usize;
        let len = s.chars().count();
        let r: String = if len >= n { s.chars().skip(len - n).collect() } else { " ".repeat(n - len) + &s };
        ok_s(r)
    }),
    ("%Instr$string$find%from=1", |a, _, _| {
        let s = a[0].str();
        let f = a[1].str();
        let from = (a[2].int().max(1) - 1) as usize;
        let byte_from = s.char_indices().nth(from).map(|(i, _)| i).unwrap_or(s.len());
        ok_int(match s[byte_from..].find(&*f) {
            Some(p) => (s[..byte_from + p].chars().count() + 1) as i32,
            None => 0,
        })
    }),
    ("$Replace$string$from$to", |a, _, _| {
        let f = a[1].str();
        if f.is_empty() {
            return Ok(a[0].clone());
        }
        ok_s(a[0].str().replace(&*f, &a[2].str()))
    }),
    ("$String$string%count", |a, _, _| ok_s(a[0].str().repeat(a[1].int().max(0) as usize))),
    ("$Chr%code", |a, _, _| ok_s(char::from_u32(a[0].int() as u32).map(String::from).unwrap_or_default())),
    ("%Asc$string", |a, _, _| ok_int(a[0].str().chars().next().map_or(-1, |c| c as i32))),
    ("$Hex%value", |a, _, _| ok_s(format!("{:08X}", a[0].int() as u32))),
    ("$Bin%value", |a, _, _| ok_s(format!("{:032b}", a[0].int() as u32))),
    // NeptuneBASIC additions
    ("$TypeOf?value", |a, _, _| ok_s(a[0].type_name())),
    ("$FormatFloat#value%decimals", |a, _, _| {
        ok_s(format!("{:.*}", a[1].int().clamp(0, 9) as usize, a[0].float()))
    }),
    // ---- file system (bbfilesystem.cpp)
    ("%FileType$path", |a, _, _| {
        let p = a[0].str();
        ok_int(match std::fs::metadata(&*p) {
            Ok(m) if m.is_dir() => 2,
            Ok(_) => 1,
            Err(_) => 0,
        })
    }),
    ("%FileSize$path", |a, _, _| ok_int(std::fs::metadata(&*a[0].str()).map(|m| m.len() as i32).unwrap_or(0))),
    ("DeleteFile$path", |a, _, _| {
        let _ = std::fs::remove_file(&*a[0].str());
        VOID
    }),
    ("CopyFile$from$to", |a, _, _| {
        let _ = std::fs::copy(&*a[0].str(), &*a[1].str());
        VOID
    }),
    ("CreateDir$path", |a, _, _| {
        let _ = std::fs::create_dir_all(&*a[0].str());
        VOID
    }),
    ("DeleteDir$path", |a, _, _| {
        let _ = std::fs::remove_dir(&*a[0].str());
        VOID
    }),
    ("$CurrentDir", |_, _, _| {
        let mut d = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        if !d.ends_with(std::path::MAIN_SEPARATOR) {
            d.push(std::path::MAIN_SEPARATOR);
        }
        ok_s(d)
    }),
    ("ChangeDir$path", |a, _, _| {
        let _ = std::env::set_current_dir(&*a[0].str());
        VOID
    }),
    // ---- streams (bbstream.cpp / bbfilesystem.cpp)
    ("%ReadFile$path", |a, c, _| {
        ok_int(match File::open(&*a[0].str()) {
            Ok(f) => c.add_stream(Stream::Read(BufReader::new(f))),
            Err(_) => 0,
        })
    }),
    ("%WriteFile$path", |a, c, _| {
        ok_int(match File::create(&*a[0].str()) {
            Ok(f) => c.add_stream(Stream::Write(BufWriter::new(f))),
            Err(_) => 0,
        })
    }),
    ("%OpenFile$path", |a, c, _| {
        ok_int(match File::options().read(true).write(true).open(&*a[0].str()) {
            Ok(f) => c.add_stream(Stream::ReadWrite(f)),
            Err(_) => 0,
        })
    }),
    ("CloseFile%stream", |a, c, _| {
        if let Some(Stream::Write(mut w)) = c.files.remove(&a[0].int()) {
            let _ = w.flush();
        }
        VOID
    }),
    ("%Eof%stream", |a, c, _| {
        let s = c.file(a[0].int())?;
        ok_int(match s {
            Stream::Read(r) => r.fill_buf().map(|b| b.is_empty()).unwrap_or(true) as i32,
            Stream::ReadWrite(f) => {
                let pos = f.stream_position().unwrap_or(0);
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                (pos >= len) as i32
            }
            Stream::Write(_) => 1,
            Stream::Tcp(t) => t.eof(),
            Stream::Udp(u) => (u.available() == 0) as i32,
        })
    }),
    ("$ReadLine%stream", |a, c, _| {
        let s = c.file(a[0].int())?;
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        loop {
            match stream_read(s, &mut b) {
                Ok(1) if b[0] == b'\n' => break,
                Ok(1) => line.push(b[0]),
                _ => break,
            }
        }
        let text = String::from_utf8_lossy(&line);
        ok_s(text.trim_end_matches('\r'))
    }),
    ("WriteLine%stream$text", |a, c, _| {
        let t = format!("{}\r\n", a[1].str());
        stream_write(c.file(a[0].int())?, t.as_bytes())?;
        VOID
    }),
    ("%ReadByte%stream", |a, c, _| ok_int(read_n::<1>(c, a[0].int())?[0] as i32)),
    ("%ReadShort%stream", |a, c, _| ok_int(u16::from_le_bytes(read_n::<2>(c, a[0].int())?) as i32)),
    ("%ReadInt%stream", |a, c, _| ok_int(i32::from_le_bytes(read_n::<4>(c, a[0].int())?))),
    ("#ReadFloat%stream", |a, c, _| ok_f(f32::from_le_bytes(read_n::<4>(c, a[0].int())?))),
    ("$ReadString%stream", |a, c, _| {
        let n = i32::from_le_bytes(read_n::<4>(c, a[0].int())?).max(0) as usize;
        let s = c.file(a[0].int())?;
        let mut buf = vec![0u8; n.min(1 << 26)];
        let mut got = 0;
        while got < buf.len() {
            match stream_read(s, &mut buf[got..]) {
                Ok(0) | Err(_) => break,
                Ok(k) => got += k,
            }
        }
        buf.truncate(got);
        ok_s(String::from_utf8_lossy(&buf).as_ref())
    }),
    ("WriteByte%stream%value", |a, c, _| {
        stream_write(c.file(a[0].int())?, &[a[1].int() as u8])?;
        VOID
    }),
    ("WriteShort%stream%value", |a, c, _| {
        stream_write(c.file(a[0].int())?, &(a[1].int() as u16).to_le_bytes())?;
        VOID
    }),
    ("WriteInt%stream%value", |a, c, _| {
        stream_write(c.file(a[0].int())?, &a[1].int().to_le_bytes())?;
        VOID
    }),
    ("WriteFloat%stream#value", |a, c, _| {
        stream_write(c.file(a[0].int())?, &a[1].float().to_le_bytes())?;
        VOID
    }),
    ("WriteString%stream$value", |a, c, _| {
        let s = a[1].str();
        let st = c.file(a[0].int())?;
        stream_write(st, &(s.len() as i32).to_le_bytes())?;
        stream_write(st, s.as_bytes())?;
        VOID
    }),
    // ---- more streams and files
    ("%ReadAvail%stream", |a, c, _| ok_int(stream_avail(c.file(a[0].int())?))),
    ("%FilePos%stream", |a, c, _| {
        let pos = match c.file(a[0].int())? {
            Stream::Read(r) => r.stream_position().unwrap_or(0),
            Stream::Write(w) => w.stream_position().unwrap_or(0),
            Stream::ReadWrite(f) => f.stream_position().unwrap_or(0),
            _ => return Err("FilePos needs a file stream".into()),
        };
        ok_int(pos as i32)
    }),
    ("%SeekFile%stream%pos", |a, c, _| {
        let to = std::io::SeekFrom::Start(a[1].int().max(0) as u64);
        let pos = match c.file(a[0].int())? {
            Stream::Read(r) => r.seek(to),
            Stream::Write(w) => w.seek(to),
            Stream::ReadWrite(f) => f.seek(to),
            _ => return Err("SeekFile needs a file stream".into()),
        };
        ok_int(pos.map_or(-1, |p| p as i32))
    }),
    ("CopyStream%src_stream%dest_stream%buffer_size=16384", |a, c, _| {
        let mut buf = vec![0u8; a[2].int().clamp(1, 1 << 24) as usize];
        loop {
            let n = stream_read(c.file(a[0].int())?, &mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            stream_write(c.file(a[1].int())?, &buf[..n])?;
        }
        VOID
    }),
    ("%ReadDir$dirname", |a, c, _| {
        let Ok(rd) = std::fs::read_dir(&*a[0].str()) else { return ok_int(0) };
        let mut names: std::collections::VecDeque<String> = [".".to_string(), "..".to_string()].into();
        let mut rest: Vec<String> = rd.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned())).collect();
        rest.sort_by_key(|n| n.to_lowercase());
        names.extend(rest);
        let h = c.handle();
        c.dirs.insert(h, names);
        ok_int(h)
    }),
    ("$NextFile%dir", |a, c, _| {
        let d = c.dirs.get_mut(&a[0].int()).ok_or_else(|| format!("directory {} is not open", a[0].int()))?;
        ok_s(d.pop_front().unwrap_or_default())
    }),
    ("CloseDir%dir", |a, c, _| {
        c.dirs.remove(&a[0].int());
        VOID
    }),
    // ---- banks (bbbank.cpp)
    ("%CreateBank%size=0", |a, c, _| {
        let h = c.handle();
        c.banks.insert(h, vec![0; a[0].int().clamp(0, 1 << 30) as usize]);
        ok_int(h)
    }),
    ("FreeBank%bank", |a, c, _| {
        c.banks.remove(&a[0].int());
        VOID
    }),
    ("%BankSize%bank", |a, c, _| ok_int(c.bank(a[0].int())?.len() as i32)),
    ("ResizeBank%bank%size", |a, c, _| {
        let n = a[1].int().clamp(0, 1 << 30) as usize;
        c.bank(a[0].int())?.resize(n, 0);
        VOID
    }),
    ("CopyBank%src_bank%src_offset%dest_bank%dest_offset%count", |a, c, _| {
        let n = a[4].int().max(0) as usize;
        let src = c.bank(a[0].int())?;
        let r = bank_range(src, a[1].int(), n)?;
        let data = src[r].to_vec();
        let dst = c.bank(a[2].int())?;
        let r = bank_range(dst, a[3].int(), n)?;
        dst[r].copy_from_slice(&data);
        VOID
    }),
    ("%PeekByte%bank%offset", |a, c, _| ok_int(peek::<1>(c, a)?[0] as i32)),
    ("%PeekShort%bank%offset", |a, c, _| ok_int(u16::from_le_bytes(peek::<2>(c, a)?) as i32)),
    ("%PeekInt%bank%offset", |a, c, _| ok_int(i32::from_le_bytes(peek::<4>(c, a)?))),
    ("#PeekFloat%bank%offset", |a, c, _| ok_f(f32::from_le_bytes(peek::<4>(c, a)?))),
    ("PokeByte%bank%offset%value", |a, c, _| poke(c, a, &[a[2].int() as u8])),
    ("PokeShort%bank%offset%value", |a, c, _| poke(c, a, &(a[2].int() as u16).to_le_bytes())),
    ("PokeInt%bank%offset%value", |a, c, _| poke(c, a, &a[2].int().to_le_bytes())),
    ("PokeFloat%bank%offset#value", |a, c, _| poke(c, a, &a[2].float().to_le_bytes())),
    ("%ReadBytes%bank%stream%offset%count", |a, c, _| {
        let n = a[3].int().max(0) as usize;
        let mut buf = vec![0u8; n];
        let mut got = 0;
        while got < n {
            match stream_read(c.file(a[1].int())?, &mut buf[got..]) {
                Ok(0) | Err(_) => break,
                Ok(k) => got += k,
            }
        }
        let b = c.bank(a[0].int())?;
        let r = bank_range(b, a[2].int(), got)?;
        b[r].copy_from_slice(&buf[..got]);
        ok_int(got as i32)
    }),
    ("%WriteBytes%bank%stream%offset%count", |a, c, _| {
        let n = a[3].int().max(0) as usize;
        let b = c.bank(a[0].int())?;
        let r = bank_range(b, a[2].int(), n)?;
        let data = b[r].to_vec();
        stream_write(c.file(a[1].int())?, &data)?;
        ok_int(n as i32)
    }),
    // ---- timers (bbsys.cpp)
    ("%CreateTimer%hertz", |a, c, _| {
        let hz = a[0].int().max(1) as f64;
        let period = std::time::Duration::from_secs_f64(1.0 / hz);
        let h = c.handle();
        c.timers.insert(h, Timer { period, next: Instant::now() + period });
        ok_int(h)
    }),
    ("%WaitTimer%timer", |a, c, _| {
        let t = c.timers.get_mut(&a[0].int()).ok_or_else(|| format!("Timer {} does not exist", a[0].int()))?;
        let now = Instant::now();
        if now < t.next {
            std::thread::sleep(t.next - now);
        }
        // ticks that have passed, at least one; catch up without bursts
        let now = Instant::now();
        let mut ticks = 0;
        while t.next <= now && ticks < 1000 {
            t.next += t.period;
            ticks += 1;
        }
        if ticks >= 1000 {
            t.next = now + t.period;
        }
        ok_int(ticks.max(1))
    }),
    ("FreeTimer%timer", |a, c, _| {
        c.timers.remove(&a[0].int());
        VOID
    }),
    // ---- system
    ("%RndSeed", |_, c, _| ok_int(c.seed)),
    ("SetEnv$name$value", |a, _, _| {
        // SAFETY: programs are single-threaded; nothing reads the environment concurrently
        unsafe { std::env::set_var(&*a[0].str(), &*a[1].str()) };
        VOID
    }),
    // (ext) easing curves (see ease.rs)
    ("#Ease#t%ease=3", |a, _, _| ok_f(crate::ease::ease(a[0].float(), a[1].int()))),
    ("#EaseBetween#from#to#t%ease=3", |a, _, _| {
        let (from, to) = (a[0].float(), a[1].float());
        ok_f(from + (to - from) * crate::ease::ease(a[2].float(), a[3].int()))
    }),
    // (ext) settings and save data (see settings.rs)
    ("$UserFolder$app=\"\"", |a, _, _| ok_s(crate::settings::user_folder(&a[0].str())?)),
    ("SetSetting$name$value", |a, c, _| {
        c.settings.set(&a[0].str(), &a[1].str())?;
        VOID
    }),
    ("$GetSetting$name$default=\"\"", |a, c, _| ok_s(c.settings.get(&a[0].str()).map(str::to_string).unwrap_or_else(|| a[1].str().to_string()))),
    ("%GetSettingInt$name%default=0", |a, c, _| {
        let v = c.settings.get(&a[0].str()).and_then(|s| s.trim().parse::<f64>().ok());
        ok_int(v.map_or(a[1].int(), |f| f.round() as i32))
    }),
    ("#GetSettingFloat$name#default=0", |a, c, _| {
        let v = c.settings.get(&a[0].str()).and_then(|s| s.trim().parse::<f32>().ok());
        ok_f(v.unwrap_or(a[1].float()))
    }),
    ("%SettingExists$name", |a, c, _| ok_int(c.settings.get(&a[0].str()).is_some() as i32)),
    ("DeleteSetting$name", |a, c, _| {
        c.settings.delete(&a[0].str());
        VOID
    }),
    ("ClearSettings", |_, c, _| {
        c.settings.clear();
        VOID
    }),
    ("SaveSettings$file", |a, c, _| {
        let path = a[0].str().to_string();
        std::fs::write(&path, c.settings.to_text()).map_err(|e| format!("SaveSettings: cannot write '{path}': {e}"))?;
        VOID
    }),
    ("%LoadSettings$file", |a, c, _| {
        let Ok(text) = std::fs::read_to_string(a[0].str().to_string()) else { return ok_int(0) };
        c.settings.load_text(&text).map_err(|e| format!("LoadSettings: {e}"))?;
        ok_int(1)
    }),
    ("$SystemProperty$property", |a, _, _| {        let p = a[0].str().to_lowercase();
        let dir = |p: Option<std::path::PathBuf>| p.map(|d| format!("{}{}", d.display(), std::path::MAIN_SEPARATOR)).unwrap_or_default();
        ok_s(match p.as_str() {
            "appdir" => dir(std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()))),
            "tempdir" => dir(Some(std::env::temp_dir())),
            "currentdir" => dir(std::env::current_dir().ok()),
            "windowsdir" => dir(std::env::var_os("WINDIR").map(Into::into)),
            "systemdir" => dir(std::env::var_os("WINDIR").map(|w| std::path::Path::new(&w).join("System32"))),
            "homedir" => dir(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(Into::into)),
            "os" => std::env::consts::OS.to_string(),
            "cpucount" => std::thread::available_parallelism().map_or(1, |n| n.get()).to_string(),
            _ => String::new(),
        })
    }),
    ("%CallDLL$dll_name$proc_name%in_bank=0%out_bank=0", |a, c, _| {
        let mut input = if a[2].int() != 0 { c.bank(a[2].int())?.clone() } else { Vec::new() };
        let mut output = if a[3].int() != 0 { c.bank(a[3].int())?.clone() } else { Vec::new() };
        let r = call_dll(&a[0].str(), &a[1].str(), &mut input, &mut output)?;
        if a[3].int() != 0 {
            *c.bank(a[3].int())? = output;
        }
        ok_int(r)
    }),
    ("ExecFile$file", |a, _, _| {
        exec_file(&a[0].str());
        VOID
    }),
    // ---- networking (bbsockets.cpp)
    ("%OpenTCPStream$server%port", |a, c, _| {
        let tcp = crate::net::Tcp::connect(&a[0].str(), a[1].int().clamp(0, 65535) as u16, c.tcp_read_ms.max(2000));
        ok_int(match tcp {
            Some(t) => c.add_stream(Stream::Tcp(t)),
            None => 0,
        })
    }),
    ("CloseTCPStream%stream", |a, c, _| {
        c.files.remove(&a[0].int());
        VOID
    }),
    ("%CreateTCPServer%port", |a, c, _| {
        ok_int(match crate::net::Server::bind(a[0].int().clamp(0, 65535) as u16) {
            Some(s) => {
                let h = c.handle();
                c.servers.insert(h, s);
                h
            }
            None => 0,
        })
    }),
    ("CloseTCPServer%server", |a, c, _| {
        c.servers.remove(&a[0].int());
        VOID
    }),
    ("%AcceptTCPStream%server", |a, c, _| {
        let (accept, read) = (c.tcp_accept_ms, c.tcp_read_ms);
        let s = c.servers.get(&a[0].int()).ok_or_else(|| format!("TCP server {} does not exist", a[0].int()))?;
        ok_int(match s.accept(accept, read) {
            Some(t) => c.add_stream(Stream::Tcp(t)),
            None => 0,
        })
    }),
    ("%TCPStreamIP%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Tcp(t) => ok_int(crate::net::ip_to_int(t.peer.ip())),
        _ => Err("not a TCP stream".into()),
    }),
    ("%TCPStreamPort%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Tcp(t) => ok_int(t.peer.port() as i32),
        _ => Err("not a TCP stream".into()),
    }),
    ("TCPTimeouts%read_millis%accept_millis", |a, c, _| {
        c.tcp_read_ms = a[0].int().max(0) as u32;
        c.tcp_accept_ms = a[1].int().max(0) as u32;
        VOID
    }),
    ("%CreateUDPStream%port=0", |a, c, _| {
        let ms = c.udp_ms;
        ok_int(match crate::net::Udp::bind(a[0].int().clamp(0, 65535) as u16, ms) {
            Some(u) => c.add_stream(Stream::Udp(u)),
            None => 0,
        })
    }),
    ("CloseUDPStream%stream", |a, c, _| {
        c.files.remove(&a[0].int());
        VOID
    }),
    ("SendUDPMsg%stream%dest_ip%dest_port=0", |a, c, _| match c.file(a[0].int())? {
        Stream::Udp(u) => {
            u.send(a[1].int(), a[2].int().clamp(0, 65535) as u16);
            VOID
        }
        _ => Err("not a UDP stream".into()),
    }),
    ("%RecvUDPMsg%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Udp(u) => ok_int(u.recv()),
        _ => Err("not a UDP stream".into()),
    }),
    ("%UDPStreamIP%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Udp(u) => ok_int(u.sock.local_addr().map_or(0, |a| crate::net::ip_to_int(a.ip()))),
        _ => Err("not a UDP stream".into()),
    }),
    ("%UDPStreamPort%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Udp(u) => ok_int(u.sock.local_addr().map_or(0, |a| a.port() as i32)),
        _ => Err("not a UDP stream".into()),
    }),
    ("%UDPMsgIP%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Udp(u) => ok_int(u.msg_from.map_or(0, |a| crate::net::ip_to_int(a.ip()))),
        _ => Err("not a UDP stream".into()),
    }),
    ("%UDPMsgPort%stream", |a, c, _| match c.file(a[0].int())? {
        Stream::Udp(u) => ok_int(u.msg_from.map_or(0, |a| a.port() as i32)),
        _ => Err("not a UDP stream".into()),
    }),
    ("UDPTimeouts%recv_millis", |a, c, _| {
        c.udp_ms = a[0].int().max(0) as u32;
        let ms = c.udp_ms;
        for s in c.files.values_mut() {
            if let Stream::Udp(u) = s {
                let _ = u.sock.set_read_timeout(Some(std::time::Duration::from_millis(ms.max(1) as u64)));
            }
        }
        VOID
    }),
    // ---- multiplayer games (bbnet: DirectPlay in Blitz3D, iroh here; see netgame.rs)
    ("%StartNetGame", |_, c, ctx| {
        let game = c.program_game_name();
        c.net.borrow_mut().open(ctx, Plan::Auto, &game, None)
    }),
    ("%HostNetGame$game_name", |a, c, ctx| c.net.borrow_mut().open(ctx, Plan::Host, &a[0].str(), None)),
    ("%JoinNetGame$game_name$server_ip", |a, c, ctx| c.net.borrow_mut().open(ctx, Plan::Join(a[1].str().to_string()), &a[0].str(), None)),
    ("StopNetGame", |_, c, _| {
        c.net.borrow_mut().game = None;
        VOID
    }),
    ("%CreateNetPlayer$name", |a, c, ctx| c.net.borrow_mut().create_player(ctx, &a[0].str())),
    ("DeleteNetPlayer%player", |a, c, _| {
        c.net.borrow().with((), |g| g.delete_player(a[0].int()));
        VOID
    }),
    ("$NetPlayerName%player", |a, c, _| ok_s(c.net.borrow().with(String::new(), |g| g.player_name(a[0].int())))),
    ("%NetPlayerLocal%player", |a, c, _| ok_int(c.net.borrow().with(false, |g| g.is_local(a[0].int())) as i32)),
    ("%SendNetMsg%type$data%from_player%to_player=0%reliable=1", |a, c, _| {
        ok_int(c.net.borrow().with(false, |g| g.send(a[0].int(), &a[1].str(), a[2].int(), a[3].int(), a[4].int() != 0)) as i32)
    }),
    ("%RecvNetMsg", |_, c, _| {
        let mut n = c.net.borrow_mut();
        match n.with(None, |g| g.recv()) {
            Some(m) => {
                n.msg = m;
                ok_int(1)
            }
            None => ok_int(0),
        }
    }),
    ("%NetMsgType", |_, c, _| ok_int(c.net.borrow().msg.kind)),
    ("%NetMsgFrom", |_, c, _| ok_int(c.net.borrow().msg.from)),
    ("%NetMsgTo", |_, c, _| ok_int(c.net.borrow().msg.to)),
    ("$NetMsgData", |_, c, _| ok_s(c.net.borrow().msg.data.as_str())),
    // (ext) easy multiplayer
    ("%NetPlay$game$player_name=\"\"", |a, c, ctx| c.net.borrow_mut().open(ctx, Plan::Auto, &a[0].str(), Some(&a[1].str()))),
    ("%NetCreate$game$player_name=\"\"", |a, c, ctx| c.net.borrow_mut().open(ctx, Plan::Host, &a[0].str(), Some(&a[1].str()))),
    ("%NetJoin$invite$player_name=\"\"", |a, c, ctx| c.net.borrow_mut().open(ctx, Plan::Join(a[0].str().to_string()), "", Some(&a[1].str()))),
    ("$NetInvite", |_, c, _| ok_s(c.net.borrow().with(String::new(), |g| g.invite.clone()))),
    ("$NetError", |_, c, _| {
        let n = c.net.borrow();
        ok_s(n.with(n.error.clone(), |g| if g.error.is_empty() { n.error.clone() } else { g.error.clone() }))
    }),
    ("%NetIsHost", |_, c, _| ok_int(c.net.borrow().with(false, |g| g.is_host()) as i32)),
    ("%NetMe", |_, c, _| ok_int(c.net.borrow().with(0, |g| g.me))),
    ("%NetHostPlayer", |_, c, _| ok_int(c.net.borrow().with(0, |g| g.host_player))),
    ("%NetCountPlayers", |_, c, _| ok_int(c.net.borrow().with(0, |g| g.player_count()))),
    ("%NetPlayerAt%index", |a, c, _| ok_int(c.net.borrow().with(0, |g| g.player_at(a[0].int())))),
    ("%NetPing", |_, c, _| ok_int(c.net.borrow().with(0, |g| g.ping_ms))),
    ("NetSet$key$value", |a, c, _| {
        c.net.borrow().with((), |g| g.set_value(&a[0].str(), &a[1].str()));
        VOID
    }),
    ("$NetGet$key$default=\"\"", |a, c, _| {
        ok_s(c.net.borrow().with(a[1].str().to_string(), |g| g.value(&a[0].str()).map_or_else(|| a[1].str().to_string(), str::to_string)))
    }),
    ("%NetSend$event$data=\"\"%to_player=0", |a, c, _| ok_int(c.net.borrow().with(false, |g| g.send_event(&a[0].str(), &a[1].str(), a[2].int())) as i32)),
    ("%NetRecv", |_, c, _| {
        let mut n = c.net.borrow_mut();
        match n.with(None, |g| g.next_event()) {
            Some(e) => {
                n.event = e;
                ok_int(1)
            }
            None => ok_int(0),
        }
    }),
    ("$NetEvent", |_, c, _| ok_s(c.net.borrow().event.name.as_str())),
    ("$NetData", |_, c, _| ok_s(c.net.borrow().event.data.as_str())),
    ("%NetFrom", |_, c, _| ok_int(c.net.borrow().event.from)),
    ("%NetEntity", |_, c, _| ok_int(c.net.borrow().event.entity)),
    ("NetSimulate%lag_millis%loss_percent=0", |a, c, _| {
        let mut n = c.net.borrow_mut();
        let (lag, loss) = (a[0].int().max(0) as u32, a[1].int().clamp(0, 100) as u32);
        n.options.lag_ms = lag;
        n.options.loss_percent = loss;
        n.with((), |g| g.simulate(lag, loss));
        VOID
    }),
    ("NetRelay$url", |a, c, _| {
        let url = a[0].str();
        c.net.borrow_mut().options.relay = (!url.is_empty()).then(|| url.to_string());
        VOID
    }),
    ("$DottedIP%ip", |a, _, _| ok_s(crate::net::dotted(a[0].int()))),
    ("%CountHostIPs$host_name", |a, c, _| {
        let ips = crate::net::host_ips(&a[0].str());
        let n = ips.len() as i32;
        c.host_ips = ips;
        ok_int(n)
    }),
    ("%HostIP%index", |a, c, _| {
        let i = a[0].int();
        ok_int(usize::try_from(i - 1).ok().and_then(|i| c.host_ips.get(i)).copied().unwrap_or(0))
    }),
];

/// Built-in constants available to every program.
pub fn constants() -> Vec<(String, ConstVal)> {
    let mut v = vec![("PI2".to_string(), ConstVal::Float(std::f32::consts::TAU))];
    v.extend(crate::ease::constants());
    v
}

/// Environment with the standard library and loaded plugins; hosts append
/// their own commands.
pub fn base_env() -> Env {
    let mut commands: Vec<_> = COMMANDS.iter().map(|(s, _)| parse_sig(s)).collect();
    commands.extend(crate::plugin::command_sigs());
    let mut constants = constants();
    constants.extend(crate::plugin::constants());
    Env { commands, constants }
}

/// Number of commands the VM runs itself: the standard library and plugins.
pub fn core_count() -> usize {
    COMMANDS.len() + crate::plugin::count()
}

pub(crate) fn call(id: u32, args: &[Value], core: &mut Core, ctx: &mut NativeCtx) -> Result<Value, String> {
    match COMMANDS.get(id as usize) {
        Some(c) => (c.1)(args, core, ctx),
        None => crate::plugin::call(id as usize - COMMANDS.len(), args),
    }
}

/// (date "DD Mon YYYY", time "HH:MM:SS") in UTC, computed without extra dependencies.
fn civil_now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    (
        format!("{:02} {} {}", d, MON[(m - 1) as usize], y),
        format!("{:02}:{:02}:{:02}", tod / 3600, (tod / 60) % 60, tod % 60),
    )
}

