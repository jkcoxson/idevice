// Jackson Coxson

use std::ffi::{CString, c_char};

use idevice::{
    IdeviceError, IdeviceService, core_device_proxy::CoreDeviceProxy, tcp::adapter::Adapter,
};

use crate::{
    IdeviceErrorCode, IdeviceHandle, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
};

pub struct CoreDeviceProxyHandle(pub CoreDeviceProxy);
pub struct AdapterHandle(pub Adapter);

/// Automatically creates and connects to Core Device Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - A TcpProvider
/// * [`client`] - On success, will be set to point to a newly allocated CoreDeviceProxy handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut CoreDeviceProxyHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<CoreDeviceProxy, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = CoreDeviceProxy::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(CoreDeviceProxyHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => {
            // If connection failed, the provider_box was already forgotten,
            // so we need to reconstruct it to avoid leak
            let _ = unsafe { Box::from_raw(provider) };
            e.into()
        }
    }
}

/// Automatically creates and connects to Core Device Proxy, returning a client handle
///
/// # Arguments
/// * [`provider`] - A UsbmuxdProvider
/// * [`client`] - On success, will be set to point to a newly allocated CoreDeviceProxy handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut CoreDeviceProxyHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<CoreDeviceProxy, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = CoreDeviceProxy::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(CoreDeviceProxyHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Automatically creates and connects to Core Device Proxy, returning a client handle
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated CoreDeviceProxy handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut CoreDeviceProxyHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r: Result<CoreDeviceProxy, IdeviceError> =
        RUNTIME.block_on(async move { CoreDeviceProxy::new(socket).await });
    match r {
        Ok(r) => {
            let boxed = Box::new(CoreDeviceProxyHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
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
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `length` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_send(
    handle: *mut CoreDeviceProxyHandle,
    data: *const u8,
    length: usize,
) -> IdeviceErrorCode {
    if handle.is_null() || data.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let proxy = unsafe { &mut (*handle).0 };
    let data_slice = unsafe { std::slice::from_raw_parts(data, length) };

    let res = RUNTIME.block_on(async move { proxy.send(data_slice).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
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
/// An error code indicating success or failure
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
) -> IdeviceErrorCode {
    if handle.is_null() || data.is_null() || length.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let proxy = unsafe { &mut (*handle).0 };

    let res = RUNTIME.block_on(async move { proxy.recv().await });

    match res {
        Ok(received_data) => {
            let received_len = received_data.len();
            if received_len > max_length {
                return IdeviceErrorCode::BufferTooSmall;
            }

            unsafe {
                std::ptr::copy_nonoverlapping(received_data.as_ptr(), data, received_len);
                *length = received_len;
            }

            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Gets the client parameters from the handshake
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`mtu`] - Pointer to store the MTU value
/// * [`address`] - Pointer to store the IP address string (must be at least 16 bytes)
/// * [`netmask`] - Pointer to store the netmask string (must be at least 16 bytes)
///
/// # Returns
/// An error code indicating success or failure
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
) -> IdeviceErrorCode {
    if handle.is_null() {
        log::error!("Passed null handle");
        return IdeviceErrorCode::InvalidArg;
    }

    let proxy = unsafe { &(*handle).0 };
    let params = &proxy.handshake.client_parameters;

    unsafe {
        *mtu = params.mtu;
    }

    unsafe {
        *address = CString::new(params.address.clone()).unwrap().into_raw();
        *netmask = CString::new(params.netmask.clone()).unwrap().into_raw();
    }

    IdeviceErrorCode::IdeviceSuccess
}

/// Gets the server address from the handshake
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`address`] - Pointer to store the server address string (must be at least 16 bytes)
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `address` must be a valid pointer to a buffer of at least 16 bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_get_server_address(
    handle: *mut CoreDeviceProxyHandle,
    address: *mut *mut c_char,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let proxy = unsafe { &(*handle).0 };

    unsafe {
        *address = CString::new(proxy.handshake.server_address.clone())
            .unwrap()
            .into_raw();
    }

    IdeviceErrorCode::IdeviceSuccess
}

/// Gets the server RSD port from the handshake
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`port`] - Pointer to store the port number
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `port` must be a valid pointer to a u16
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_get_server_rsd_port(
    handle: *mut CoreDeviceProxyHandle,
    port: *mut u16,
) -> IdeviceErrorCode {
    if handle.is_null() || port.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let proxy = unsafe { &(*handle).0 };
    unsafe {
        *port = proxy.handshake.server_rsd_port;
    }

    IdeviceErrorCode::IdeviceSuccess
}

/// Creates a software TCP tunnel adapter
///
/// # Arguments
/// * [`handle`] - The CoreDeviceProxy handle
/// * [`adapter`] - Pointer to store the newly created adapter handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library, and never used again
/// `adapter` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn core_device_proxy_create_tcp_adapter(
    handle: *mut CoreDeviceProxyHandle,
    adapter: *mut *mut AdapterHandle,
) -> IdeviceErrorCode {
    if handle.is_null() || adapter.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let proxy = unsafe { Box::from_raw(handle) };
    let result = proxy.0.create_software_tunnel();

    match result {
        Ok(adapter_obj) => {
            let boxed = Box::new(AdapterHandle(adapter_obj));
            unsafe { *adapter = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
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
        log::debug!("Freeing core_device_proxy");
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
        log::debug!("Freeing adapter");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
