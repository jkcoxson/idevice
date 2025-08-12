// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::os::raw::{c_float, c_int};
use std::ptr::{self, null_mut};

use idevice::core_device::AppServiceClient;
use idevice::{IdeviceError, ReadWrite, RsdService};

use crate::core_device_proxy::AdapterHandle;
use crate::rsd::RsdHandshakeHandle;
use crate::{IdeviceFfiError, RUNTIME, ffi_err};

/// Opaque handle to an AppServiceClient
pub struct AppServiceHandle(pub AppServiceClient<Box<dyn ReadWrite>>);

/// C-compatible app list entry
#[repr(C)]
pub struct AppListEntryC {
    pub is_removable: c_int,
    pub name: *mut c_char,
    pub is_first_party: c_int,
    pub path: *mut c_char,
    pub bundle_identifier: *mut c_char,
    pub is_developer_app: c_int,
    pub bundle_version: *mut c_char, // NULL if None
    pub is_internal: c_int,
    pub is_hidden: c_int,
    pub is_app_clip: c_int,
    pub version: *mut c_char, // NULL if None
}

/// C-compatible launch response
#[repr(C)]
pub struct LaunchResponseC {
    pub process_identifier_version: u32,
    pub pid: u32,
    pub executable_url: *mut c_char,
    pub audit_token: *mut u32,
    pub audit_token_len: usize,
}

/// C-compatible process token
#[repr(C)]
pub struct ProcessTokenC {
    pub pid: u32,
    pub executable_url: *mut c_char, // NULL if None
}

/// C-compatible signal response
#[repr(C)]
pub struct SignalResponseC {
    pub pid: u32,
    pub executable_url: *mut c_char, // NULL if None
    pub device_timestamp: u64,       // Unix timestamp
    pub signal: u32,
}

/// C-compatible icon data
#[repr(C)]
pub struct IconDataC {
    pub data: *mut u8,
    pub data_len: usize,
    pub icon_width: f64,
    pub icon_height: f64,
    pub minimum_width: f64,
    pub minimum_height: f64,
}

