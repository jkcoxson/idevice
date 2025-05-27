// Jackson Coxson

use crate::core_device_proxy::AdapterHandle;
use crate::rsd::RsdHandshakeHandle;
use crate::{IdeviceErrorCode, RUNTIME, ReadWriteOpaque};
use idevice::dvt::remote_server::RemoteServerClient;
use idevice::tcp::stream::AdapterStream;
use idevice::{IdeviceError, ReadWrite, RsdService};

/// Opaque handle to a RemoteServerClient
pub struct RemoteServerHandle(pub RemoteServerClient<Box<dyn ReadWrite>>);

/// Creates a new RemoteServerClient from a ReadWrite connection
///
/// # Arguments
/// * [`socket`] - The connection to use for communication, an object that implements ReadWrite
/// * [`handle`] - Pointer to store the newly created RemoteServerClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. It is consumed and may
/// not be used again.
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_server_new(
    socket: *mut ReadWriteOpaque,
    handle: *mut *mut RemoteServerHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let wrapper = unsafe { &mut *socket };

    let res: Result<RemoteServerClient<Box<dyn ReadWrite>>, IdeviceError> =
        match wrapper.inner.take() {
            Some(stream) => RUNTIME.block_on(async move {
                let mut client = RemoteServerClient::new(stream);
                client.read_message(0).await?;
                Ok(client)
            }),
            None => return IdeviceErrorCode::InvalidArg,
        };

    match res {
        Ok(client) => {
            let boxed = Box::new(RemoteServerHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Creates a new RemoteServerClient from a handshake and adapter
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `handshake` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_server_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut RemoteServerHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || handshake.is_null() || handshake.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let res: Result<RemoteServerClient<AdapterStream>, IdeviceError> =
        RUNTIME.block_on(async move {
            let provider_ref = unsafe { &mut (*provider).0 };
            let handshake_ref = unsafe { &mut (*handshake).0 };

            // Connect using the reference
            RemoteServerClient::connect_rsd(provider_ref, handshake_ref).await
        });

    match res {
        Ok(d) => {
            let boxed = Box::new(RemoteServerHandle(RemoteServerClient::new(Box::new(
                d.into_inner(),
            ))));
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
pub unsafe extern "C" fn remote_server_free(handle: *mut RemoteServerHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
