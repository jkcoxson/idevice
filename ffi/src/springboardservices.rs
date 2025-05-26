use std::ffi::{CStr, c_void};

use idevice::{
    IdeviceError, IdeviceService, provider::IdeviceProvider,
    springboardservices::SpringBoardServicesClient,
};

use crate::{IdeviceErrorCode, IdeviceHandle, RUNTIME, provider::IdeviceProviderHandle};

pub struct SpringBoardServicesClientHandle(pub SpringBoardServicesClient);

/// Connects to the Springboard service using a provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated SpringBoardServicesClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut SpringBoardServicesClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<SpringBoardServicesClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        SpringBoardServicesClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(SpringBoardServicesClientHandle(r));
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

/// Creates a new SpringBoardServices client from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated SpringBoardServicesClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library. The socket is consumed,
/// and may not be used again.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut SpringBoardServicesClientHandle,
) -> IdeviceErrorCode {
    if socket.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = SpringBoardServicesClient::new(socket);
    let boxed = Box::new(SpringBoardServicesClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Gets the icon of the specified app by bundle identifier
///
/// # Arguments
/// * `client` - A valid SpringBoardServicesClient handle
/// * `bundle_identifier` - The identifiers of the app to get icon
/// * `out_result` - On success, will be set to point to a newly allocated png data
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_result` must be a valid, non-null pointer to a location where the result will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_get_icon(
    client: *mut SpringBoardServicesClientHandle,
    bundle_identifier: *const libc::c_char,
    out_result: *mut *mut c_void,
    out_result_len: *mut libc::size_t,
) -> IdeviceErrorCode {
    if client.is_null() || out_result.is_null() || out_result_len.is_null() {
        log::error!("Invalid arguments: {client:?}, {out_result:?}");
        return IdeviceErrorCode::InvalidArg;
    }
    let client = unsafe { &mut *client };

    let name_cstr = unsafe { CStr::from_ptr(bundle_identifier) };
    let bundle_id = match name_cstr.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let res: Result<Vec<u8>, IdeviceError> =
        RUNTIME.block_on(async { client.0.get_icon_pngdata(bundle_id).await });

    match res {
        Ok(r) => {
            let len = r.len();
            let boxed_slice = r.into_boxed_slice();
            let ptr = boxed_slice.as_ptr();
            std::mem::forget(boxed_slice);

            unsafe {
                *out_result = ptr as *mut c_void;
                *out_result_len = len;
            }
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees an SpringBoardServicesClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_free(handle: *mut SpringBoardServicesClientHandle) {
    if !handle.is_null() {
        log::debug!("Freeing springboard_services_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
