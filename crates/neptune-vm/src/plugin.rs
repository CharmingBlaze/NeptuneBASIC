//! Native plugins: shared libraries that add real BASIC commands and
//! constants, on Windows (`.dll`), Linux (`.so`) and macOS (`.dylib`).
//!
//! A plugin exports one C function, `neptune_plugin_init`, which receives an
//! [`NbHost`] table and registers commands with Blitz signatures
//! (`"%AddInts%a%b"`, `"$Greet$name"`, ...) and constants. Registered
//! commands are type-checked by the compiler, show up in `neptune commands`
//! and editor completion, and are called with converted arguments, exactly
//! like built-in commands. The C interface is in `sdk/neptune_plugin.h`; the
//! `neptune-plugin` crate wraps it for Rust.
//!
//! Plugins are loaded before a program is compiled, from (in order) the
//! `plugins` folder next to the `neptune` executable, the `plugins` folder
//! in the current directory, and any folders in `NEPTUNE_PLUGIN_PATH`.
//! Plugins are native code with full access to the computer, like any
//! program you run: only use plugins you trust.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Mutex, OnceLock};

use neptune_lang::commands::{parse_sig, CommandSig, ConstVal};

use crate::dynlib::Library;
use crate::value::Value;

/// Version of the plugin interface; plugins built for another version are refused.
pub const API_VERSION: u32 = 1;

pub const NB_NULL: i32 = 0;
pub const NB_INT: i32 = 1;
pub const NB_FLOAT: i32 = 2;
pub const NB_STRING: i32 = 3;

/// A value passed to or returned from a plugin command.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NbValue {
    pub kind: i32,
    pub i: i32,
    pub f: f32,
    /// UTF-8 text (not necessarily NUL-terminated) and its length in bytes.
    pub s: *const c_char,
    pub len: usize,
}

impl Default for NbValue {
    fn default() -> Self {
        NbValue { kind: NB_NULL, i: 0, f: 0.0, s: std::ptr::null(), len: 0 }
    }
}

/// A command: returns 0 on success; non-zero means an error, with the
/// message in `result` as a string.
pub type NbCommand = unsafe extern "C" fn(user: *mut c_void, args: *const NbValue, argc: i32, result: *mut NbValue) -> i32;

/// The table a plugin's `neptune_plugin_init` receives.
#[repr(C)]
pub struct NbHost {
    pub api_version: u32,
    pub host: *mut c_void,
    pub add_command: unsafe extern "C" fn(host: *mut c_void, signature: *const c_char, command: NbCommand, user: *mut c_void) -> i32,
    pub add_constant_int: unsafe extern "C" fn(host: *mut c_void, name: *const c_char, value: i32),
    pub add_constant_float: unsafe extern "C" fn(host: *mut c_void, name: *const c_char, value: f32),
    pub add_constant_string: unsafe extern "C" fn(host: *mut c_void, name: *const c_char, value: *const c_char),
}

pub type InitFn = unsafe extern "C" fn(host: *const NbHost) -> i32;

struct Command {
    sig: CommandSig,
    signature: String,
    func: NbCommand,
    user: *mut c_void,
    plugin: usize,
}

/// A loaded plugin.
pub struct Plugin {
    pub path: String,
    pub commands: Vec<String>,
    _lib: Option<Library>,
}

#[derive(Default)]
pub struct Registry {
    commands: Vec<Command>,
    constants: Vec<(String, ConstVal)>,
    pub plugins: Vec<Plugin>,
    /// Errors from registration (bad signatures, duplicates), reported once.
    pub errors: Vec<String>,
}

// Plugin function pointers and user data are only called on the VM's thread.
unsafe impl Send for Registry {}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Registration state while a plugin's init function runs.
struct Loading<'a> {
    reg: &'a mut Registry,
    plugin: usize,
    taken: &'a dyn Fn(&str) -> bool,
}

