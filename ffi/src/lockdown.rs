// Jackson Coxson

use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, RsdService as _, lockdown::LockdownClient,
    provider::IdeviceProvider,
};
use plist_ffi::plist_t;

use crate::{
    IdeviceFfiError, IdeviceHandle, IdevicePairingFile, core_device_proxy::AdapterHandle, ffi_err,
    provider::IdeviceProviderHandle, rsd::RsdHandshakeHandle, run_sync_local,
};

pub struct LockdowndClientHandle(pub LockdownClient);

/// Connects to lockdownd service using provider
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
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<LockdownClient, IdeviceError> = run_sync_local(async move {
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

/// Creates a new LockdownClient via RSD
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`client`] - On success, will be set to point to a newly allocated LockdownClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `handshake` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    client: *mut *mut LockdowndClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<LockdownClient, IdeviceError> = run_sync_local(async move {
        let provider_ref = unsafe { &mut (*provider).0 };
        let handshake_ref = unsafe { &mut (*handshake).0 };
        LockdownClient::connect_rsd(provider_ref, handshake_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LockdowndClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
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
    let res: Result<(), IdeviceError> = run_sync_local(async move {
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

    let res: Result<(u16, bool), IdeviceError> = run_sync_local(async move {
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

/// Pairs with the device using lockdownd
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
/// * `host_id` - The host ID (null-terminated string)
/// * `system_buid` - The system BUID (null-terminated string)
/// * `pairing_file` - On success, will be set to point to a newly allocated IdevicePairingFile handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `host_id` must be a valid null-terminated string
/// `system_buid` must be a valid null-terminated string
/// `pairing_file` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
#[cfg(feature = "pair")]
pub unsafe extern "C" fn lockdownd_pair(
    client: *mut LockdowndClientHandle,
    host_id: *const libc::c_char,
    system_buid: *const libc::c_char,
    host_name: *const libc::c_char,
    pairing_file: *mut *mut IdevicePairingFile,
) -> *mut IdeviceFfiError {
    if client.is_null() || host_id.is_null() || system_buid.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let host_id = unsafe {
        std::ffi::CStr::from_ptr(host_id)
            .to_string_lossy()
            .into_owned()
    };
    let system_buid = unsafe {
        std::ffi::CStr::from_ptr(system_buid)
            .to_string_lossy()
            .into_owned()
    };

    let host_name = if host_name.is_null() {
        None
    } else {
        Some(
            match unsafe { std::ffi::CStr::from_ptr(host_name) }.to_str() {
                Ok(v) => v,
                Err(_) => {
                    return ffi_err!(IdeviceError::FfiInvalidString);
                }
            },
        )
    };

    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };

        client_ref.pair(host_id, system_buid, host_name).await
    });

    match res {
        Ok(pairing_file_res) => {
            let boxed_pairing_file = Box::new(IdevicePairingFile(pairing_file_res));
            unsafe { *pairing_file = Box::into_raw(boxed_pairing_file) };
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
    if client.is_null() || out_plist.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let value = if key.is_null() {
        None
    } else {
        Some(match unsafe { std::ffi::CStr::from_ptr(key) }.to_str() {
            Ok(v) => v,
            Err(_) => {
                return ffi_err!(IdeviceError::FfiInvalidString);
            }
        })
    };

    let domain = if domain.is_null() {
        None
    } else {
        Some(match unsafe { std::ffi::CStr::from_ptr(domain) }.to_str() {
            Ok(v) => v,
            Err(_) => {
                return ffi_err!(IdeviceError::FfiInvalidString);
            }
        })
    };

    let res: Result<plist::Value, IdeviceError> = run_sync_local(async move {
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

/// Tells the device to enter recovery mode
///
/// # Arguments
/// * `client` - A valid LockdowndClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_enter_recovery(
    client: *mut LockdowndClientHandle,
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

/// Sets a value in lockdownd  
///  
/// # Arguments  
/// * `client` - A valid LockdowndClient handle  
/// * `key` - The key to set (null-terminated string)  
/// * `value` - The value to set as a plist  
/// * `domain` - The domain to set in (null-terminated string, optional)  
///  
/// # Returns  
/// An IdeviceFfiError on error, null on success  
///  
/// # Safety  
/// `client` must be a valid pointer to a handle allocated by this library  
/// `key` must be a valid null-terminated string  
/// `value` must be a valid plist  
/// `domain` must be a valid null-terminated string or NULL  
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_set_value(
    client: *mut LockdowndClientHandle,
    key: *const libc::c_char,
    value: plist_t,
    domain: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || key.is_null() || value.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let key = match unsafe { std::ffi::CStr::from_ptr(key) }.to_str() {
        Ok(k) => k,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let domain = if domain.is_null() {
        None
    } else {
        Some(match unsafe { std::ffi::CStr::from_ptr(domain) }.to_str() {
            Ok(d) => d,
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        })
    };

    let value = unsafe { &mut *value }.borrow_self().clone();

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.set_value(key, value, domain).await
    });

    match res {
        Ok(_) => null_mut(),
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
        tracing::debug!("Freeing lockdownd_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
