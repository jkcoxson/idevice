// Jackson Coxson

#[cfg(feature = "tunnel_tcp_stack")]
pub mod adapter;
#[cfg(feature = "afc")]
pub mod afc;
#[cfg(feature = "amfi")]
pub mod amfi;
#[cfg(feature = "core_device")]
pub mod core_device;
#[cfg(feature = "core_device_proxy")]
pub mod core_device_proxy;
#[cfg(feature = "debug_proxy")]
pub mod debug_proxy;
mod errors;
#[cfg(feature = "heartbeat")]
pub mod heartbeat;
#[cfg(feature = "installation_proxy")]
pub mod installation_proxy;
#[cfg(feature = "location_simulation")]
pub mod location_simulation;
pub mod lockdown;
pub mod logging;
#[cfg(feature = "misagent")]
pub mod misagent;
#[cfg(feature = "mobile_image_mounter")]
pub mod mobile_image_mounter;
#[cfg(feature = "syslog_relay")]
pub mod os_trace_relay;
mod pairing_file;
#[cfg(feature = "dvt")]
pub mod process_control;
pub mod provider;
#[cfg(feature = "dvt")]
pub mod remote_server;
#[cfg(feature = "xpc")]
pub mod rsd;
#[cfg(feature = "springboardservices")]
pub mod springboardservices;
#[cfg(feature = "syslog_relay")]
pub mod syslog_relay;
#[cfg(feature = "usbmuxd")]
pub mod usbmuxd;
pub mod util;

pub use errors::*;
pub use pairing_file::*;

use idevice::{Idevice, IdeviceSocket, ReadWrite};
use once_cell::sync::Lazy;
use std::{
    ffi::{CStr, CString, c_char},
    ptr::null_mut,
};
use tokio::runtime::{self, Runtime};

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
});

pub const LOCKDOWN_PORT: u16 = 62078;

#[repr(C)]
pub struct ReadWriteOpaque {
    pub inner: Option<Box<dyn ReadWrite>>,
}

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
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `label` must be a valid null-terminated C string
/// `idevice` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_new(
    socket: *mut IdeviceSocketHandle,
    label: *const c_char,
    idevice: *mut *mut IdeviceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || label.is_null() || idevice.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Get socket ownership
    let socket_box = unsafe { Box::from_raw(socket) };

    // Convert C string to Rust string
    let c_str = match unsafe { CStr::from_ptr(label).to_str() } {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    // Create new Idevice instance
    let dev = Idevice::new((*socket_box).0, c_str);
    let boxed = Box::new(IdeviceHandle(dev));
    unsafe { *idevice = Box::into_raw(boxed) };

    null_mut()
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
/// An IdeviceFfiError on error, null on success
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
) -> *mut IdeviceFfiError {
    if addr.is_null() {
        log::error!("socket addr null pointer");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Convert C string to Rust string
    let label = match unsafe { CStr::from_ptr(label).to_str() } {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let addr = match util::c_socket_to_rust(addr, addr_len) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
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
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets the device type
///
/// # Arguments
/// * [`idevice`] - The Idevice handle
/// * [`device_type`] - On success, will be set to point to a newly allocated string containing the device type
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `idevice` must be a valid, non-null pointer to an Idevice handle
/// `device_type` must be a valid, non-null pointer to a location where the string pointer will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_get_type(
    idevice: *mut IdeviceHandle,
    device_type: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if idevice.is_null() || device_type.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Get the Idevice reference
    let dev = unsafe { &mut (*idevice).0 };

    // Run the get_type method in the runtime
    let result = RUNTIME.block_on(async { dev.get_type().await });

    match result {
        Ok(type_str) => match CString::new(type_str) {
            Ok(c_string) => {
                unsafe { *device_type = c_string.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Performs RSD checkin
///
/// # Arguments
/// * [`idevice`] - The Idevice handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `idevice` must be a valid, non-null pointer to an Idevice handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_rsd_checkin(idevice: *mut IdeviceHandle) -> *mut IdeviceFfiError {
    if idevice.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Get the Idevice reference
    let dev = unsafe { &mut (*idevice).0 };

    // Run the rsd_checkin method in the runtime
    let result = RUNTIME.block_on(async { dev.rsd_checkin().await });

    match result {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Starts a TLS session
///
/// # Arguments
/// * [`idevice`] - The Idevice handle
/// * [`pairing_file`] - The pairing file to use for TLS
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `idevice` must be a valid, non-null pointer to an Idevice handle
/// `pairing_file` must be a valid, non-null pointer to a pairing file handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_start_session(
    idevice: *mut IdeviceHandle,
    pairing_file: *const IdevicePairingFile,
) -> *mut IdeviceFfiError {
    if idevice.is_null() || pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Get the Idevice reference
    let dev = unsafe { &mut (*idevice).0 };

    // Get the pairing file reference
    let pf = unsafe { &(*pairing_file).0 };

    // Run the start_session method in the runtime
    let result = RUNTIME.block_on(async { dev.start_session(pf).await });

    match result {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
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
