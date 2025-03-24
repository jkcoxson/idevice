// Jackson Coxson

use std::ffi::{CStr, c_char};

use crate::{IdeviceErrorCode, RUNTIME, util::c_socket_to_rust};
use idevice::{
    IdeviceError,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};

pub struct UsbmuxdConnectionHandle(pub UsbmuxdConnection);
pub struct UsbmuxdAddrHandle(pub UsbmuxdAddr);

/// Connects to a usbmuxd instance over TCP
///
/// # Arguments
/// * [`addr`] - The socket address to connect to
/// * [`addr_len`] - Length of the socket
/// * [`tag`] - A tag that will be returned by usbmuxd responses
/// * [`usbmuxd_connection`] - On success, will be set to point to a newly allocated UsbmuxdConnection handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid sockaddr
/// `usbmuxd_connection` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_new_tcp_connection(
    addr: *const libc::sockaddr,
    addr_len: libc::socklen_t,
    tag: u32,
    usbmuxd_connection: *mut *mut UsbmuxdConnectionHandle,
) -> IdeviceErrorCode {
    let addr = match c_socket_to_rust(addr, addr_len) {
        Ok(a) => a,
        Err(e) => return e,
    };

    let res: Result<UsbmuxdConnection, IdeviceError> = RUNTIME.block_on(async move {
        let stream = tokio::net::TcpStream::connect(addr).await?;
        Ok(UsbmuxdConnection::new(Box::new(stream), tag))
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(UsbmuxdConnectionHandle(r));
            unsafe { *usbmuxd_connection = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Connects to a usbmuxd instance over unix socket
///
/// # Arguments
/// * [`addr`] - The socket path to connect to
/// * [`tag`] - A tag that will be returned by usbmuxd responses
/// * [`usbmuxd_connection`] - On success, will be set to point to a newly allocated UsbmuxdConnection handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid CStr
/// `usbmuxd_connection` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
#[cfg(unix)]
pub unsafe extern "C" fn idevice_usbmuxd_new_unix_socket_connection(
    addr: *const c_char,
    tag: u32,
    usbmuxd_connection: *mut *mut UsbmuxdConnectionHandle,
) -> IdeviceErrorCode {
    let addr = match unsafe { CStr::from_ptr(addr).to_str() } {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let res: Result<UsbmuxdConnection, IdeviceError> = RUNTIME.block_on(async move {
        let stream = tokio::net::UnixStream::connect(addr).await?;
        Ok(UsbmuxdConnection::new(Box::new(stream), tag))
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(UsbmuxdConnectionHandle(r));
            unsafe { *usbmuxd_connection = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Connects to a usbmuxd instance over the default connection for the platform
///
/// # Arguments
/// * [`addr`] - The socket path to connect to
/// * [`tag`] - A tag that will be returned by usbmuxd responses
/// * [`usbmuxd_connection`] - On success, will be set to point to a newly allocated UsbmuxdConnection handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid CStr
/// `usbmuxd_connection` must be a valid, non-null pointer to a location where the handle will be stored
pub unsafe extern "C" fn idevice_usbmuxd_new_default_connection(
    tag: u32,
    usbmuxd_connection: *mut *mut UsbmuxdConnectionHandle,
) -> IdeviceErrorCode {
    let addr = match UsbmuxdAddr::from_env_var() {
        Ok(a) => a,
        Err(e) => {
            log::error!("Invalid address set: {e:?}");
            return IdeviceErrorCode::InvalidArg;
        }
    };

    let res: Result<UsbmuxdConnection, IdeviceError> =
        RUNTIME.block_on(async move { addr.connect(tag).await });

    match res {
        Ok(r) => {
            let boxed = Box::new(UsbmuxdConnectionHandle(r));
            unsafe { *usbmuxd_connection = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees a UsbmuxdConnection handle
///
/// # Arguments
/// * [`usbmuxd_connection`] - The UsbmuxdConnection handle to free
///
/// # Safety
/// `usbmuxd_connection` must be a valid pointer to a UsbmuxdConnection handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_connection_free(
    usbmuxd_connection: *mut UsbmuxdConnectionHandle,
) {
    if !usbmuxd_connection.is_null() {
        let _ = unsafe { Box::from_raw(usbmuxd_connection) };
    }
}

/// Creates a usbmuxd TCP address struct
///
/// # Arguments
/// * [`addr`] - The socket address to connect to
/// * [`addr_len`] - Length of the socket
/// * [`usbmuxd_addr`] - On success, will be set to point to a newly allocated UsbmuxdAddr handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid sockaddr
/// `usbmuxd_Addr` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_tcp_addr_new(
    addr: *const libc::sockaddr,
    addr_len: libc::socklen_t,
    usbmuxd_addr: *mut *mut UsbmuxdAddrHandle,
) -> IdeviceErrorCode {
    let addr = match c_socket_to_rust(addr, addr_len) {
        Ok(a) => a,
        Err(e) => return e,
    };

    let u = UsbmuxdAddr::TcpSocket(addr);

    let boxed = Box::new(UsbmuxdAddrHandle(u));
    unsafe { *usbmuxd_addr = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Creates a new UsbmuxdAddr struct with a unix socket
///
/// # Arguments
/// * [`addr`] - The socket path to connect to
/// * [`usbmuxd_addr`] - On success, will be set to point to a newly allocated UsbmuxdAddr handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `addr` must be a valid CStr
/// `usbmuxd_addr` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
#[cfg(unix)]
pub unsafe extern "C" fn idevice_usbmuxd_unix_addr_new(
    addr: *const c_char,
    usbmuxd_addr: *mut *mut UsbmuxdAddrHandle,
) -> IdeviceErrorCode {
    let addr = match unsafe { CStr::from_ptr(addr).to_str() } {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let u = UsbmuxdAddr::UnixSocket(addr.to_string());

    let boxed = Box::new(UsbmuxdAddrHandle(u));
    unsafe { *usbmuxd_addr = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Frees a UsbmuxdAddr handle
///
/// # Arguments
/// * [`usbmuxd_addr`] - The UsbmuxdAddr handle to free
///
/// # Safety
/// `usbmuxd_addr` must be a valid pointer to a UsbmuxdAddr handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_addr_free(usbmuxd_addr: *mut UsbmuxdAddrHandle) {
    if !usbmuxd_addr.is_null() {
        let _ = unsafe { Box::from_raw(usbmuxd_addr) };
    }
}
