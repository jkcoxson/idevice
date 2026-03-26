// Jackson Coxson

use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, bt_packet_logger::BtPacketLoggerClient, provider::IdeviceProvider,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync_local,
};

pub struct BtPacketLoggerClientHandle(pub BtPacketLoggerClient);

/// Represents a parsed BT packet from the logger
#[repr(C)]
pub struct BtPacketHandle {
    /// Header: advisory length
    pub length: u32,
    /// Header: timestamp seconds
    pub ts_secs: u32,
    /// Header: timestamp microseconds
    pub ts_usecs: u32,
    /// Packet kind byte (0x00=HciCmd, 0x01=HciEvt, 0x02=AclSent, 0x03=AclRecv, etc.)
    pub kind: u8,
    /// H4-ready payload data
    pub h4_data: *mut u8,
    /// Length of h4_data
    pub h4_data_len: usize,
}

/// Automatically creates and connects to BTPacketLogger, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated BtPacketLoggerClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bt_packet_logger_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut BtPacketLoggerClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<BtPacketLoggerClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        BtPacketLoggerClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(BtPacketLoggerClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Creates a new BtPacketLoggerClient from an existing socket
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated BtPacketLoggerClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bt_packet_logger_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut BtPacketLoggerClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = BtPacketLoggerClient::new(socket);
    let boxed = Box::new(BtPacketLoggerClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Reads the next BT packet from the logger
///
/// # Arguments
/// * `client` - A valid BtPacketLoggerClient handle
/// * `packet` - On success, will be set to point to a newly allocated BtPacketHandle.
///   May be set to NULL if EOF was reached.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// The returned packet must be freed with `bt_packet_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bt_packet_logger_next_packet(
    client: *mut BtPacketLoggerClientHandle,
    packet: *mut *mut BtPacketHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() || packet.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.next_packet().await
    });
    match res {
        Ok(maybe) => match maybe {
            Some((hdr, kind, h4)) => {
                let kind_byte = match kind {
                    idevice::bt_packet_logger::BtPacketKind::HciCmd => 0x00,
                    idevice::bt_packet_logger::BtPacketKind::HciEvt => 0x01,
                    idevice::bt_packet_logger::BtPacketKind::AclSent => 0x02,
                    idevice::bt_packet_logger::BtPacketKind::AclRecv => 0x03,
                    idevice::bt_packet_logger::BtPacketKind::ScoSent => 0x08,
                    idevice::bt_packet_logger::BtPacketKind::ScoRecv => 0x09,
                    idevice::bt_packet_logger::BtPacketKind::Other(b) => b,
                };
                let h4_len = h4.len();
                let mut h4_boxed = h4.into_boxed_slice();
                let h4_ptr = h4_boxed.as_mut_ptr();
                std::mem::forget(h4_boxed);

                let handle = Box::new(BtPacketHandle {
                    length: hdr.length,
                    ts_secs: hdr.ts_secs,
                    ts_usecs: hdr.ts_usecs,
                    kind: kind_byte,
                    h4_data: h4_ptr,
                    h4_data_len: h4_len,
                });
                unsafe { *packet = Box::into_raw(handle) };
                null_mut()
            }
            None => {
                unsafe { *packet = null_mut() };
                null_mut()
            }
        },
        Err(e) => ffi_err!(e),
    }
}

/// Frees a BtPacketHandle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bt_packet_free(handle: *mut BtPacketHandle) {
    if !handle.is_null() {
        let handle = unsafe { Box::from_raw(handle) };
        if !handle.h4_data.is_null() && handle.h4_data_len > 0 {
            let _ = unsafe {
                Vec::from_raw_parts(handle.h4_data, handle.h4_data_len, handle.h4_data_len)
            };
        }
    }
}

/// Frees a BtPacketLoggerClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bt_packet_logger_client_free(handle: *mut BtPacketLoggerClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing BtPacketLoggerClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
