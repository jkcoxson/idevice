// Jackson Coxson

pub mod adapter;
pub mod core_device_proxy;
pub mod debug_proxy;
mod errors;
pub mod heartbeat;
pub mod installation_proxy;
pub mod lockdownd;
pub mod logging;
pub mod mounter;
mod pairing_file;
pub mod process_control;
pub mod provider;
pub mod remote_server;
pub mod remotexpc;
pub mod usbmuxd;
pub mod util;

pub use errors::*;
pub use pairing_file::*;

use idevice::{Idevice, IdeviceSocket};
use once_cell::sync::Lazy;
use std::ffi::{CStr, CString, c_char};
use tokio::runtime::{self, Runtime};

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
});

pub const LOCKDOWN_PORT: u16 = 62078;

/// Opaque C-compatible handle to an Idevice connection
pub struct IdeviceHandle(pub Idevice);
pub struct IdeviceSocketHandle(IdeviceSocket);

// https://github.com/mozilla/cbindgen/issues/539
#[allow(non_camel_case_types, unused)]
struct sockaddr;

/// Creates a new Idevice connection
///
/// # Arguments
/// * [`socket`] - Socket for communication with the device
/// * [`label`] - Label for the connection
/// * [`idevice`] - On success, will be set to point to a newly allocated Idevice handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `label` must be a valid null-terminated C string
/// `idevice` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_new(
    socket: *mut IdeviceSocketHandle,
    label: *const c_char,
    idevice: *mut *mut IdeviceHandle,
) -> IdeviceErrorCode {
    if socket.is_null() || label.is_null() || idevice.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    // Get socket ownership
    let socket_box = unsafe { Box::from_raw(socket) };

    // Convert C string to Rust string
    let c_str = match unsafe { CStr::from_ptr(label).to_str() } {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidString,
    };

    // Create new Idevice instance
    let dev = Idevice::new((*socket_box).0, c_str);
    let boxed = Box::new(IdeviceHandle(dev));
    unsafe { *idevice = Box::into_raw(boxed) };

    IdeviceErrorCode::IdeviceSuccess
}

/// Creates a new Idevice connection
///
/// # Arguments
/// * [`addr`] - The socket address to connect to
/// * [`addr_len`] - Length of the socket
/// * [`label`] - Label for the connection
/// * [`idevice`] - On success, will be set to point to a newly allocated Idevice handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid sockaddr
/// `label` must be a valid null-terminated C string
/// `idevice` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_new_tcp_socket(
    addr: *const libc::sockaddr,
    addr_len: libc::socklen_t,
    label: *const c_char,
    idevice: *mut *mut IdeviceHandle,
) -> IdeviceErrorCode {
    if addr.is_null() {
        log::error!("socket addr null pointer");
        return IdeviceErrorCode::InvalidArg;
    }

    // Convert C string to Rust string
    let label = match unsafe { CStr::from_ptr(label).to_str() } {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let addr = match util::c_socket_to_rust(addr, addr_len) {
        Ok(a) => a,
        Err(e) => return e,
    };

    let device: Result<idevice::Idevice, idevice::IdeviceError> = RUNTIME.block_on(async move {
        Ok(idevice::Idevice::new(
            Box::new(tokio::net::TcpStream::connect(addr).await?),
            label,
        ))
    });

    match device {
        Ok(dev) => {
            let boxed = Box::new(IdeviceHandle(dev));
            unsafe { *idevice = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Gets the device type
///
/// # Arguments
/// * [`idevice`] - The Idevice handle
/// * [`device_type`] - On success, will be set to point to a newly allocated string containing the device type
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `idevice` must be a valid, non-null pointer to an Idevice handle
/// `device_type` must be a valid, non-null pointer to a location where the string pointer will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_get_type(
    idevice: *mut IdeviceHandle,
    device_type: *mut *mut c_char,
) -> IdeviceErrorCode {
    if idevice.is_null() || device_type.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    // Get the Idevice reference
    let dev = unsafe { &mut (*idevice).0 };

    // Run the get_type method in the runtime
    let result = RUNTIME.block_on(async { dev.get_type().await });

    match result {
        Ok(type_str) => match CString::new(type_str) {
            Ok(c_string) => {
                unsafe { *device_type = c_string.into_raw() };
                IdeviceErrorCode::IdeviceSuccess
            }
            Err(_) => IdeviceErrorCode::InvalidString,
        },
        Err(e) => e.into(),
    }
}

/// Performs RSD checkin
///
/// # Arguments
/// * [`idevice`] - The Idevice handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `idevice` must be a valid, non-null pointer to an Idevice handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_rsd_checkin(idevice: *mut IdeviceHandle) -> IdeviceErrorCode {
    if idevice.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    // Get the Idevice reference
    let dev = unsafe { &mut (*idevice).0 };

    // Run the rsd_checkin method in the runtime
    let result = RUNTIME.block_on(async { dev.rsd_checkin().await });

    match result {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Starts a TLS session
///
/// # Arguments
/// * [`idevice`] - The Idevice handle
/// * [`pairing_file`] - The pairing file to use for TLS
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `idevice` must be a valid, non-null pointer to an Idevice handle
/// `pairing_file` must be a valid, non-null pointer to a pairing file handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_start_session(
    idevice: *mut IdeviceHandle,
    pairing_file: *const IdevicePairingFile,
) -> IdeviceErrorCode {
    if idevice.is_null() || pairing_file.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    // Get the Idevice reference
    let dev = unsafe { &mut (*idevice).0 };

    // Get the pairing file reference
    let pf = unsafe { &(*pairing_file).0 };

    // Run the start_session method in the runtime
    let result = RUNTIME.block_on(async { dev.start_session(pf).await });

    match result {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Frees an Idevice handle
///
/// # Arguments
/// * [`idevice`] - The Idevice handle to free
///
/// # Safety
/// `idevice` must be a valid pointer to an Idevice handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_free(idevice: *mut IdeviceHandle) {
    if !idevice.is_null() {
        let _ = unsafe { Box::from_raw(idevice) };
    }
}

/// Frees a string allocated by this library
///
/// # Arguments
/// * [`string`] - The string to free
///
/// # Safety
/// `string` must be a valid pointer to a string that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_string_free(string: *mut c_char) {
    if !string.is_null() {
        let _ = unsafe { CString::from_raw(string) };
    }
}
