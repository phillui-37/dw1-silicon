//! Desktop launches do not allocate a console. A terminal launch can reuse its parent,
//! and `--console` requests a new console when there is no parent terminal.

#[cfg(not(windows))]
pub fn init_console(_requested: bool) {}

#[cfg(windows)]
pub fn init_console(requested: bool) {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
        fn AllocConsole() -> i32;
        fn GetStdHandle(which: u32) -> Handle;
        fn SetStdHandle(which: u32, handle: Handle) -> i32;
        fn GetFileType(handle: Handle) -> u32;
    }
    unsafe extern "C" {
        fn __acrt_iob_func(index: u32) -> *mut c_void;
        fn freopen_s(
            reopened: *mut *mut c_void,
            filename: *const std::ffi::c_char,
            mode: *const std::ffi::c_char,
            stream: *mut c_void,
        ) -> i32;
    }
    // Keep inherited files and pipes: attaching a console replaces the standard handles.
    let std_handles = [(-11i32) as u32, (-12i32) as u32];
    unsafe {
        let inherited = std_handles.map(|which| {
            let handle = GetStdHandle(which);
            let kind = GetFileType(handle);
            (handle, kind == 1 || kind == 3) // FILE_TYPE_DISK / FILE_TYPE_PIPE
        });
        let attached = AttachConsole(u32::MAX) != 0 || (requested && AllocConsole() != 0);
        for (index, which) in std_handles.into_iter().enumerate() {
            let (handle, redirected) = inherited[index];
            if redirected {
                SetStdHandle(which, handle);
            } else if attached {
                // The GUI subsystem starts without CRT stdout/stderr descriptors. Reopen
                // these streams too, so the game's C printf and Rust output both work.
                let mut reopened = std::ptr::null_mut();
                freopen_s(
                    &mut reopened,
                    c"CONOUT$".as_ptr(),
                    c"w".as_ptr(),
                    __acrt_iob_func(index as u32 + 1),
                );
            }
        }
    }
}
