//! Remote Service Discovery (RSD) Client Bindings
//!
//! Provides C-compatible bindings for RSD handshake and service discovery on iOS devices.

use std::ffi::{CStr, CString};
use std::ptr;

use idevice::rsd::RsdHandshake;

use crate::{IdeviceErrorCode, RUNTIME, ReadWriteOpaque};

/// Opaque handle to an RsdHandshake
pub struct RsdHandshakeHandle(pub RsdHandshake);

/// C-compatible representation of an RSD service
#[repr(C)]
pub struct CRsdService {
    /// Service name (null-terminated string)
    pub name: *mut libc::c_char,
    /// Required entitlement (null-terminated string)
    pub entitlement: *mut libc::c_char,
    /// Port number
    pub port: u16,
    /// Whether service uses remote XPC
    pub uses_remote_xpc: bool,
    /// Number of features
    pub features_count: libc::size_t,
    /// Array of feature strings
    pub features: *mut *mut libc::c_char,
    /// Service version (-1 if not present)
    pub service_version: i64,
}

/// Array of RSD services returned by rsd_get_services
#[repr(C)]
pub struct CRsdServiceArray {
    /// Array of services
    pub services: *mut CRsdService,
    /// Number of services in array
    pub count: libc::size_t,
}

/// Creates a new RSD handshake from a ReadWrite connection
///
/// # Arguments
/// * [`socket`] - The connection to use for communication
/// * [`handle`] - Pointer to store the newly created RsdHandshake handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `socket` must be a valid pointer to a ReadWrite handle allocated by this library. It is
/// consumed and may not be used again.
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_handshake_new(
    socket: *mut ReadWriteOpaque,
    handle: *mut *mut RsdHandshakeHandle,
) -> IdeviceErrorCode {
    if socket.is_null() || handle.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let wrapper = unsafe { &mut *socket };

    let res = match wrapper.inner.take() {
        Some(mut w) => RUNTIME.block_on(async move { RsdHandshake::new(w.as_mut()).await }),
        None => {
            return IdeviceErrorCode::InvalidArg;
        }
    };

    match res {
        Ok(handshake) => {
            let boxed = Box::new(RsdHandshakeHandle(handshake));
            unsafe { *handle = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Gets the protocol version from the RSD handshake
///
/// # Arguments
/// * [`handle`] - A valid RsdHandshake handle
/// * [`version`] - Pointer to store the protocol version
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `version` must be a valid pointer to store the version
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_get_protocol_version(
    handle: *mut RsdHandshakeHandle,
    version: *mut libc::size_t,
) -> IdeviceErrorCode {
    if handle.is_null() || version.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    unsafe {
        *version = (*handle).0.protocol_version;
    }
    IdeviceErrorCode::IdeviceSuccess
}

/// Gets the UUID from the RSD handshake
///
/// # Arguments
/// * [`handle`] - A valid RsdHandshake handle
/// * [`uuid`] - Pointer to store the UUID string (caller must free with rsd_free_string)
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `uuid` must be a valid pointer to store the string pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_get_uuid(
    handle: *mut RsdHandshakeHandle,
    uuid: *mut *mut libc::c_char,
) -> IdeviceErrorCode {
    if handle.is_null() || uuid.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let uuid_str = &unsafe { &*handle }.0.uuid;
    match CString::new(uuid_str.as_str()) {
        Ok(c_str) => {
            unsafe { *uuid = c_str.into_raw() };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(_) => IdeviceErrorCode::InvalidString,
    }
}

/// Gets all available services from the RSD handshake
///
/// # Arguments
/// * [`handle`] - A valid RsdHandshake handle
/// * [`services`] - Pointer to store the services array
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `services` must be a valid pointer to store the services array
/// Caller must free the returned array with rsd_free_services
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_get_services(
    handle: *mut RsdHandshakeHandle,
    services: *mut *mut CRsdServiceArray,
) -> IdeviceErrorCode {
    if handle.is_null() || services.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let handshake = unsafe { &*handle };
    let service_map = &handshake.0.services;

    let count = service_map.len();
    let mut c_services = Vec::with_capacity(count);

    for (name, service) in service_map.iter() {
        // Convert name
        let c_name = match CString::new(name.as_str()) {
            Ok(s) => s.into_raw(),
            Err(_) => continue,
        };

        // Convert entitlement
        let c_entitlement = match CString::new(service.entitlement.as_str()) {
            Ok(s) => s.into_raw(),
            Err(_) => {
                unsafe {
                    let _ = CString::from_raw(c_name);
                }
                continue;
            }
        };

        // Convert features
        let (features_ptr, features_count) = match &service.features {
            Some(features) => {
                let mut c_features = Vec::with_capacity(features.len());

                for feature in features {
                    match CString::new(feature.as_str()) {
                        Ok(s) => c_features.push(s.into_raw()),
                        Err(_) => {
                            // Clean up already allocated features
                            for f in c_features {
                                unsafe {
                                    let _ = CString::from_raw(f);
                                }
                            }
                            // Return early to avoid the move below
                            return IdeviceErrorCode::InvalidString;
                        }
                    }
                }

                // All features converted successfully
                let boxed = c_features.into_boxed_slice();
                let ptr = Box::into_raw(boxed) as *mut *mut libc::c_char;
                (ptr, features.len())
            }
            None => (ptr::null_mut(), 0),
        };

        let c_service = CRsdService {
            name: c_name,
            entitlement: c_entitlement,
            port: service.port,
            uses_remote_xpc: service.uses_remote_xpc,
            features_count,
            features: features_ptr,
            service_version: service.service_version.unwrap_or(-1),
        };

        c_services.push(c_service);
    }

    let boxed_services = c_services.into_boxed_slice();
    let array = Box::new(CRsdServiceArray {
        services: Box::into_raw(boxed_services) as *mut CRsdService,
        count,
    });

    unsafe { *services = Box::into_raw(array) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Checks if a specific service is available
///
/// # Arguments
/// * [`handle`] - A valid RsdHandshake handle
/// * [`service_name`] - Name of the service to check for
/// * [`available`] - Pointer to store the availability result
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `service_name` must be a valid C string
/// `available` must be a valid pointer to store the boolean result
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_service_available(
    handle: *mut RsdHandshakeHandle,
    service_name: *const libc::c_char,
    available: *mut bool,
) -> IdeviceErrorCode {
    if handle.is_null() || service_name.is_null() || available.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let name = match unsafe { CStr::from_ptr(service_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let handshake = unsafe { &*handle };
    unsafe { *available = handshake.0.services.contains_key(name) };

    IdeviceErrorCode::IdeviceSuccess
}

/// Gets information about a specific service
///
/// # Arguments
/// * [`handle`] - A valid RsdHandshake handle
/// * [`service_name`] - Name of the service to get info for
/// * [`service_info`] - Pointer to store the service information
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `service_name` must be a valid C string
/// `service_info` must be a valid pointer to store the service info
/// Caller must free the returned service with rsd_free_service
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_get_service_info(
    handle: *mut RsdHandshakeHandle,
    service_name: *const libc::c_char,
    service_info: *mut *mut CRsdService,
) -> IdeviceErrorCode {
    if handle.is_null() || service_name.is_null() || service_info.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let name = match unsafe { CStr::from_ptr(service_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    let handshake = unsafe { &*handle };
    let service = match handshake.0.services.get(name) {
        Some(s) => s,
        None => return IdeviceErrorCode::ServiceNotFound,
    };

    // Convert service to C representation (similar to rsd_get_services logic)
    let c_name = match CString::new(name) {
        Ok(s) => s.into_raw(),
        Err(_) => return IdeviceErrorCode::InvalidString,
    };

    let c_entitlement = match CString::new(service.entitlement.as_str()) {
        Ok(s) => s.into_raw(),
        Err(_) => {
            unsafe {
                let _ = CString::from_raw(c_name);
            }
            return IdeviceErrorCode::InvalidString;
        }
    };

    // Convert features
    let (features_ptr, features_count) = match &service.features {
        Some(features) => {
            let mut c_features = Vec::with_capacity(features.len());
            for feature in features {
                match CString::new(feature.as_str()) {
                    Ok(s) => c_features.push(s.into_raw()),
                    Err(_) => {
                        // Clean up already allocated features
                        for f in c_features {
                            unsafe {
                                let _ = CString::from_raw(f);
                            }
                        }
                        // Clean up name and entitlement
                        unsafe {
                            let _ = CString::from_raw(c_name);
                            let _ = CString::from_raw(c_entitlement);
                        }
                        return IdeviceErrorCode::InvalidString;
                    }
                }
            }
            let boxed = c_features.into_boxed_slice();
            (
                Box::into_raw(boxed) as *mut *mut libc::c_char,
                features.len(),
            )
        }
        None => (ptr::null_mut(), 0),
    };

    let c_service = Box::new(CRsdService {
        name: c_name,
        entitlement: c_entitlement,
        port: service.port,
        uses_remote_xpc: service.uses_remote_xpc,
        features_count,
        features: features_ptr,
        service_version: service.service_version.unwrap_or(-1),
    });

    unsafe { *service_info = Box::into_raw(c_service) };
    IdeviceErrorCode::IdeviceSuccess
}

/// Frees a string returned by RSD functions
///
/// # Arguments
/// * [`string`] - The string to free
///
/// # Safety
/// Must only be called with strings returned from RSD functions
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_free_string(string: *mut libc::c_char) {
    if !string.is_null() {
        unsafe {
            let _ = CString::from_raw(string);
        }
    }
}

/// Frees a single service returned by rsd_get_service_info
///
/// # Arguments
/// * [`service`] - The service to free
///
/// # Safety
/// Must only be called with services returned from rsd_get_service_info
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_free_service(service: *mut CRsdService) {
    if service.is_null() {
        return;
    }

    let service_box = unsafe { Box::from_raw(service) };

    // Free name
    if !service_box.name.is_null() {
        unsafe {
            let _ = CString::from_raw(service_box.name);
        }
    }

    // Free entitlement
    if !service_box.entitlement.is_null() {
        unsafe {
            let _ = CString::from_raw(service_box.entitlement);
        }
    }

    // Free features array
    if !service_box.features.is_null() && service_box.features_count > 0 {
        let features_slice = unsafe {
            std::slice::from_raw_parts_mut(service_box.features, service_box.features_count)
        };
        for feature_ptr in features_slice.iter() {
            if !feature_ptr.is_null() {
                unsafe {
                    let _ = CString::from_raw(*feature_ptr);
                }
            }
        }
        unsafe {
            let _ = Box::from_raw(features_slice);
        }
    }
}

/// Frees services array returned by rsd_get_services
///
/// # Arguments
/// * [`services`] - The services array to free
///
/// # Safety
/// Must only be called with arrays returned from rsd_get_services
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_free_services(services: *mut CRsdServiceArray) {
    if services.is_null() {
        return;
    }

    let services_box = unsafe { Box::from_raw(services) };

    if !services_box.services.is_null() && services_box.count > 0 {
        let services_slice =
            unsafe { std::slice::from_raw_parts_mut(services_box.services, services_box.count) };

        // Free each service
        for service in services_slice.iter() {
            // Free name
            if !service.name.is_null() {
                unsafe {
                    let _ = CString::from_raw(service.name);
                }
            }

            // Free entitlement
            if !service.entitlement.is_null() {
                unsafe {
                    let _ = CString::from_raw(service.entitlement);
                }
            }

            // Free features array
            if !service.features.is_null() && service.features_count > 0 {
                let features_slice = unsafe {
                    std::slice::from_raw_parts_mut(service.features, service.features_count)
                };
                for feature_ptr in features_slice.iter() {
                    if !feature_ptr.is_null() {
                        unsafe {
                            let _ = CString::from_raw(*feature_ptr);
                        }
                    }
                }
                unsafe {
                    let _ = Box::from_raw(features_slice);
                }
            }
        }

        unsafe {
            let _ = Box::from_raw(services_slice);
        }
    }
}

/// Frees an RSD handshake handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_handshake_free(handle: *mut RsdHandshakeHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
