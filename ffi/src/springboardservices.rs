use std::{
    ffi::{CStr, c_void},
    ptr::null_mut,
};

use idevice::{
    IdeviceError, IdeviceService, provider::IdeviceProvider,
    springboardservices::SpringBoardServicesClient,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync,
    run_sync_local,
};

pub struct SpringBoardServicesClientHandle(pub SpringBoardServicesClient);

/// Connects to the Springboard service using a provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated SpringBoardServicesClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut SpringBoardServicesClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<SpringBoardServicesClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        SpringBoardServicesClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(SpringBoardServicesClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            // If connection failed, the provider_box was already forgotten,
            // so we need to reconstruct it to avoid leak
            let _ = unsafe { Box::from_raw(provider) };
            ffi_err!(e)
        }
    }
}

/// Creates a new SpringBoardServices client from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated SpringBoardServicesClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut SpringBoardServicesClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = SpringBoardServicesClient::new(socket);
    let boxed = Box::new(SpringBoardServicesClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Gets the icon of the specified app by bundle identifier
///
/// # Arguments
/// * `client` - A valid SpringBoardServicesClient handle
/// * `bundle_identifier` - The identifiers of the app to get icon
/// * `out_result` - On success, will be set to point to a newly allocated png data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_result` must be a valid, non-null pointer to a location where the result will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_get_icon(
    client: *mut SpringBoardServicesClientHandle,
    bundle_identifier: *const libc::c_char,
    out_result: *mut *mut c_void,
    out_result_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        tracing::error!("Invalid arguments: {client:?}, {out_result:?}");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let client = unsafe { &mut *client };

    let name_cstr = unsafe { CStr::from_ptr(bundle_identifier) };
    let bundle_id = match name_cstr.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<Vec<u8>, IdeviceError> =
        run_sync(async { client.0.get_icon_pngdata(bundle_id).await });

    match res {
        Ok(r) => {
            let len = r.len();
            let boxed_slice = r.into_boxed_slice();
            let ptr = boxed_slice.as_ptr();
            std::mem::forget(boxed_slice);

            unsafe {
                *out_result = ptr as *mut c_void;
                *out_result_len = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets the home screen wallpaper preview as PNG image
///
/// # Arguments
/// * `client` - A valid SpringBoardServicesClient handle
/// * `out_result` - On success, will be set to point to newly allocated png image
/// * `out_result_len` - On success, will contain the size of the data in bytes
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_result` and `out_result_len` must be valid, non-null pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_get_home_screen_wallpaper_preview(
    client: *mut SpringBoardServicesClientHandle,
    out_result: *mut *mut c_void,
    out_result_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        tracing::error!("Invalid arguments: {client:?}, {out_result:?}");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let client = unsafe { &mut *client };

    let res: Result<Vec<u8>, IdeviceError> =
        run_sync(async { client.0.get_home_screen_wallpaper_preview_pngdata().await });

    match res {
        Ok(r) => {
            let len = r.len();
            let boxed_slice = r.into_boxed_slice();
            let ptr = boxed_slice.as_ptr();
            std::mem::forget(boxed_slice);

            unsafe {
                *out_result = ptr as *mut c_void;
                *out_result_len = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets the lock screen wallpaper preview as PNG image
///
/// # Arguments
/// * `client` - A valid SpringBoardServicesClient handle
/// * `out_result` - On success, will be set to point to newly allocated png image
/// * `out_result_len` - On success, will contain the size of the data in bytes
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_result` and `out_result_len` must be valid, non-null pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_get_lock_screen_wallpaper_preview(
    client: *mut SpringBoardServicesClientHandle,
    out_result: *mut *mut c_void,
    out_result_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        tracing::error!("Invalid arguments: {client:?}, {out_result:?}");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let client = unsafe { &mut *client };

    let res: Result<Vec<u8>, IdeviceError> =
        run_sync(async { client.0.get_lock_screen_wallpaper_preview_pngdata().await });

    match res {
        Ok(r) => {
            let len = r.len();
            let boxed_slice = r.into_boxed_slice();
            let ptr = boxed_slice.as_ptr();
            std::mem::forget(boxed_slice);

            unsafe {
                *out_result = ptr as *mut c_void;
                *out_result_len = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Get device orientation
///
/// # Arguments
/// * `client` - A valid SpringBoardServicesClient handle
/// * `out_orientation` - On success, will contain the orientation value (0-4)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_orientation` must be a valid, non-null pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_get_interface_orientation(
    client: *mut SpringBoardServicesClientHandle,
    out_orientation: *mut u8,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_orientation.is_null() {
        tracing::error!("Invalid arguments: {client:?}, {out_orientation:?}");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let client = unsafe { &mut *client };

    let res = run_sync(async { client.0.get_interface_orientation().await });

    match res {
        Ok(orientation) => {
            unsafe {
                *out_orientation = orientation as u8;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an SpringBoardServicesClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_free(handle: *mut SpringBoardServicesClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing springboard_services_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
