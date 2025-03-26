// Jackson Coxson

use std::ffi::{CStr, CString, c_char};

use idevice::{tcp::adapter::Adapter, xpc::XPCDevice};

use crate::{IdeviceErrorCode, RUNTIME, core_device_proxy::AdapterHandle};

/// Opaque handle to an XPCDevice
pub struct XPCDeviceAdapterHandle(pub XPCDevice<Adapter>);

/// Opaque handle to an XPCService
#[repr(C)]
pub struct XPCServiceHandle {
    pub entitlement: *mut c_char,
    pub port: u16,
    pub uses_remote_xpc: bool,
    pub features: *mut *mut c_char,
    pub features_count: usize,
    pub service_version: i64,
}

impl XPCServiceHandle {
    pub fn new(
        entitlement: *mut c_char,
        port: u16,
        uses_remote_xpc: bool,
        features: *mut *mut c_char,
        features_count: usize,
        service_version: i64,
    ) -> Self {
        Self {
            entitlement,
            port,
            uses_remote_xpc,
            features,
            features_count,
            service_version,
        }
    }
}

/// Creates a new XPCDevice from an adapter
///
/// # Arguments
/// * [`adapter`] - The adapter to use for communication
/// * [`device`] - Pointer to store the newly created XPCDevice handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `adapter` must be a valid pointer to a handle allocated by this library
/// `device` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_device_new(
    adapter: *mut AdapterHandle,
    device: *mut *mut XPCDeviceAdapterHandle,
) -> IdeviceErrorCode {
    if adapter.is_null() || device.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let adapter = unsafe { Box::from_raw(adapter) };
    let res = RUNTIME.block_on(async move { XPCDevice::new(adapter.0).await });

    match res {
        // we have to unwrap res to avoid just getting a reference
        Ok(_) => {
            let boxed = Box::new(XPCDeviceAdapterHandle(res.unwrap()));
            unsafe { *device = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees an XPCDevice handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_device_free(handle: *mut XPCDeviceAdapterHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Gets a service by name from the XPCDevice
///
/// # Arguments
/// * [`handle`] - The XPCDevice handle
/// * [`service_name`] - The name of the service to get
/// * [`service`] - Pointer to store the newly created XPCService handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `service_name` must be a valid null-terminated C string
/// `service` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_device_get_service(
    handle: *mut XPCDeviceAdapterHandle,
    service_name: *const c_char,
    service: *mut *mut XPCServiceHandle,
) -> IdeviceErrorCode {
    if handle.is_null() || service_name.is_null() || service.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let device = unsafe { &(*handle).0 };
    let service_name_cstr = unsafe { CStr::from_ptr(service_name) };
    let service_name = match service_name_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let xpc_service = match device.services.get(service_name) {
        Some(s) => s,
        None => return IdeviceErrorCode::ServiceNotFound,
    };

    // Convert features to C array if they exist
    let (features_ptr, features_count) = if let Some(features) = &xpc_service.features {
        let mut features_vec: Vec<*mut c_char> = features
            .iter()
            .map(|f| CString::new(f.as_str()).unwrap().into_raw())
            .collect();
        features_vec.shrink_to_fit();

        let mut features_vec = Box::new(features_vec);
        let features_ptr = features_vec.as_mut_ptr();
        let features_len = features_vec.len();

        Box::leak(features_vec);
        (features_ptr, features_len)
    } else {
        (std::ptr::null_mut(), 0)
    };

    let boxed = Box::new(XPCServiceHandle {
        entitlement: CString::new(xpc_service.entitlement.as_str())
            .unwrap()
            .into_raw(),
        port: xpc_service.port,
        uses_remote_xpc: xpc_service.uses_remote_xpc,
        features: features_ptr,
        features_count,
        service_version: xpc_service.service_version.unwrap_or(-1),
    });

    unsafe { *service = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Returns the adapter in the RemoteXPC Device
///
/// # Arguments
/// * [`handle`] - The handle to get the adapter from
/// * [`adapter`] - The newly allocated AdapterHandle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL, and never used again
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_device_adapter_into_inner(
    handle: *mut XPCDeviceAdapterHandle,
    adapter: *mut *mut AdapterHandle,
) -> IdeviceErrorCode {
    if handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }
    let device = unsafe { Box::from_raw(handle).0 };
    let adapter_obj = device.into_inner();
    let boxed = Box::new(AdapterHandle(adapter_obj));
    unsafe { *adapter = Box::into_raw(boxed) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Frees an XPCService handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_service_free(handle: *mut XPCServiceHandle) {
    if !handle.is_null() {
        let handle = unsafe { Box::from_raw(handle) };

        // Free the entitlement string
        if !handle.entitlement.is_null() {
            let _ = unsafe { CString::from_raw(handle.entitlement) };
        }

        // Free the features array
        if !handle.features.is_null() {
            for i in 0..handle.features_count {
                let feature_ptr = unsafe { *handle.features.add(i) };
                if !feature_ptr.is_null() {
                    let _ = unsafe { CString::from_raw(feature_ptr) };
                }
            }
            let _ = unsafe {
                Vec::from_raw_parts(
                    handle.features,
                    handle.features_count,
                    handle.features_count,
                )
            };
        }
    }
}

/// Gets the list of available service names
///
/// # Arguments
/// * [`handle`] - The XPCDevice handle
/// * [`names`] - Pointer to store the array of service names
/// * [`count`] - Pointer to store the number of services
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `names` must be a valid pointer to a location where the array will be stored
/// `count` must be a valid pointer to a location where the count will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_device_get_service_names(
    handle: *mut XPCDeviceAdapterHandle,
    names: *mut *mut *mut c_char,
    count: *mut usize,
) -> IdeviceErrorCode {
    if handle.is_null() || names.is_null() || count.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let device = unsafe { &(*handle).0 };
    let service_names: Vec<CString> = device
        .services
        .keys()
        .map(|k| CString::new(k.as_str()).unwrap())
        .collect();

    let mut name_ptrs: Vec<*mut c_char> = service_names.into_iter().map(|s| s.into_raw()).collect();

    if name_ptrs.is_empty() {
        unsafe {
            *names = std::ptr::null_mut();
            *count = 0;
        }
    } else {
        name_ptrs.shrink_to_fit();
        unsafe {
            *names = name_ptrs.as_mut_ptr();
            *count = name_ptrs.len();
        }
        std::mem::forget(name_ptrs);
    }

    IdeviceErrorCode::IdeviceSuccess
}

/// Frees a list of service names
///
/// # Arguments
/// * [`names`] - The array of service names to free
/// * [`count`] - The number of services in the array
///
/// # Safety
/// `names` must be a valid pointer to an array of `count` C strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xpc_device_free_service_names(names: *mut *mut c_char, count: usize) {
    if !names.is_null() && count > 0 {
        let names_vec = unsafe { Vec::from_raw_parts(names, count, count) };
        for name in names_vec {
            if !name.is_null() {
                let _ = unsafe { CString::from_raw(name) };
            }
        }
    }
}
