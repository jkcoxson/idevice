// Jackson Coxson

use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, companion_proxy::CompanionProxy, provider::IdeviceProvider,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync_local,
};

pub struct CompanionProxyClientHandle(pub CompanionProxy);

/// Automatically creates and connects to Companion Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated CompanionProxy handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn companion_proxy_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut CompanionProxyClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<CompanionProxy, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        CompanionProxy::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(CompanionProxyClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Creates a new CompanionProxy client from an existing socket
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated CompanionProxy handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn companion_proxy_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut CompanionProxyClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = CompanionProxy::new(socket);
    let boxed = Box::new(CompanionProxyClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Gets the device registry from Companion Proxy, returning paired watch UDIDs
///
/// # Arguments
/// * `client` - A valid CompanionProxy handle
/// * `udids` - On success, will be set to point to a newly allocated array of C strings
/// * `udids_len` - On success, will be set to the length of the array
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// The returned strings must be freed with `idevice_string_free` and the outer array
/// with `idevice_outer_slice_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn companion_proxy_get_device_registry(
    client: *mut CompanionProxyClientHandle,
    udids: *mut *mut *mut std::ffi::c_char,
    udids_len: *mut usize,
) -> *mut IdeviceFfiError {
    if client.is_null() || udids.is_null() || udids_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<Vec<String>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_device_registry().await
    });
    match res {
        Ok(list) => {
            let mut c_strings: Vec<*mut std::ffi::c_char> = list
                .into_iter()
                .filter_map(|s| std::ffi::CString::new(s).ok().map(|cs| cs.into_raw()))
                .collect();
            let len = c_strings.len();
            let ptr = c_strings.as_mut_ptr();
            std::mem::forget(c_strings);
            unsafe {
                *udids = ptr;
                *udids_len = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Starts forwarding a service port through the companion proxy
///
/// # Arguments
/// * `client` - A valid CompanionProxy handle
/// * `port` - The remote port number on the watch
/// * `local_port` - On success, will be set to the local forwarded port number
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn companion_proxy_start_forwarding_service_port(
    client: *mut CompanionProxyClientHandle,
    port: u16,
    local_port: *mut u16,
) -> *mut IdeviceFfiError {
    if client.is_null() || local_port.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<u16, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .start_forwarding_service_port(port, None, None)
            .await
    });
    match res {
        Ok(p) => {
            unsafe { *local_port = p };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Stops forwarding a service port through the companion proxy
///
/// # Arguments
/// * `client` - A valid CompanionProxy handle
/// * `port` - The remote port number to stop forwarding
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn companion_proxy_stop_forwarding_service_port(
    client: *mut CompanionProxyClientHandle,
    port: u16,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.stop_forwarding_service_port(port).await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a CompanionProxy client handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn companion_proxy_client_free(handle: *mut CompanionProxyClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing CompanionProxyClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
