// Jackson Coxson

use std::ffi::c_void;

use idevice::{
    IdeviceError, IdeviceService, heartbeat::HeartbeatClient,
    installation_proxy::InstallationProxyClient,
};

use crate::{
    IdeviceErrorCode, IdeviceHandle, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
    util,
};

pub struct HeartbeatClientHandle(pub HeartbeatClient);
#[allow(non_camel_case_types)]
pub struct plist_t;

/// Automatically creates and connects to Installation Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - A TcpProvider
/// * [`client`] - On success, will be set to point to a newly allocated InstallationProxyClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heartbeat_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut HeartbeatClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<HeartbeatClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = HeartbeatClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(HeartbeatClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => {
            // If connection failed, the provider_box was already forgotten,
            // so we need to reconstruct it to avoid leak
            let _ = unsafe { Box::from_raw(provider) };
            e.into()
        }
    }
}

/// Automatically creates and connects to Installation Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - A UsbmuxdProvider
/// * [`client`] - On success, will be set to point to a newly allocated InstallationProxyClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heartbeat_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut HeartbeatClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<HeartbeatClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = HeartbeatClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(HeartbeatClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Automatically creates and connects to Installation Proxy, returning a client handle
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated InstallationProxyClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heartbeat_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut HeartbeatClientHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = HeartbeatClient::new(socket);
    let boxed = Box::new(HeartbeatClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Sends a polo to the device
///
/// # Arguments
/// * `client` - A valid HeartbeatClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heartbeat_send_polo(
    client: *mut HeartbeatClientHandle,
) -> IdeviceErrorCode {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the client
        let mut client_box = unsafe { Box::from_raw(client) };

        // Get a reference to the inner value
        let client_ref = &mut client_box.0;
        let res = client_ref.send_polo().await;

        std::mem::forget(client_box);
        res
    });
    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Sends a polo to the device
///
/// # Arguments
/// * `client` - A valid HeartbeatClient handle
/// * `interval` - The time to wait for a marco
/// * `new_interval` - A pointer to set the requested marco
///
/// # Returns
/// An error code indicating success or failure.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heartbeat_get_marco(
    client: *mut HeartbeatClientHandle,
    interval: u64,
    new_interval: *mut u64,
) -> IdeviceErrorCode {
    let res: Result<u64, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the client
        let mut client_box = unsafe { Box::from_raw(client) };

        // Get a reference to the inner value
        let client_ref = &mut client_box.0;
        let new = client_ref.get_marco(interval).await;

        std::mem::forget(client_box);
        new
    });
    match res {
        Ok(n) => {
            unsafe { *new_interval = n };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees a handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn heartbeat_client_free(handle: *mut HeartbeatClientHandle) {
    if !handle.is_null() {
        log::debug!("Freeing installation_proxy_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