/// Creates a new AppServiceClient using RSD connection
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` and `handshake` must be valid pointers to handles allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut AppServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<AppServiceClient<Box<dyn ReadWrite>>, IdeviceError> =
        RUNTIME.block_on(async move {
            let provider_ref = unsafe { &mut (*provider).0 };
            let handshake_ref = unsafe { &mut (*handshake).0 };

            AppServiceClient::connect_rsd(provider_ref, handshake_ref).await
        });

    match res {
        Ok(client) => {
            let boxed = Box::new(AppServiceHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new AppServiceClient from a socket
///
/// # Arguments
/// * [`socket`] - The socket to use for communication
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_new(
    socket: *mut Box<dyn ReadWrite>,
    handle: *mut *mut AppServiceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket = unsafe { Box::from_raw(socket) };
    let res = RUNTIME.block_on(async move { AppServiceClient::new(*socket).await });

    match res {
        Ok(client) => {
            let new_handle = AppServiceHandle(client);
            unsafe { *handle = Box::into_raw(Box::new(new_handle)) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an AppServiceClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_free(handle: *mut AppServiceHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Lists applications on the device
///
/// # Arguments
/// * [`handle`] - The AppServiceClient handle
/// * [`app_clips`] - Include app clips
/// * [`removable_apps`] - Include removable apps
/// * [`hidden_apps`] - Include hidden apps
/// * [`internal_apps`] - Include internal apps
/// * [`default_apps`] - Include default apps
/// * [`apps`] - Pointer to store the array of apps (caller must free)
/// * [`count`] - Pointer to store the number of apps
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle`, `apps`, and `count` must be valid pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_list_apps(
    handle: *mut AppServiceHandle,
    app_clips: c_int,
    removable_apps: c_int,
    hidden_apps: c_int,
    internal_apps: c_int,
    default_apps: c_int,
    apps: *mut *mut AppListEntryC,
    count: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || apps.is_null() || count.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move {
        client
            .list_apps(
                app_clips != 0,
                removable_apps != 0,
                hidden_apps != 0,
                internal_apps != 0,
                default_apps != 0,
            )
            .await
    });

    match res {
        Ok(app_list) => {
            let mut c_apps = Vec::with_capacity(app_list.len());

            for app in app_list {
                let name = match CString::new(app.name) {
                    Ok(s) => s.into_raw(),
                    Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                };
                let path = match CString::new(app.path) {
                    Ok(s) => s.into_raw(),
                    Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                };
                let bundle_id = match CString::new(app.bundle_identifier) {
                    Ok(s) => s.into_raw(),
                    Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                };
                let bundle_version = match app.bundle_version {
                    Some(v) => match CString::new(v) {
                        Ok(s) => s.into_raw(),
                        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                    },
                    None => ptr::null_mut(),
                };
                let version = match app.version {
                    Some(v) => match CString::new(v) {
                        Ok(s) => s.into_raw(),
                        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                    },
                    None => ptr::null_mut(),
                };

                c_apps.push(AppListEntryC {
                    is_removable: if app.is_removable { 1 } else { 0 },
                    name,
                    is_first_party: if app.is_first_party { 1 } else { 0 },
                    path,
                    bundle_identifier: bundle_id,
                    is_developer_app: if app.is_developer_app { 1 } else { 0 },
                    bundle_version,
                    is_internal: if app.is_internal { 1 } else { 0 },
                    is_hidden: if app.is_hidden { 1 } else { 0 },
                    is_app_clip: if app.is_app_clip { 1 } else { 0 },
                    version,
                });
            }

            let mut c_apps = c_apps.into_boxed_slice();
            let len = c_apps.len();
            let ptr = c_apps.as_mut_ptr();
            std::mem::forget(c_apps);

            unsafe {
                *apps = ptr;
                *count = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an array of AppListEntryC structures
///
/// # Safety
/// `apps` must be a valid pointer to an array allocated by app_service_list_apps
/// `count` must match the count returned by app_service_list_apps
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_free_app_list(apps: *mut AppListEntryC, count: usize) {
    if !apps.is_null() && count > 0 {
        let apps_slice = unsafe { std::slice::from_raw_parts_mut(apps, count) };
        for app in apps_slice {
            if !app.name.is_null() {
                let _ = unsafe { CString::from_raw(app.name) };
            }
            if !app.path.is_null() {
                let _ = unsafe { CString::from_raw(app.path) };
            }
            if !app.bundle_identifier.is_null() {
                let _ = unsafe { CString::from_raw(app.bundle_identifier) };
            }
            if !app.bundle_version.is_null() {
                let _ = unsafe { CString::from_raw(app.bundle_version) };
            }
            if !app.version.is_null() {
                let _ = unsafe { CString::from_raw(app.version) };
            }
        }
        let _ = unsafe { Vec::from_raw_parts(apps, count, count) };
    }
}

/// Launches an application
///
/// # Arguments
/// * [`handle`] - The AppServiceClient handle
/// * [`bundle_id`] - Bundle identifier of the app to launch
/// * [`argv`] - NULL-terminated array of arguments
/// * [`argc`] - Number of arguments
/// * [`kill_existing`] - Whether to kill existing instances
/// * [`start_suspended`] - Whether to start suspended
/// * [`response`] - Pointer to store the launch response (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_launch_app(
    handle: *mut AppServiceHandle,
    bundle_id: *const c_char,
    argv: *const *const c_char,
    argc: usize,
    kill_existing: c_int,
    start_suspended: c_int,
    response: *mut *mut LaunchResponseC,
) -> *mut IdeviceFfiError {
    if handle.is_null() || bundle_id.is_null() || response.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id_str = match unsafe { CStr::from_ptr(bundle_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let mut args = Vec::new();
    if !argv.is_null() && argc > 0 {
        let argv_slice = unsafe { std::slice::from_raw_parts(argv, argc) };
        for &arg in argv_slice {
            if !arg.is_null()
                && let Ok(arg_str) = unsafe { CStr::from_ptr(arg) }.to_str()
            {
                args.push(arg_str);
            }
        }
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move {
        client
            .launch_application(
                bundle_id_str,
                &args,
                kill_existing != 0,
                start_suspended != 0,
                None, // environment
                None, // platform_options
            )
            .await
    });

    match res {
        Ok(launch_response) => {
            let executable_url = match CString::new(launch_response.executable_url.relative) {
                Ok(s) => s.into_raw(),
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            };

            let audit_token_len = launch_response.audit_token.len();
            let mut audit_token_vec = launch_response.audit_token.into_boxed_slice();
            let audit_token_ptr = audit_token_vec.as_mut_ptr();
            std::mem::forget(audit_token_vec);

            let c_response = Box::new(LaunchResponseC {
                process_identifier_version: launch_response.process_identifier_version,
                pid: launch_response.pid,
                executable_url,
                audit_token: audit_token_ptr,
                audit_token_len,
            });

            unsafe { *response = Box::into_raw(c_response) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a LaunchResponseC structure
///
/// # Safety
/// `response` must be a valid pointer allocated by app_service_launch_app
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_free_launch_response(response: *mut LaunchResponseC) {
    if !response.is_null() {
        let response = unsafe { Box::from_raw(response) };
        if !response.executable_url.is_null() {
            let _ = unsafe { CString::from_raw(response.executable_url) };
        }
        if !response.audit_token.is_null() && response.audit_token_len > 0 {
            let _ = unsafe {
                std::slice::from_raw_parts(response.audit_token, response.audit_token_len)
            };
        }
    }
}

/// Lists running processes
///
/// # Arguments
/// * [`handle`] - The AppServiceClient handle
/// * [`processes`] - Pointer to store the array of processes (caller must free)
/// * [`count`] - Pointer to store the number of processes
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_list_processes(
    handle: *mut AppServiceHandle,
    processes: *mut *mut ProcessTokenC,
    count: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || processes.is_null() || count.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.list_processes().await });

    match res {
        Ok(process_list) => {
            let mut c_processes = Vec::with_capacity(process_list.len());

            for process in process_list {
                let executable_url = match process.executable_url {
                    Some(url) => match CString::new(url.relative) {
                        Ok(s) => s.into_raw(),
                        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                    },
                    None => ptr::null_mut(),
                };

                c_processes.push(ProcessTokenC {
                    pid: process.pid,
                    executable_url,
                });
            }

            let mut c_processes = c_processes.into_boxed_slice();
            let len = c_processes.len();
            let ptr = c_processes.as_mut_ptr();
            std::mem::forget(c_processes);

            unsafe {
                *processes = ptr;
                *count = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an array of ProcessTokenC structures
///
/// # Safety
/// `processes` must be a valid pointer allocated by app_service_list_processes
/// `count` must match the count returned by app_service_list_processes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_free_process_list(
    processes: *mut ProcessTokenC,
    count: usize,
) {
    if !processes.is_null() && count > 0 {
        let processes_slice = unsafe { std::slice::from_raw_parts_mut(processes, count) };
        for process in processes_slice {
            if !process.executable_url.is_null() {
                let _ = unsafe { CString::from_raw(process.executable_url) };
            }
        }
        let _ = unsafe { std::slice::from_raw_parts(processes, count) };
    }
}

/// Uninstalls an application
///
/// # Arguments
/// * [`handle`] - The AppServiceClient handle
/// * [`bundle_id`] - Bundle identifier of the app to uninstall
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_uninstall_app(
    handle: *mut AppServiceHandle,
    bundle_id: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || bundle_id.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id_str = match unsafe { CStr::from_ptr(bundle_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.uninstall_app(bundle_id_str).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Sends a signal to a process
///
/// # Arguments
/// * [`handle`] - The AppServiceClient handle
/// * [`pid`] - Process ID
/// * [`signal`] - Signal number
/// * [`response`] - Pointer to store the signal response (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_send_signal(
    handle: *mut AppServiceHandle,
    pid: u32,
    signal: u32,
    response: *mut *mut SignalResponseC,
) -> *mut IdeviceFfiError {
    if handle.is_null() || response.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.send_signal(pid, signal).await });

    match res {
        Ok(signal_response) => {
            let executable_url = match signal_response.process.executable_url {
                Some(url) => match CString::new(url.relative) {
                    Ok(s) => s.into_raw(),
                    Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                },
                None => ptr::null_mut(),
            };

            let timestamp: std::time::SystemTime = signal_response.device_timestamp.into();
            let timestamp = timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            let c_response = Box::new(SignalResponseC {
                pid: signal_response.process.pid,
                executable_url,
                device_timestamp: timestamp,
                signal: signal_response.signal,
            });

            unsafe { *response = Box::into_raw(c_response) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a SignalResponseC structure
///
/// # Safety
/// `response` must be a valid pointer allocated by app_service_send_signal
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_free_signal_response(response: *mut SignalResponseC) {
    if !response.is_null() {
        let response = unsafe { Box::from_raw(response) };
        if !response.executable_url.is_null() {
            let _ = unsafe { CString::from_raw(response.executable_url) };
        }
    }
}

/// Fetches an app icon
///
/// # Arguments
/// * [`handle`] - The AppServiceClient handle
/// * [`bundle_id`] - Bundle identifier of the app
/// * [`width`] - Icon width
/// * [`height`] - Icon height
/// * [`scale`] - Icon scale
/// * [`allow_placeholder`] - Whether to allow placeholder icons
/// * [`icon_data`] - Pointer to store the icon data (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_fetch_app_icon(
    handle: *mut AppServiceHandle,
    bundle_id: *const c_char,
    width: c_float,
    height: c_float,
    scale: c_float,
    allow_placeholder: c_int,
    icon_data: *mut *mut IconDataC,
) -> *mut IdeviceFfiError {
    if handle.is_null() || bundle_id.is_null() || icon_data.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id_str = match unsafe { CStr::from_ptr(bundle_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move {
        client
            .fetch_app_icon(bundle_id_str, width, height, scale, allow_placeholder != 0)
            .await
    });

    match res {
        Ok(icon) => {
            let data_vec: Vec<u8> = icon.data.into();
            let mut data_vec = data_vec.into_boxed_slice();
            let data_len = data_vec.len();
            let data_ptr = data_vec.as_mut_ptr();
            std::mem::forget(data_vec);

            let c_icon = Box::new(IconDataC {
                data: data_ptr,
                data_len,
                icon_width: icon.icon_width,
                icon_height: icon.icon_height,
                minimum_width: icon.minimum_width,
                minimum_height: icon.minimum_height,
            });

            unsafe { *icon_data = Box::into_raw(c_icon) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an IconDataC structure
///
/// # Safety
/// `icon_data` must be a valid pointer allocated by app_service_fetch_app_icon
#[unsafe(no_mangle)]
pub unsafe extern "C" fn app_service_free_icon_data(icon_data: *mut IconDataC) {
    if !icon_data.is_null() {
        let icon_data = unsafe { Box::from_raw(icon_data) };
        if !icon_data.data.is_null() && icon_data.data_len > 0 {
            let _ = unsafe { std::slice::from_raw_parts(icon_data.data, icon_data.data_len) };
        }
    }
}
