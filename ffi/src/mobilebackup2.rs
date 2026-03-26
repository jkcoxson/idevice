// Jackson Coxson

use std::ffi::{CStr, c_char, c_void};
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::ptr::null_mut;

use idevice::mobilebackup2::{BackupDelegate, DirEntryInfo, MobileBackup2Client};
use idevice::{IdeviceError, IdeviceService, provider::IdeviceProvider};
use plist_ffi::PlistWrapper;

use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync_local,
};

pub struct MobileBackup2ClientHandle(pub MobileBackup2Client);

// ---------------------------------------------------------------------------
// C function-pointer table that mirrors BackupDelegate
// ---------------------------------------------------------------------------

/// C-compatible delegate for mobilebackup2 operations.
///
/// All function pointers are required except `on_file_received` and
/// `on_progress` which may be NULL.
///
/// Every path argument is a null-terminated UTF-8 string.
/// `context` is forwarded unchanged from the struct field.
#[repr(C)]
pub struct Mobilebackup2BackupDelegateFFI {
    pub context: *mut c_void,

    pub get_free_disk_space: extern "C" fn(path: *const c_char, context: *mut c_void) -> u64,

    pub open_file_read: extern "C" fn(
        path: *const c_char,
        out_data: *mut *mut u8,
        out_len: *mut usize,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,

    pub create_file_write:
        extern "C" fn(path: *const c_char, context: *mut c_void) -> *mut IdeviceFfiError,

    pub write_chunk: extern "C" fn(
        path: *const c_char,
        data: *const u8,
        len: usize,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,

    pub close_file:
        extern "C" fn(path: *const c_char, context: *mut c_void) -> *mut IdeviceFfiError,

    pub create_dir_all:
        extern "C" fn(path: *const c_char, context: *mut c_void) -> *mut IdeviceFfiError,

    pub remove: extern "C" fn(path: *const c_char, context: *mut c_void) -> *mut IdeviceFfiError,

    pub rename: extern "C" fn(
        from: *const c_char,
        to: *const c_char,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,

    pub copy: extern "C" fn(
        src: *const c_char,
        dst: *const c_char,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,

    pub exists: extern "C" fn(path: *const c_char, context: *mut c_void) -> bool,

    pub is_dir: extern "C" fn(path: *const c_char, context: *mut c_void) -> bool,

    /// Optional progress callback. May be NULL.
    pub on_progress: Option<
        extern "C" fn(
            bytes_done: u64,
            bytes_total: u64,
            overall_progress: f64,
            context: *mut c_void,
        ),
    >,
}

// Safety: the C side is responsible for thread safety of its context.
unsafe impl Send for Mobilebackup2BackupDelegateFFI {}
unsafe impl Sync for Mobilebackup2BackupDelegateFFI {}

// ---------------------------------------------------------------------------
// Adapter: wraps C function pointers into a Rust BackupDelegate
// ---------------------------------------------------------------------------

/// For write operations we need something that implements Write.
/// We buffer writes in the FFI adapter and call the C write_chunk callback.
struct FfiFileWriter {
    path_c: std::ffi::CString,
    delegate: *const Mobilebackup2BackupDelegateFFI,
}

impl Write for FfiFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let d = unsafe { &*self.delegate };
        let err = (d.write_chunk)(self.path_c.as_ptr(), buf.as_ptr(), buf.len(), d.context);
        if err.is_null() {
            Ok(buf.len())
        } else {
            // Free the error and return a generic IO error
            unsafe { crate::errors::idevice_error_free(err) };
            Err(std::io::Error::other("write_chunk failed"))
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for FfiFileWriter {
    fn drop(&mut self) {
        let d = unsafe { &*self.delegate };
        let _ = (d.close_file)(self.path_c.as_ptr(), d.context);
    }
}

// Safety: the C side is responsible for thread safety.
unsafe impl Send for FfiFileWriter {}

fn path_to_cstring(path: &Path) -> std::ffi::CString {
    std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap_or_default()
}

fn ffi_err_to_idevice(err: *mut IdeviceFfiError) -> IdeviceError {
    if err.is_null() {
        return IdeviceError::UnexpectedResponse;
    }
    let msg = unsafe {
        if (*err).message.is_null() {
            "FFI delegate error".to_string()
        } else {
            CStr::from_ptr((*err).message).to_string_lossy().to_string()
        }
    };
    unsafe { crate::errors::idevice_error_free(err) };
    IdeviceError::InternalError(msg)
}

impl BackupDelegate for Mobilebackup2BackupDelegateFFI {
    fn get_free_disk_space(&self, path: &Path) -> u64 {
        let c = path_to_cstring(path);
        (self.get_free_disk_space)(c.as_ptr(), self.context)
    }

    fn open_file_read<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Read + Send>, IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let c = path_to_cstring(path);
            let mut data: *mut u8 = null_mut();
            let mut len: usize = 0;
            let err = (self.open_file_read)(c.as_ptr(), &mut data, &mut len, self.context);
            if !err.is_null() {
                return Err(ffi_err_to_idevice(err));
            }
            let vec = if data.is_null() || len == 0 {
                Vec::new()
            } else {
                unsafe { Vec::from_raw_parts(data, len, len) }
            };
            Ok(Box::new(std::io::Cursor::new(vec)) as Box<dyn Read + Send>)
        })
    }

    fn create_file_write<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Write + Send>, IdeviceError>> + Send + 'a>>
    {
        Box::pin(async move {
            let c = path_to_cstring(path);
            let err = (self.create_file_write)(c.as_ptr(), self.context);
            if !err.is_null() {
                return Err(ffi_err_to_idevice(err));
            }
            Ok(Box::new(FfiFileWriter {
                path_c: c,
                delegate: self as *const _,
            }) as Box<dyn Write + Send>)
        })
    }

    fn create_dir_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let c = path_to_cstring(path);
            let err = (self.create_dir_all)(c.as_ptr(), self.context);
            if err.is_null() {
                Ok(())
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }

    fn remove<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let c = path_to_cstring(path);
            let err = (self.remove)(c.as_ptr(), self.context);
            if err.is_null() {
                Ok(())
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let cf = path_to_cstring(from);
            let ct = path_to_cstring(to);
            let err = (self.rename)(cf.as_ptr(), ct.as_ptr(), self.context);
            if err.is_null() {
                Ok(())
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }

    fn copy<'a>(
        &'a self,
        src: &'a Path,
        dst: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let cs = path_to_cstring(src);
            let cd = path_to_cstring(dst);
            let err = (self.copy)(cs.as_ptr(), cd.as_ptr(), self.context);
            if err.is_null() {
                Ok(())
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let c = path_to_cstring(path);
            (self.exists)(c.as_ptr(), self.context)
        })
    }

    fn is_dir<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let c = path_to_cstring(path);
            (self.is_dir)(c.as_ptr(), self.context)
        })
    }

    fn list_dir<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntryInfo>, IdeviceError>> + Send + 'a>> {
        // The C delegate doesn't need list_dir - the library's FsBackupDelegate
        // handles it via tokio::fs. For FFI, return empty since this is only used
        // for DLContentsOfDirectory which is rare.
        Box::pin(async { Ok(Vec::new()) })
    }

    fn on_progress(&self, bytes_done: u64, bytes_total: u64, overall_progress: f64) {
        if let Some(cb) = self.on_progress {
            cb(bytes_done, bytes_total, overall_progress, self.context);
        }
    }
}

