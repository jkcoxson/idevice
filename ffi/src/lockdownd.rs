use log::error;
use plist::Value;
use serde::{Deserialize, Serialize};
use idevice::{IdeviceError, IdeviceService, lockdownd::LockdowndClient};

use crate::{pairing_file, Idevice, IdeviceError, IdeviceService};
pub struct LockdowndClientHandle(pub LockdowndClient);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_client_connect(
    provider: *mut UsbmuxdProviderHandle,
    client: *mut *mut LockdowndClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Provider is null");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<LockdowndClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_box = unsafe { Box::from_raw(provider) };
        let provider_ref = &provider_box.0;
        let result = LockdowndClient::connect(provider_ref).await;
        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LockdowndClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdownd_client_get_value(
    client: *mut LockdowndClientHandle,
    value: *const libc::c_char,
    ptr: *mut c_void,
) -> IdeviceErrorCode {
    if value.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let value_cstr = unsafe { std::ffi::CStr::from_ptr(value) };
    let val = match value_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };
    let res: Result<Value, IdeviceError> = RUNTIME.block_on(async move {
        let mut client_box = unsafe { Box::from_raw(client) };
        let client_ref = &mut client_box.0;
        let result = client_ref.get_value(val).await;
        std::mem::forget(client_box);
        result
    });

    match res {
        Ok(pvalue) => {
            let plist_ptr = plist_to_libplist(pvalue) as *mut c_void; 

            unsafe {
                *ptr = plist_ptr;
            }

            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }

}