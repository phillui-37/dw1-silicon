//! Trapping stubs for SDK calls that aren't implemented yet.
//!
//! The game C links against every SDK function it calls from the start (Phase 1); the ones
//! nobody has written yet stop the program with their name instead of doing something
//! plausible. A stub takes no arguments: on arm64 the caller's arguments sit unread in
//! registers, and nothing returns, so the C prototype doesn't matter.

/// Called with the message just before the program stops (`stop`, `unimplemented`): the app
/// shows it (a message box when there's no terminal). Must not take the GPU, GTE or drive locks.
static STOP_HOOK: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Sets what `stop` calls before stopping (once; later calls are ignored).
pub fn set_stop_hook(hook: fn(&str)) {
    let _ = STOP_HOOK.set(hook);
}

/// Stops the program with `message` (printed to stderr, then shown by the app's hook): the PS1
/// layer met something it doesn't handle, and going on would be guessing.
#[cold]
pub fn stop(message: &str) -> ! {
    eprintln!("{message}");
    if let Some(hook) = STOP_HOOK.get() {
        hook(message);
    }
    std::process::abort();
}

/// Stops the program at an SDK call with no implementation yet.
#[cold]
pub fn unimplemented(name: &str) -> ! {
    let message =
        format!("psyq: unimplemented SDK call `{name}` (trapping stub; see crates/psyq/src/stubs.rs)");
    eprintln!("{message}");
    // Which game code asked: on macOS, execinfo's symbolised frames; elsewhere the
    // standard library's backtrace prints the same callers.
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn backtrace(buffer: *mut *mut std::ffi::c_void, size: i32) -> i32;
            fn backtrace_symbols_fd(buffer: *const *mut std::ffi::c_void, size: i32, fd: i32);
        }
        let mut frames = [std::ptr::null_mut(); 12];
        unsafe {
            let n = backtrace(frames.as_mut_ptr(), frames.len() as i32);
            // Skip this function and the stub.
            if n > 2 {
                backtrace_symbols_fd(frames.as_ptr().add(2), n - 2, 2);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let trace = std::backtrace::Backtrace::force_capture();
        eprintln!("{trace}");
    }
    if let Some(hook) = STOP_HOOK.get() {
        hook(&message);
    }
    std::process::abort();
}

/// Defines each named function as an `extern "C"` trapping stub with the C name.
macro_rules! stubs {
    ($($name:ident)*) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub extern "C" fn $name() {
                $crate::trap::unimplemented(stringify!($name));
            }
        )*
    };
}

pub(crate) use stubs;

/// `dw1_native_fatal(what)`: the game C's native-only code (under `DW1_NATIVE`) hit a limit the
/// PS1 code doesn't have, such as a side table's size; stops the program like a trapping stub.
///
/// # Safety
/// `what` must be a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dw1_native_fatal(what: *const std::ffi::c_char) -> ! {
    let what = unsafe { std::ffi::CStr::from_ptr(what) }.to_string_lossy();
    unimplemented(&format!("native limit: {what}"));
}