unsafe fn c_str(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

unsafe extern "C" fn host_add_command(host: *mut c_void, signature: *const c_char, command: NbCommand, user: *mut c_void) -> i32 {
    let l = &mut *(host as *mut Loading);
    let text = c_str(signature);
    if text.is_empty() || !text.trim_start_matches(['%', '#', '$', '?']).starts_with(|c: char| c.is_ascii_alphabetic()) {
        l.reg.errors.push(format!("plugin {}: bad command signature '{text}'", l.reg.plugins[l.plugin].path));
        return 1;
    }
    let sig = parse_sig(&text);
    if (l.taken)(&sig.name) || l.reg.commands.iter().any(|c| c.sig.name == sig.name) {
        l.reg.errors.push(format!("plugin {}: command '{}' already exists", l.reg.plugins[l.plugin].path, sig.display));
        return 1;
    }
    l.reg.plugins[l.plugin].commands.push(sig.display.clone());
    l.reg.commands.push(Command { sig, signature: text, func: command, user, plugin: l.plugin });
    0
}

unsafe extern "C" fn host_add_int(host: *mut c_void, name: *const c_char, value: i32) {
    let l = &mut *(host as *mut Loading);
    l.reg.constants.push((c_str(name), ConstVal::Int(value)));
}
unsafe extern "C" fn host_add_float(host: *mut c_void, name: *const c_char, value: f32) {
    let l = &mut *(host as *mut Loading);
    l.reg.constants.push((c_str(name), ConstVal::Float(value)));
}
unsafe extern "C" fn host_add_string(host: *mut c_void, name: *const c_char, value: *const c_char) {
    let l = &mut *(host as *mut Loading);
    l.reg.constants.push((c_str(name), ConstVal::Str(c_str(value).into())));
}

/// Runs a plugin's init function against the registry. `taken` says whether
/// a command name already belongs to the language or runtime.
pub fn register(path: &str, lib: Option<Library>, init: InitFn, taken: &dyn Fn(&str) -> bool) -> Result<usize, String> {
    let mut reg = registry().lock().unwrap();
    let plugin = reg.plugins.len();
    reg.plugins.push(Plugin { path: path.to_string(), commands: Vec::new(), _lib: lib });
    let before = reg.commands.len();
    let mut loading = Loading { reg: &mut reg, plugin, taken };
    let table = NbHost {
        api_version: API_VERSION,
        host: &mut loading as *mut Loading as *mut c_void,
        add_command: host_add_command,
        add_constant_int: host_add_int,
        add_constant_float: host_add_float,
        add_constant_string: host_add_string,
    };
    // SAFETY: the plugin's own init function, given a valid table for this call
    let status = unsafe { init(&table) };
    if status != 0 {
        reg.commands.truncate(before);
        reg.plugins[plugin].commands.clear();
        return Err(format!("plugin '{path}' failed to start (code {status})"));
    }
    Ok(reg.commands.len() - before)
}

/// Loads one plugin file.
pub fn load(path: &str, taken: &dyn Fn(&str) -> bool) -> Result<usize, String> {
    let lib = Library::open(path)?;
    let init = lib.symbol("neptune_plugin_init").ok_or_else(|| format!("'{path}' is not a NeptuneBASIC plugin (no neptune_plugin_init)"))?;
    // SAFETY: the symbol is the plugin entry point with the documented signature
    let init: InitFn = unsafe { std::mem::transmute::<*mut c_void, InitFn>(init) };
    register(path, Some(lib), init, taken)
}

/// Folders searched for plugins, in order.
pub fn search_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("plugins"))) {
        dirs.push(d);
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("plugins"));
    }
    if let Some(extra) = std::env::var_os("NEPTUNE_PLUGIN_PATH") {
        dirs.extend(std::env::split_paths(&extra));
    }
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let c = d.canonicalize().unwrap_or(d.clone());
        let new = !seen.contains(&c);
        seen.push(c);
        new
    });
    dirs
}

/// Loads every plugin in the search folders (once). Returns problems found.
pub fn load_default(taken: &dyn Fn(&str) -> bool) -> Vec<String> {
    static DONE: OnceLock<()> = OnceLock::new();
    if DONE.set(()).is_err() {
        return Vec::new();
    }
    let mut problems = Vec::new();
    for dir in search_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(crate::dynlib::extension())))
            .collect();
        files.sort();
        for f in files {
            if let Err(e) = load(&f.to_string_lossy(), taken) {
                problems.push(e);
            }
        }
    }
    problems.extend(std::mem::take(&mut registry().lock().unwrap().errors));
    problems
}

/// Signatures of every plugin command, in id order.
pub fn command_sigs() -> Vec<CommandSig> {
    registry().lock().unwrap().commands.iter().map(|c| c.sig.clone()).collect()
}

pub fn constants() -> Vec<(String, ConstVal)> {
    registry().lock().unwrap().constants.clone()
}

/// Loaded plugin files with the commands each one added.
pub fn loaded() -> Vec<(String, Vec<String>)> {
    registry().lock().unwrap().plugins.iter().map(|p| (p.path.clone(), p.commands.clone())).collect()
}

pub fn count() -> usize {
    registry().lock().unwrap().commands.len()
}

