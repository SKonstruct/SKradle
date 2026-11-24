//! On Windows SKradle is a GUI program, so double-clicking it opens no console window. Started from a terminal with a
//! command-line flag (`--selftest`, `--shots`, ...), it attaches to that terminal so its messages are shown there;
//! when its output is redirected (a file, a pipe, CI) the redirect is already in place and nothing is done.

// The few Win32 calls below have no safe wrapper in the standard library; they stay in this one module under the
// crate's `deny(unsafe_code)`.
#![allow(unsafe_code)]

/// Call first thing in `main`. Does nothing off Windows.
pub fn attach_parent_for_flags() {
    #[cfg(windows)]
    if std::env::args().skip(1).any(|a| a.starts_with("--")) {
        windows::attach_parent();
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn SetStdHandle(which: u32, handle: *mut c_void) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
    }

    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // -11
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4; // -12
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_WRITE: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: isize = -1;

    fn missing(handle: *mut c_void) -> bool {
        handle.is_null() || handle as isize == INVALID_HANDLE_VALUE
    }

    pub fn attach_parent() {
        // SAFETY: plain Win32 calls with valid arguments; the handle made here is deliberately kept open for the life of
        // the process as its standard output and error.
        unsafe {
            let no_out = missing(GetStdHandle(STD_OUTPUT_HANDLE));
            let no_err = missing(GetStdHandle(STD_ERROR_HANDLE));
            if !(no_out || no_err) || AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return;
            }
            let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
            let console = CreateFileW(
                name.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                0,
                null_mut(),
            );
            if missing(console) {
                return;
            }
            if no_out {
                SetStdHandle(STD_OUTPUT_HANDLE, console);
            }
            if no_err {
                SetStdHandle(STD_ERROR_HANDLE, console);
            }
        }
    }
}
