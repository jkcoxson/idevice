// Jackson Coxson

use std::ffi::c_void;

use idevice::{IdeviceError, IdeviceService, installation_proxy::InstallationProxyClient};

use crate::{
    IdeviceErrorCode, IdeviceHandle, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
    util,
};

pub struct InstallationProxyClientHandle(pub InstallationProxyClient);

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
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<InstallationProxyClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = InstallationProxyClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(InstallationProxyClientHandle(r));
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
pub unsafe extern "C" fn installation_proxy_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut InstallationProxyClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<InstallationProxyClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = InstallationProxyClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
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
/// * [`client`] - A valid InstallationProxyClient handle
/// * [`application_type`] - The application type to filter by (optional, NULL for "Any")
/// * [`bundle_identifiers`] - The identifiers to filter by (optional, NULL for all apps)
/// * [`out_result`] - On success, will be set to point to a newly allocated array of PlistRef
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
                .map(|v| util::plist_to_libplist(&v))
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_install(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: *mut c_void,
) -> IdeviceErrorCode {
    if client.is_null() || package_path.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .install(package_path, options)
            .await
    });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_install_with_callback(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: *mut c_void,
    callback: extern "C" fn(progress: u64, context: *mut c_void),
    context: *mut c_void,
) -> IdeviceErrorCode {
    if client.is_null() || package_path.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

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
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_upgrade(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: *mut c_void,
) -> IdeviceErrorCode {
    if client.is_null() || package_path.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .upgrade(package_path, options)
            .await
    });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `package_path` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_upgrade_with_callback(
    client: *mut InstallationProxyClientHandle,
    package_path: *const libc::c_char,
    options: *mut c_void,
    callback: extern "C" fn(progress: u64, context: *mut c_void),
    context: *mut c_void,
) -> IdeviceErrorCode {
    if client.is_null() || package_path.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let package_path = unsafe { std::ffi::CStr::from_ptr(package_path) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

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
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `bundle_id` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_uninstall(
    client: *mut InstallationProxyClientHandle,
    bundle_id: *const libc::c_char,
    options: *mut c_void,
) -> IdeviceErrorCode {
    if client.is_null() || bundle_id.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let bundle_id = unsafe { std::ffi::CStr::from_ptr(bundle_id) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .uninstall(bundle_id, options)
            .await
    });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `bundle_id` must be a valid C string
/// `options` must be a valid plist dictionary or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_uninstall_with_callback(
    client: *mut InstallationProxyClientHandle,
    bundle_id: *const libc::c_char,
    options: *mut c_void,
    callback: extern "C" fn(progress: u64, context: *mut c_void),
    context: *mut c_void,
) -> IdeviceErrorCode {
    if client.is_null() || bundle_id.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let bundle_id = unsafe { std::ffi::CStr::from_ptr(bundle_id) }
        .to_string_lossy()
        .into_owned();
    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

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
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `capabilities` must be a valid array of plist values or NULL
/// `options` must be a valid plist dictionary or NULL
/// `out_result` must be a valid pointer to a bool
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_check_capabilities_match(
    client: *mut InstallationProxyClientHandle,
    capabilities: *const *mut c_void,
    capabilities_len: libc::size_t,
    options: *mut c_void,
    out_result: *mut bool,
) -> IdeviceErrorCode {
    if client.is_null() || out_result.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let capabilities = if capabilities.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(capabilities, capabilities_len) }
            .iter()
            .map(|&ptr| util::libplist_to_plist(ptr))
            .collect()
    };

    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .check_capabilities_match(capabilities, options)
            .await
    });

    match res {
        Ok(result) => {
            unsafe { *out_result = result };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `options` must be a valid plist dictionary or NULL
/// `out_result` must be a valid, non-null pointer to a location where the result will be stored
/// `out_result_len` must be a valid, non-null pointer to a location where the length will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn installation_proxy_browse(
    client: *mut InstallationProxyClientHandle,
    options: *mut c_void,
    out_result: *mut *mut c_void,
    out_result_len: *mut libc::size_t,
) -> IdeviceErrorCode {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let options = if options.is_null() {
        None
    } else {
        Some(util::libplist_to_plist(options))
    };

    let res: Result<Vec<*mut c_void>, IdeviceError> = RUNTIME.block_on(async {
        unsafe { &mut *client }.0.browse(options).await.map(|apps| {
            apps.into_iter()
                .map(|v| util::plist_to_libplist(&v))
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
