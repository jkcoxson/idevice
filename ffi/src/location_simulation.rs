use crate::{IdeviceFfiError, ffi_err, provider::IdeviceProviderHandle, run_sync_local};
use idevice::{
    IdeviceError, IdeviceService, provider::IdeviceProvider,
    services::simulate_location::LocationSimulationService,
};
use std::ptr::null_mut;

// Opaque handle wrapping the Rust service client
pub struct LocationSimulationServiceHandle(pub LocationSimulationService);

/// Connects to the Location Simulation service using a provider  
/// This is the location_simulation api for iOS 16 and below
/// You must have a developer disk image mounted to use this API
///
/// # Safety  
/// `provider` must be valid; `client` must be a non-null pointer to store the handle.  
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_connect(
    provider: *mut IdeviceProviderHandle,
    handle: *mut *mut LocationSimulationServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handle.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<LocationSimulationService, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        LocationSimulationService::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LocationSimulationServiceHandle(r));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Sets the device's simulated location.
/// This is the location_simulation api for iOS 16 and below.
///
/// # Safety
/// `handle` must be a valid pointer to a `LocationSimulationServiceHandle` returned by `lockdown_location_simulation_connect`.
/// `latitude` and `longitude` must be valid, null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_set(
    handle: *mut *mut LocationSimulationServiceHandle,
    latitude: *const libc::c_char,
    longtiude: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || latitude.is_null() || longtiude.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let latitude = unsafe { std::ffi::CStr::from_ptr(latitude) }
        .to_string_lossy()
        .into_owned();
    let longtiude = unsafe { std::ffi::CStr::from_ptr(longtiude) }
        .to_string_lossy()
        .into_owned();

    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (**handle).0 };

        client_ref.set(&latitude, &longtiude).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Clears the device's simulated location, returning it to the actual location.
/// This is the location_simulation api for iOS 16 and below.
///
/// # Safety
/// `handle` must be a valid pointer to a `LocationSimulationServiceHandle` returned by `lockdown_location_simulation_connect`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_clear(
    handle: *mut *mut LocationSimulationServiceHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (**handle).0 };

        client_ref.clear().await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a LocationSimulationService handle  
///  
/// # Safety  
/// `handle` must be a pointer returned by `lockdown_location_simulation_connect`.  
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_free(
    handle: *mut LocationSimulationServiceHandle,
) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
