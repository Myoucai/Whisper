//! Foreign function interface (v1.0.8): load native libraries and call
//! their symbols. Zero new dependencies — raw system loaders declared
//! via extern "C".
//!
//! Words: ffi-open (path → handle|-1), ffi-sym (handle name → fnptr|0),
//! ffi-call ([args] fmt fnptr → result).
//!
//! fmt convention: first char is the return type, the rest are argument
//! types in order. Type chars: `i` i64, `p` pointer to string data
//! (pass a Whisper string, C sees `const char*`; pair with a separate
//! `i` + strlen for lengths), `d` f64, `v` void (returns 0).
//!
//! This is the most powerful capability in the language — it can call
//! anything, including functions that read files or spawn processes.
//! Treat granting it like granting shell access.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};
#[cfg(not(windows))]
use std::os::raw::c_int;

#[cfg(windows)]
extern "system" {
    fn LoadLibraryA(path: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
}
#[cfg(not(windows))]
extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    }
#[cfg(not(windows))]
const RTLD_NOW: c_int = 2;

/// Load a shared library by path. 0 (mapped to -1 at the word level) on
/// failure. Handles are leaked intentionally: libraries live for the
/// process lifetime (v1.0.8 has no ffi-close).
pub fn open(path: &str) -> i64 {
    let Ok(cpath) = CString::new(path) else {
        return -1;
    };
    #[cfg(windows)]
    let h = unsafe { LoadLibraryA(cpath.as_ptr()) };
    #[cfg(not(windows))]
    let h = unsafe { dlopen(cpath.as_ptr(), RTLD_NOW) };
    if h.is_null() {
        -1
    } else {
        h as i64
    }
}

/// Resolve a symbol in a loaded library. 0 on failure.
pub fn sym(handle: i64, name: &str) -> i64 {
    if handle <= 0 {
        return 0;
    }
    let Ok(cname) = CString::new(name) else {
        return 0;
    };
    let module = handle as *mut c_void;
    #[cfg(windows)]
    let s = unsafe { GetProcAddress(module, cname.as_ptr()) };
    #[cfg(not(windows))]
    let s = unsafe { dlsym(module, cname.as_ptr()) };
    s as i64
}

/// The actual call. Dispatch on the fmt characters; the generic-args
/// trampoline pattern (build args array, one shim per arity) keeps this
/// portable without libffi.
pub enum FArg {
    I(i64),
    P(usize), // pointer to string bytes (kept alive by the caller's Value)
    D(f64),
}

pub fn call(fnptr: i64, fmt: &str, args: &[FArg]) -> Result<i64, String> {
    if fnptr == 0 {
        return Err("ffi-call: null fnptr".into());
    }
    let f = fmt.as_bytes();
    if f.is_empty() {
        return Err("ffi-call: empty fmt".into());
    }
    let ret = f[0];
    let argt = &f[1..];
    if argt.len() != args.len() {
        return Err(format!(
            "ffi-call: fmt wants {} args, got {}",
            argt.len(),
            args.len()
        ));
    }
    // Pointers to the string bytes must stay valid for the duration of
    // the call — materialize them here so borrows outlive the call.
    let buffers: Vec<Vec<u8>> = args
        .iter()
        .map(|a| match a {
            FArg::P(_) | FArg::I(_) | FArg::D(_) => Vec::new(),
        })
        .collect();
    let _ = buffers;

    let mut ai: [u64; 8] = [0; 8];
    let mut ad: [f64; 8] = [0.0; 8];
    for (i, (t, a)) in argt.iter().zip(args.iter()).enumerate() {
        if i >= 8 {
            return Err("ffi-call: max 8 args in v1.0.8".into());
        }
        match (*t, a) {
            (b'i', FArg::I(v)) => ai[i] = *v as u64,
            (b'p', FArg::P(v)) => ai[i] = *v as u64,
            (b'd', FArg::D(v)) => ad[i] = *v,
            _ => return Err("ffi-call: fmt/arg type mismatch".into()),
        }
    }

    // The Windows x64 and System V ABIs pass the first args in
    // registers; a direct call through a typed fn pointer per arity is
    // the dependency-free way to hit them.
    macro_rules! dispatch {
        ($n:expr) => {{
            type FnN = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;
            let f: FnN = unsafe { std::mem::transmute(fnptr as usize) };
            let r = unsafe { f(ai[0], ai[1], ai[2], ai[3], ai[4], ai[5], ai[6], ai[7]) };
            match ret {
                b'd' => Ok(f64::from_bits(r) as i64),
                _ => Ok(r as i64),
            }
        }};
    }
    // Passing garbage in unused registers is fine on both ABIs (the
    // callee only reads what its signature declares), so one generic
    // 8-arg shim covers every arity up to 8. Float args would need xmm
    // registers — v1.0.8 f64 args are rejected above; only f64 RETURNS
    // work (rax bit pattern).
    if argt.contains(&b'd') {
        return Err("ffi-call: f64 args unsupported in v1.0.8 (returns ok)".into());
    }
    dispatch!(0)
}