// ---------------------------------------------------------------------------
// FFI functions
// ---------------------------------------------------------------------------

/// Connects to the mobilebackup2 service via a provider
///
/// # Safety
/// All pointer arguments must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut MobileBackup2ClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res: Result<MobileBackup2Client, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        MobileBackup2Client::connect(provider_ref).await
    });
    match res {
        Ok(r) => {
            unsafe { *client = Box::into_raw(Box::new(MobileBackup2ClientHandle(r))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a mobilebackup2 client from an existing connection (consumes the socket)
///
/// # Safety
/// `socket` is consumed and must not be used after this call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut MobileBackup2ClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let res: Result<MobileBackup2Client, IdeviceError> =
        run_sync_local(async { MobileBackup2Client::from_stream(socket).await });
    match res {
        Ok(r) => {
            unsafe { *client = Box::into_raw(Box::new(MobileBackup2ClientHandle(r))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a mobilebackup2 client handle
///
/// # Safety
/// `handle` must be valid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_client_free(handle: *mut MobileBackup2ClientHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Creates a backup of the device
///
/// # Arguments
/// * `client` - A valid MobileBackup2Client handle
/// * `backup_root` - Path to the backup root directory (null-terminated UTF-8)
/// * `source_identifier` - Source UDID (null-terminated UTF-8, or NULL for current device)
/// * `options` - Optional plist dictionary of backup options (NULL for defaults)
/// * `delegate` - Pointer to a populated Mobilebackup2BackupDelegateFFI struct
/// * `out_response` - On success, receives the device response plist (caller must free). May be NULL.
///
/// # Safety
/// All non-null pointers must be valid. `delegate` must remain valid for the entire call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_backup(
    client: *mut MobileBackup2ClientHandle,
    backup_root: *const c_char,
    source_identifier: *const c_char,
    options: crate::plist_t,
    delegate: *const Mobilebackup2BackupDelegateFFI,
    out_response: *mut crate::plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || backup_root.is_null() || delegate.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let backup_root = PathBuf::from(
        unsafe { CStr::from_ptr(backup_root) }
            .to_string_lossy()
            .as_ref(),
    );
    let source = if source_identifier.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(source_identifier) }
                .to_string_lossy()
                .to_string(),
        )
    };
    let opts = if options.is_null() {
        None
    } else {
        let pw = unsafe { &mut *(options as *mut PlistWrapper) };
        if let plist::Value::Dictionary(d) = pw.borrow_self().clone() {
            Some(d)
        } else {
            None
        }
    };

    let delegate_ref = unsafe { &*delegate };

    let res = run_sync_local(async {
        unsafe { &mut *client }
            .0
            .backup_from_path(&backup_root, source.as_deref(), opts, delegate_ref)
            .await
    });

    match res {
        Ok(response) => {
            if !out_response.is_null() {
                if let Some(dict) = response {
                    unsafe {
                        *out_response = PlistWrapper::new_node(plist::Value::Dictionary(dict))
                            .into_ptr() as crate::plist_t
                    };
                } else {
                    unsafe { *out_response = null_mut() };
                }
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Restores a backup to the device
///
/// # Safety
/// All non-null pointers must be valid. `delegate` must remain valid for the entire call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_restore(
    client: *mut MobileBackup2ClientHandle,
    backup_root: *const c_char,
    source_identifier: *const c_char,
    options: crate::plist_t,
    delegate: *const Mobilebackup2BackupDelegateFFI,
    out_response: *mut crate::plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || backup_root.is_null() || delegate.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let backup_root = PathBuf::from(
        unsafe { CStr::from_ptr(backup_root) }
            .to_string_lossy()
            .as_ref(),
    );
    let source = if source_identifier.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(source_identifier) }
                .to_string_lossy()
                .to_string(),
        )
    };

    // Build RestoreOptions from plist dict keys if provided
    let restore_opts = if options.is_null() {
        None
    } else {
        let pw = unsafe { &mut *(options as *mut PlistWrapper) };
        if let plist::Value::Dictionary(d) = pw.borrow_self().clone() {
            let mut opts = idevice::mobilebackup2::RestoreOptions::new();
            if let Some(v) = d.get("RestoreShouldReboot").and_then(|v| v.as_boolean()) {
                opts = opts.with_reboot(v);
            }
            if let Some(v) = d.get("RestoreDontCopyBackup").and_then(|v| v.as_boolean()) {
                opts = opts.with_copy(!v);
            }
            if let Some(v) = d
                .get("RestorePreserveSettings")
                .and_then(|v| v.as_boolean())
            {
                opts = opts.with_preserve_settings(v);
            }
            if let Some(v) = d.get("RestoreSystemFiles").and_then(|v| v.as_boolean()) {
                opts = opts.with_system_files(v);
            }
            if let Some(v) = d.get("RemoveItemsNotRestored").and_then(|v| v.as_boolean()) {
                opts = opts.with_remove_items_not_restored(v);
            }
            if let Some(v) = d.get("Password").and_then(|v| v.as_string()) {
                opts = opts.with_password(v);
            }
            Some(opts)
        } else {
            None
        }
    };

    let delegate_ref = unsafe { &*delegate };

    let res = run_sync_local(async {
        unsafe { &mut *client }
            .0
            .restore_from_path(&backup_root, source.as_deref(), restore_opts, delegate_ref)
            .await
    });

    match res {
        Ok(response) => {
            if !out_response.is_null() {
                if let Some(dict) = response {
                    unsafe {
                        *out_response = PlistWrapper::new_node(plist::Value::Dictionary(dict))
                            .into_ptr() as crate::plist_t
                    };
                } else {
                    unsafe { *out_response = null_mut() };
                }
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Changes the backup password on the device
///
/// # Safety
/// All non-null pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_change_password(
    client: *mut MobileBackup2ClientHandle,
    backup_root: *const c_char,
    old_password: *const c_char,
    new_password: *const c_char,
    delegate: *const Mobilebackup2BackupDelegateFFI,
) -> *mut IdeviceFfiError {
    if client.is_null() || backup_root.is_null() || delegate.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let backup_root = PathBuf::from(
        unsafe { CStr::from_ptr(backup_root) }
            .to_string_lossy()
            .as_ref(),
    );
    let old_pw = if old_password.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(old_password) }
                .to_string_lossy()
                .to_string(),
        )
    };
    let new_pw = if new_password.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(new_password) }
                .to_string_lossy()
                .to_string(),
        )
    };

    let delegate_ref = unsafe { &*delegate };

    let res = run_sync_local(async {
        unsafe { &mut *client }
            .0
            .change_password_from_path(
                &backup_root,
                old_pw.as_deref(),
                new_pw.as_deref(),
                delegate_ref,
            )
            .await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Disconnects from the mobilebackup2 service
///
/// # Safety
/// `client` must be a valid handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobilebackup2_disconnect(
    client: *mut MobileBackup2ClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let res = run_sync_local(async { unsafe { &mut *client }.0.disconnect().await });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}
