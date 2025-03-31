use std::ffi::{CStr, c_void};

use idevice::{IdeviceError, IdeviceService, sbservices::SpringBoardServicesClient};

use crate::{
    IdeviceErrorCode, IdeviceHandle, RUNTIME,
    provider::{TcpProviderHandle, UsbmuxdProviderHandle},
};

pub struct SpringBoardServicesClientHandle(pub SpringBoardServicesClient);

#[allow(non_camel_case_types)]
pub struct plist_t;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut SpringBoardServicesClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<SpringBoardServicesClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = SpringBoardServicesClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_connect_usbmuxd(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut SpringBoardServicesClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<SpringBoardServicesClient, IdeviceError> = RUNTIME.block_on(async move {
        // Take ownership of the provider (without immediately dropping it)
        let provider_box = unsafe { Box::from_raw(provider) };

        // Get a reference to the inner value
        let provider_ref = &provider_box.0;

        // Connect using the reference
        let result = SpringBoardServicesClient::connect(provider_ref).await;

        // Explicitly keep the provider_box alive until after connect completes
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(SpringBoardServicesClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn springboard_services_free(handle: *mut SpringBoardServicesClientHandle) {
    if !handle.is_null() {
        log::debug!("Freeing springboard_services_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
