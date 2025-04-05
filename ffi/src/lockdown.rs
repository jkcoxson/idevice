// Jackson Coxson

use std::ffi::c_void;

use idevice::{IdeviceError, IdeviceService, lockdown::LockdowndClient};

use crate::{
    IdeviceErrorCode, IdeviceHandle, IdevicePairingFile, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
};

pub struct LockdowndClientHandle(pub LockdowndClient);

/// Connects to lockdownd service using TCP provider
///
/// # Arguments
/// * [`provider`] - A TcpProvider
/// * [`client`] - On success, will be set to point to a newly allocated LockdowndClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut LockdowndClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<LockdowndClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_box = unsafe { Box::from_raw(provider) };
        let provider_ref = &provider_box.0;
        let result = LockdowndClient::connect(provider_ref).await;
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LockdowndClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => {
            let _ = unsafe { Box::from_raw(provider) };
            e.into()
        }
    }
}

/// Connects to lockdownd service using Usbmuxd provider
///
/// # Arguments
/// * [`provider`] - A UsbmuxdProvider
/// * [`client`] - On success, will be set to point to a newly allocated LockdowndClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut LockdowndClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<LockdowndClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_box = unsafe { Box::from_raw(provider) };
        let provider_ref = &provider_box.0;
        let result = LockdowndClient::connect(provider_ref).await;
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LockdowndClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Creates a new LockdowndClient from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated LockdowndClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut LockdowndClientHandle,
) -> IdeviceErrorCode {
    if socket.is_null() || client.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = LockdowndClient::new(socket);
    let boxed = Box::new(LockdowndClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Starts a session with lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `pairing_file` - An IdevicePairingFile alocated by this library
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `pairing_file` must be a valid plist_t containing a pairing file
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_start_session(
    client: *mut LockdowndClientHandle,
    pairing_file: *mut IdevicePairingFile,
) -> IdeviceErrorCode {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let mut client_box = unsafe { Box::from_raw(client) };
        let pairing_file = unsafe { Box::from_raw(pairing_file) };

        let client_ref = &mut client_box.0;
        let res = client_ref.start_session(&pairing_file.0).await;

        std::mem::forget(client_box);
        std::mem::forget(pairing_file);
        res
    });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Starts a service through lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `identifier` - The service identifier to start (null-terminated string)
/// * `port` - Pointer to store the returned port number
/// * `ssl` - Pointer to store whether SSL should be enabled
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `identifier` must be a valid null-terminated string
/// `port` and `ssl` must be valid pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_start_service(
    client: *mut LockdowndClientHandle,
    identifier: *const libc::c_char,
    port: *mut u16,
    ssl: *mut bool,
) -> IdeviceErrorCode {
    if identifier.is_null() || port.is_null() || ssl.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let identifier = unsafe { std::ffi::CStr::from_ptr(identifier) }
        .to_string_lossy()
        .into_owned();

    let res: Result<(u16, bool), IdeviceError> = RUNTIME.block_on(async move {
        let mut client_box = unsafe { Box::from_raw(client) };
        let client_ref = &mut client_box.0;
        let res = client_ref.start_service(identifier).await;
        std::mem::forget(client_box);
        res
    });

    match res {
        Ok((p, s)) => {
            unsafe {
                *port = p;
                *ssl = s;
            }
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Gets a value from lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `value` - The value to get (null-terminated string)
/// * `out_plist` - Pointer to store the returned plist value
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `value` must be a valid null-terminated string
/// `out_plist` must be a valid pointer to store the plist
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_get_value(
    client: *mut LockdowndClientHandle,
    value: *const libc::c_char,
    out_plist: *mut *mut c_void,
) -> IdeviceErrorCode {
    if value.is_null() || out_plist.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let value = unsafe { std::ffi::CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();

    let res: Result<plist::Value, IdeviceError> = RUNTIME.block_on(async move {
        let mut client_box = unsafe { Box::from_raw(client) };
        let client_ref = &mut client_box.0;
        let res = client_ref.get_value(value).await;
        std::mem::forget(client_box);
        res
    });

    match res {
        Ok(value) => {
            unsafe {
                *out_plist = crate::util::plist_to_libplist(&value);
            }
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Gets all values from lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `out_plist` - Pointer to store the returned plist dictionary
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_plist` must be a valid pointer to store the plist
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_get_all_values(
    client: *mut LockdowndClientHandle,
    out_plist: *mut *mut c_void,
) -> IdeviceErrorCode {
    if out_plist.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<plist::Dictionary, IdeviceError> = RUNTIME.block_on(async move {
        let mut client_box = unsafe { Box::from_raw(client) };
        let client_ref = &mut client_box.0;
        let res = client_ref.get_all_values().await;
        std::mem::forget(client_box);
        res
    });

    match res {
        Ok(dict) => {
            unsafe {
                *out_plist = crate::util::plist_to_libplist(&plist::Value::Dictionary(dict));
            }
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees a LockdowndClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_client_free(handle: *mut LockdowndClientHandle) {
    if !handle.is_null() {
        log::debug!("Freeing lockdownd_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
