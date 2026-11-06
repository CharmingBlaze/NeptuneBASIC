//! Write NeptuneBASIC plugins in Rust.
//!
//! ```ignore
//! use neptune_plugin::{neptune_plugin, Arg, Plugin, Ret};
//!
//! fn init(p: &mut Plugin) {
//!     p.command("$Greet$name", |a| Ok(Ret::Str(format!("Hello, {}!", a[0].str()))));
//!     p.command("%AddInts%a%b", |a| Ok((a[0].int() + a[1].int()).into()));
//!     p.constant_int("GREETINGS", 3);
//! }
//! neptune_plugin!(init);
//! ```
//!
//! Build the crate as a `cdylib` and put the library in a `plugins` folder
//! next to `neptune` or next to your program. See `sdk/neptune_plugin.h` for
//! the signature notation and rules.

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CString};

pub const API_VERSION: u32 = 1;

const NB_NULL: i32 = 0;
const NB_INT: i32 = 1;
const NB_FLOAT: i32 = 2;
const NB_STRING: i32 = 3;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NbValue {
    pub kind: i32,
    pub i: i32,
    pub f: f32,
    pub s: *const c_char,
    pub len: usize,
}

pub type NbCommand = unsafe extern "C" fn(*mut c_void, *const NbValue, i32, *mut NbValue) -> i32;

#[repr(C)]
pub struct NbHost {
    pub api_version: u32,
    pub host: *mut c_void,
    pub add_command: unsafe extern "C" fn(*mut c_void, *const c_char, NbCommand, *mut c_void) -> i32,
    pub add_constant_int: unsafe extern "C" fn(*mut c_void, *const c_char, i32),
    pub add_constant_float: unsafe extern "C" fn(*mut c_void, *const c_char, f32),
    pub add_constant_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char),
}

/// An argument, already converted to the type the signature declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arg<'a> {
    Null,
    Int(i32),
    Float(f32),
    Str(&'a str),
}

impl Arg<'_> {
    pub fn int(&self) -> i32 {
        match *self {
            Arg::Int(i) => i,
            Arg::Float(f) => f as i32,
            Arg::Str(s) => s.trim().parse().unwrap_or(0),
            Arg::Null => 0,
        }
    }
    pub fn float(&self) -> f32 {
        match *self {
            Arg::Int(i) => i as f32,
            Arg::Float(f) => f,
            Arg::Str(s) => s.trim().parse().unwrap_or(0.0),
            Arg::Null => 0.0,
        }
    }
    pub fn str(&self) -> String {
        match *self {
            Arg::Int(i) => i.to_string(),
            Arg::Float(f) => f.to_string(),
            Arg::Str(s) => s.to_string(),
            Arg::Null => String::new(),
        }
    }
}

/// A command's result.
#[derive(Clone, Debug, PartialEq)]
pub enum Ret {
    None,
    Int(i32),
    Float(f32),
    Str(String),
}

impl From<i32> for Ret {
    fn from(v: i32) -> Self {
        Ret::Int(v)
    }
}
impl From<f32> for Ret {
    fn from(v: f32) -> Self {
        Ret::Float(v)
    }
}
impl From<String> for Ret {
    fn from(v: String) -> Self {
        Ret::Str(v)
    }
}
impl From<&str> for Ret {
    fn from(v: &str) -> Self {
        Ret::Str(v.to_string())
    }
}
impl From<bool> for Ret {
    fn from(v: bool) -> Self {
        Ret::Int(v as i32)
    }
}
impl From<()> for Ret {
    fn from(_: ()) -> Self {
        Ret::None
    }
}

type Func = Box<dyn Fn(&[Arg]) -> Result<Ret, String>>;

thread_local! {
    /// Returned strings live here until the next call.
    static RETURNED: RefCell<String> = const { RefCell::new(String::new()) };
}

