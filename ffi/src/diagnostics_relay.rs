// Jackson Coxson

use std::{
    ffi::{CStr, c_char},
    ptr::null_mut,
};

use idevice::{
    IdeviceError, IdeviceService, diagnostics_relay::DiagnosticsRelayClient,
    provider::IdeviceProvider,
};
use plist_ffi::plist_t;

use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync_local,
};

pub struct DiagnosticsRelayClientHandle(pub DiagnosticsRelayClient);

/// Automatically creates and connects to Diagnostics Relay, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated DiagnosticsRelayClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut DiagnosticsRelayClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<DiagnosticsRelayClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        // Connect using the reference
        DiagnosticsRelayClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(DiagnosticsRelayClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Automatically creates and connects to Diagnostics Relay, returning a client handle
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated DiagnosticsRelayClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut DiagnosticsRelayClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = DiagnosticsRelayClient::new(socket);
    let boxed = Box::new(DiagnosticsRelayClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Queries the device IO registry
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
/// * `current_plane` - A string to search by or null
/// * `entry_name` - A string to search by or null
/// * `entry_class` - A string to search by or null
/// * `res` - Will be set to a pointer of a plist dictionary node on search success
///
/// # Returns
/// An IdeviceFfiError on error, null on success. Note that res can be null on success
/// if the search resulted in no values.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_ioregistry(
    client: *mut DiagnosticsRelayClientHandle,
    current_plane: *const c_char,
    entry_name: *const c_char,
    entry_class: *const c_char,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let current_plane = if current_plane.is_null() {
        None
    } else {
        Some(match unsafe { CStr::from_ptr(current_plane) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                return ffi_err!(IdeviceError::FfiInvalidString);
            }
        })
    };
    let entry_name = if entry_name.is_null() {
        None
    } else {
        Some(match unsafe { CStr::from_ptr(entry_name) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                return ffi_err!(IdeviceError::FfiInvalidString);
            }
        })
    };
    let entry_class = if entry_class.is_null() {
        None
    } else {
        Some(match unsafe { CStr::from_ptr(entry_class) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                return ffi_err!(IdeviceError::FfiInvalidString);
            }
        })
    };

    let output: Result<Option<plist::Dictionary>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .ioregistry(current_plane, entry_name, entry_class)
            .await
    });

    match output {
        Ok(output) => {
            let output = match output {
                Some(res) => {
                    plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(res)).into_ptr()
                }
                None => null_mut(),
            };

            unsafe { *res = output }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Requests MobileGestalt information from the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
/// * `keys` - Optional list of specific keys to request. If None, requests all available keys
/// * `res` - Will be set to a pointer of a plist dictionary node on search success
///
/// # Returns
/// An IdeviceFfiError on error, null on success. Note that res can be null on success
/// if the search resulted in no values.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_mobilegestalt(
    client: *mut DiagnosticsRelayClientHandle,
    keys: *const *const c_char,
    keys_len: usize,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let keys = if keys.is_null() {
        let keys = unsafe { std::slice::from_raw_parts(keys, keys_len) };
        Some(
            keys.iter()
                .filter_map(|x| unsafe {
                    match CStr::from_ptr(*x).to_str() {
                        Ok(s) => Some(s.to_string()),
                        Err(_) => None,
                    }
                })
                .collect::<Vec<String>>(),
        )
    } else {
        None
    };

    let output: Result<Option<plist::Dictionary>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.mobilegestalt(keys).await
    });

    match output {
        Ok(output) => {
            let output = match output {
                Some(res) => {
                    plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(res)).into_ptr()
                }
                None => null_mut(),
            };

            unsafe { *res = output }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Requests gas gauge information from the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on search success
///
/// # Returns
/// An IdeviceFfiError on error, null on success. Note that res can be null on success
/// if the search resulted in no values.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_gasguage(
    client: *mut DiagnosticsRelayClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<Option<plist::Dictionary>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.gasguage().await
    });

    match output {
        Ok(output) => {
            let output = match output {
                Some(res) => {
                    plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(res)).into_ptr()
                }
                None => null_mut(),
            };

            unsafe { *res = output }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Requests nand information from the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on search success
///
/// # Returns
/// An IdeviceFfiError on error, null on success. Note that res can be null on success
/// if the search resulted in no values.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_nand(
    client: *mut DiagnosticsRelayClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<Option<plist::Dictionary>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.nand().await
    });

    match output {
        Ok(output) => {
            let output = match output {
                Some(res) => {
                    plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(res)).into_ptr()
                }
                None => null_mut(),
            };

            unsafe { *res = output }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Requests all available information from the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on search success
///
/// # Returns
/// An IdeviceFfiError on error, null on success. Note that res can be null on success
/// if the search resulted in no values.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_all(
    client: *mut DiagnosticsRelayClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<Option<plist::Dictionary>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.all().await
    });

    match output {
        Ok(output) => {
            let output = match output {
                Some(res) => {
                    plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(res)).into_ptr()
                }
                None => null_mut(),
            };

            unsafe { *res = output }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Restarts the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_restart(
    client: *mut DiagnosticsRelayClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.restart().await
    });

    match output {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Shuts down the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_shutdown(
    client: *mut DiagnosticsRelayClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.shutdown().await
    });

    match output {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Puts the device to sleep
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_sleep(
    client: *mut DiagnosticsRelayClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.sleep().await
    });

    match output {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Requests WiFi diagnostics from the device
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
/// * `res` - Will be set to a pointer of a plist dictionary node on search success
///
/// # Returns
/// An IdeviceFfiError on error, null on success. Note that res can be null on success
/// if the search resulted in no values.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_wifi(
    client: *mut DiagnosticsRelayClientHandle,
    res: *mut plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<Option<plist::Dictionary>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.wifi().await
    });

    match output {
        Ok(output) => {
            let output = match output {
                Some(res) => {
                    plist_ffi::PlistWrapper::new_node(plist::Value::Dictionary(res)).into_ptr()
                }
                None => null_mut(),
            };

            unsafe { *res = output }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Puts the device to sleep
///
/// # Arguments
/// * `client` - A valid DiagnosticsRelayClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success.
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_relay_client_goodbye(
    client: *mut DiagnosticsRelayClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let output: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.goodbye().await
    });

    match output {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
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
pub unsafe extern "C" fn diagnostics_relay_client_free(handle: *mut DiagnosticsRelayClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing DiagnosticsRelayClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
