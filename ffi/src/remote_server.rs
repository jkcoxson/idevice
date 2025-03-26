// Jackson Coxson

use crate::core_device_proxy::AdapterHandle;
use crate::{IdeviceErrorCode, RUNTIME};
use idevice::IdeviceError;
use idevice::dvt::remote_server::RemoteServerClient;
use idevice::tcp::adapter::Adapter;

/// Opaque handle to a RemoteServerClient
pub struct RemoteServerAdapterHandle(pub RemoteServerClient<Adapter>);

/// Creates a new RemoteServerClient from a ReadWrite connection
///
/// # Arguments
/// * [`connection`] - The connection to use for communication
/// * [`handle`] - Pointer to store the newly created RemoteServerClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `connection` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_server_adapter_new(
    adapter: *mut crate::core_device_proxy::AdapterHandle,
    handle: *mut *mut RemoteServerAdapterHandle,
) -> IdeviceErrorCode {
    if adapter.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let connection = unsafe { Box::from_raw(adapter) };

    let res: Result<RemoteServerClient<Adapter>, IdeviceError> = RUNTIME.block_on(async move {
        let mut client = RemoteServerClient::new(connection.0);
        client.read_message(0).await?; // Until Message has bindings, we'll do the first read
        Ok(client)
    });

    match res {
        Ok(client) => {
            let boxed = Box::new(RemoteServerAdapterHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees a RemoteServerClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_server_free(handle: *mut RemoteServerAdapterHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Returns the underlying connection from a RemoteServerClient
///
/// # Arguments
/// * [`handle`] - The handle to get the connection from
/// * [`connection`] - The newly allocated ConnectionHandle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL, and never used again
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_server_adapter_into_inner(
    handle: *mut RemoteServerAdapterHandle,
    connection: *mut *mut AdapterHandle,
) -> IdeviceErrorCode {
    if handle.is_null() || connection.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let server = unsafe { Box::from_raw(handle) };
    let connection_obj = server.0.into_inner();
    let boxed = Box::new(AdapterHandle(connection_obj));
    unsafe { *connection = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}
