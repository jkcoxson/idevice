// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::ptr::null_mut;

use idevice::installcoordination_proxy::InstallcoordinationProxy;
use idevice::{IdeviceError, ReadWrite, RsdService};

use crate::{IdeviceFfiError, ReadWriteOpaque, ffi_err, run_sync_local};

pub struct InstallcoordinationProxyHandle(pub InstallcoordinationProxy<Box<dyn ReadWrite>>);

/// Creates a new InstallcoordinationProxy client from a ReadWrite stream
///
/// # Arguments
/// * [`socket`] - A ReadWriteOpaque handle (consumed)
/// * [`client`] - On success, will be set to point to a newly allocated handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installcoordination_proxy_new(
    socket: *mut ReadWriteOpaque,
    client: *mut *mut InstallcoordinationProxyHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) };
    let inner = match socket.inner {
        Some(i) => i,
        None => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<InstallcoordinationProxy<Box<dyn ReadWrite>>, IdeviceError> =
        run_sync_local(async move {
            <InstallcoordinationProxy<Box<dyn ReadWrite>> as RsdService>::from_stream(inner).await
        });

    match res {
        Ok(r) => {
            let boxed = Box::new(InstallcoordinationProxyHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Uninstalls an app by bundle ID
///
/// # Arguments
/// * `client` - A valid InstallcoordinationProxy handle
/// * `bundle_id` - The bundle identifier of the app to uninstall
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `bundle_id` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installcoordination_proxy_uninstall_app(
    client: *mut InstallcoordinationProxyHandle,
    bundle_id: *const c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || bundle_id.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let bundle_id = match unsafe { CStr::from_ptr(bundle_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.uninstall_app(bundle_id).await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Queries the install path of an app by bundle ID
///
/// # Arguments
/// * `client` - A valid InstallcoordinationProxy handle
/// * `bundle_id` - The bundle identifier of the app to query
/// * `path` - On success, will be set to a newly allocated C string with the install path
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `bundle_id` must be a valid null-terminated C string
/// The returned string must be freed with `idevice_string_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installcoordination_proxy_query_app_path(
    client: *mut InstallcoordinationProxyHandle,
    bundle_id: *const c_char,
    path: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || bundle_id.is_null() || path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let bundle_id = match unsafe { CStr::from_ptr(bundle_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let res: Result<String, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.query_app_path(bundle_id).await
    });
    match res {
        Ok(p) => match CString::new(p) {
            Ok(c_string) => {
                unsafe { *path = c_string.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Frees an InstallcoordinationProxy client handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installcoordination_proxy_client_free(
    handle: *mut InstallcoordinationProxyHandle,
) {
    if !handle.is_null() {
        tracing::debug!("Freeing InstallcoordinationProxyHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
