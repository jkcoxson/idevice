// Jackson Coxson

use std::{
    ffi::{CStr, c_char},
    ptr::null_mut,
};

use idevice::{ReadWrite, dvt::process_control::ProcessControlClient};
use plist::{Dictionary, Value};

use crate::{IdeviceFfiError, RUNTIME, ffi_err, remote_server::RemoteServerHandle};

/// Opaque handle to a ProcessControlClient
pub struct ProcessControlHandle<'a>(pub ProcessControlClient<'a, Box<dyn ReadWrite>>);

/// Creates a new ProcessControlClient from a RemoteServerClient
///
/// # Arguments
/// * [`server`] - The RemoteServerClient to use
/// * [`handle`] - Pointer to store the newly created ProcessControlClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn process_control_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut ProcessControlHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = RUNTIME.block_on(async move { ProcessControlClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(ProcessControlHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a ProcessControlClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn process_control_free(handle: *mut ProcessControlHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Launches an application on the device
///
/// # Arguments
/// * [`handle`] - The ProcessControlClient handle
/// * [`bundle_id`] - The bundle identifier of the app to launch
/// * [`env_vars`] - NULL-terminated array of environment variables (format "KEY=VALUE")
/// * [`arguments`] - NULL-terminated array of arguments
/// * [`start_suspended`] - Whether to start the app suspended
/// * [`kill_existing`] - Whether to kill existing instances of the app
/// * [`pid`] - Pointer to store the process ID of the launched app
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid or NULL where appropriate
#[unsafe(no_mangle)]
pub unsafe extern "C" fn process_control_launch_app(
    handle: *mut ProcessControlHandle<'static>,
    bundle_id: *const c_char,
    env_vars: *const *const c_char,
    env_vars_count: usize,
    arguments: *const *const c_char,
    arguments_count: usize,
    start_suspended: bool,
    kill_existing: bool,
    pid: *mut u64,
) -> *mut IdeviceFfiError {
    if handle.is_null() || bundle_id.is_null() || pid.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let bundle_id = unsafe { CStr::from_ptr(bundle_id) };
    let bundle_id = match bundle_id.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let mut env_dict = Dictionary::new();
    if !env_vars.is_null() {
        let env_vars_slice = unsafe { std::slice::from_raw_parts(env_vars, env_vars_count) };
        for &env_var in env_vars_slice {
            if !env_var.is_null() {
                let env_var = unsafe { CStr::from_ptr(env_var) };
                if let Ok(env_var) = env_var.to_str()
                    && let Some((key, value)) = env_var.split_once('=')
                {
                    env_dict.insert(key.to_string(), Value::String(value.to_string()));
                }
            }
        }
    }

    let mut args_dict = Dictionary::new();
    if !arguments.is_null() {
        let args_slice = unsafe { std::slice::from_raw_parts(arguments, arguments_count) };
        for (i, &arg) in args_slice.iter().enumerate() {
            if !arg.is_null() {
                let arg = unsafe { CStr::from_ptr(arg) };
                if let Ok(arg) = arg.to_str() {
                    args_dict.insert(i.to_string(), Value::String(arg.to_string()));
                }
            }
        }
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move {
        client
            .launch_app(
                bundle_id,
                Some(env_dict),
                Some(args_dict),
                start_suspended,
                kill_existing,
            )
            .await
    });

    match res {
        Ok(p) => {
            unsafe { *pid = p };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Kills a running process
///
/// # Arguments
/// * [`handle`] - The ProcessControlClient handle
/// * [`pid`] - The process ID to kill
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn process_control_kill_app(
    handle: *mut ProcessControlHandle<'static>,
    pid: u64,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.kill_app(pid).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Disables memory limits for a process
///
/// # Arguments
/// * [`handle`] - The ProcessControlClient handle
/// * [`pid`] - The process ID to modify
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn process_control_disable_memory_limit(
    handle: *mut ProcessControlHandle<'static>,
    pid: u64,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.disable_memory_limit(pid).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}
