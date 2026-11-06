//! Loading shared libraries (`.dll`, `.so`, `.dylib`) on Windows, Linux and
//! macOS without extra crates. Libraries stay loaded for the life of the
//! program (their functions may be called at any time).

use std::ffi::{c_void, CString};

pub struct Library {
    handle: *mut c_void,
    pub path: String,
}

// The handle is only used to look up symbols, which the OS allows from any thread.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        pub fn LoadLibraryW(name: *const u16) -> *mut c_void;
        pub fn GetProcAddress(module: *mut c_void, name: *const std::ffi::c_char) -> *mut c_void;
        pub fn GetLastError() -> u32;
    }
}

#[cfg(unix)]
mod os {
    use std::ffi::{c_char, c_int, c_void};
    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    extern "C" {
        pub fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        pub fn dlerror() -> *mut c_char;
    }
    pub const RTLD_NOW: c_int = 2;
}

impl Library {
    /// Opens a library by path (or by name, using the system's search rules).
    pub fn open(path: &str) -> Result<Library, String> {
        #[cfg(windows)]
        {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: a NUL-terminated wide string
            let handle = unsafe { os::LoadLibraryW(wide.as_ptr()) };
            if handle.is_null() {
                // SAFETY: reads the calling thread's last error code
                let code = unsafe { os::GetLastError() };
                return Err(format!("cannot load '{path}' (Windows error {code})"));
            }
            Ok(Library { handle, path: path.to_string() })
        }
        #[cfg(unix)]
        {
            let c = CString::new(path).map_err(|_| "bad library path".to_string())?;
            // SAFETY: a NUL-terminated path; dlerror returns a C string or null
            unsafe {
                let handle = os::dlopen(c.as_ptr(), os::RTLD_NOW);
                if handle.is_null() {
                    let e = os::dlerror();
                    let msg = if e.is_null() { "unknown error".to_string() } else { std::ffi::CStr::from_ptr(e).to_string_lossy().into_owned() };
                    return Err(format!("cannot load '{path}': {msg}"));
                }
                Ok(Library { handle, path: path.to_string() })
            }
        }
    }

    /// Address of an exported function, or `None`.
    pub fn symbol(&self, name: &str) -> Option<*mut c_void> {
        let c = CString::new(name).ok()?;
        // SAFETY: the handle is a loaded library and the name is NUL-terminated
        let p = unsafe {
            #[cfg(windows)]
            {
                os::GetProcAddress(self.handle, c.as_ptr())
            }
            #[cfg(unix)]
            {
                os::dlsym(self.handle, c.as_ptr())
            }
        };
        (!p.is_null()).then_some(p)
    }
}

/// The file extension shared libraries use on this platform.
pub fn extension() -> &'static str {
    if cfg!(windows) {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_system_library() {
        let name = if cfg!(windows) {
            "kernel32.dll"
        } else if cfg!(target_os = "macos") {
            "/usr/lib/libSystem.B.dylib"
        } else {
            "libc.so.6"
        };
        let lib = Library::open(name).unwrap();
        let sym = if cfg!(windows) { "GetTickCount" } else { "strlen" };
        assert!(lib.symbol(sym).is_some());
        assert!(lib.symbol("no_such_function_anywhere").is_none());
        assert!(Library::open("no_such_library_here").is_err());
    }
}
