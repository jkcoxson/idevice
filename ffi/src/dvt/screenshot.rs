// Jackson Coxson

use std::ptr::null_mut;

use idevice::{ReadWrite, dvt::screenshot::ScreenshotClient};

use crate::{IdeviceFfiError, dvt::remote_server::RemoteServerHandle, ffi_err, run_sync};

/// An opaque FFI handle for a [`ScreenshotClient`].
///
/// This type wraps a [`ScreenshotClient`] that communicates with
/// a connected device to capture screenshots through the DVT (Device Virtualization Toolkit) service.
pub struct ScreenshotClientHandle<'a>(pub ScreenshotClient<'a, Box<dyn ReadWrite>>);

/// Creates a new [`ScreenshotClient`] associated with a given [`RemoteServerHandle`].
///
/// # Arguments
/// * `server` - A pointer to a valid [`RemoteServerHandle`], previously created by this library.
/// * `handle` - A pointer to a location where the newly created [`ScreenshotClientHandle`] will be stored.
///
/// # Returns
/// * `null_mut()` on success.
/// * A pointer to an [`IdeviceFfiError`] on failure.
///
/// # Safety
/// - `server` must be a non-null pointer to a valid remote server handle allocated by this library.
/// - `handle` must be a non-null pointer to a writable memory location where the handle will be stored.
/// - The returned handle must later be freed using [`screenshot_client_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshot_client_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut ScreenshotClientHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = run_sync(async move { ScreenshotClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(ScreenshotClientHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a [`ScreenshotClientHandle`].
///
/// This releases all memory associated with the handle.  
/// After calling this function, the handle pointer must not be used again.
///
/// # Arguments
/// * `handle` - Pointer to a [`ScreenshotClientHandle`] previously returned by [`screenshot_client_new`].
///
/// # Safety
/// - `handle` must either be `NULL` or a valid pointer created by this library.
/// - Double-freeing or using the handle after freeing causes undefined behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshot_client_free(handle: *mut ScreenshotClientHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Captures a screenshot from the connected device.
///
/// On success, this function writes a pointer to the PNG-encoded screenshot data and its length
/// into the provided output arguments. The caller is responsible for freeing this data using
/// `idevice_data_free`.
///
/// # Arguments
/// * `handle` - A pointer to a valid [`ScreenshotClientHandle`].
/// * `data` - Output pointer where the screenshot buffer pointer will be written.
/// * `len` - Output pointer where the buffer length (in bytes) will be written.
///
/// # Returns
/// * `null_mut()` on success.
/// * A pointer to an [`IdeviceFfiError`] on failure.
///
/// # Safety
/// - `handle` must be a valid pointer to a [`ScreenshotClientHandle`].
/// - `data` and `len` must be valid writable pointers.
/// - The data returned through `*data` must be freed by the caller with `idevice_data_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshot_client_take_screenshot(
    handle: *mut ScreenshotClientHandle<'static>,
    data: *mut *mut u8,
    len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() || len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.take_screenshot().await });

    match res {
        Ok(r) => {
            let mut r = r.into_boxed_slice();
            unsafe {
                *data = r.as_mut_ptr();
                *len = r.len();
            }
            std::mem::forget(r); // Prevent Rust from freeing the buffer
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
