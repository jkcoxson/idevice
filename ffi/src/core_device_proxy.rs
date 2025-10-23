// Jackson Coxson

use std::{
    ffi::{CString, c_char},
    ptr::null_mut,
};

use idevice::{
    IdeviceError, IdeviceService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
};

use crate::{IdeviceFfiError, IdeviceHandle, RUNTIME, ffi_err, provider::IdeviceProviderHandle};

pub struct CoreDeviceProxyHandle(pub CoreDeviceProxy);
pub struct AdapterHandle(pub idevice::tcp::handle::AdapterHandle);

/// Automatically creates and connects to Core Device Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated CoreDeviceProxy handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut CoreDeviceProxyHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<CoreDeviceProxy, IdeviceError> = RUNTIME.block_on(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };

        // Connect using the reference
        CoreDeviceProxy::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(CoreDeviceProxyHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Automatically creates and connects to Core Device Proxy, returning a client handle
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated CoreDeviceProxy handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. It is consumed and
/// may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut CoreDeviceProxyHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r: Result<CoreDeviceProxy, IdeviceError> =
        RUNTIME.block_on(async move { CoreDeviceProxy::new(socket).await });
    match r {
        Ok(r) => {
            let boxed = Box::new(CoreDeviceProxyHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Sends data through the CoreDeviceProxy tunnel
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`data`] - The data to send
/// * [`length`] - The length of the data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `length` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_send(
    handle: *mut CoreDeviceProxyHandle,
    data: *const u8,
    length: usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let proxy = unsafe { &mut (*handle).0 };
    let data_slice = unsafe { std::slice::from_raw_parts(data, length) };

    let res = RUNTIME.block_on(async move { proxy.send(data_slice).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Receives data from the CoreDeviceProxy tunnel
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`data`] - Pointer to a buffer where the received data will be stored
/// * [`length`] - Pointer to store the actual length of received data
/// * [`max_length`] - Maximum number of bytes that can be stored in `data`
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `max_length` bytes
/// `length` must be a valid pointer to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_recv(
    handle: *mut CoreDeviceProxyHandle,
    data: *mut u8,
    length: *mut usize,
    max_length: usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() || length.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let proxy = unsafe { &mut (*handle).0 };

    let res = RUNTIME.block_on(async move { proxy.recv().await });

    match res {
        Ok(received_data) => {
            let received_len = received_data.len();
            if received_len > max_length {
                return ffi_err!(IdeviceError::FfiBufferTooSmall(received_len, max_length));
            }

            unsafe {
                std::ptr::copy_nonoverlapping(received_data.as_ptr(), data, received_len);
                *length = received_len;
            }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets the client parameters from the handshake
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`mtu`] - Pointer to store the MTU value
/// * [`address`] - Pointer to store the IP address string
/// * [`netmask`] - Pointer to store the netmask string
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `mtu` must be a valid pointer to a u16
/// `address` and `netmask` must be valid pointers to buffers of at least 16 bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_get_client_parameters(
    handle: *mut CoreDeviceProxyHandle,
    mtu: *mut u16,
    address: *mut *mut c_char,
    netmask: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        tracing::error!("Passed null handle");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let proxy = unsafe { &(*handle).0 };
    let params = &proxy.handshake.client_parameters;

    unsafe {
        *mtu = params.mtu;
    }

    // Allocate both strings, but handle partial failure
    let address_cstring = match CString::new(params.address.clone()) {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let netmask_cstring = match CString::new(params.netmask.clone()) {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    // Only assign to output pointers after both succeed
    unsafe {
        *address = address_cstring.into_raw();
        *netmask = netmask_cstring.into_raw();
    }

    null_mut()
}

/// Gets the server address from the handshake
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`address`] - Pointer to store the server address string
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `address` must be a valid pointer to a buffer of at least 16 bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_get_server_address(
    handle: *mut CoreDeviceProxyHandle,
    address: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let proxy = unsafe { &(*handle).0 };

    unsafe {
        *address = match CString::new(proxy.handshake.server_address.clone()) {
            Ok(s) => s.into_raw(),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        };
    }

    null_mut()
}

/// Gets the server RSD port from the handshake
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`port`] - Pointer to store the port number
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `port` must be a valid pointer to a u16
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_get_server_rsd_port(
    handle: *mut CoreDeviceProxyHandle,
    port: *mut u16,
) -> *mut IdeviceFfiError {
    if handle.is_null() || port.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let proxy = unsafe { &(*handle).0 };
    unsafe {
        *port = proxy.handshake.server_rsd_port;
    }

    null_mut()
}

/// Creates a software TCP tunnel adapter
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`adapter`] - Pointer to store the newly created adapter handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library, and never used again
/// `adapter` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_create_tcp_adapter(
    handle: *mut CoreDeviceProxyHandle,
    adapter: *mut *mut AdapterHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() || adapter.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let proxy = unsafe { Box::from_raw(handle) };
    let result = proxy.0.create_software_tunnel();

    match result {
        Ok(adapter_obj) => {
            // We have to run this in the RUNTIME since we're spawning a new thread
            let adapter_handle = RUNTIME.block_on(async move { adapter_obj.to_async_handle() });

            let boxed = Box::new(AdapterHandle(adapter_handle));
            unsafe { *adapter = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_free(handle: *mut CoreDeviceProxyHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing core_device_proxy");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Frees a handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_free(handle: *mut AdapterHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing adapter");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
