// Jackson Coxson

use std::{
    ffi::{CStr, c_char},
    ptr::null_mut,
};

use idevice::{
    IdeviceError, IdeviceService, afc::AfcClient, house_arrest::HouseArrestClient,
    provider::IdeviceProvider,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, afc::AfcClientHandle, ffi_err, provider::IdeviceProviderHandle,
    run_sync_local,
};

pub struct HouseArrestClientHandle(pub HouseArrestClient);

/// Connects to the House Arrest service using a TCP provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated HouseArrestClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn house_arrest_client_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut HouseArrestClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };

        HouseArrestClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(HouseArrestClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new HouseArrestClient from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated HouseArrestClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn house_arrest_client_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut HouseArrestClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = HouseArrestClient::new(socket);
    let boxed = Box::new(HouseArrestClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Vends a container for an app
///
/// # Arguments
/// * [`client`] - The House Arrest client
/// * [`bundle_id`] - The bundle ID to vend for
/// * [`afc_client`] - The new AFC client for the underlying connection
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a allocated by this library
/// `bundle_id` must be a NULL-terminated string
/// `afc_client` must be a valid, non-null pointer where the new AFC client will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn house_arrest_vend_container(
    client: *mut HouseArrestClientHandle,
    bundle_id: *const c_char,
    afc_client: *mut *mut AfcClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() || bundle_id.is_null() || afc_client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id = unsafe { CStr::from_ptr(bundle_id) }.to_string_lossy();
    let client_ref = unsafe { Box::from_raw(client) }.0; // take ownership and drop

    let res: Result<AfcClient, IdeviceError> =
        run_sync_local(async move { client_ref.vend_container(bundle_id).await });

    match res {
        Ok(a) => {
            let a = Box::into_raw(Box::new(AfcClientHandle(a)));
            unsafe { *afc_client = a };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Vends documents for an app
///
/// # Arguments
/// * [`client`] - The House Arrest client
/// * [`bundle_id`] - The bundle ID to vend for
/// * [`afc_client`] - The new AFC client for the underlying connection
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a allocated by this library
/// `bundle_id` must be a NULL-terminated string
/// `afc_client` must be a valid, non-null pointer where the new AFC client will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn house_arrest_vend_documents(
    client: *mut HouseArrestClientHandle,
    bundle_id: *const c_char,
    afc_client: *mut *mut AfcClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() || bundle_id.is_null() || afc_client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id = unsafe { CStr::from_ptr(bundle_id) }.to_string_lossy();
    let client_ref = unsafe { Box::from_raw(client) }.0; // take ownership and drop

    let res: Result<AfcClient, IdeviceError> =
        run_sync_local(async move { client_ref.vend_documents(bundle_id).await });

    match res {
        Ok(a) => {
            let a = Box::into_raw(Box::new(AfcClientHandle(a)));
            unsafe { *afc_client = a };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Frees an HouseArrestClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn house_arrest_client_free(handle: *mut HouseArrestClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing house_arrest_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