/// Calls plugin command `index` with converted arguments.
pub fn call(index: usize, args: &[Value]) -> Result<Value, String> {
    let (func, user, sig, path) = {
        let reg = registry().lock().unwrap();
        let c = reg.commands.get(index).ok_or("unknown plugin command")?;
        (c.func, c.user, c.signature.clone(), reg.plugins[c.plugin].path.clone())
    };
    // strings live in `texts` for the duration of the call
    let texts: Vec<Option<CString>> = args
        .iter()
        .map(|a| match a {
            Value::Str(s) => Some(CString::new(s.flat().replace('\0', "")).unwrap_or_default()),
            _ => None,
        })
        .collect();
    let values: Vec<NbValue> = args
        .iter()
        .zip(&texts)
        .map(|(a, t)| match (a, t) {
            (_, Some(c)) => NbValue { kind: NB_STRING, s: c.as_ptr(), len: c.as_bytes().len(), ..Default::default() },
            (Value::Int(i), _) => NbValue { kind: NB_INT, i: *i, f: *i as f32, ..Default::default() },
            (Value::Float(f), _) => NbValue { kind: NB_FLOAT, f: *f, i: *f as i32, ..Default::default() },
            _ => NbValue::default(),
        })
        .collect();
    let mut result = NbValue::default();
    // SAFETY: the plugin registered this function with the documented signature;
    // `values` and the strings they point to outlive the call
    let status = unsafe { func(user, values.as_ptr(), values.len() as i32, &mut result) };
    // SAFETY: the plugin promises `s`/`len` describe valid UTF-8 until its next call
    let text = |r: &NbValue| unsafe {
        if r.s.is_null() {
            String::new()
        } else if r.len > 0 {
            String::from_utf8_lossy(std::slice::from_raw_parts(r.s as *const u8, r.len)).into_owned()
        } else {
            CStr::from_ptr(r.s).to_string_lossy().into_owned()
        }
    };
    if status != 0 {
        let msg = text(&result);
        return Err(if msg.is_empty() { format!("plugin command {sig} ({path}) failed") } else { msg });
    }
    Ok(match result.kind {
        NB_INT => Value::Int(result.i),
        NB_FLOAT => Value::Float(result.f),
        NB_STRING => Value::Str(text(&result).into()),
        _ => Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn add(_: *mut c_void, args: *const NbValue, argc: i32, out: *mut NbValue) -> i32 {
        let a = std::slice::from_raw_parts(args, argc as usize);
        *out = NbValue { kind: NB_INT, i: a[0].i + a[1].i, ..Default::default() };
        0
    }

    unsafe extern "C" fn shout(_: *mut c_void, args: *const NbValue, _: i32, out: *mut NbValue) -> i32 {
        thread_local!(static BUF: std::cell::RefCell<String> = Default::default());
        let a = &*args;
        let s = String::from_utf8_lossy(std::slice::from_raw_parts(a.s as *const u8, a.len)).to_uppercase() + "!";
        BUF.with(|b| {
            *b.borrow_mut() = s;
            let b = b.borrow();
            *out = NbValue { kind: NB_STRING, s: b.as_ptr() as *const c_char, len: b.len(), ..Default::default() };
        });
        0
    }

    unsafe extern "C" fn fail(_: *mut c_void, _: *const NbValue, _: i32, out: *mut NbValue) -> i32 {
        static MSG: &[u8] = b"it broke\0";
        *out = NbValue { kind: NB_STRING, s: MSG.as_ptr() as *const c_char, ..Default::default() };
        1
    }

    unsafe extern "C" fn init(host: *const NbHost) -> i32 {
        let h = &*host;
        assert_eq!(h.api_version, API_VERSION);
        (h.add_command)(h.host, c"%TestAddInts%a%b".as_ptr(), add, std::ptr::null_mut());
        (h.add_command)(h.host, c"$TestShout$text".as_ptr(), shout, std::ptr::null_mut());
        (h.add_command)(h.host, c"TestFail".as_ptr(), fail, std::ptr::null_mut());
        // refused: the name is taken
        (h.add_command)(h.host, c"%Print".as_ptr(), add, std::ptr::null_mut());
        (h.add_constant_int)(h.host, c"TEST_ANSWER".as_ptr(), 42);
        0
    }

    #[test]
    fn registers_and_calls_commands() {
        let n = register("test-plugin", None, init, &|name| name == "print").unwrap();
        assert_eq!(n, 3);
        let sigs = command_sigs();
        let base = sigs.iter().position(|s| s.name == "testaddints").unwrap();
        assert!(matches!(call(base, &[Value::Int(2), Value::Int(40)]).unwrap(), Value::Int(42)));
        assert_eq!(call(base + 1, &[Value::Str("hi é".into())]).unwrap().str().to_string(), "HI É!");
        assert_eq!(call(base + 2, &[]).unwrap_err(), "it broke");
        assert!(constants().iter().any(|(n, v)| n == "TEST_ANSWER" && *v == ConstVal::Int(42)));
        assert!(registry().lock().unwrap().errors.iter().any(|e| e.contains("already exists")));
    }
}
