//! Spike: the launcher core exposes a small C interface; the UI layer (C++ Widgets) calls it.
use std::ffi::{c_char, CStr, CString};

/// Returns a heap string the caller must free with `skr_free_string`. Null on invalid input.
#[no_mangle]
pub extern "C" fn skr_describe_save(scale: i32, java_path: *const c_char) -> *mut c_char {
    if java_path.is_null() {
        return std::ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(java_path) }.to_string_lossy();
    let msg = format!("saved: render scale {}% with {} (computed in Rust)", scale, path);
    CString::new(msg).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn skr_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_a_save() {
        let path = CString::new("/usr/bin/java").unwrap();
        let raw = skr_describe_save(150, path.as_ptr());
        assert!(!raw.is_null());
        let text = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_owned();
        skr_free_string(raw);
        assert_eq!(text, "saved: render scale 150% with /usr/bin/java (computed in Rust)");
    }

    #[test]
    fn null_path_is_rejected() {
        assert!(skr_describe_save(100, std::ptr::null()).is_null());
    }
}
