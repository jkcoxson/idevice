// Jackson Coxson

use std::{
    ffi::{CStr, CString, c_char},
    ptr::null_mut,
};

use idevice::{ReadWrite, dvt::condition_inducer::ConditionInducerClient};

use crate::{IdeviceFfiError, dvt::remote_server::RemoteServerHandle, ffi_err, run_sync};

/// Opaque handle to a ConditionInducerClient
pub struct ConditionInducerHandle<'a>(pub ConditionInducerClient<'a, Box<dyn ReadWrite>>);

/// A single condition profile
#[repr(C)]
pub struct IdeviceConditionProfile {
    pub identifier: *mut c_char,
    pub description: *mut c_char,
}

/// A condition inducer group containing profiles
#[repr(C)]
pub struct IdeviceConditionGroup {
    pub identifier: *mut c_char,
    pub profiles: *mut IdeviceConditionProfile,
    pub profiles_count: usize,
}

/// Creates a new ConditionInducerClient from a RemoteServerClient
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut ConditionInducerHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = run_sync(async move { ConditionInducerClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(ConditionInducerHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a ConditionInducerClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_free(handle: *mut ConditionInducerHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Frees a single IdeviceConditionGroup and all its heap-allocated fields
///
/// # Safety
/// `group` must be a valid pointer allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_group_free(group: *mut IdeviceConditionGroup) {
    if group.is_null() {
        return;
    }
    let g = unsafe { Box::from_raw(group) };
    if !g.identifier.is_null() {
        let _ = unsafe { CString::from_raw(g.identifier) };
    }
    if !g.profiles.is_null() {
        let profiles = unsafe { std::slice::from_raw_parts(g.profiles, g.profiles_count) };
        for p in profiles {
            if !p.identifier.is_null() {
                let _ = unsafe { CString::from_raw(p.identifier) };
            }
            if !p.description.is_null() {
                let _ = unsafe { CString::from_raw(p.description) };
            }
        }
        let _ = unsafe { Vec::from_raw_parts(g.profiles, g.profiles_count, g.profiles_count) };
    }
}

/// Frees an array of IdeviceConditionGroup pointers
///
/// # Safety
/// `groups` must be a valid pointer to an array of length `count` allocated by this library,
/// or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_groups_free(
    groups: *mut *mut IdeviceConditionGroup,
    count: usize,
) {
    if groups.is_null() {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(groups, count) };
    for &g in slice {
        unsafe { condition_inducer_group_free(g) };
    }
    let _ = unsafe { Vec::from_raw_parts(groups, count, count) };
}

/// Returns the available condition inducer groups
///
/// # Arguments
/// * [`handle`] - The ConditionInducerClient handle
/// * [`groups_out`] - On success, set to a heap-allocated array of group pointers
/// * [`count_out`] - On success, set to the number of groups returned
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null. Free with `condition_inducer_groups_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_available_conditions(
    handle: *mut ConditionInducerHandle<'static>,
    groups_out: *mut *mut *mut IdeviceConditionGroup,
    count_out: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || groups_out.is_null() || count_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.available_conditions().await });

    match res {
        Ok(groups) => {
            let mut ptrs: Vec<*mut IdeviceConditionGroup> = groups
                .into_iter()
                .map(|g| {
                    let identifier = CString::new(g.identifier).unwrap_or_default().into_raw();
                    let mut profiles: Vec<IdeviceConditionProfile> = g
                        .profiles
                        .into_iter()
                        .map(|p| IdeviceConditionProfile {
                            identifier: CString::new(p.identifier).unwrap_or_default().into_raw(),
                            description: CString::new(p.description).unwrap_or_default().into_raw(),
                        })
                        .collect();
                    profiles.shrink_to_fit();
                    let profiles_count = profiles.len();
                    let profiles_ptr = profiles.as_mut_ptr();
                    std::mem::forget(profiles);
                    Box::into_raw(Box::new(IdeviceConditionGroup {
                        identifier,
                        profiles: profiles_ptr,
                        profiles_count,
                    }))
                })
                .collect();
            ptrs.shrink_to_fit();
            unsafe { *count_out = ptrs.len() };
            let ptr = ptrs.as_mut_ptr();
            std::mem::forget(ptrs);
            unsafe { *groups_out = ptr };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Enables a specific condition profile
///
/// # Arguments
/// * [`handle`] - The ConditionInducerClient handle
/// * [`condition_identifier`] - The condition group identifier (null-terminated C string)
/// * [`profile_identifier`] - The profile identifier within the group (null-terminated C string)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_enable(
    handle: *mut ConditionInducerHandle<'static>,
    condition_identifier: *const c_char,
    profile_identifier: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || condition_identifier.is_null() || profile_identifier.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let cond_id = match unsafe { CStr::from_ptr(condition_identifier).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let prof_id = match unsafe { CStr::from_ptr(profile_identifier).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.enable_condition(&cond_id, &prof_id).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Disables the currently active condition
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn condition_inducer_disable(
    handle: *mut ConditionInducerHandle<'static>,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.disable_condition().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}
