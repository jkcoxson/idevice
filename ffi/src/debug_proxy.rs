// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::os::raw::c_int;
use std::ptr::{self, null_mut};

use idevice::debug_proxy::{DebugProxyClient, DebugserverCommand};
use idevice::{IdeviceError, ReadWrite, RsdService};

use crate::core_device_proxy::AdapterHandle;
use crate::rsd::RsdHandshakeHandle;
use crate::{IdeviceFfiError, RUNTIME, ffi_err};

/// Opaque handle to a DebugProxyClient
pub struct DebugProxyHandle(pub DebugProxyClient<Box<dyn ReadWrite>>);

/// Represents a debugserver command
#[repr(C)]
pub struct DebugserverCommandHandle {
    pub name: *mut c_char,
    pub argv: *mut *mut c_char,
    pub argv_count: usize,
}

/// Creates a new DebugserverCommand
///
/// # Safety
/// Caller must free with debugserver_command_free
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debugserver_command_new(
    name: *const c_char,
    argv: *const *const c_char,
    argv_count: usize,
) -> *mut DebugserverCommandHandle {
    if name.is_null() {
        return ptr::null_mut();
    }

    let name_cstr = unsafe { CStr::from_ptr(name) };
    let name = match name_cstr.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ptr::null_mut(),
    };

    let mut argv_vec = Vec::new();
    if !argv.is_null() && argv_count > 0 {
        let argv_slice = unsafe { std::slice::from_raw_parts(argv, argv_count) };
        for &arg in argv_slice {
            if !arg.is_null() {
                let arg_cstr = unsafe { CStr::from_ptr(arg) };
                if let Ok(arg_str) = arg_cstr.to_str() {
                    argv_vec.push(arg_str.to_string());
                }
            }
        }
    }
    let argv_len = argv_vec.len();

    let boxed = Box::new(DebugserverCommandHandle {
        name: match CString::new(name) {
            Ok(n) => n.into_raw(),
            Err(_) => return ptr::null_mut(),
        },
        argv: if argv_vec.is_empty() {
            ptr::null_mut()
        } else {
            let argv_ptrs: Result<Vec<*mut c_char>, _> = argv_vec
                .into_iter()
                .map(|s| CString::new(s).map(|cs| cs.into_raw()))
                .collect();

            let mut argv_ptrs = match argv_ptrs {
                Ok(ptrs) => ptrs,
                Err(_) => return ptr::null_mut(),
            };
            argv_ptrs.shrink_to_fit();
            let ptr = argv_ptrs.as_mut_ptr();
            std::mem::forget(argv_ptrs);
            ptr
        },
        argv_count: argv_len,
    });

    Box::into_raw(boxed)
}

/// Frees a DebugserverCommand
///
/// # Safety
/// `command` must be a valid pointer or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debugserver_command_free(command: *mut DebugserverCommandHandle) {
    if !command.is_null() {
        let command = unsafe { Box::from_raw(command) };

        // Free name
        if !command.name.is_null() {
            let _ = unsafe { CString::from_raw(command.name) };
        }

        // Free argv
        if !command.argv.is_null() && command.argv_count > 0 {
            let argv_slice =
                unsafe { std::slice::from_raw_parts_mut(command.argv, command.argv_count) };
            for &mut arg in argv_slice {
                if !arg.is_null() {
                    let _ = unsafe { CString::from_raw(arg) };
                }
            }
            let _ = unsafe {
                Vec::from_raw_parts(command.argv, command.argv_count, command.argv_count)
            };
        }
    }
}

