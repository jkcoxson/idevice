// Jackson Coxson

use crate::{IdeviceErrorCode, RUNTIME};
use idevice::dvt::remote_server::RemoteServerClient;
use idevice::{IdeviceError, ReadWrite};

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
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_server_new(
    socket: *mut Box<dyn ReadWrite>,
    handle: *mut *mut RemoteServerHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let connection = unsafe { Box::from_raw(socket) };

    let res: Result<RemoteServerClient<Box<dyn ReadWrite>>, IdeviceError> =
        RUNTIME.block_on(async move {
            let mut client = RemoteServerClient::new(*connection);
            client.read_message(0).await?; // Until Message has bindings, we'll do the first read
            Ok(client)
        });

    match res {
        Ok(client) => {
            let boxed = Box::new(RemoteServerHandle(client));
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
