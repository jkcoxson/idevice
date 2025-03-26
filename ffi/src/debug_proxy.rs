// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::os::raw::c_int;
use std::ptr;

use idevice::debug_proxy::{DebugProxyClient, DebugserverCommand};
use idevice::tcp::adapter::Adapter;

use crate::core_device_proxy::AdapterHandle;
use crate::{IdeviceErrorCode, RUNTIME};

/// Opaque handle to a DebugProxyClient
pub struct DebugProxyAdapterHandle(pub DebugProxyClient<Adapter>);

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
        name: CString::new(name).unwrap().into_raw(),
        argv: if argv_vec.is_empty() {
            ptr::null_mut()
        } else {
            let mut argv_ptrs: Vec<*mut c_char> = argv_vec
                .into_iter()
                .map(|s| CString::new(s).unwrap().into_raw())
                .collect();
            argv_ptrs.shrink_to_fit();
            argv_ptrs.as_mut_ptr()
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
/// * [`socket`] - The socket to use for communication
/// * [`handle`] - Pointer to store the newly created DebugProxyClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_adapter_new(
    socket: *mut AdapterHandle,
    handle: *mut *mut DebugProxyAdapterHandle,
) -> IdeviceErrorCode {
    if socket.is_null() || handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let socket = unsafe { Box::from_raw(socket) };
    let client = DebugProxyClient::new(socket.0);

    let boxed = Box::new(DebugProxyAdapterHandle(client));
    unsafe { *handle = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Frees a DebugProxyClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_free(handle: *mut DebugProxyAdapterHandle) {
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
/// An error code indicating success or failure
///
/// # Safety
/// `handle` and `command` must be valid pointers
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_command(
    handle: *mut DebugProxyAdapterHandle,
    command: *mut DebugserverCommandHandle,
    response: *mut *mut c_char,
) -> IdeviceErrorCode {
    if handle.is_null() || command.is_null() || response.is_null() {
        return IdeviceErrorCode::InvalidArg;
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
            let cstr = CString::new(r).unwrap();
            unsafe { *response = cstr.into_raw() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Ok(None) => {
            unsafe { *response = ptr::null_mut() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Reads a response from the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
/// * [`response`] - Pointer to store the response (caller must free)
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_read_response(
    handle: *mut DebugProxyAdapterHandle,
    response: *mut *mut c_char,
) -> IdeviceErrorCode {
    if handle.is_null() || response.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.read_response().await });

    match res {
        Ok(Some(r)) => {
            let cstr = CString::new(r).unwrap();
            unsafe { *response = cstr.into_raw() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Ok(None) => {
            unsafe { *response = ptr::null_mut() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer
/// `data` must be a valid pointer to `len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_raw(
    handle: *mut DebugProxyAdapterHandle,
    data: *const u8,
    len: usize,
) -> IdeviceErrorCode {
    if handle.is_null() || data.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let data_slice = unsafe { std::slice::from_raw_parts(data, len) };
    let res = RUNTIME.block_on(async move { client.send_raw(data_slice).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_read(
    handle: *mut DebugProxyAdapterHandle,
    len: usize,
    response: *mut *mut c_char,
) -> IdeviceErrorCode {
    if handle.is_null() || response.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.read(len).await });

    match res {
        Ok(r) => {
            let cstr = CString::new(r).unwrap();
            unsafe { *response = cstr.into_raw() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer
/// `argv` must be a valid pointer to `argv_count` C strings or NULL
/// `response` must be a valid pointer to a location where the string will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_set_argv(
    handle: *mut DebugProxyAdapterHandle,
    argv: *const *const c_char,
    argv_count: usize,
    response: *mut *mut c_char,
) -> IdeviceErrorCode {
    if handle.is_null() || response.is_null() {
        return IdeviceErrorCode::InvalidArg;
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
            let cstr = CString::new(r).unwrap();
            unsafe { *response = cstr.into_raw() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Sends an ACK to the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_ack(
    handle: *mut DebugProxyAdapterHandle,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.send_ack().await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Sends a NACK to the debug proxy
///
/// # Arguments
/// * [`handle`] - The DebugProxyClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_send_nack(
    handle: *mut DebugProxyAdapterHandle,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.send_noack().await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
pub unsafe extern "C" fn debug_proxy_set_ack_mode(
    handle: *mut DebugProxyAdapterHandle,
    enabled: c_int,
) {
    if !handle.is_null() {
        let client = unsafe { &mut (*handle).0 };
        client.set_ack_mode(enabled != 0);
    }
}

/// Returns the underlying socket from a DebugProxyClient
///
/// # Arguments
/// * [`handle`] - The handle to get the socket from
/// * [`adapter`] - The newly allocated ConnectionHandle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL, and never used again
#[unsafe(no_mangle)]
pub unsafe extern "C" fn debug_proxy_adapter_into_inner(
    handle: *mut DebugProxyAdapterHandle,
    adapter: *mut *mut AdapterHandle,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { Box::from_raw(handle) };
    let socket_obj = client.0.into_inner();
    let boxed = Box::new(AdapterHandle(socket_obj));
    unsafe { *adapter = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}
