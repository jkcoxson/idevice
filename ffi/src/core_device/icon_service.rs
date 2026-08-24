// Jackson Coxson

use std::ffi::{CStr, c_char};
use std::os::raw::{c_float, c_int};
use std::ptr::null_mut;

use idevice::core_device::{AppIcon, AppIconTarget, IconServiceClient};
use idevice::{IdeviceError, ReadWrite};

use crate::{IdeviceFfiError, ReadWriteOpaque, ffi_err, run_sync, run_sync_local};
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
use crate::{core_device_proxy::AdapterHandle, rsd::RsdHandshakeHandle};
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
use idevice::RsdService as _;

/// Opaque handle to an IconServiceClient
pub struct IconServiceHandle(pub IconServiceClient<Box<dyn ReadWrite>>);

/// A rendered app icon
#[repr(C)]
pub struct AppIconC {
    /// PNG-encoded image data
    pub png_data: *mut u8,
    pub png_data_len: usize,
    /// Icon dimensions in pixels, i.e. the points multiplied by the scale
    pub pixel_width: f64,
    pub pixel_height: f64,
    /// Icon dimensions in points, as actually rendered. May be smaller than
    /// what was requested.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    /// 1 when the device had no real icon for the app and rendered a generic
    /// placeholder instead
    pub is_placeholder: c_int,
}

fn icon_to_c(icon: AppIcon) -> *mut AppIconC {
    let data: Vec<u8> = icon.png_data.into();
    // Boxing shrinks the capacity to the length, so the caller can free it as a
    // len == capacity Vec.
    let mut data = data.into_boxed_slice();
    let png_data_len = data.len();
    let png_data = data.as_mut_ptr();
    std::mem::forget(data);

    Box::into_raw(Box::new(AppIconC {
        png_data,
        png_data_len,
        pixel_width: icon.pixel_size.0,
        pixel_height: icon.pixel_size.1,
        width: icon.size.0,
        height: icon.size.1,
        scale: icon.scale,
        is_placeholder: icon.is_placeholder as c_int,
    }))
}

/// Creates a new IconServiceClient using RSD connection
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` and `handshake` must be valid pointers to handles allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn icon_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut IconServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<IconServiceClient<Box<dyn ReadWrite>>, IdeviceError> = run_sync_local(async {
        let provider_ref = unsafe { &mut (*provider).0 };
        let handshake_ref = unsafe { &mut (*handshake).0 };

        IconServiceClient::connect_rsd(provider_ref, handshake_ref).await
    });

    match res {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(IconServiceHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new IconServiceClient from a socket
///
/// # Arguments
/// * [`socket`] - The socket to use for communication. Consumed regardless of the result.
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn icon_service_new(
    socket: *mut ReadWriteOpaque,
    handle: *mut *mut IconServiceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket = unsafe { Box::from_raw(socket) };
    let res = run_sync(async move { IconServiceClient::new(socket.inner.unwrap()).await });

    match res {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(IconServiceHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Fetches an app's icon, rendered as a PNG
///
/// # Arguments
/// * [`handle`] - The IconServiceClient handle
/// * [`bundle_identifier`] - Bundle identifier of the app, or NULL to use `app_path`
/// * [`app_path`] - Path of the app on the device, or NULL to use `bundle_identifier`
/// * [`width`] - Requested icon width in points
/// * [`height`] - Requested icon height in points
/// * [`scale`] - Requested icon scale
/// * [`allow_placeholder`] - Whether the device may render a generic placeholder
/// * [`icon`] - Pointer to store the icon, freed with `icon_service_free_icon`
///
/// Exactly one of `bundle_identifier` and `app_path` must be passed.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All non-NULL pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn icon_service_fetch_icon(
    handle: *mut IconServiceHandle,
    bundle_identifier: *const c_char,
    app_path: *const c_char,
    width: c_float,
    height: c_float,
    scale: c_float,
    allow_placeholder: c_int,
    icon: *mut *mut AppIconC,
) -> *mut IdeviceFfiError {
    if handle.is_null() || icon.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let target = match (bundle_identifier.is_null(), app_path.is_null()) {
        (false, true) => match unsafe { CStr::from_ptr(bundle_identifier) }.to_str() {
            Ok(s) => AppIconTarget::BundleIdentifier(s.to_string()),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        },
        (true, false) => match unsafe { CStr::from_ptr(app_path) }.to_str() {
            Ok(s) => AppIconTarget::AppPath(s.to_string()),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        },
        // Neither or both passed
        _ => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move {
        client
            .fetch_icon(target, width, height, scale, allow_placeholder != 0)
            .await
    });

    match res {
        Ok(i) => {
            unsafe { *icon = icon_to_c(i) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an AppIconC
///
/// # Safety
/// `icon` must be a pointer returned by `icon_service_fetch_icon`, or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn icon_service_free_icon(icon: *mut AppIconC) {
    if icon.is_null() {
        return;
    }
    let icon = unsafe { Box::from_raw(icon) };
    if !icon.png_data.is_null() {
        let _ = unsafe { Vec::from_raw_parts(icon.png_data, icon.png_data_len, icon.png_data_len) };
    }
}

/// Frees an IconServiceClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn icon_service_free(handle: *mut IconServiceHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
