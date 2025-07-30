// Jackson Coxson

use std::{ffi::c_void, ptr::null_mut};

use idevice::{
    IdeviceError, IdeviceService, mobile_image_mounter::ImageMounter, provider::IdeviceProvider,
};
use plist::Value;
use plist_ffi::{PlistWrapper, plist_t};

use crate::{IdeviceFfiError, IdeviceHandle, RUNTIME, ffi_err, provider::IdeviceProviderHandle};

pub struct ImageMounterHandle(pub ImageMounter);

/// Connects to the Image Mounter service using a provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated ImageMounter handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut ImageMounterHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<ImageMounter, IdeviceError> = RUNTIME.block_on(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        ImageMounter::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(ImageMounterHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            let _ = unsafe { Box::from_raw(provider) };
            ffi_err!(e)
        }
    }
}

/// Creates a new ImageMounter client from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated ImageMounter handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut ImageMounterHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = ImageMounter::new(socket);
    let boxed = Box::new(ImageMounterHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Frees an ImageMounter handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_free(handle: *mut ImageMounterHandle) {
    if !handle.is_null() {
        log::debug!("Freeing image_mounter_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Gets a list of mounted devices
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`devices`] - Will be set to point to a slice of device plists on success
/// * [`devices_len`] - Will be set to the number of devices copied
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `devices` must be a valid, non-null pointer to a location where the plist will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_copy_devices(
    client: *mut ImageMounterHandle,
    devices: *mut *mut plist_t,
    devices_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    let res: Result<Vec<Value>, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.copy_devices().await
    });

    match res {
        Ok(devices_list) => {
            let devices_list = devices_list
                .into_iter()
                .map(|x| plist_ffi::PlistWrapper::new_node(x).into_ptr())
                .collect::<Vec<plist_t>>();
            let len = devices_list.len();
            let boxed_slice = devices_list.into_boxed_slice();
            let ptr = Box::leak(boxed_slice).as_mut_ptr();

            unsafe {
                *devices = ptr as *mut plist_t;
                *devices_len = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Looks up an image and returns its signature
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`image_type`] - The type of image to look up
/// * [`signature`] - Will be set to point to the signature data on success
/// * [`signature_len`] - Will be set to the length of the signature data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `image_type` must be a valid null-terminated C string
/// `signature` and `signature_len` must be valid pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_lookup_image(
    client: *mut ImageMounterHandle,
    image_type: *const libc::c_char,
    signature: *mut *mut u8,
    signature_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if image_type.is_null() || signature.is_null() || signature_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_type_cstr = unsafe { std::ffi::CStr::from_ptr(image_type) };
    let image_type = match image_type_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<Vec<u8>, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.lookup_image(image_type).await
    });

    match res {
        Ok(sig) => {
            let mut boxed = sig.into_boxed_slice();
            unsafe {
                *signature = boxed.as_mut_ptr();
                *signature_len = boxed.len();
            }
            std::mem::forget(boxed);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Uploads an image to the device
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`image_type`] - The type of image being uploaded
/// * [`image`] - Pointer to the image data
/// * [`image_len`] - Length of the image data
/// * [`signature`] - Pointer to the signature data
/// * [`signature_len`] - Length of the signature data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `image_type` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_upload_image(
    client: *mut ImageMounterHandle,
    image_type: *const libc::c_char,
    image: *const u8,
    image_len: libc::size_t,
    signature: *const u8,
    signature_len: libc::size_t,
) -> *mut IdeviceFfiError {
    if image_type.is_null() || image.is_null() || signature.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_type_cstr = unsafe { std::ffi::CStr::from_ptr(image_type) };
    let image_type = match image_type_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let image_slice = unsafe { std::slice::from_raw_parts(image, image_len) };
    let signature_slice = unsafe { std::slice::from_raw_parts(signature, signature_len) };

    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .upload_image(image_type, image_slice, signature_slice.to_vec())
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Mounts an image on the device
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`image_type`] - The type of image being mounted
/// * [`signature`] - Pointer to the signature data
/// * [`signature_len`] - Length of the signature data
/// * [`trust_cache`] - Pointer to trust cache data (optional)
/// * [`trust_cache_len`] - Length of trust cache data (0 if none)
/// * [`info_plist`] - Pointer to info plist (optional)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid (except optional ones which can be null)
/// `image_type` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_mount_image(
    client: *mut ImageMounterHandle,
    image_type: *const libc::c_char,
    signature: *const u8,
    signature_len: libc::size_t,
    trust_cache: *const u8,
    trust_cache_len: libc::size_t,
    info_plist: *const c_void,
) -> *mut IdeviceFfiError {
    if image_type.is_null() || signature.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_type_cstr = unsafe { std::ffi::CStr::from_ptr(image_type) };
    let image_type = match image_type_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let signature_slice = unsafe { std::slice::from_raw_parts(signature, signature_len) };
    let trust_cache = if !trust_cache.is_null() && trust_cache_len > 0 {
        Some(unsafe { std::slice::from_raw_parts(trust_cache, trust_cache_len).to_vec() })
    } else {
        None
    };

    let info_plist = if !info_plist.is_null() {
        Some(
            unsafe { Box::from_raw(info_plist as *mut Value) }
                .as_ref()
                .clone(),
        )
    } else {
        None
    };

    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .mount_image(
                image_type,
                signature_slice.to_vec(),
                trust_cache,
                info_plist,
            )
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Unmounts an image from the device
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`mount_path`] - The path where the image is mounted
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `mount_path` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_unmount_image(
    client: *mut ImageMounterHandle,
    mount_path: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if mount_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let mount_path_cstr = unsafe { std::ffi::CStr::from_ptr(mount_path) };
    let mount_path = match mount_path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.unmount_image(mount_path).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Queries the developer mode status
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`status`] - Will be set to the developer mode status (1 = enabled, 0 = disabled)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `status` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_query_developer_mode_status(
    client: *mut ImageMounterHandle,
    status: *mut libc::c_int,
) -> *mut IdeviceFfiError {
    if status.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<bool, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.query_developer_mode_status().await
    });

    match res {
        Ok(s) => {
            unsafe { *status = if s { 1 } else { 0 } };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Mounts a developer image
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`image`] - Pointer to the image data
/// * [`image_len`] - Length of the image data
/// * [`signature`] - Pointer to the signature data
/// * [`signature_len`] - Length of the signature data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_mount_developer(
    client: *mut ImageMounterHandle,
    image: *const u8,
    image_len: libc::size_t,
    signature: *const u8,
    signature_len: libc::size_t,
) -> *mut IdeviceFfiError {
    if image.is_null() || signature.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_slice = unsafe { std::slice::from_raw_parts(image, image_len) };
    let signature_slice = unsafe { std::slice::from_raw_parts(signature, signature_len) };

    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .mount_developer(image_slice, signature_slice.to_vec())
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Queries the personalization manifest from the device
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`image_type`] - The type of image to query
/// * [`signature`] - Pointer to the signature data
/// * [`signature_len`] - Length of the signature data
/// * [`manifest`] - Will be set to point to the manifest data on success
/// * [`manifest_len`] - Will be set to the length of the manifest data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `image_type` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_query_personalization_manifest(
    client: *mut ImageMounterHandle,
    image_type: *const libc::c_char,
    signature: *const u8,
    signature_len: libc::size_t,
    manifest: *mut *mut u8,
    manifest_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if image_type.is_null() || signature.is_null() || manifest.is_null() || manifest_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_type_cstr = unsafe { std::ffi::CStr::from_ptr(image_type) };
    let image_type = match image_type_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let signature_slice = unsafe { std::slice::from_raw_parts(signature, signature_len) };

    let res: Result<Vec<u8>, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .query_personalization_manifest(image_type, signature_slice.to_vec())
            .await
    });

    match res {
        Ok(m) => {
            let mut boxed = m.into_boxed_slice();
            unsafe {
                *manifest = boxed.as_mut_ptr();
                *manifest_len = boxed.len();
            }
            std::mem::forget(boxed);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Queries the nonce from the device
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`personalized_image_type`] - The type of image to query (optional)
/// * [`nonce`] - Will be set to point to the nonce data on success
/// * [`nonce_len`] - Will be set to the length of the nonce data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client`, `nonce`, and `nonce_len` must be valid pointers
/// `personalized_image_type` can be NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_query_nonce(
    client: *mut ImageMounterHandle,
    personalized_image_type: *const libc::c_char,
    nonce: *mut *mut u8,
    nonce_len: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if nonce.is_null() || nonce_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_type = if !personalized_image_type.is_null() {
        let image_type_cstr = unsafe { std::ffi::CStr::from_ptr(personalized_image_type) };
        match image_type_cstr.to_str() {
            Ok(s) => Some(s.to_string()),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
        }
    } else {
        None
    };

    let res: Result<Vec<u8>, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.query_nonce(image_type).await
    });

    match res {
        Ok(n) => {
            let mut boxed = n.into_boxed_slice();
            unsafe {
                *nonce = boxed.as_mut_ptr();
                *nonce_len = boxed.len();
            }
            std::mem::forget(boxed);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Queries personalization identifiers from the device
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`image_type`] - The type of image to query (optional)
/// * [`identifiers`] - Will be set to point to the identifiers plist on success
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` and `identifiers` must be valid pointers
/// `image_type` can be NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_query_personalization_identifiers(
    client: *mut ImageMounterHandle,
    image_type: *const libc::c_char,
    identifiers: *mut plist_t,
) -> *mut IdeviceFfiError {
    if identifiers.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_type = if !image_type.is_null() {
        let image_type_cstr = unsafe { std::ffi::CStr::from_ptr(image_type) };
        match image_type_cstr.to_str() {
            Ok(s) => Some(s.to_string()),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
        }
    } else {
        None
    };

    let res: Result<plist::Dictionary, IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref
            .query_personalization_identifiers(image_type)
            .await
    });

    match res {
        Ok(id) => {
            let plist = PlistWrapper::new_node(Value::Dictionary(id)).into_ptr();
            unsafe { *identifiers = plist };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Rolls the personalization nonce
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_roll_personalization_nonce(
    client: *mut ImageMounterHandle,
) -> *mut IdeviceFfiError {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.roll_personalization_nonce().await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Rolls the cryptex nonce
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_roll_cryptex_nonce(
    client: *mut ImageMounterHandle,
) -> *mut IdeviceFfiError {
    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.roll_cryptex_nonce().await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Mounts a personalized developer image
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`provider`] - A valid provider handle
/// * [`image`] - Pointer to the image data
/// * [`image_len`] - Length of the image data
/// * [`trust_cache`] - Pointer to the trust cache data
/// * [`trust_cache_len`] - Length of the trust cache data
/// * [`build_manifest`] - Pointer to the build manifest data
/// * [`build_manifest_len`] - Length of the build manifest data
/// * [`info_plist`] - Pointer to info plist (optional)
/// * [`unique_chip_id`] - The device's unique chip ID
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid (except optional ones which can be null)
#[cfg(feature = "tss")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_mount_personalized(
    client: *mut ImageMounterHandle,
    provider: *mut IdeviceProviderHandle,
    image: *const u8,
    image_len: libc::size_t,
    trust_cache: *const u8,
    trust_cache_len: libc::size_t,
    build_manifest: *const u8,
    build_manifest_len: libc::size_t,
    info_plist: *const c_void,
    unique_chip_id: u64,
) -> *mut IdeviceFfiError {
    if provider.is_null() || image.is_null() || trust_cache.is_null() || build_manifest.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_slice = unsafe { std::slice::from_raw_parts(image, image_len) };
    let trust_cache_slice = unsafe { std::slice::from_raw_parts(trust_cache, trust_cache_len) };
    let build_manifest_slice =
        unsafe { std::slice::from_raw_parts(build_manifest, build_manifest_len) };

    let info_plist = if !info_plist.is_null() {
        Some(
            unsafe { Box::from_raw(info_plist as *mut Value) }
                .as_ref()
                .clone(),
        )
    } else {
        None
    };

    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        client_ref
            .mount_personalized(
                provider_ref,
                image_slice.to_vec(),
                trust_cache_slice.to_vec(),
                build_manifest_slice,
                info_plist,
                unique_chip_id,
            )
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Mounts a personalized developer image with progress callback
///
/// # Arguments
/// * [`client`] - A valid ImageMounter handle
/// * [`provider`] - A valid provider handle
/// * [`image`] - Pointer to the image data
/// * [`image_len`] - Length of the image data
/// * [`trust_cache`] - Pointer to the trust cache data
/// * [`trust_cache_len`] - Length of the trust cache data
/// * [`build_manifest`] - Pointer to the build manifest data
/// * [`build_manifest_len`] - Length of the build manifest data
/// * [`info_plist`] - Pointer to info plist (optional)
/// * [`unique_chip_id`] - The device's unique chip ID
/// * [`callback`] - Progress callback function
/// * [`context`] - User context to pass to callback
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid (except optional ones which can be null)
#[cfg(feature = "tss")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_mounter_mount_personalized_with_callback(
    client: *mut ImageMounterHandle,
    provider: *mut IdeviceProviderHandle,
    image: *const u8,
    image_len: libc::size_t,
    trust_cache: *const u8,
    trust_cache_len: libc::size_t,
    build_manifest: *const u8,
    build_manifest_len: libc::size_t,
    info_plist: *const c_void,
    unique_chip_id: u64,
    callback: extern "C" fn(progress: libc::size_t, total: libc::size_t, context: *mut c_void),
    context: *mut c_void,
) -> *mut IdeviceFfiError {
    if provider.is_null() || image.is_null() || trust_cache.is_null() || build_manifest.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let image_slice = unsafe { std::slice::from_raw_parts(image, image_len) };
    let trust_cache_slice = unsafe { std::slice::from_raw_parts(trust_cache, trust_cache_len) };
    let build_manifest_slice =
        unsafe { std::slice::from_raw_parts(build_manifest, build_manifest_len) };

    let info_plist = if !info_plist.is_null() {
        Some(
            unsafe { Box::from_raw(info_plist as *mut Value) }
                .as_ref()
                .clone(),
        )
    } else {
        None
    };

    let res: Result<(), IdeviceError> = RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };

        let callback_wrapper = |((progress, total), context)| async move {
            callback(progress, total, context);
        };

        client_ref
            .mount_personalized_with_callback(
                provider_ref,
                image_slice.to_vec(),
                trust_cache_slice.to_vec(),
                build_manifest_slice,
                info_plist,
                unique_chip_id,
                callback_wrapper,
                context,
            )
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}