unsafe extern "C" fn trampoline(user: *mut c_void, args: *const NbValue, argc: i32, out: *mut NbValue) -> i32 {
    let f = &*(user as *const Func);
    let raw = if argc > 0 { std::slice::from_raw_parts(args, argc as usize) } else { &[] };
    let args: Vec<Arg> = raw
        .iter()
        .map(|v| match v.kind {
            NB_INT => Arg::Int(v.i),
            NB_FLOAT => Arg::Float(v.f),
            NB_STRING if !v.s.is_null() => {
                let bytes = if v.len > 0 { std::slice::from_raw_parts(v.s as *const u8, v.len) } else { std::ffi::CStr::from_ptr(v.s).to_bytes() };
                Arg::Str(std::str::from_utf8(bytes).unwrap_or(""))
            }
            NB_STRING => Arg::Str(""),
            _ => Arg::Null,
        })
        .collect();
    // a panic must not cross into the host
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&args)))
        .unwrap_or_else(|_| Err("plugin command panicked".to_string()));
    let set_text = |text: String, out: *mut NbValue| {
        RETURNED.with(|b| {
            *b.borrow_mut() = text;
            let b = b.borrow();
            *out = NbValue { kind: NB_STRING, i: 0, f: 0.0, s: b.as_ptr() as *const c_char, len: b.len() };
        });
    };
    match r {
        Ok(Ret::None) => {
            *out = NbValue { kind: NB_NULL, i: 0, f: 0.0, s: std::ptr::null(), len: 0 };
            0
        }
        Ok(Ret::Int(i)) => {
            *out = NbValue { kind: NB_INT, i, f: i as f32, s: std::ptr::null(), len: 0 };
            0
        }
        Ok(Ret::Float(f)) => {
            *out = NbValue { kind: NB_FLOAT, i: f as i32, f, s: std::ptr::null(), len: 0 };
            0
        }
        Ok(Ret::Str(s)) => {
            set_text(s, out);
            0
        }
        Err(e) => {
            set_text(e, out);
            1
        }
    }
}

/// Registers commands and constants while the plugin starts.
pub struct Plugin<'a> {
    host: &'a NbHost,
    /// Problems registering (bad signatures, names already taken).
    pub errors: Vec<String>,
}

impl Plugin<'_> {
    /// Adds a command. `signature` uses Blitz notation, e.g. `"%AddInts%a%b"`.
    pub fn command(&mut self, signature: &str, f: impl Fn(&[Arg]) -> Result<Ret, String> + 'static) {
        let Ok(sig) = CString::new(signature) else {
            self.errors.push(format!("bad signature {signature:?}"));
            return;
        };
        // lives as long as the program: commands can be called at any time
        let user = Box::into_raw(Box::new(Box::new(f) as Func)) as *mut c_void;
        // SAFETY: the host table is valid during init; `user` is the boxed closure
        let status = unsafe { (self.host.add_command)(self.host.host, sig.as_ptr(), trampoline, user) };
        if status != 0 {
            self.errors.push(format!("could not add {signature}"));
        }
    }

    pub fn constant_int(&mut self, name: &str, value: i32) {
        if let Ok(n) = CString::new(name) {
            // SAFETY: valid host table and NUL-terminated name
            unsafe { (self.host.add_constant_int)(self.host.host, n.as_ptr(), value) }
        }
    }

    pub fn constant_float(&mut self, name: &str, value: f32) {
        if let Ok(n) = CString::new(name) {
            // SAFETY: valid host table and NUL-terminated name
            unsafe { (self.host.add_constant_float)(self.host.host, n.as_ptr(), value) }
        }
    }

    pub fn constant_string(&mut self, name: &str, value: &str) {
        if let (Ok(n), Ok(v)) = (CString::new(name), CString::new(value)) {
            // SAFETY: valid host table and NUL-terminated strings
            unsafe { (self.host.add_constant_string)(self.host.host, n.as_ptr(), v.as_ptr()) }
        }
    }
}

/// Runs a plugin's init function; used by [`neptune_plugin!`].
///
/// # Safety
/// `host` must be the table NeptuneBASIC passes to `neptune_plugin_init`.
pub unsafe fn run_init(host: *const NbHost, init: fn(&mut Plugin)) -> i32 {
    let Some(host) = host.as_ref() else { return 1 };
    if host.api_version != API_VERSION {
        return 1;
    }
    let mut p = Plugin { host, errors: Vec::new() };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| init(&mut p))) {
        Ok(()) => 0,
        Err(_) => 2,
    }
}

/// Exports `neptune_plugin_init` calling your init function.
#[macro_export]
macro_rules! neptune_plugin {
    ($init:path) => {
        /// Entry point NeptuneBASIC looks for.
        ///
        /// # Safety
        /// Called by NeptuneBASIC with its host table.
        #[no_mangle]
        pub unsafe extern "C" fn neptune_plugin_init(host: *const $crate::NbHost) -> i32 {
            $crate::run_init(host, $init)
        }
    };
}
