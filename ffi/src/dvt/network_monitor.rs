// Jackson Coxson

use std::{ffi::CString, ptr::null_mut};

use idevice::{
    ReadWrite,
    dvt::network_monitor::{NetworkEvent, NetworkMonitorClient},
};

use crate::{IdeviceFfiError, dvt::remote_server::RemoteServerHandle, ffi_err, run_sync};

/// Opaque handle to a NetworkMonitorClient
pub struct NetworkMonitorHandle<'a>(pub NetworkMonitorClient<'a, Box<dyn ReadWrite>>);

/// Network event type discriminant
#[repr(C)]
pub enum IdeviceNetworkEventType {
    InterfaceDetection = 0,
    ConnectionDetection = 1,
    ConnectionUpdate = 2,
    Unknown = 255,
}

/// A socket address (IPv4 or IPv6), represented as a null-terminated string + port
#[repr(C)]
pub struct IdeviceSocketAddress {
    /// Address family (e.g. 2 = AF_INET, 30 = AF_INET6)
    pub family: u8,
    pub port: u16,
    /// Null-terminated address string. Must be freed with `idevice_string_free`.
    pub addr: *mut std::ffi::c_char,
}

/// A network event emitted by the device
#[repr(C)]
pub struct IdeviceNetworkEvent {
    pub event_type: IdeviceNetworkEventType,

    // --- InterfaceDetection ---
    pub interface_index: u32,
    /// Null-terminated interface name. Must be freed with `idevice_string_free`.
    /// Only valid when event_type == InterfaceDetection.
    pub interface_name: *mut std::ffi::c_char,

    // --- ConnectionDetection ---
    pub local_addr: IdeviceSocketAddress,
    pub remote_addr: IdeviceSocketAddress,
    /// PID of the process owning the connection. Valid for ConnectionDetection.
    pub pid: u32,
    pub recv_buffer_size: u64,
    pub recv_buffer_used: u64,
    pub serial_number: u64,
    pub kind: u32,

    // --- ConnectionUpdate ---
    pub rx_packets: u64,
    pub rx_bytes: u64,
    pub tx_packets: u64,
    pub tx_bytes: u64,
    pub rx_dups: u64,
    pub rx_ooo: u64,
    pub tx_retx: u64,
    pub min_rtt: u64,
    pub avg_rtt: u64,
    pub connection_serial: u64,
    pub time: u64,

    // --- Unknown ---
    pub unknown_type: u64,
}

/// Frees an IdeviceNetworkEvent and its heap-allocated string fields
///
/// # Safety
/// `event` must be a valid pointer allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn network_monitor_event_free(event: *mut IdeviceNetworkEvent) {
    if event.is_null() {
        return;
    }
    let e = unsafe { Box::from_raw(event) };
    if !e.interface_name.is_null() {
        let _ = unsafe { CString::from_raw(e.interface_name) };
    }
    if !e.local_addr.addr.is_null() {
        let _ = unsafe { CString::from_raw(e.local_addr.addr) };
    }
    if !e.remote_addr.addr.is_null() {
        let _ = unsafe { CString::from_raw(e.remote_addr.addr) };
    }
}

