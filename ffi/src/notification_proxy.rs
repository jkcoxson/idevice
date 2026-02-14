// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, notification_proxy::NotificationProxyClient,
    provider::IdeviceProvider,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync_local,
};

pub struct NotificationProxyClientHandle(pub NotificationProxyClient);

/// Automatically creates and connects to Notification Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated NotificationProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut NotificationProxyClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<NotificationProxyClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        NotificationProxyClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(NotificationProxyClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Creates a new NotificationProxyClient from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated NotificationProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut NotificationProxyClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = NotificationProxyClient::new(socket);
    let boxed = Box::new(NotificationProxyClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Posts a notification to the device
///
/// # Arguments
/// * `client` - A valid NotificationProxyClient handle
/// * `name` - C string containing the notification name
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `name` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_post(
    client: *mut NotificationProxyClientHandle,
    name: *const c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || name.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.post_notification(name_str).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Observes a specific notification
///
/// # Arguments
/// * `client` - A valid NotificationProxyClient handle
/// * `name` - C string containing the notification name to observe
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `name` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_observe(
    client: *mut NotificationProxyClientHandle,
    name: *const c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || name.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.observe_notification(name_str).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Observes multiple notifications at once
///
/// # Arguments
/// * `client` - A valid NotificationProxyClient handle
/// * `names` - A null-terminated array of C strings containing notification names
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `names` must be a valid pointer to a null-terminated array of null-terminated C strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_observe_multiple(
    client: *mut NotificationProxyClientHandle,
    names: *const *const c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || names.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let mut notification_names: Vec<String> = Vec::new();
    let mut i = 0;
    loop {
        let ptr = unsafe { *names.add(i) };
        if ptr.is_null() {
            break;
        }
        match unsafe { CStr::from_ptr(ptr) }.to_str() {
            Ok(s) => notification_names.push(s.to_string()),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
        i += 1;
    }

    let refs: Vec<&str> = notification_names.iter().map(|s| s.as_str()).collect();

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.observe_notifications(&refs).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Receives the next notification from the device
///
/// # Arguments
/// * `client` - A valid NotificationProxyClient handle
/// * `name_out` - On success, will be set to a newly allocated C string containing the notification name
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `name_out` must be a valid pointer. The returned string must be freed with `notification_proxy_free_string`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_receive(
    client: *mut NotificationProxyClientHandle,
    name_out: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || name_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<String, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.receive_notification().await
    });

    match res {
        Ok(name) => match CString::new(name) {
            Ok(c_string) => {
                unsafe { *name_out = c_string.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Receives the next notification with a timeout
///
/// # Arguments
/// * `client` - A valid NotificationProxyClient handle
/// * `interval` - Timeout in seconds to wait for a notification
/// * `name_out` - On success, will be set to a newly allocated C string containing the notification name
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `name_out` must be a valid pointer. The returned string must be freed with `notification_proxy_free_string`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_receive_with_timeout(
    client: *mut NotificationProxyClientHandle,
    interval: u64,
    name_out: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || name_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<String, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.receive_notification_with_timeout(interval).await
    });

    match res {
        Ok(name) => match CString::new(name) {
            Ok(c_string) => {
                unsafe { *name_out = c_string.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Frees a string returned by notification_proxy_receive
///
/// # Safety
/// `s` must be a valid pointer returned from `notification_proxy_receive`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notification_proxy_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = unsafe { CString::from_raw(s) };
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
pub unsafe extern "C" fn notification_proxy_client_free(
    handle: *mut NotificationProxyClientHandle,
) {
    if !handle.is_null() {
        tracing::debug!("Freeing notification_proxy_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
