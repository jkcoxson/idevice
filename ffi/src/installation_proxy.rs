// Jackson Coxson

use std::{ffi::c_void, ptr::null_mut};

use idevice::{
    IdeviceError, IdeviceService, installation_proxy::InstallationProxyClient,
    provider::IdeviceProvider,
};
use plist_ffi::{PlistWrapper, plist_t};

use crate::{IdeviceFfiError, IdeviceHandle, RUNTIME, ffi_err, provider::IdeviceProviderHandle};

pub struct InstallationProxyClientHandle(pub InstallationProxyClient);

/// Automatically creates and connects to Installation Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated InstallationProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_connect_tcp(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut InstallationProxyClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<InstallationProxyClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        InstallationProxyClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(InstallationProxyClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Automatically creates and connects to Installation Proxy, returning a client handle
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated InstallationProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut InstallationProxyClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = InstallationProxyClient::new(socket);
    let boxed = Box::new(InstallationProxyClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Gets installed apps on the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`application_type`] - The application type to filter by (optional, NULL for "Any")
/// * [`bundle_identifiers`] - The identifiers to filter by (optional, NULL for all apps)
/// * [`out_result`] - On success, will be set to point to a newly allocated array of PlistRef
///
/// # Returns
/// An IdeviceFfiError on error, null on success
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
) -> *mut IdeviceFfiError {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        log::error!("Invalid arguments: {client:?}, {out_result:?}");
        return ffi_err!(IdeviceError::FfiInvalidArg);
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

    let res: Result<Vec<plist_t>, IdeviceError> = RUNTIME.block_on(async {
        client.0.get_apps(app_type, bundle_ids).await.map(|apps| {
            apps.into_values()
                .map(|v| PlistWrapper::new_node(v).into_ptr())
                .collect()
        })
    });

    match res {
        Ok(mut r) => {
            let ptr = r.as_mut_ptr();
            let len = r.len();
            std::mem::forget(r);

            unsafe {
                *out_result = ptr as *mut c_void;
                *out_result_len = len;
            }
            null_mut()
        }
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
pub unsafe extern "C" fn installation_proxy_client_free(
    handle: *mut InstallationProxyClientHandle,
) {
    if !handle.is_null() {
        log::debug!("Freeing installation_proxy_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Installs an application package on the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`package_path`] - Path to the .ipa package in the AFC jail
/// * [`options`] - Optional installation options as a plist dictionary (can be NULL)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_install(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || package_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .install(package_path, options)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Installs an application package on the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`package_path`] - Path to the .ipa package in the AFC jail
/// * [`options`] - Optional installation options as a plist dictionary (can be NULL)
/// * [`callback`] - Progress callback function
/// * [`context`] - User context to pass to callback
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_install_with_callback(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: plist_t,
    callback: extern "C" fn(progress: u64, context: *mut c_void),
    context: *mut c_void,
) -> *mut IdeviceFfiError {
    if client.is_null() || package_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        let callback_wrapper = |(progress, context)| async move {
            callback(progress, context);
        };

        unsafe { &mut *client }
            .0
            .install_with_callback(package_path, options, callback_wrapper, context)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Upgrades an existing application on the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`package_path`] - Path to the .ipa package in the AFC jail
/// * [`options`] - Optional upgrade options as a plist dictionary (can be NULL)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_upgrade(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || package_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .upgrade(package_path, options)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Upgrades an existing application on the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`package_path`] - Path to the .ipa package in the AFC jail
/// * [`options`] - Optional upgrade options as a plist dictionary (can be NULL)
/// * [`callback`] - Progress callback function
/// * [`context`] - User context to pass to callback
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_upgrade_with_callback(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: plist_t,
    callback: extern "C" fn(progress: u64, context: *mut c_void),
    context: *mut c_void,
) -> *mut IdeviceFfiError {
    if client.is_null() || package_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        let callback_wrapper = |(progress, context)| async move {
            callback(progress, context);
        };

        unsafe { &mut *client }
            .0
            .upgrade_with_callback(package_path, options, callback_wrapper, context)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Uninstalls an application from the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`bundle_id`] - Bundle identifier of the application to uninstall
/// * [`options`] - Optional uninstall options as a plist dictionary (can be NULL)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `bundle_id` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_uninstall(
    client: *mut InstallationProxyClientHandle,
    bundle_id: *const libc::c_char,
    options: plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || bundle_id.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id = unsafe { std::ffi::CStr::from_ptr(bundle_id) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .uninstall(bundle_id, options)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Uninstalls an application from the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`bundle_id`] - Bundle identifier of the application to uninstall
/// * [`options`] - Optional uninstall options as a plist dictionary (can be NULL)
/// * [`callback`] - Progress callback function
/// * [`context`] - User context to pass to callback
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `bundle_id` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_uninstall_with_callback(
    client: *mut InstallationProxyClientHandle,
    bundle_id: *const libc::c_char,
    options: plist_t,
    callback: extern "C" fn(progress: u64, context: *mut c_void),
    context: *mut c_void,
) -> *mut IdeviceFfiError {
    if client.is_null() || bundle_id.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id = unsafe { std::ffi::CStr::from_ptr(bundle_id) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        let callback_wrapper = |(progress, context)| async move {
            callback(progress, context);
        };

        unsafe { &mut *client }
            .0
            .uninstall_with_callback(bundle_id, options, callback_wrapper, context)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Checks if the device capabilities match the required capabilities
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`capabilities`] - Array of plist values representing required capabilities
/// * [`capabilities_len`] - Length of the capabilities array
/// * [`options`] - Optional check options as a plist dictionary (can be NULL)
/// * [`out_result`] - Will be set to true if all capabilities are supported, false otherwise
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `capabilities` must be a valid array of plist values or NULL
/// `options` must be a valid plist dictionary or NULL
/// `out_result` must be a valid pointer to a bool
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_check_capabilities_match(
    client: *mut InstallationProxyClientHandle,
    capabilities: *const plist_t,
    capabilities_len: libc::size_t,
    options: plist_t,
    out_result: *mut bool,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_result.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let capabilities = if capabilities.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(capabilities, capabilities_len) }
            .iter()
            .map(|ptr| unsafe { &mut **ptr }.borrow_self().clone())
            .collect()
    };

    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .check_capabilities_match(capabilities, options)
            .await
    });

    match res {
        Ok(result) => {
            unsafe { *out_result = result };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Browses installed applications on the device
///
/// # Arguments
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`options`] - Optional browse options as a plist dictionary (can be NULL)
/// * [`out_result`] - On success, will be set to point to a newly allocated array of PlistRef
/// * [`out_result_len`] - Will be set to the length of the result array
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `options` must be a valid plist dictionary or NULL
/// `out_result` must be a valid, non-null pointer to a location where the result will be stored
/// `out_result_len` must be a valid, non-null pointer to a location where the length will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_browse(
    client: *mut InstallationProxyClientHandle,
    options: plist_t,
    out_result: *mut *mut plist_t,
    out_result_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let options = if options.is_null() {
        None
    } else {
        Some(unsafe { &mut *options })
    }
    .map(|x| x.borrow_self().clone());

    let res: Result<Vec<plist_t>, IdeviceError> = RUNTIME.block_on(async {
        unsafe { &mut *client }.0.browse(options).await.map(|apps| {
            apps.into_iter()
                .map(|v| PlistWrapper::new_node(v).into_ptr())
                .collect()
        })
    });

    match res {
        Ok(r) => {
            let mut r = r.into_boxed_slice();
            let ptr = r.as_mut_ptr();
            let len = r.len();
            std::mem::forget(r);

            unsafe {
                *out_result = ptr;
                *out_result_len = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