/// Creates a new DebugProxyClient
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `handshake` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut DebugProxyHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || handshake.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<DebugProxyClient<Box<dyn ReadWrite>>, IdeviceError> =
        RUNTIME.block_on(async move {
            let provider_ref = unsafe { &mut (*provider).0 };
            let handshake_ref = unsafe { &mut (*handshake).0 };

            // Connect using the reference
            DebugProxyClient::connect_rsd(provider_ref, handshake_ref).await
        });

    match res {
        Ok(d) => {
            let boxed = Box::new(DebugProxyHandle(DebugProxyClient::new(Box::new(
                d.into_inner(),
            ))));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new DebugProxyClient
///
/// # Arguments
/// * [`socket`] - The socket to use for communication. Any object that supports ReadWrite.
/// * [`handle`] - Pointer to store the newly created DebugProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_new(
    socket: *mut Box<dyn ReadWrite>,
    handle: *mut *mut DebugProxyHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket = unsafe { Box::from_raw(socket) };
    let client = DebugProxyClient::new(*socket);
    let new_handle = DebugProxyHandle(client);

    unsafe { *handle = Box::into_raw(Box::new(new_handle)) };
    null_mut()
}

/// Frees a DebugProxyClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_free(handle: *mut DebugProxyHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Sends a command to the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`command`] - The command to send
/// * [`response`] - Pointer to store the response (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` and `command` must be valid pointers
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_command(
    handle: *mut DebugProxyHandle,
    command: *mut DebugserverCommandHandle,
    response: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || command.is_null() || response.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let cmd = DebugserverCommand {
        name: unsafe {
            CStr::from_ptr((*command).name)
                .to_string_lossy()
                .into_owned()
        },
        argv: if unsafe { &*command }.argv.is_null() {
            Vec::new()
        } else {
            let argv_slice =
                unsafe { std::slice::from_raw_parts((*command).argv, (*command).argv_count) };
            argv_slice
                .iter()
                .map(|&arg| unsafe { CStr::from_ptr(arg).to_string_lossy().into_owned() })
                .collect()
        },
    };

    let res = RUNTIME.block_on(async move { client.send_command(cmd).await });

    match res {
        Ok(Some(r)) => {
            let cstr = match CString::new(r) {
                Ok(c) => c,
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            };
            unsafe { *response = cstr.into_raw() };
            null_mut()
        }
        Ok(None) => {
            unsafe { *response = ptr::null_mut() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads a response from the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`response`] - Pointer to store the response (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_read_response(
    handle: *mut DebugProxyHandle,
    response: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || response.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.read_response().await });

    match res {
        Ok(Some(r)) => {
            let cstr = match CString::new(r) {
                Ok(c) => c,
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            };
            unsafe { *response = cstr.into_raw() };
            null_mut()
        }
        Ok(None) => {
            unsafe { *response = ptr::null_mut() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Sends raw data to the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`data`] - The data to send
/// * [`len`] - Length of the data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer
/// `data` must be a valid pointer to `len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_raw(
    handle: *mut DebugProxyHandle,
    data: *const u8,
    len: usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let data_slice = unsafe { std::slice::from_raw_parts(data, len) };
    let res = RUNTIME.block_on(async move { client.send_raw(data_slice).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads data from the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`len`] - Maximum number of bytes to read
/// * [`response`] - Pointer to store the response (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_read(
    handle: *mut DebugProxyHandle,
    len: usize,
    response: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || response.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.read(len).await });

    match res {
        Ok(r) => {
            let cstr = match CString::new(r) {
                Ok(c) => c,
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            };
            unsafe { *response = cstr.into_raw() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Sets the argv for the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`argv`] - NULL-terminated array of arguments
/// * [`argv_count`] - Number of arguments
/// * [`response`] - Pointer to store the response (caller must free)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer
/// `argv` must be a valid pointer to `argv_count` C strings or NULL
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_set_argv(
    handle: *mut DebugProxyHandle,
    argv: *const *const c_char,
    argv_count: usize,
    response: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || response.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let argv_vec = if argv.is_null() || argv_count == 0 {
        Vec::new()
    } else {
        let argv_slice = unsafe { std::slice::from_raw_parts(argv, argv_count) };
        argv_slice
            .iter()
            .filter_map(|&arg| {
                if arg.is_null() {
                    None
                } else {
                    Some(unsafe { CStr::from_ptr(arg).to_string_lossy().into_owned() })
                }
            })
            .collect()
    };

    let res = RUNTIME.block_on(async move { client.set_argv(argv_vec).await });

    match res {
        Ok(r) => {
            let cstr = match CString::new(r) {
                Ok(c) => c,
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            };
            unsafe { *response = cstr.into_raw() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Sends an ACK to the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_ack(
    handle: *mut DebugProxyHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.send_ack().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Sends a NACK to the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_nack(
    handle: *mut DebugProxyHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.send_noack().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Sets the ACK mode for the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`enabled`] - Whether ACK mode should be enabled
///
/// # Safety
/// `handle` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_set_ack_mode(handle: *mut DebugProxyHandle, enabled: c_int) {
    if !handle.is_null() {
        let client = unsafe { &mut (*handle).0 };
        client.set_ack_mode(enabled != 0);
    }
}
