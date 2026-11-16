//! Launcher PIE minimo: usa il motore già presente nella libreria dell'APK.
#[cfg(target_os = "android")]
fn main() {
    use std::ffi::{c_char, c_int, c_void, CString};
    #[link(name = "dl")]
    extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(lib: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn prctl(option: c_int, ...) -> c_int;
        fn getppid() -> c_int;
    }
    let Some(path) = std::env::args().nth(1).and_then(|s| CString::new(s).ok()) else { std::process::exit(64) };
    unsafe {
        let parent = getppid();
        if prctl(1, 9, 0, 0, 0) != 0 || getppid() != parent { std::process::exit(70); }
        let lib = dlopen(path.as_ptr(), 2);
        if lib.is_null() { std::process::exit(71); }
        let symbol = dlsym(lib, b"cedro_lavoro_main\0".as_ptr().cast());
        if symbol.is_null() { std::process::exit(72); }
        let run: extern "C" fn() -> c_int = std::mem::transmute(symbol);
        std::process::exit(run());
    }
}
#[cfg(not(target_os = "android"))]
fn main() {}
