// Jackson Coxson

use idevice::pairing_file::PairingFile;
use std::ffi::{CStr, c_char};

use crate::IdeviceErrorCode;

/// Opaque C-compatible handle to a PairingFile
pub struct IdevicePairingFile(pub PairingFile);

/// Reads a pairing file from the specified path
///
/// # Arguments
/// * [`path`] - Path to the pairing file
/// * [`pairing_file`] - On success, will be set to point to a newly allocated pairing file instance
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `path` must be a valid null-terminated C string
/// `pairing_file` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_pairing_file_read(
    path: *const c_char,
    pairing_file: *mut *mut IdevicePairingFile,
) -> IdeviceErrorCode {
    if path.is_null() || pairing_file.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    // Convert C string to Rust path
    let c_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return IdeviceErrorCode::InvalidArg,
    };

    // Read the pairing file
    match PairingFile::read_from_file(c_str) {
        Ok(pf) => {
            let boxed = Box::new(IdevicePairingFile(pf));
            unsafe {
                *pairing_file = Box::into_raw(boxed);
            }
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Parses a pairing file from a byte buffer
///
/// # Arguments
/// * [`data`] - Pointer to the buffer containing pairing file data
/// * [`size`] - Size of the buffer in bytes
/// * [`pairing_file`] - On success, will be set to point to a newly allocated pairing file instance
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `data` must be a valid pointer to a buffer of at least `size` bytes
/// `pairing_file` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_pairing_file_from_bytes(
    data: *const u8,
    size: usize,
    pairing_file: *mut *mut IdevicePairingFile,
) -> IdeviceErrorCode {
    if data.is_null() || pairing_file.is_null() || size == 0 {
        return IdeviceErrorCode::InvalidArg;
    }

    // Convert to Rust slice
    let bytes = unsafe { std::slice::from_raw_parts(data, size) };

    // Parse the pairing file
    match PairingFile::from_bytes(bytes) {
        Ok(pf) => {
            let boxed = Box::new(IdevicePairingFile(pf));
            unsafe { *pairing_file = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Serializes a pairing file to XML format
///
/// # Arguments
/// * [`pairing_file`] - The pairing file to serialize
/// * [`data`] - On success, will be set to point to a newly allocated buffer containing the serialized data
/// * [`size`] - On success, will be set to the size of the allocated buffer
///
/// # Returns
/// An error code indicating success or failure
///
/// # Safety
/// `pairing_file` must be a valid, non-null pointer to a pairing file instance
/// `data` must be a valid, non-null pointer to a location where the buffer pointer will be stored
/// `size` must be a valid, non-null pointer to a location where the buffer size will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_pairing_file_serialize(
    pairing_file: *const IdevicePairingFile,
    data: *mut *mut u8,
    size: *mut usize,
) -> IdeviceErrorCode {
    if pairing_file.is_null() || data.is_null() || size.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    // Get the pairing file
    let pf = unsafe { &(*pairing_file).0 };

    // Serialize the pairing file
    match pf.clone().serialize() {
        Ok(buffer) => {
            let buffer_size = buffer.len();
            let buffer_ptr = Box::into_raw(buffer.into_boxed_slice()) as *mut u8;
            unsafe { *data = buffer_ptr };
            unsafe { *size = buffer_size };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

/// Frees a pairing file instance
///
/// # Arguments
/// * [`pairing_file`] - The pairing file to free
///
/// # Safety
/// `pairing_file` must be a valid pointer to a pairing file instance that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_pairing_file_free(pairing_file: *mut IdevicePairingFile) {
    if !pairing_file.is_null() {
        log::debug!("Freeing pairing file");
        let _ = unsafe { Box::from_raw(pairing_file) };
    }
}
