//! Engine-independent standard library: console, math, strings, files,
//! time and random numbers. Names and signatures follow Blitz3D's
//! `basic.cpp`, `bbmath.cpp`, `bbstring.cpp`, `bbfilesystem.cpp` and
//! `bbstream.cpp`; NeptuneBASIC additions are marked in [`COMMANDS`].

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, Write};
use std::rc::Rc;
use std::time::Instant;

use neptune_lang::commands::{parse_sig, ConstVal};
use neptune_lang::sema::Env;

use crate::value::Value;
use crate::vm::NativeCtx;

/// Where `Print`/`Write` output goes.
pub enum Output {
    Stdout,
    /// Captured output (tests, embedding, IDE consoles).
    Buffer(String),
}

enum Stream {
    Read(BufReader<File>),
    Write(BufWriter<File>),
    ReadWrite(File),
}

/// State used by standard library commands.
pub struct Core {
    pub output: Output,
    start: Instant,
    rng: u64,
    files: HashMap<i32, Stream>,
    next_file: i32,
    /// Lines returned by `Input` when set (tests/embedding); otherwise stdin.
    pub input_lines: Option<std::collections::VecDeque<String>>,
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
            input_lines: None,
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
        }
    }

    /// Takes captured output (when using [`Output::Buffer`]).
    pub fn take_output(&mut self) -> String {
        match &mut self.output {
            Output::Buffer(b) => std::mem::take(b),
            Output::Stdout => String::new(),
        }
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
    }
}
fn stream_write(s: &mut Stream, buf: &[u8]) -> Result<(), String> {
    let r = match s {
        Stream::Write(w) => w.write_all(buf),
        Stream::ReadWrite(f) => f.write_all(buf),
        Stream::Read(_) => return Err("stream is not writable".into()),
    };
    r.map_err(|e| e.to_string())
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
    ("$CommandLine", |_, _, _| ok_s(std::env::args().skip(2).collect::<Vec<_>>().join(" "))),
    ("$GetEnv$name", |a, _, _| ok_s(std::env::var(&*a[0].str()).unwrap_or_default())),
    ("$CurrentDate", |_, _, _| ok_s(civil_now().0)),
    ("$CurrentTime", |_, _, _| ok_s(civil_now().1)),
    // ---- random (bbmath.cpp)
    ("SeedRnd%seed", |a, c, _| {
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
];

/// Built-in constants available to every program.
pub fn constants() -> Vec<(String, ConstVal)> {
    vec![("PI2".to_string(), ConstVal::Float(std::f32::consts::TAU))]
}

/// Environment with the standard library; hosts append their own commands.
pub fn base_env() -> Env {
    Env { commands: COMMANDS.iter().map(|(s, _)| parse_sig(s)).collect(), constants: constants() }
}

pub(crate) fn call(id: u32, args: &[Value], core: &mut Core, ctx: &mut NativeCtx) -> Result<Value, String> {
    (COMMANDS[id as usize].1)(args, core, ctx)
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

