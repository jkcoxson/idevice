// Jackson Coxson

use idevice::{ReadWrite, dvt::location_simulation::LocationSimulationClient};

use crate::{IdeviceErrorCode, RUNTIME, remote_server::RemoteServerHandle};

/// Opaque handle to a ProcessControlClient
pub struct LocationSimulationHandle<'a>(pub LocationSimulationClient<'a, Box<dyn ReadWrite>>);

/// Creates a new ProcessControlClient from a RemoteServerClient
///
/// # Arguments
/// * [`server`] - The RemoteServerClient to use
/// * [`handle`] - Pointer to store the newly created ProcessControlClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut LocationSimulationHandle<'static>,
) -> IdeviceErrorCode {
    if server.is_null() || handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let server = unsafe { &mut (*server).0 };
    let res = RUNTIME.block_on(async move { LocationSimulationClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(LocationSimulationHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees a ProcessControlClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_free(handle: *mut LocationSimulationHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Clears the location set
///
/// # Arguments
/// * [`handle`] - The LocationSimulation handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// All pointers must be valid or NULL where appropriate
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_clear(
    handle: *mut LocationSimulationHandle<'static>,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.clear().await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Sets the location
///
/// # Arguments
/// * [`handle`] - The LocationSimulation handle
/// * [`latitude`] - The latitude to set
/// * [`longitude`] - The longitude to set
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// All pointers must be valid or NULL where appropriate
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_set(
    handle: *mut LocationSimulationHandle<'static>,
    latitude: f64,
    longitude: f64,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.set(latitude, longitude).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}
