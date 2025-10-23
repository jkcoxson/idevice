// Jackson Coxson

use std::{
    ffi::{CStr, CString, c_char},
    pin::Pin,
    ptr::null_mut,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, IdevicePairingFile, ffi_err, run_sync, run_sync_local,
    util::{SockAddr, c_socket_to_rust, idevice_sockaddr, idevice_socklen_t},
};
use futures::{Stream, StreamExt};
use idevice::{
    IdeviceError,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection, UsbmuxdDevice, UsbmuxdListenEvent},
};
use tracing::error;

pub struct UsbmuxdConnectionHandle(pub UsbmuxdConnection);
pub struct UsbmuxdAddrHandle(pub UsbmuxdAddr);
pub struct UsbmuxdDeviceHandle(pub UsbmuxdDevice);
pub struct UsbmuxdListenerHandle<'a>(
    Pin<Box<dyn Stream<Item = Result<UsbmuxdListenEvent, IdeviceError>> + 'a>>,
);

/// Connects to a usbmuxd instance over TCP
///
/// # Arguments
/// * [`addr`] - The socket address to connect to
/// * [`addr_len`] - Length of the socket
/// * [`tag`] - A tag that will be returned by usbmuxd responses
/// * [`usbmuxd_connection`] - On success, will be set to point to a newly allocated UsbmuxdConnection handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `addr` must be a valid sockaddr
/// `usbmuxd_connection` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_new_tcp_connection(
    addr: *const idevice_sockaddr,
    addr_len: idevice_socklen_t,
    tag: u32,
    out: *mut *mut UsbmuxdConnectionHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null() || out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Reinterpret as the real platform sockaddr for parsing
    let addr = addr as *const SockAddr;

    let addr = match c_socket_to_rust(addr, addr_len as _) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
    };

    let res = run_sync(async move {
        let stream = tokio::net::TcpStream::connect(addr).await?;
        Ok::<_, IdeviceError>(UsbmuxdConnection::new(Box::new(stream), tag))
    });

    match res {
        Ok(conn) => {
            unsafe { *out = Box::into_raw(Box::new(UsbmuxdConnectionHandle(conn))) };
            std::ptr::null_mut()
        }
        Err(e) => ffi_err!(e),
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
/// An IdeviceFfiError on error, null on success
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
) -> *mut IdeviceFfiError {
    let addr = match unsafe { CStr::from_ptr(addr).to_str() } {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<UsbmuxdConnection, IdeviceError> = run_sync(async move {
        let stream = tokio::net::UnixStream::connect(addr).await?;
        Ok(UsbmuxdConnection::new(Box::new(stream), tag))
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(UsbmuxdConnectionHandle(r));
            unsafe { *usbmuxd_connection = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
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
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `addr` must be a valid CStr
/// `usbmuxd_connection` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_new_default_connection(
    tag: u32,
    usbmuxd_connection: *mut *mut UsbmuxdConnectionHandle,
) -> *mut IdeviceFfiError {
    let addr = match UsbmuxdAddr::from_env_var() {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Invalid address set: {e:?}");
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
    };

    let res: Result<UsbmuxdConnection, IdeviceError> =
        run_sync(async move { addr.connect(tag).await });

    match res {
        Ok(r) => {
            let boxed = Box::new(UsbmuxdConnectionHandle(r));
            unsafe { *usbmuxd_connection = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets a list of connected devices from usbmuxd.
///
/// The returned list must be freed with `idevice_usbmuxd_device_list_free`.
///
/// # Arguments
/// * `usbmuxd_conn` - A valid connection to usbmuxd.
/// * `devices` - A pointer to a C-style array of `UsbmuxdDeviceHandle` pointers. On success, this will be filled.
/// * `count` - A pointer to an integer. On success, this will be filled with the number of devices found.
///
/// # Returns
/// An `IdeviceFfiError` on error, `null` on success.
///
/// # Safety
/// * `usbmuxd_conn` must be a valid pointer.
/// * `devices` and `count` must be valid, non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_get_devices(
    usbmuxd_conn: *mut UsbmuxdConnectionHandle,
    devices: *mut *mut *mut UsbmuxdDeviceHandle,
    count: *mut libc::c_int,
) -> *mut IdeviceFfiError {
    if usbmuxd_conn.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let conn = unsafe { &mut (*usbmuxd_conn).0 };

    let res = run_sync(async { conn.get_devices().await });

    match res {
        Ok(device_vec) => {
            unsafe {
                *count = device_vec.len() as libc::c_int;
            }
            let mut c_arr = Vec::with_capacity(device_vec.len());
            for device in device_vec {
                let handle = Box::new(UsbmuxdDeviceHandle(device));
                c_arr.push(Box::into_raw(handle));
            }
            let mut c_arr = c_arr.into_boxed_slice();
            unsafe {
                *devices = c_arr.as_mut_ptr();
            }
            std::mem::forget(c_arr); // Prevent deallocation of the slice's buffer
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Connects to a service on a given device.
///
/// This function consumes the `UsbmuxdConnectionHandle`. The handle will be invalid after this call
/// and must not be used again. The caller is NOT responsible for freeing it.
/// A new `IdeviceHandle` is returned on success, which must be freed by the caller.
///
/// # Arguments
/// * `usbmuxd_connection` - The connection to use. It will be consumed.
/// * `device_id` - The ID of the device to connect to.
/// * `port` - The TCP port on the device to connect to.
/// * `idevice` - On success, points to the new device connection handle.
///
/// # Returns
/// An `IdeviceFfiError` on error, `null` on success.
///
/// # Safety
/// * `usbmuxd_connection` must be a valid pointer allocated by this library and never used again.
///   The value is consumed.
/// * `idevice` must be a valid, non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_connect_to_device(
    usbmuxd_connection: *mut UsbmuxdConnectionHandle,
    device_id: u32,
    port: u16,
    label: *const c_char,
    idevice: *mut *mut IdeviceHandle,
) -> *mut IdeviceFfiError {
    if usbmuxd_connection.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Take ownership of the connection handle
    let conn = unsafe {
        let conn = std::ptr::read(&(*usbmuxd_connection).0); // move the inner connection
        drop(Box::from_raw(usbmuxd_connection)); // free the wrapper
        conn
    };

    let label = unsafe {
        match CStr::from_ptr(label).to_str() {
            Ok(s) => s,
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
        }
    };

    let res = run_sync(async move { conn.connect_to_device(device_id, port, label).await });

    match res {
        Ok(device_conn) => {
            let boxed = Box::new(IdeviceHandle(device_conn));
            unsafe {
                *idevice = Box::into_raw(boxed);
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads the pairing record for a given device UDID.
///
/// The returned `PairingFileHandle` must be freed with `idevice_pair_record_free`.
///
/// # Arguments
/// * `usbmuxd_conn` - A valid connection to usbmuxd.
/// * `udid` - The UDID of the device.
/// * `pair_record` - On success, points to the new pairing file handle.
///
/// # Returns
/// An `IdeviceFfiError` on error, `null` on success.
///
/// # Safety
/// * `usbmuxd_conn` must be a valid pointer.
/// * `udid` must be a valid, null-terminated C string.
/// * `pair_record` must be a valid, non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_get_pair_record(
    usbmuxd_conn: *mut UsbmuxdConnectionHandle,
    udid: *const c_char,
    pair_record: *mut *mut IdevicePairingFile,
) -> *mut IdeviceFfiError {
    if usbmuxd_conn.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let conn = unsafe { &mut (*usbmuxd_conn).0 };

    let udid_str = unsafe {
        match CStr::from_ptr(udid).to_str() {
            Ok(s) => s,
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
        }
    };

    let res = run_sync(async { conn.get_pair_record(udid_str).await });

    match res {
        Ok(pf) => {
            let boxed = Box::new(IdevicePairingFile(pf));
            unsafe {
                *pair_record = Box::into_raw(boxed);
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Saves the pairing record for a given device UDID.
///
/// # Arguments
/// * `usbmuxd_conn` - A valid connection to usbmuxd.
/// * `device_id` - The muxer ID for the device
/// * `udid` - The UDID of the device.
/// * `pair_record` - The bytes of the pairing record plist to save
/// * `pair_record_len` - the length of the pairing record bytes
///
/// # Returns
/// An `IdeviceFfiError` on error, `null` on success.
///
/// # Safety
/// * `usbmuxd_conn` must be a valid pointer.
/// * `udid` must be a valid, null-terminated C string.
/// * `pair_record` must be a valid, non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_save_pair_record(
    usbmuxd_conn: *mut UsbmuxdConnectionHandle,
    device_id: u32,
    udid: *const c_char,
    pair_record: *mut u8,
    pair_record_len: usize,
) -> *mut IdeviceFfiError {
    if usbmuxd_conn.is_null() || pair_record.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let conn = unsafe { &mut (*usbmuxd_conn).0 };
    let pair_record =
        unsafe { std::slice::from_raw_parts_mut(pair_record, pair_record_len) }.to_vec();

    let udid_str = unsafe {
        match CStr::from_ptr(udid).to_str() {
            Ok(s) => s,
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
        }
    };

    let res = run_sync_local(async {
        conn.save_pair_record(device_id, udid_str, pair_record)
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Listens on the socket for connections and disconnections
///
/// # Safety
/// Pass valid pointers. Free the stream with ``idevice_usbmuxd_listener_handle_free``.
/// The stream must outlive the usbmuxd connection, and the usbmuxd connection cannot
/// be used for other requests.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_listen(
    usbmuxd_conn: *mut UsbmuxdConnectionHandle,
    stream_handle: *mut *mut UsbmuxdListenerHandle,
) -> *mut IdeviceFfiError {
    if usbmuxd_conn.is_null() || stream_handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let conn = unsafe { &mut (*usbmuxd_conn).0 };

    let res = run_sync_local(async { conn.listen().await });

    match res {
        Ok(s) => {
            unsafe { *stream_handle = Box::into_raw(Box::new(UsbmuxdListenerHandle(s))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a stream created by ``listen`` or does nothing on null
///
/// # Safety
/// Pass a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_listener_handle_free(
    stream_handle: *mut UsbmuxdListenerHandle,
) {
    if stream_handle.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(stream_handle) };
}

/// Gets the next event from the stream.
/// Connect will be set to true if the event is a connection event,
/// and the connection_device will be filled with the device information.
/// If connection is false, the mux ID of the device will be filled.
///
/// # Arguments
/// * `stream_handle` - The handle to the stream returned by listen
/// * `connect` - The bool that will be set
/// * `connection_device` - The pointer that will be filled on a connect event
/// * `disconnection_id` - The mux ID that will be set on a disconnect event
///
/// # Safety
/// Pass valid pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_listener_next(
    stream_handle: *mut UsbmuxdListenerHandle,
    connect: *mut bool,
    connection_device: *mut *mut UsbmuxdDeviceHandle,
    disconnection_id: *mut u32,
) -> *mut IdeviceFfiError {
    if stream_handle.is_null()
        || connect.is_null()
        || connection_device.is_null()
        || disconnection_id.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let stream = unsafe { &mut (*stream_handle).0 };

    let res = run_sync_local(async { stream.next().await });

    match res {
        Some(res) => match res {
            Ok(s) => {
                match s {
                    UsbmuxdListenEvent::Connected(usbmuxd_device) => {
                        unsafe { *connect = true };
                        unsafe {
                            *connection_device =
                                Box::into_raw(Box::new(UsbmuxdDeviceHandle(usbmuxd_device)))
                        };
                    }
                    UsbmuxdListenEvent::Disconnected(id) => unsafe { *disconnection_id = id },
                }
                null_mut()
            }
            Err(e) => ffi_err!(e),
        },
        None => {
            ffi_err!(IdeviceError::Socket(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "end of stream"
            )))
        }
    }
}

/// Reads the BUID (Boot-Unique ID) from usbmuxd.
///
/// The returned string must be freed with `idevice_string_free`.
///
/// # Arguments
/// * `usbmuxd_conn` - A valid connection to usbmuxd.
/// * `buid` - On success, points to a newly allocated, null-terminated C string.
///
/// # Returns
/// An `IdeviceFfiError` on error, `null` on success.
///
/// # Safety
/// * `usbmuxd_conn` must be a valid pointer.
/// * `buid` must be a valid, non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_get_buid(
    usbmuxd_conn: *mut UsbmuxdConnectionHandle,
    buid: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if usbmuxd_conn.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let conn = unsafe { &mut (*usbmuxd_conn).0 };

    let res = run_sync(async { conn.get_buid().await });

    match res {
        Ok(buid_str) => match CString::new(buid_str) {
            Ok(c_str) => {
                unsafe { *buid = c_str.into_raw() };
                null_mut()
            }
            Err(e) => {
                error!("Unable to convert BUID string to CString: {e:?}. Null interior byte.");
                ffi_err!(IdeviceError::UnexpectedResponse)
            }
        },
        Err(e) => ffi_err!(e),
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
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `addr` must be a valid sockaddr
/// `usbmuxd_Addr` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_tcp_addr_new(
    addr: *const idevice_sockaddr, // <- portable
    addr_len: idevice_socklen_t,
    usbmuxd_addr: *mut *mut UsbmuxdAddrHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null() || usbmuxd_addr.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    // Reinterpret as the real platform sockaddr for parsing
    let addr = addr as *const SockAddr;

    let addr = match c_socket_to_rust(addr, addr_len as _) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
    };

    let u = UsbmuxdAddr::TcpSocket(addr);
    let boxed = Box::new(UsbmuxdAddrHandle(u));
    unsafe {
        *usbmuxd_addr = Box::into_raw(boxed);
    }
    std::ptr::null_mut()
}

/// Creates a new UsbmuxdAddr struct with a unix socket
///
/// # Arguments
/// * [`addr`] - The socket path to connect to
/// * [`usbmuxd_addr`] - On success, will be set to point to a newly allocated UsbmuxdAddr handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `addr` must be a valid CStr
/// `usbmuxd_addr` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
#[cfg(unix)]
pub unsafe extern "C" fn idevice_usbmuxd_unix_addr_new(
    addr: *const c_char,
    usbmuxd_addr: *mut *mut UsbmuxdAddrHandle,
) -> *mut IdeviceFfiError {
    let addr = match unsafe { CStr::from_ptr(addr).to_str() } {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let u = UsbmuxdAddr::UnixSocket(addr.to_string());

    let boxed = Box::new(UsbmuxdAddrHandle(u));
    unsafe { *usbmuxd_addr = Box::into_raw(boxed) };
    null_mut()
}

/// Creates a default UsbmuxdAddr struct for the platform
///
/// # Arguments
/// * [`usbmuxd_addr`] - On success, will be set to point to a newly allocated UsbmuxdAddr handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `usbmuxd_addr` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_default_addr_new(
    usbmuxd_addr: *mut *mut UsbmuxdAddrHandle,
) -> *mut IdeviceFfiError {
    let addr = UsbmuxdAddr::default();
    let boxed = Box::new(UsbmuxdAddrHandle(addr));
    unsafe { *usbmuxd_addr = Box::into_raw(boxed) };
    null_mut()
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

/// Frees a list of devices returned by `idevice_usbmuxd_get_devices`.
///
/// # Arguments
/// * `devices` - The array of device handles to free.
/// * `count` - The number of elements in the array.
///
/// # Safety
/// `devices` must be a valid pointer to an array of `count` device handles
/// allocated by this library, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_device_list_free(
    devices: *mut *mut UsbmuxdDeviceHandle,
    count: libc::c_int,
) {
    if devices.is_null() {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(devices, count as usize) };
    for &mut ptr in slice {
        if !ptr.is_null() {
            let _ = unsafe { Box::from_raw(ptr) };
        }
    }
}

/// Frees a usbmuxd device
///
/// # Arguments
/// * `device` - The device handle to free.
///
/// # Safety
/// `device` must be a valid pointer to the device handle
/// allocated by this library, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_device_free(device: *mut UsbmuxdDeviceHandle) {
    if device.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(device) };
}

/// Gets the UDID from a device handle.
/// The returned string must be freed by the caller using `idevice_string_free`.
///
/// # Safety
/// `device` must be a valid pointer to a `UsbmuxdDeviceHandle`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_device_get_udid(
    device: *const UsbmuxdDeviceHandle,
) -> *mut c_char {
    if device.is_null() {
        return null_mut();
    }
    let device = unsafe { &(*device).0 };
    match CString::new(device.udid.as_str()) {
        Ok(s) => s.into_raw(),
        Err(_) => null_mut(),
    }
}

/// Gets the device ID from a device handle.
///
/// # Safety
/// `device` must be a valid pointer to a `UsbmuxdDeviceHandle`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_device_get_device_id(
    device: *const UsbmuxdDeviceHandle,
) -> u32 {
    if device.is_null() {
        return 0;
    }
    unsafe { (*device).0.device_id }
}

#[repr(C)]
enum UsbmuxdConnectionType {
    Usb = 1,
    Network = 2,
    Unknown = 3,
}

/// Gets the connection type (UsbmuxdConnectionType) from a device handle.
///
/// # Returns
/// The enum value of the connection type, or 0 for null device handles
///
/// # Safety
/// `device` must be a valid pointer to a `UsbmuxdDeviceHandle`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_usbmuxd_device_get_connection_type(
    device: *const UsbmuxdDeviceHandle,
) -> u8 {
    if device.is_null() {
        return 0;
    }
    let ct = unsafe { &(*device).0.connection_type };

    let ct = match ct {
        idevice::usbmuxd::Connection::Usb => UsbmuxdConnectionType::Usb,
        idevice::usbmuxd::Connection::Network(_) => UsbmuxdConnectionType::Network,
        idevice::usbmuxd::Connection::Unknown(_) => UsbmuxdConnectionType::Unknown,
    };
    ct as u8
}