/// Creates a new NetworkMonitorClient from a RemoteServerClient
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn network_monitor_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut NetworkMonitorHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = run_sync(async move { NetworkMonitorClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(NetworkMonitorHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a NetworkMonitorClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn network_monitor_free(handle: *mut NetworkMonitorHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Starts network monitoring. No reply is expected.
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn network_monitor_start(
    handle: *mut NetworkMonitorHandle<'static>,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.start_monitoring().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Stops network monitoring.
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn network_monitor_stop(
    handle: *mut NetworkMonitorHandle<'static>,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.stop_monitoring().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

fn make_socket_addr(
    sa: Option<idevice::dvt::network_monitor::SocketAddress>,
) -> IdeviceSocketAddress {
    match sa {
        Some(a) => IdeviceSocketAddress {
            family: a.family,
            port: a.port,
            addr: CString::new(a.addr).unwrap_or_default().into_raw(),
        },
        None => IdeviceSocketAddress {
            family: 0,
            port: 0,
            addr: null_mut(),
        },
    }
}

/// Reads the next network event pushed by the device. Blocks until an event arrives.
///
/// # Arguments
/// * [`handle`] - The NetworkMonitorClient handle
/// * [`event_out`] - On success, set to a heap-allocated IdeviceNetworkEvent
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null. Free the event with `network_monitor_event_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn network_monitor_next_event(
    handle: *mut NetworkMonitorHandle<'static>,
    event_out: *mut *mut IdeviceNetworkEvent,
) -> *mut IdeviceFfiError {
    if handle.is_null() || event_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.next_event().await });

    match res {
        Ok(event) => {
            let c_event = match event {
                NetworkEvent::InterfaceDetection(e) => IdeviceNetworkEvent {
                    event_type: IdeviceNetworkEventType::InterfaceDetection,
                    interface_index: e.interface_index,
                    interface_name: CString::new(e.name).unwrap_or_default().into_raw(),
                    local_addr: make_socket_addr(None),
                    remote_addr: make_socket_addr(None),
                    pid: 0,
                    recv_buffer_size: 0,
                    recv_buffer_used: 0,
                    serial_number: 0,
                    kind: 0,
                    rx_packets: 0,
                    rx_bytes: 0,
                    tx_packets: 0,
                    tx_bytes: 0,
                    rx_dups: 0,
                    rx_ooo: 0,
                    tx_retx: 0,
                    min_rtt: 0,
                    avg_rtt: 0,
                    connection_serial: 0,
                    time: 0,
                    unknown_type: 0,
                },
                NetworkEvent::ConnectionDetection(e) => IdeviceNetworkEvent {
                    event_type: IdeviceNetworkEventType::ConnectionDetection,
                    interface_index: e.interface_index,
                    interface_name: null_mut(),
                    local_addr: make_socket_addr(e.local_address),
                    remote_addr: make_socket_addr(e.remote_address),
                    pid: e.pid,
                    recv_buffer_size: e.recv_buffer_size,
                    recv_buffer_used: e.recv_buffer_used,
                    serial_number: e.serial_number,
                    kind: e.kind,
                    rx_packets: 0,
                    rx_bytes: 0,
                    tx_packets: 0,
                    tx_bytes: 0,
                    rx_dups: 0,
                    rx_ooo: 0,
                    tx_retx: 0,
                    min_rtt: 0,
                    avg_rtt: 0,
                    connection_serial: 0,
                    time: 0,
                    unknown_type: 0,
                },
                NetworkEvent::ConnectionUpdate(e) => IdeviceNetworkEvent {
                    event_type: IdeviceNetworkEventType::ConnectionUpdate,
                    interface_index: 0,
                    interface_name: null_mut(),
                    local_addr: make_socket_addr(None),
                    remote_addr: make_socket_addr(None),
                    pid: 0,
                    recv_buffer_size: 0,
                    recv_buffer_used: 0,
                    serial_number: 0,
                    kind: 0,
                    rx_packets: e.rx_packets,
                    rx_bytes: e.rx_bytes,
                    tx_packets: e.tx_packets,
                    tx_bytes: e.tx_bytes,
                    rx_dups: e.rx_dups,
                    rx_ooo: e.rx_ooo,
                    tx_retx: e.tx_retx,
                    min_rtt: e.min_rtt,
                    avg_rtt: e.avg_rtt,
                    connection_serial: e.connection_serial,
                    time: e.time,
                    unknown_type: 0,
                },
                NetworkEvent::Unknown(t) => IdeviceNetworkEvent {
                    event_type: IdeviceNetworkEventType::Unknown,
                    interface_index: 0,
                    interface_name: null_mut(),
                    local_addr: make_socket_addr(None),
                    remote_addr: make_socket_addr(None),
                    pid: 0,
                    recv_buffer_size: 0,
                    recv_buffer_used: 0,
                    serial_number: 0,
                    kind: 0,
                    rx_packets: 0,
                    rx_bytes: 0,
                    tx_packets: 0,
                    tx_bytes: 0,
                    rx_dups: 0,
                    rx_ooo: 0,
                    tx_retx: 0,
                    min_rtt: 0,
                    avg_rtt: 0,
                    connection_serial: 0,
                    time: 0,
                    unknown_type: t,
                },
            };
            unsafe { *event_out = Box::into_raw(Box::new(c_event)) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
