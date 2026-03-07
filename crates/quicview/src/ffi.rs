//! Minimal C FFI surface for embedding QuicView in non-Rust hosts.
//!
//! Follows the same opaque-handle pattern used in QuicRTC and QuicSignal.

use std::ffi::CStr;
use std::os::raw::c_char;

/// Opaque handle to a QuicView session.
pub struct QuicViewHandle {
    _private: (),
}

/// Return the library version as a static C string.
///
/// # Safety
///
/// The returned pointer is valid for the lifetime of the process.
#[unsafe(no_mangle)]
pub extern "C" fn quicview_version() -> *const c_char {
    // SAFETY: string literal is NUL-terminated and static.
    c"0.1.0".as_ptr()
}

/// Create a new QuicView handle. The caller must free it with
/// [`quicview_destroy`].
///
/// # Safety
///
/// Returns a valid pointer that must be freed with `quicview_destroy`.
#[unsafe(no_mangle)]
pub extern "C" fn quicview_create() -> *mut QuicViewHandle {
    let handle = Box::new(QuicViewHandle { _private: () });
    Box::into_raw(handle)
}

/// Destroy a handle previously created by [`quicview_create`].
///
/// # Safety
///
/// `handle` must be a pointer returned by `quicview_create` and must not
/// be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn quicview_destroy(handle: *mut QuicViewHandle) {
    if !handle.is_null() {
        // SAFETY: caller guarantees `handle` was allocated by `quicview_create`.
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

/// Return the library version as a Rust `&str`.
///
/// # Safety
///
/// The returned pointer from `quicview_version` must be a valid C string.
pub fn version_str() -> &'static str {
    // SAFETY: `quicview_version` returns a pointer to a static NUL-terminated string.
    unsafe { CStr::from_ptr(quicview_version()) }
        .to_str()
        .expect("version string is valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_version() {
        assert_eq!(version_str(), "0.1.0");
    }

    #[test]
    fn ffi_create_destroy() {
        let h = quicview_create();
        assert!(!h.is_null());
        unsafe { quicview_destroy(h) };
    }
}
