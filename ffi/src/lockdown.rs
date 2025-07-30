// Jackson Coxson

use std::ptr::null_mut;

use idevice::{IdeviceError, IdeviceService, lockdown::LockdownClient, provider::IdeviceProvider};
use plist_ffi::{PlistWrapper, plist_t};

use crate::{
    IdeviceFfiError, IdeviceHandle, IdevicePairingFile, RUNTIME, ffi_err,
    provider::IdeviceProviderHandle,
};

pub struct LockdowndClientHandle(pub LockdownClient);

/// Connects to lockdownd service using TCP provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated LockdowndClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut LockdowndClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<LockdownClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        LockdownClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LockdowndClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            let _ = unsafe { Box::from_raw(provider) };
            ffi_err!(e)
        }
    }
}

/// Creates a new LockdowndClient from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle.
/// * [`client`] - On success, will be set to point to a newly allocated LockdowndClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and maybe not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut LockdowndClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = LockdownClient::new(socket);
    let boxed = Box::new(LockdowndClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Starts a session with lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `pairing_file` - An IdevicePairingFile alocated by this library
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `pairing_file` must be a valid plist_t containing a pairing file
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_start_session(
    client: *mut LockdowndClientHandle,
    pairing_file: *mut IdevicePairingFile,
) -> *mut IdeviceFfiError {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        let pairing_file_ref = unsafe { &(*pairing_file).0 };

        client_ref.start_session(pairing_file_ref).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
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
/// An IdeviceFfiError on error, null on success
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
) -> *mut IdeviceFfiError {
    if identifier.is_null() || port.is_null() || ssl.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let identifier = unsafe { std::ffi::CStr::from_ptr(identifier) }
        .to_string_lossy()
        .into_owned();

    let res: Result<(u16, bool), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.start_service(identifier).await
    });

    match res {
        Ok((p, s)) => {
            unsafe {
                *port = p;
                *ssl = s;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets a value from lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `key` - The value to get (null-terminated string)
/// * `domain` - The value to get (null-terminated string)
/// * `out_plist` - Pointer to store the returned plist value
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `value` must be a valid null-terminated string
/// `out_plist` must be a valid pointer to store the plist
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_get_value(
    client: *mut LockdowndClientHandle,
    key: *const libc::c_char,
    domain: *const libc::c_char,
    out_plist: *mut plist_t,
) -> *mut IdeviceFfiError {
    if key.is_null() || out_plist.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let value = unsafe { std::ffi::CStr::from_ptr(key) }
        .to_string_lossy()
        .into_owned();

    let domain = if domain.is_null() {
        None
    } else {
        Some(
            unsafe { std::ffi::CStr::from_ptr(domain) }
                .to_string_lossy()
                .into_owned(),
        )
    };

    let res: Result<plist::Value, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_value(value, domain).await
    });

    match res {
        Ok(value) => {
            unsafe {
                *out_plist = plist_ffi::PlistWrapper::new_node(value).into_ptr();
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets all values from lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `out_plist` - Pointer to store the returned plist dictionary
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_plist` must be a valid pointer to store the plist
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_get_all_values(
    client: *mut LockdowndClientHandle,
    domain: *const libc::c_char,
    out_plist: *mut plist_t,
) -> *mut IdeviceFfiError {
    if out_plist.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let domain = if domain.is_null() {
        None
    } else {
        Some(
            unsafe { std::ffi::CStr::from_ptr(domain) }
                .to_string_lossy()
                .into_owned(),
        )
    };

    let res: Result<plist::Dictionary, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_all_values(domain).await
    });

    match res {
        Ok(dict) => {
            unsafe {
                *out_plist = PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
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
