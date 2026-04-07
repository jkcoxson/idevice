// Jackson Coxson

use std::ffi::{CStr, c_char};
use std::ptr::null_mut;

use idevice::restore_service::RestoreServiceClient;
use idevice::{IdeviceError, RsdService};
use plist_ffi::plist_t;

use crate::{
    IdeviceFfiError, ReadWriteOpaque, core_device_proxy::AdapterHandle, ffi_err,
    rsd::RsdHandshakeHandle, run_sync_local,
};

pub struct RestoreServiceClientHandle(pub RestoreServiceClient);

/// Creates a new RestoreServiceClient from a ReadWrite stream
///
/// # Arguments
/// * [`socket`] - A ReadWriteOpaque handle (consumed)
/// * [`client`] - On success, will be set to point to a newly allocated handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_new(
    socket: *mut ReadWriteOpaque,
    client: *mut *mut RestoreServiceClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) };
    let inner = match socket.inner {
        Some(i) => i,
        None => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<RestoreServiceClient, IdeviceError> =
        run_sync_local(async move { RestoreServiceClient::from_stream(inner).await });

    match res {
        Ok(r) => {
            let boxed = Box::new(RestoreServiceClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new RestoreServiceClient via RSD
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`client`] - On success, will be set to point to a newly allocated RestoreServiceClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `handshake` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    client: *mut *mut RestoreServiceClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<RestoreServiceClient, IdeviceError> = run_sync_local(async move {
        let provider_ref = unsafe { &mut (*provider).0 };
        let handshake_ref = unsafe { &mut (*handshake).0 };
        RestoreServiceClient::connect_rsd(provider_ref, handshake_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(RestoreServiceClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Enters recovery mode on the device
///
/// # Arguments
/// * `client` - A valid RestoreServiceClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_enter_recovery(
    client: *mut RestoreServiceClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.enter_recovery().await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reboots the device
///
/// # Arguments
/// * `client` - A valid RestoreServiceClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_reboot(
    client: *mut RestoreServiceClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.reboot().await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Gets preflight info from the device
///
/// # Arguments
/// * `client` - A valid RestoreServiceClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on success
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_get_preflightinfo(
    client: *mut RestoreServiceClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || res.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let output: Result<plist::Dictionary, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_preflightinfo().await
    });
    match output {
        Ok(dict) => {
            let plist_ptr =
                plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
            unsafe { *res = plist_ptr };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets nonces from the device
///
/// # Arguments
/// * `client` - A valid RestoreServiceClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on success
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_get_nonces(
    client: *mut RestoreServiceClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || res.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let output: Result<plist::Dictionary, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_nonces().await
    });
    match output {
        Ok(dict) => {
            let plist_ptr =
                plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
            unsafe { *res = plist_ptr };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets app parameters from the device
///
/// # Arguments
/// * `client` - A valid RestoreServiceClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on success
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_get_app_parameters(
    client: *mut RestoreServiceClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || res.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let output: Result<plist::Dictionary, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_app_parameters().await
    });
    match output {
        Ok(dict) => {
            let plist_ptr =
                plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
            unsafe { *res = plist_ptr };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Restores the device language
///
/// # Arguments
/// * `client` - A valid RestoreServiceClient handle
/// * `language` - The language to restore to
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `language` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_restore_lang(
    client: *mut RestoreServiceClientHandle,
    language: *const c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || language.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let language = match unsafe { CStr::from_ptr(language) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.restore_lang(language).await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a RestoreServiceClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn restore_service_client_free(handle: *mut RestoreServiceClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing RestoreServiceClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
