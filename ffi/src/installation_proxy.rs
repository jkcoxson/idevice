// Jackson Coxson

use std::ffi::c_void;

use idevice::{IdeviceError, IdeviceService, installation_proxy::InstallationProxyClient};

use crate::{
    IdeviceErrorCode, IdeviceHandle, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
    util,
};

pub struct InstallationProxyClientHandle(pub InstallationProxyClient);
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
pub unsafe extern "C" fn installation_proxy_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut InstallationProxyClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }
    let provider = unsafe { Box::from_raw(provider) }.0;

    let res: Result<InstallationProxyClient, IdeviceError> = RUNTIME.block_on(async move {
        let res = InstallationProxyClient::connect(&provider).await;
        std::mem::forget(provider);
        res
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(InstallationProxyClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
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
pub unsafe extern "C" fn installation_proxy_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut InstallationProxyClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }
    let provider = unsafe { Box::from_raw(provider) }.0;

    let res: Result<InstallationProxyClient, IdeviceError> = RUNTIME.block_on(async move {
        let res = InstallationProxyClient::connect(&provider).await;
        std::mem::forget(provider);
        res
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(InstallationProxyClientHandle(r));
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
pub unsafe extern "C" fn installation_proxy_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut InstallationProxyClientHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = InstallationProxyClient::new(socket);
    let boxed = Box::new(InstallationProxyClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Gets installed apps on the device
///
/// # Arguments
/// * `client` - A valid InstallationProxyClient handle
/// * `application_type` - The application type to filter by (optional, NULL for "Any")
/// * `bundle_identifiers` - The identifiers to filter by (optional, NULL for all apps)
/// * `out_result` - On success, will be set to point to a newly allocated array of PlistRef
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_result` must be a valid, non-null pointer to a location where the result will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_get_apps(
    client: *mut InstallationProxyClientHandle,
    application_type: *const libc::c_char,
    bundle_identifiers: *const *const libc::c_char,
    bundle_identifiers_len: libc::size_t,
    out_result: *mut *mut c_void,
    out_result_len: *mut libc::size_t,
) -> IdeviceErrorCode {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        log::error!("Invalid arguments: {client:?}, {out_result:?}");
        return IdeviceErrorCode::InvalidArg;
    }
    let client = unsafe { &mut *client };

    let app_type = if application_type.is_null() {
        None
    } else {
        Some(unsafe {
            std::ffi::CStr::from_ptr(application_type)
                .to_string_lossy()
                .into_owned()
        })
    };

    let bundle_ids = if bundle_identifiers.is_null() {
        None
    } else {
        let ids = unsafe { std::slice::from_raw_parts(bundle_identifiers, bundle_identifiers_len) };
        Some(
            ids.iter()
                .map(|&s| {
                    unsafe { std::ffi::CStr::from_ptr(s) }
                        .to_string_lossy()
                        .into_owned()
                })
                .collect(),
        )
    };

    let res: Result<Vec<*mut c_void>, IdeviceError> = RUNTIME.block_on(async {
        client.0.get_apps(app_type, bundle_ids).await.map(|apps| {
            apps.into_values()
                .map(|v| util::plist_to_libplist(&v).get_pointer())
                .collect()
        })
    });

    match res {
        Ok(r) => {
            let len = r.len();
            let boxed_slice = r.into_boxed_slice();
            let ptr = boxed_slice.as_ptr();
            std::mem::forget(boxed_slice);

            unsafe {
                *out_result = ptr as *mut c_void;
                *out_result_len = len;
            }
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
pub unsafe extern "C" fn installation_proxy_client_free(
    handle: *mut InstallationProxyClientHandle,
) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
