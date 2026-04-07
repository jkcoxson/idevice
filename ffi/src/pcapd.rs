// Jackson Coxson

use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, RsdService, pcapd::PcapdClient, provider::IdeviceProvider,
};

use crate::{
    IdeviceFfiError, IdeviceHandle, core_device_proxy::AdapterHandle, ffi_err,
    provider::IdeviceProviderHandle, rsd::RsdHandshakeHandle, run_sync_local,
};

pub struct PcapdClientHandle(pub PcapdClient);

/// Represents a captured device packet from pcapd
#[repr(C)]
pub struct DevicePacketHandle {
    pub header_length: u32,
    pub header_version: u8,
    pub packet_length: u32,
    pub interface_type: u8,
    pub unit: u16,
    pub io: u8,
    pub protocol_family: u32,
    pub frame_pre_length: u32,
    pub frame_post_length: u32,
    pub interface_name: *mut std::ffi::c_char,
    pub pid: u32,
    pub comm: *mut std::ffi::c_char,
    pub svc: u32,
    pub epid: u32,
    pub ecomm: *mut std::ffi::c_char,
    pub seconds: u32,
    pub microseconds: u32,
    pub data: *mut u8,
    pub data_len: usize,
}

/// Automatically creates and connects to pcapd, returning a client handle.
/// Note that this service only works over USB or through RSD.
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated PcapdClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pcapd_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut PcapdClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<PcapdClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        PcapdClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(PcapdClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            ffi_err!(e)
        }
    }
}

/// Creates a new PcapdClient via RSD
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`client`] - On success, will be set to point to a newly allocated PcapdClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `handshake` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pcapd_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    client: *mut *mut PcapdClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<PcapdClient, IdeviceError> = run_sync_local(async move {
        let provider_ref = unsafe { &mut (*provider).0 };
        let handshake_ref = unsafe { &mut (*handshake).0 };
        PcapdClient::connect_rsd(provider_ref, handshake_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(PcapdClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new PcapdClient from an existing socket
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated PcapdClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pcapd_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut PcapdClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = PcapdClient::new(socket);
    let boxed = Box::new(PcapdClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Reads the next packet from the pcapd service
///
/// # Arguments
/// * `client` - A valid PcapdClient handle
/// * `packet` - On success, will be set to point to a newly allocated DevicePacketHandle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// The returned packet must be freed with `pcapd_device_packet_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pcapd_next_packet(
    client: *mut PcapdClientHandle,
    packet: *mut *mut DevicePacketHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() || packet.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.next_packet().await
    });
    match res {
        Ok(p) => {
            let interface_name = std::ffi::CString::new(p.interface_name)
                .unwrap_or_default()
                .into_raw();
            let comm = std::ffi::CString::new(p.comm)
                .unwrap_or_default()
                .into_raw();
            let ecomm = std::ffi::CString::new(p.ecomm)
                .unwrap_or_default()
                .into_raw();
            let data_len = p.data.len();
            let mut data = p.data.into_boxed_slice();
            let data_ptr = data.as_mut_ptr();
            std::mem::forget(data);

            let handle = Box::new(DevicePacketHandle {
                header_length: p.header_length,
                header_version: p.header_version,
                packet_length: p.packet_length,
                interface_type: p.interface_type,
                unit: p.unit,
                io: p.io,
                protocol_family: p.protocol_family,
                frame_pre_length: p.frame_pre_length,
                frame_post_length: p.frame_post_length,
                interface_name,
                pid: p.pid,
                comm,
                svc: p.svc,
                epid: p.epid,
                ecomm,
                seconds: p.seconds,
                microseconds: p.microseconds,
                data: data_ptr,
                data_len,
            });
            unsafe { *packet = Box::into_raw(handle) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a DevicePacketHandle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pcapd_device_packet_free(handle: *mut DevicePacketHandle) {
    if !handle.is_null() {
        let handle = unsafe { Box::from_raw(handle) };
        if !handle.interface_name.is_null() {
            let _ = unsafe { std::ffi::CString::from_raw(handle.interface_name) };
        }
        if !handle.comm.is_null() {
            let _ = unsafe { std::ffi::CString::from_raw(handle.comm) };
        }
        if !handle.ecomm.is_null() {
            let _ = unsafe { std::ffi::CString::from_raw(handle.ecomm) };
        }
        if !handle.data.is_null() && handle.data_len > 0 {
            let _ = unsafe { Vec::from_raw_parts(handle.data, handle.data_len, handle.data_len) };
        }
    }
}

/// Frees a PcapdClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pcapd_client_free(handle: *mut PcapdClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing PcapdClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
