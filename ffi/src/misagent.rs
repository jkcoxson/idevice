//! iOS Mobile Installation Agent (misagent) Client Bindings
//!
//! Provides C-compatible bindings for interacting with the misagent service on iOS devices.

use idevice::{IdeviceError, IdeviceService, misagent::MisagentClient, provider::IdeviceProvider};

use crate::{IdeviceErrorCode, RUNTIME, provider::IdeviceProviderHandle};

pub struct MisagentClientHandle(pub MisagentClient);

/// Automatically creates and connects to Misagent, returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated MisagentClient handle
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn misagent_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut MisagentClientHandle,
) -> IdeviceErrorCode {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<MisagentClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        MisagentClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(MisagentClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Installs a provisioning profile on the device
///
/// # Arguments
/// * [`client`] - A valid MisagentClient handle
/// * [`profile_data`] - The provisioning profile data to install
/// * [`profile_len`] - Length of the profile data
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `profile_data` must be a valid pointer to profile data of length `profile_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn misagent_install(
    client: *mut MisagentClientHandle,
    profile_data: *const u8,
    profile_len: libc::size_t,
) -> IdeviceErrorCode {
    if client.is_null() || profile_data.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let profile = unsafe { std::slice::from_raw_parts(profile_data, profile_len) }.to_vec();

    let res = RUNTIME.block_on(async { unsafe { &mut *client }.0.install(profile).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Removes a provisioning profile from the device
///
/// # Arguments
/// * [`client`] - A valid MisagentClient handle
/// * [`profile_id`] - The UUID of the profile to remove (C string)
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `profile_id` must be a valid C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn misagent_remove(
    client: *mut MisagentClientHandle,
    profile_id: *const libc::c_char,
) -> IdeviceErrorCode {
    if client.is_null() || profile_id.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let id = unsafe { std::ffi::CStr::from_ptr(profile_id) }
        .to_string_lossy()
        .into_owned();

    let res = RUNTIME.block_on(async { unsafe { &mut *client }.0.remove(&id).await });

    match res {
        Ok(_) => IdeviceErrorCode::IdeviceSuccess,
        Err(e) => e.into(),
    }
}

/// Retrieves all provisioning profiles from the device
///
/// # Arguments
/// * [`client`] - A valid MisagentClient handle
/// * [`out_profiles`] - On success, will be set to point to an array of profile data
/// * [`out_profiles_len`] - On success, will be set to the number of profiles
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `out_profiles` must be a valid pointer to store the resulting array
/// `out_profiles_len` must be a valid pointer to store the array length
#[unsafe(no_mangle)]
pub unsafe extern "C" fn misagent_copy_all(
    client: *mut MisagentClientHandle,
    out_profiles: *mut *mut *mut u8,
    out_profiles_len: *mut *mut libc::size_t,
    out_count: *mut libc::size_t,
) -> IdeviceErrorCode {
    if client.is_null()
        || out_profiles.is_null()
        || out_profiles_len.is_null()
        || out_count.is_null()
    {
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<Vec<Vec<u8>>, IdeviceError> =
        RUNTIME.block_on(async { unsafe { &mut *client }.0.copy_all().await });

    match res {
        Ok(profiles) => {
            let count = profiles.len();
            let mut profile_ptrs = Vec::with_capacity(count);
            let mut profile_lens = Vec::with_capacity(count);

            for profile in profiles {
                let len = profile.len();
                let mut boxed_profile = profile.into_boxed_slice();
                let ptr = boxed_profile.as_mut_ptr();
                std::mem::forget(boxed_profile);
                profile_ptrs.push(ptr);
                profile_lens.push(len);
            }

            let boxed_ptrs = profile_ptrs.into_boxed_slice();
            let boxed_lens = profile_lens.into_boxed_slice();

            unsafe {
                *out_profiles = Box::into_raw(boxed_ptrs) as *mut *mut u8;
                *out_profiles_len = Box::into_raw(boxed_lens) as *mut libc::size_t;
                *out_count = count;
            }

            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees profiles array returned by misagent_copy_all
///
/// # Arguments
/// * [`profiles`] - Array of profile data pointers
/// * [`lens`] - Array of profile lengths
/// * [`count`] - Number of profiles in the array
///
/// # Safety
/// Must only be called with values returned from misagent_copy_all
#[unsafe(no_mangle)]
pub unsafe extern "C" fn misagent_free_profiles(
    profiles: *mut *mut u8,
    lens: *mut libc::size_t,
    count: libc::size_t,
) {
    if profiles.is_null() || lens.is_null() || count == 0 {
        return;
    }

    let profiles = unsafe { std::slice::from_raw_parts_mut(profiles, count) };
    let lens = unsafe { std::slice::from_raw_parts_mut(lens, count) };

    for (ptr, len) in profiles.iter_mut().zip(lens.iter()) {
        if !ptr.is_null() {
            let _ = unsafe { Box::from_raw(std::slice::from_raw_parts_mut(*ptr, *len)) };
        }
    }

    let _ = unsafe { Box::from_raw(profiles as *mut [_]) };
    let _ = unsafe { Box::from_raw(lens as *mut [_]) };
}

/// Frees a misagent client handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn misagent_client_free(handle: *mut MisagentClientHandle) {
    if !handle.is_null() {
        log::debug!("Freeing misagent_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
