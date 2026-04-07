// Jackson Coxson

use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, RsdService, preboard_service::PreboardServiceClient,
    provider::IdeviceProvider,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, core_device_proxy::AdapterHandle, ffi_err,
    provider::IdeviceProviderHandle, rsd::RsdHandshakeHandle, run_sync_local,
};

pub struct PreboardServiceClientHandle(pub PreboardServiceClient);

/// Automatically creates and connects to Preboard Service, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated PreboardServiceClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preboard_service_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut PreboardServiceClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<PreboardServiceClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        PreboardServiceClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(PreboardServiceClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Creates a new PreboardServiceClient via RSD
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`client`] - On success, will be set to point to a newly allocated PreboardServiceClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `handshake` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preboard_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    client: *mut *mut PreboardServiceClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<PreboardServiceClient, IdeviceError> = run_sync_local(async move {
        let provider_ref = unsafe { &mut (*provider).0 };
        let handshake_ref = unsafe { &mut (*handshake).0 };
        PreboardServiceClient::connect_rsd(provider_ref, handshake_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(PreboardServiceClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new PreboardServiceClient from an existing socket
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated PreboardServiceClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preboard_service_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut PreboardServiceClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = PreboardServiceClient::new(socket);
    let boxed = Box::new(PreboardServiceClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Creates a stashbag on the device
///
/// # Arguments
/// * `client` - A valid PreboardServiceClient handle
/// * `manifest` - Pointer to the manifest data
/// * `manifest_len` - Length of the manifest data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `manifest` must be a valid pointer to `manifest_len` bytes of data
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preboard_service_create_stashbag(
    client: *mut PreboardServiceClientHandle,
    manifest: *const u8,
    manifest_len: usize,
) -> *mut IdeviceFfiError {
    if client.is_null() || manifest.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let manifest = unsafe { std::slice::from_raw_parts(manifest, manifest_len) };
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.create_stashbag(manifest).await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Commits a stashbag on the device
///
/// # Arguments
/// * `client` - A valid PreboardServiceClient handle
/// * `manifest` - Pointer to the manifest data
/// * `manifest_len` - Length of the manifest data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `manifest` must be a valid pointer to `manifest_len` bytes of data
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preboard_service_commit_stashbag(
    client: *mut PreboardServiceClientHandle,
    manifest: *const u8,
    manifest_len: usize,
) -> *mut IdeviceFfiError {
    if client.is_null() || manifest.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let manifest = unsafe { std::slice::from_raw_parts(manifest, manifest_len) };
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.commit_stashbag(manifest).await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a PreboardServiceClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn preboard_service_client_free(handle: *mut PreboardServiceClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing PreboardServiceClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
