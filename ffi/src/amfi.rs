// Jackson Coxson

use idevice::{IdeviceError, IdeviceService, amfi::AmfiClient};

use crate::{
    IdeviceErrorCode, IdeviceHandle, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
};

pub struct AmfiClientHandle(pub AmfiClient);

/// Automatically creates and connects to AMFI service, returning a client handle
///
/// # Arguments
/// * [`provider`] - A TcpProvider
/// * [`client`] - On success, will be set to point to a newly allocated AmfiClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn amfi_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut AmfiClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<AmfiClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = AmfiClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(AmfiClientHandle(r));
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

/// Automatically creates and connects to AMFI service, returning a client handle
///
/// # Arguments
/// * [`provider`] - A UsbmuxdProvider
/// * [`client`] - On success, will be set to point to a newly allocated AmfiClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn amfi_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut AmfiClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<AmfiClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = AmfiClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(AmfiClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Automatically creates and connects to AMFI service, returning a client handle
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated AmfiClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn amfi_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut AmfiClientHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = AmfiClient::new(socket);
    let boxed = Box::new(AmfiClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Shows the option in the settings UI
///
/// # Arguments
/// * `client` - A valid AmfiClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn amfi_reveal_developer_mode_option_in_ui(
    client: *mut AmfiClientHandle,
) -> IdeviceErrorCode {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the client
        let mut client_box = unsafe { Box::from_raw(client) };

        // Get a reference to the inner value
        let client_ref = &mut client_box.0;
        let res = client_ref.reveal_developer_mode_option_in_ui().await;

        std::mem::forget(client_box);
        res
    });
    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Enables developer mode on the device
///
/// # Arguments
/// * `client` - A valid AmfiClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn amfi_enable_developer_mode(
    client: *mut AmfiClientHandle,
) -> IdeviceErrorCode {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the client
        let mut client_box = unsafe { Box::from_raw(client) };

        // Get a reference to the inner value
        let client_ref = &mut client_box.0;
        let res = client_ref.enable_developer_mode().await;

        std::mem::forget(client_box);
        res
    });
    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Accepts developer mode on the device
///
/// # Arguments
/// * `client` - A valid AmfiClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn amfi_accept_developer_mode(
    client: *mut AmfiClientHandle,
) -> IdeviceErrorCode {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the client
        let mut client_box = unsafe { Box::from_raw(client) };

        // Get a reference to the inner value
        let client_ref = &mut client_box.0;
        let res = client_ref.accept_developer_mode().await;

        std::mem::forget(client_box);
        res
    });
    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
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
pub unsafe extern "C" fn amfi_client_free(handle: *mut AmfiClientHandle) {
    if !handle.is_null() {
        log::debug!("Freeing AmfiClient handle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
