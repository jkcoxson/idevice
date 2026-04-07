// Jackson Coxson

use std::ptr::null_mut;

use idevice::{ReadWrite, dvt::application_listing::ApplicationListingClient};
use plist_ffi::PlistWrapper;

use crate::{IdeviceFfiError, dvt::remote_server::RemoteServerHandle, ffi_err, plist_t, run_sync};

/// Opaque handle to an ApplicationListingClient
pub struct ApplicationListingHandle<'a>(pub ApplicationListingClient<'a, Box<dyn ReadWrite>>);

/// Creates a new ApplicationListingClient from a RemoteServerClient
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn application_listing_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut ApplicationListingHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = run_sync(async move { ApplicationListingClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(ApplicationListingHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees an ApplicationListingClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn application_listing_free(handle: *mut ApplicationListingHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Returns the list of installed applications as an array of plist dictionaries
///
/// # Arguments
/// * [`handle`] - The ApplicationListingClient handle
/// * [`apps_out`] - On success, set to a heap-allocated array of plist_t values (each is a dict)
/// * [`count_out`] - On success, set to the number of apps returned
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null.
/// Free the returned array with `idevice_plist_array_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn application_listing_get_apps(
    handle: *mut ApplicationListingHandle<'static>,
    apps_out: *mut *mut plist_t,
    count_out: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || apps_out.is_null() || count_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.installed_applications().await });

    match res {
        Ok(apps) => {
            let mut ptrs: Vec<plist_t> = apps
                .into_iter()
                .map(|dict| {
                    PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr() as plist_t
                })
                .collect();
            ptrs.shrink_to_fit();
            unsafe { *count_out = ptrs.len() };
            let ptr = ptrs.as_mut_ptr();
            std::mem::forget(ptrs);
            unsafe { *apps_out = ptr };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
