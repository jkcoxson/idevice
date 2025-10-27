// Jackson Coxson

use std::ffi::{CString, c_char};

#[repr(C)]
#[derive(Debug)]
pub struct IdeviceFfiError {
    pub code: i32,
    pub message: *const c_char,
}

/// Frees the IdeviceFfiError
///
/// # Safety
/// `err` must be a struct allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_error_free(err: *mut IdeviceFfiError) {
    if err.is_null() {
        return;
    }
    unsafe {
        // Free the message first
        let _ = CString::from_raw((*err).message as *mut c_char);
        // Then free the struct itself
        let _ = Box::from_raw(err);
    }
}

#[macro_export]
macro_rules! ffi_err {
    ($err:expr) => {{
        use idevice::IdeviceError;
        use std::ffi::CString;
        use $crate::IdeviceFfiError;

        let err: IdeviceError = $err.into();
        let code = err.code();
        let msg = CString::new(format!("{:?}", err))
            .unwrap_or_else(|_| CString::new("invalid error").unwrap());
        let raw_msg = msg.into_raw();

        Box::into_raw(Box::new(IdeviceFfiError {
            code,
            message: raw_msg,
        }))
    }};
}
