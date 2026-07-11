//! C FFI for the full IPSW restore stack.
//!
//! The Rust library is a protocol state machine that takes its I/O surfaces as
//! traits, so this FFI exposes each of those surfaces as a C function-pointer
//! delegate struct and lets a C/C++ caller bring its own transports:
//!
//! - [`IdeviceRestoreComponentSourceFFI`] - supplies firmware component bytes.
//! - [`IdeviceRestoreFilesystemImageFFI`] - the seekable filesystem (DMG) image.
//! - [`IdeviceRestoreDataPortConnectorFFI`] - opens restore-mode data ports.
//! - [`IdeviceRestoreProgressFFI`] - progress callbacks for a UI.
//! - [`IdeviceRestoreRecoveryTransportFFI`] - the recovery/DFU USB transport.
//! - [`IdeviceRestoreFdrConnectorFFI`] - opens FDR trust-channel connections.
//!
//! `idevice_restore_run` drives the restore-mode state machine to completion;
//! the recovery, IPSW, TSS and IMG4 helpers cover the surrounding steps.

use std::ffi::{CStr, CString, c_char, c_void};
use std::future::Future;
use std::pin::Pin;
use std::ptr::null_mut;

use idevice::restore::{
    ComponentReader, ComponentSource, DataPortConnector, RestoreCancel, RestoreContext,
    RestoreError, RestoreOptions, RestoreProgressEvent, RestoredClient,
    asr::FilesystemImage,
    img4::build_preboard_manifest,
    ipsw::{self, Ipsw},
    progress_channel, run_restore,
};
use idevice::tss::{TSSRequest, extract_img4_ticket, select_build_identity};
use idevice::{Idevice, IdeviceError};
use plist_ffi::PlistWrapper;

use crate::{IdeviceFfiError, IdeviceHandle, ffi_err, run_sync_local};

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

/// Turns a `*mut IdeviceFfiError` returned by a C delegate into an `IdeviceError`,
/// freeing the FFI error.
fn ffi_err_to_idevice(err: *mut IdeviceFfiError) -> IdeviceError {
    if err.is_null() {
        return IdeviceError::Restore(RestoreError::Other("null FFI error pointer".into()));
    }
    let msg = unsafe {
        if (*err).message.is_null() {
            "FFI delegate error".to_string()
        } else {
            CStr::from_ptr((*err).message).to_string_lossy().to_string()
        }
    };
    unsafe { crate::errors::idevice_error_free(err) };
    IdeviceError::Restore(RestoreError::Other(msg))
}

/// Borrows a `plist_t` that must be a dictionary, cloning it.
fn plist_to_dict(p: crate::plist_t) -> Option<plist::Dictionary> {
    if p.is_null() {
        return None;
    }
    let pw = unsafe { &mut *(p as *mut PlistWrapper) };
    match pw.borrow_self().clone() {
        plist::Value::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Allocates a `plist_t` node owning `dict`.
fn dict_to_plist(dict: plist::Dictionary) -> crate::plist_t {
    PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr() as crate::plist_t
}

/// Reads a C string into a `String`, or `None` for NULL.
fn cstr_opt(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string())
    }
}

/// Writes `bytes` out to a caller-freed buffer (free with `idevice_data_free`).
fn bytes_out(bytes: Vec<u8>, out_data: *mut *mut u8, out_len: *mut usize) {
    let mut boxed = bytes.into_boxed_slice();
    unsafe {
        *out_data = boxed.as_mut_ptr();
        *out_len = boxed.len();
    }
    std::mem::forget(boxed);
}

/// Reads a `(ptr, len)` pair the C side handed back, taking ownership of the
/// allocation (it must have been allocated with the system allocator).
unsafe fn take_c_bytes(data: *mut u8, len: usize) -> Vec<u8> {
    if data.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { Vec::from_raw_parts(data, len, len) }
    }
}

/// Builds an `Option<Vec<u8>>` from a `(ptr, len)` input slice (borrowed, copied).
unsafe fn opt_slice(ptr: *const u8, len: usize) -> Option<Vec<u8>> {
    if ptr.is_null() || len == 0 {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec())
    }
}

// ---------------------------------------------------------------------------
// RestoredClient
// ---------------------------------------------------------------------------

/// Opaque handle to a restore-mode `com.apple.mobile.restored` client.
pub struct RestoredClientHandle(pub RestoredClient);

/// Connects to `restored` over an existing [`IdeviceHandle`] (consumes it).
///
/// # Safety
/// `idevice` is consumed and must not be used afterwards. `out_client` must be a
/// valid, non-null location for the resulting handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restored_connect(
    idevice: *mut IdeviceHandle,
    out_client: *mut *mut RestoredClientHandle,
) -> *mut IdeviceFfiError {
    if idevice.is_null() || out_client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let dev = unsafe { Box::from_raw(idevice) }.0;
    match run_sync_local(async move { RestoredClient::connect(dev).await }) {
        Ok(c) => {
            unsafe { *out_client = Box::into_raw(Box::new(RestoredClientHandle(c))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Finds a restore-mode device by ECID over usbmux and connects to `restored`.
///
/// After a normal to restore transition the device re-enumerates with a new usbmux
/// id, so this polls the device list and matches on `HardwareInfo.UniqueChipID`,
/// retrying until `timeout_ms` elapses.
///
/// # Safety
/// `addr` must be a valid `UsbmuxdAddrHandle` (it is borrowed, not consumed);
/// `out_client` must be valid; `label` a valid C string or NULL.
#[cfg(feature = "usbmuxd")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restored_connect_by_ecid(
    addr: *mut crate::usbmuxd::UsbmuxdAddrHandle,
    ecid: u64,
    label: *const c_char,
    timeout_ms: u64,
    out_client: *mut *mut RestoredClientHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null() || out_client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let addr = unsafe { &*addr }.0.clone();
    let label = cstr_opt(label).unwrap_or_else(|| "idevice-restore".to_string());
    let res = crate::run_sync(async move {
        RestoredClient::connect_by_ecid(
            &addr,
            ecid,
            &label,
            std::time::Duration::from_millis(timeout_ms),
        )
        .await
    });
    match res {
        Ok(c) => {
            unsafe { *out_client = Box::into_raw(Box::new(RestoredClientHandle(c))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads the device's ECID (from `HardwareInfo`).
///
/// # Safety
/// `client` and `out_ecid` must be valid, non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restored_get_ecid(
    client: *mut RestoredClientHandle,
    out_ecid: *mut u64,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_ecid.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let client = unsafe { &mut *client };
    match run_sync_local(async { client.0.ecid().await }) {
        Ok(ecid) => {
            unsafe { *out_ecid = ecid };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads the usbmux `device_id` the client was found on.
///
/// Only meaningful when the client was created with
/// `idevice_restored_connect_by_ecid`; writes `true` to `out_has_device_id` in
/// that case (and the id to `out_device_id`), or `false` otherwise (e.g. clients
/// built from an existing `Idevice`). Pass the id to
/// `idevice_restore_connect_usb_port` so data-port / FDR connections target this
/// same device.
///
/// # Safety
/// `client`, `out_device_id`, `out_has_device_id` must be valid, non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restored_get_device_id(
    client: *mut RestoredClientHandle,
    out_device_id: *mut u32,
    out_has_device_id: *mut bool,
) -> *mut IdeviceFfiError {
    if client.is_null() || out_device_id.is_null() || out_has_device_id.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let client = unsafe { &*client };
    match client.0.device_id {
        Some(id) => unsafe {
            *out_device_id = id;
            *out_has_device_id = true;
        },
        None => unsafe {
            *out_has_device_id = false;
        },
    }
    null_mut()
}

/// Frees a [`RestoredClientHandle`].
///
/// # Safety
/// `client` must be a handle allocated by this library, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restored_free(client: *mut RestoredClientHandle) {
    if !client.is_null() {
        let _ = unsafe { Box::from_raw(client) };
    }
}

/// Connects to `port` on the usbmux device identified by `device_id`, returning a
/// new [`IdeviceHandle`]. A convenience for restore data-port and FDR connectors.
///
/// `device_id` must be the id `idevice_restored_get_device_id` reported for the
/// restore-mode client, so the connection targets the device being restored.
///
/// The entire find-device-and-connect sequence runs in one async task; splitting
/// it across separate blocking calls corrupts the shared tokio I/O state, so this
/// is the supported way to build those connectors from C.
///
/// This is a single attempt (the restore state machine retries data-port
/// connects itself); it errors rather than blocking when the device or port is
/// not yet available.
///
/// # Safety
/// `addr` must be a valid `UsbmuxdAddrHandle` (it is borrowed, not consumed);
/// `out_idevice` must be valid; `label` a valid C string or NULL.
#[cfg(feature = "usbmuxd")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_connect_usb_port(
    addr: *mut crate::usbmuxd::UsbmuxdAddrHandle,
    device_id: u32,
    port: u16,
    label: *const c_char,
    out_idevice: *mut *mut IdeviceHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null() || out_idevice.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let addr = unsafe { &*addr }.0.clone();
    let label = cstr_opt(label).unwrap_or_else(|| "idevice-restore".to_string());
    let res = crate::run_sync(async move {
        let mux = addr.connect(1).await?;
        mux.connect_to_device(device_id, port, &label).await
    });
    match res {
        Ok(idevice) => {
            unsafe { *out_idevice = Box::into_raw(Box::new(IdeviceHandle(idevice))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

// ---------------------------------------------------------------------------
// ComponentSource delegate
// ---------------------------------------------------------------------------

/// C delegate supplying firmware component bytes by archive path.
///
/// `read_component` (required) reads a whole component into a system-allocated
/// buffer (ownership transfers to the library, which frees it). The optional
/// streaming trio (`open_component`/`read_chunk`/`close_component`) lets large
/// source boot objects stream without buffering; when `open_component` is NULL the
/// library falls back to buffering via `read_component`.
#[repr(C)]
pub struct IdeviceRestoreComponentSourceFFI {
    pub context: *mut c_void,
    pub read_component: extern "C" fn(
        path: *const c_char,
        out_data: *mut *mut u8,
        out_len: *mut usize,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,
    pub open_component: Option<
        extern "C" fn(
            path: *const c_char,
            out_reader: *mut *mut c_void,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
    >,
    pub read_chunk: Option<
        extern "C" fn(
            reader: *mut c_void,
            buf: *mut u8,
            buf_len: usize,
            out_read: *mut usize,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
    >,
    pub close_component: Option<extern "C" fn(reader: *mut c_void, context: *mut c_void)>,
}

unsafe impl Send for IdeviceRestoreComponentSourceFFI {}
unsafe impl Sync for IdeviceRestoreComponentSourceFFI {}

/// A [`ComponentReader`] driven by the delegate's `read_chunk`/`close_component`.
struct FfiComponentReader {
    reader: *mut c_void,
    read_chunk:
        extern "C" fn(*mut c_void, *mut u8, usize, *mut usize, *mut c_void) -> *mut IdeviceFfiError,
    close: Option<extern "C" fn(*mut c_void, *mut c_void)>,
    context: *mut c_void,
}

unsafe impl Send for FfiComponentReader {}

impl ComponentReader for FfiComponentReader {
    fn read<'a>(
        &'a mut self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let mut read = 0usize;
            let err = (self.read_chunk)(
                self.reader,
                buf.as_mut_ptr(),
                buf.len(),
                &mut read,
                self.context,
            );
            if err.is_null() {
                Ok(read)
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }
}

impl Drop for FfiComponentReader {
    fn drop(&mut self) {
        if let Some(close) = self.close {
            close(self.reader, self.context);
        }
    }
}

/// An already-buffered [`ComponentReader`], for the non-streaming fallback.
struct BufferedReader {
    data: Vec<u8>,
    pos: usize,
}

impl ComponentReader for BufferedReader {
    fn read<'a>(
        &'a mut self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let n = (self.data.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        })
    }
}

impl ComponentSource for IdeviceRestoreComponentSourceFFI {
    fn read_component<'a>(
        &'a mut self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, IdeviceError>> + Send + 'a>> {
        Box::pin(async move {
            let cpath = CString::new(path).unwrap_or_default();
            let mut data: *mut u8 = null_mut();
            let mut len: usize = 0;
            let err = (self.read_component)(cpath.as_ptr(), &mut data, &mut len, self.context);
            if err.is_null() {
                Ok(unsafe { take_c_bytes(data, len) })
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }

    fn open_component<'a>(
        &'a mut self,
        path: &'a str,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn ComponentReader + Send + 'a>, IdeviceError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            match (self.open_component, self.read_chunk) {
                (Some(open), Some(read_chunk)) => {
                    let cpath = CString::new(path).unwrap_or_default();
                    let mut reader: *mut c_void = null_mut();
                    let err = open(cpath.as_ptr(), &mut reader, self.context);
                    if !err.is_null() {
                        return Err(ffi_err_to_idevice(err));
                    }
                    Ok(Box::new(FfiComponentReader {
                        reader,
                        read_chunk,
                        close: self.close_component,
                        context: self.context,
                    }) as Box<dyn ComponentReader + Send>)
                }
                _ => {
                    let data = self.read_component(path).await?;
                    Ok(Box::new(BufferedReader { data, pos: 0 })
                        as Box<dyn ComponentReader + Send>)
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// FilesystemImage delegate
// ---------------------------------------------------------------------------

/// C delegate exposing a seekable, sized filesystem (DMG) image for ASR.
#[repr(C)]
pub struct IdeviceRestoreFilesystemImageFFI {
    pub context: *mut c_void,
    /// Returns the total image size in bytes.
    pub size: extern "C" fn(out_size: *mut u64, context: *mut c_void) -> *mut IdeviceFfiError,
    /// Reads up to `len` bytes at `offset` into a system-allocated buffer whose
    /// ownership transfers to the library.
    pub read_at: extern "C" fn(
        offset: u64,
        len: usize,
        out_data: *mut *mut u8,
        out_len: *mut usize,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,
}

unsafe impl Send for IdeviceRestoreFilesystemImageFFI {}

impl FilesystemImage for IdeviceRestoreFilesystemImageFFI {
    fn size(&mut self) -> Pin<Box<dyn Future<Output = Result<u64, IdeviceError>> + Send + '_>> {
        Box::pin(async move {
            let mut size = 0u64;
            let err = (self.size)(&mut size, self.context);
            if err.is_null() {
                Ok(size)
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }

    fn read_at(
        &mut self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, IdeviceError>> + Send + '_>> {
        Box::pin(async move {
            let mut data: *mut u8 = null_mut();
            let mut out_len: usize = 0;
            let err = (self.read_at)(offset, len, &mut data, &mut out_len, self.context);
            if err.is_null() {
                Ok(unsafe { take_c_bytes(data, out_len) })
            } else {
                Err(ffi_err_to_idevice(err))
            }
        })
    }
}

// ---------------------------------------------------------------------------
// DataPortConnector delegate
// ---------------------------------------------------------------------------

/// C delegate opening fresh connections to restore-mode data ports.
#[repr(C)]
pub struct IdeviceRestoreDataPortConnectorFFI {
    pub context: *mut c_void,
    /// Connects to `port`, yielding a new [`IdeviceHandle`] (ownership transfers
    /// to the library).
    pub connect: extern "C" fn(
        port: u16,
        out_idevice: *mut *mut IdeviceHandle,
        context: *mut c_void,
    ) -> *mut IdeviceFfiError,
}

unsafe impl Send for IdeviceRestoreDataPortConnectorFFI {}
unsafe impl Sync for IdeviceRestoreDataPortConnectorFFI {}

impl DataPortConnector for IdeviceRestoreDataPortConnectorFFI {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        // The C callback is synchronous, so run it eagerly here. This keeps the
        // non-`Send` context pointer out of the returned future entirely — the
        // future only ever holds the already-`Send` `Result` — which sidesteps
        // both the `Send` bound and any pointer-provenance laundering.
        let mut handle: *mut IdeviceHandle = null_mut();
        let err = (self.connect)(port, &mut handle, self.context);
        let result = if !err.is_null() {
            Err(ffi_err_to_idevice(err))
        } else if handle.is_null() {
            Err(IdeviceError::Restore(RestoreError::DataPortConnect(port)))
        } else {
            Ok(unsafe { Box::from_raw(handle) }.0)
        };
        Box::pin(async move { result })
    }
}

// ---------------------------------------------------------------------------
// RestoreProgress delegate
// ---------------------------------------------------------------------------

/// C delegate receiving restore progress callbacks. Any field may be NULL.
#[repr(C)]
pub struct IdeviceRestoreProgressFFI {
    pub context: *mut c_void,
    /// The device's operation code and completion percentage (0–100).
    pub operation: Option<extern "C" fn(operation: u64, progress: u64, context: *mut c_void)>,
    /// A named host step (the `DataType` being serviced).
    pub step: Option<extern "C" fn(name: *const c_char, context: *mut c_void)>,
    pub transfer: Option<
        extern "C" fn(
            component: *const c_char,
            sent: u64,
            total: u64,
            has_total: bool,
            context: *mut c_void,
        ),
    >,
}

unsafe impl Send for IdeviceRestoreProgressFFI {}

impl IdeviceRestoreProgressFFI {
    /// Invokes the matching C callback for one progress event.
    fn dispatch(&self, event: &RestoreProgressEvent) {
        match event {
            RestoreProgressEvent::Operation {
                operation,
                progress,
            } => {
                if let Some(cb) = self.operation {
                    cb(*operation, *progress, self.context);
                }
            }
            RestoreProgressEvent::Step(name) => {
                if let Some(cb) = self.step {
                    let c = CString::new(name.as_str()).unwrap_or_default();
                    cb(c.as_ptr(), self.context);
                }
            }
            RestoreProgressEvent::Transfer {
                component,
                sent,
                total,
            } => {
                if let Some(cb) = self.transfer {
                    let c = CString::new(component.as_str()).unwrap_or_default();
                    cb(
                        c.as_ptr(),
                        *sent,
                        total.unwrap_or(0),
                        total.is_some(),
                        self.context,
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cancellation handle
// ---------------------------------------------------------------------------

/// An opaque, shareable cancellation flag for an in-flight restore.
///
/// Create one with `idevice_restore_cancel_handle_new`, pass it to
/// `idevice_restore_run`, and call `idevice_restore_cancel` from another thread to
/// request a graceful cancel (the device is rebooted toward recovery). Free it with
/// `idevice_restore_cancel_handle_free` once the restore has returned.
pub struct IdeviceRestoreCancelHandle(pub RestoreCancel);

/// Allocates a cancellation handle.
///
/// # Safety
/// `out_handle` must be a valid, non-null location for the handle pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_cancel_handle_new(
    out_handle: *mut *mut IdeviceRestoreCancelHandle,
) -> *mut IdeviceFfiError {
    if out_handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let handle = Box::new(IdeviceRestoreCancelHandle(RestoreCancel::new()));
    unsafe { *out_handle = Box::into_raw(handle) };
    null_mut()
}

/// Requests cancellation of the restore this handle was passed to.
///
/// Safe to call from any thread while the restore runs; it is a no-op if `handle`
/// is NULL.
///
/// # Safety
/// `handle` must be a valid handle from `idevice_restore_cancel_handle_new` (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_cancel(handle: *mut IdeviceRestoreCancelHandle) {
    if let Some(handle) = unsafe { handle.as_ref() } {
        handle.0.cancel();
    }
}

/// Frees a cancellation handle.
///
/// # Safety
/// `handle` must be a valid handle from `idevice_restore_cancel_handle_new` (or NULL)
/// and must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_cancel_handle_free(
    handle: *mut IdeviceRestoreCancelHandle,
) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

// ---------------------------------------------------------------------------
// RestoreOptions
// ---------------------------------------------------------------------------

/// Builds the default iOS `RestoreOptions` dictionary sent with `StartRestore`.
///
/// The caller may tweak the returned plist before passing it to
/// `idevice_restore_run`, and must free it with `plist_free`.
///
/// # Safety
/// `out_options` must be a valid, non-null location for the plist.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_options_new(
    out_options: *mut crate::plist_t,
) -> *mut IdeviceFfiError {
    if out_options.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    unsafe { *out_options = dict_to_plist(RestoreOptions::new().build()) };
    null_mut()
}

// ---------------------------------------------------------------------------
// run_restore
// ---------------------------------------------------------------------------

/// Drives the restore-mode state machine to completion.
///
/// Sends `StartRestore` with `options`, then services the device's data requests
/// (personalizing components with `tss_ticket`, streaming the filesystem over
/// ASR, proxying its key requests) until the device reports success.
///
/// # Arguments
/// * `client` - connected [`RestoredClientHandle`].
/// * `build_identity` - the selected build-identity dictionary (plist).
/// * `board_id`, `chip_id`, `ecid` - device identifiers.
/// * `tss_ticket`/`tss_ticket_len` - the `ApImg4Ticket` (IM4M) from TSS.
/// * `components` - component-source delegate (required).
/// * `filesystem` - filesystem-image delegate, or NULL for a restore that sends
///   no filesystem.
/// * `data_ports` - data-port connector delegate (required).
/// * `progress` - progress delegate, or NULL.
/// * `cancel` - cancellation handle from `idevice_restore_cancel_handle_new`, or
///   NULL. When another thread calls `idevice_restore_cancel` on it, the restore
///   stops and the device is rebooted toward recovery (returning a `Cancelled`
///   error).
/// * `options` - the `RestoreOptions` plist (see `idevice_restore_options_new`).
///
/// # Safety
/// All non-NULL pointers must be valid for the duration of the call, and each
/// delegate struct must remain valid until this returns.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn idevice_restore_run(
    client: *mut RestoredClientHandle,
    build_identity: crate::plist_t,
    board_id: u64,
    chip_id: u64,
    ecid: u64,
    tss_ticket: *const u8,
    tss_ticket_len: usize,
    components: *mut IdeviceRestoreComponentSourceFFI,
    filesystem: *mut IdeviceRestoreFilesystemImageFFI,
    data_ports: *mut IdeviceRestoreDataPortConnectorFFI,
    progress: *mut IdeviceRestoreProgressFFI,
    cancel: *mut IdeviceRestoreCancelHandle,
    options: crate::plist_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || components.is_null() || data_ports.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(build_identity) = plist_to_dict(build_identity) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_identity is not a plist dictionary".into(),
        )));
    };
    let options = plist_to_dict(options).unwrap_or_else(|| RestoreOptions::new().build());
    let ticket = unsafe { opt_slice(tss_ticket, tss_ticket_len) }.unwrap_or_default();

    let client = unsafe { &mut *client };
    let components = unsafe { &mut *components };
    let data_ports = unsafe { &mut *data_ports };
    let filesystem_opt: Option<&mut dyn FilesystemImage> = if filesystem.is_null() {
        None
    } else {
        Some(unsafe { &mut *filesystem })
    };
    let progress_delegate: Option<&IdeviceRestoreProgressFFI> = unsafe { progress.as_ref() };
    let cancel_flag: Option<RestoreCancel> =
        unsafe { cancel.as_ref() }.map(|handle| handle.0.clone());

    // Progress crosses from the state machine's concurrent tasks over a channel;
    // a drain future invokes the C callbacks on this (single) thread.
    let (progress_tx, mut progress_rx) = progress_channel();
    let progress_sender = progress_delegate.map(|_| progress_tx);

    let ctx = RestoreContext {
        restored: &mut client.0,
        build_identity: &build_identity,
        board_id,
        chip_id,
        ecid,
        tss_ticket: &ticket,
        components,
        filesystem: filesystem_opt,
        data_ports,
        progress: progress_sender,
        cancel: cancel_flag,
    };

    let result = run_sync_local(async move {
        let drain = async {
            if let Some(delegate) = progress_delegate {
                while let Some(event) = progress_rx.recv().await {
                    delegate.dispatch(&event);
                }
            }
        };
        let (res, ()) = futures::future::join(run_restore(ctx, options), drain).await;
        res
    });

    match result {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

// ---------------------------------------------------------------------------
// IPSW archive
// ---------------------------------------------------------------------------

type IpswReader = tokio::io::BufReader<tokio::fs::File>;

/// Opaque handle to an opened IPSW archive.
pub struct IpswHandle(pub Ipsw<IpswReader>);

/// Opens an IPSW archive from a filesystem path.
///
/// # Safety
/// `path` must be a valid C string; `out_ipsw` a valid, non-null location.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_ipsw_open(
    path: *const c_char,
    out_ipsw: *mut *mut IpswHandle,
) -> *mut IdeviceFfiError {
    if path.is_null() || out_ipsw.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string();
    let res = run_sync_local(async move {
        let file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| IdeviceError::Restore(RestoreError::Ipsw(format!("open {path}: {e}"))))?;
        Ipsw::new(tokio::io::BufReader::new(file)).await
    });
    match res {
        Ok(ipsw) => {
            unsafe { *out_ipsw = Box::into_raw(Box::new(IpswHandle(ipsw))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads and parses the archive's `BuildManifest.plist`.
///
/// # Safety
/// `ipsw` must be a valid handle; `out_manifest` a valid, non-null location. The
/// returned plist must be freed with `plist_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_ipsw_build_manifest(
    ipsw: *mut IpswHandle,
    out_manifest: *mut crate::plist_t,
) -> *mut IdeviceFfiError {
    if ipsw.is_null() || out_manifest.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let ipsw = unsafe { &mut *ipsw };
    match run_sync_local(async { ipsw.0.build_manifest().await }) {
        Ok(m) => {
            unsafe { *out_manifest = dict_to_plist(m) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads a component named in `build_identity` into a caller-freed buffer.
///
/// # Safety
/// `ipsw`, `name`, `out_data`, `out_len` must be valid. Free the buffer with
/// `idevice_data_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_ipsw_read_component(
    ipsw: *mut IpswHandle,
    build_identity: crate::plist_t,
    name: *const c_char,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> *mut IdeviceFfiError {
    if ipsw.is_null() || name.is_null() || out_data.is_null() || out_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(bi) = plist_to_dict(build_identity) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_identity is not a plist dictionary".into(),
        )));
    };
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .to_string();
    let ipsw_addr = ipsw as usize;
    let res = crate::run_sync(async move {
        let ipsw = unsafe { &mut *(ipsw_addr as *mut IpswHandle) };
        ipsw.0.read_component(&bi, &name).await
    });
    match res {
        Ok(bytes) => {
            bytes_out(bytes, out_data, out_len);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads an arbitrary archive entry by exact path into a caller-freed buffer.
///
/// # Safety
/// `ipsw`, `path`, `out_data`, `out_len` must be valid. Free with
/// `idevice_data_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_ipsw_read_file(
    ipsw: *mut IpswHandle,
    path: *const c_char,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> *mut IdeviceFfiError {
    if ipsw.is_null() || path.is_null() || out_data.is_null() || out_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string();
    let ipsw_addr = ipsw as usize;
    let res = crate::run_sync(async move {
        let ipsw = unsafe { &mut *(ipsw_addr as *mut IpswHandle) };
        ipsw.0.read_file(&path).await
    });
    match res {
        Ok(bytes) => {
            bytes_out(bytes, out_data, out_len);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Extracts an archive entry to a file on disk (streamed, for large images).
///
/// # Safety
/// `ipsw`, `entry_path`, `dest_path` must be valid C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_ipsw_extract_to_file(
    ipsw: *mut IpswHandle,
    entry_path: *const c_char,
    dest_path: *const c_char,
) -> *mut IdeviceFfiError {
    if ipsw.is_null() || entry_path.is_null() || dest_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let entry = unsafe { CStr::from_ptr(entry_path) }
        .to_string_lossy()
        .to_string();
    let dest = unsafe { CStr::from_ptr(dest_path) }
        .to_string_lossy()
        .to_string();
    let ipsw = unsafe { &mut *ipsw };
    match run_sync_local(async {
        let mut out = tokio::fs::File::create(&dest).await.map_err(|e| {
            IdeviceError::Restore(RestoreError::Ipsw(format!("failed to create {dest}: {e}")))
        })?;
        ipsw.0.extract_to_writer(&entry, &mut out).await
    }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees an [`IpswHandle`].
///
/// # Safety
/// `ipsw` must be a handle allocated by this library, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_ipsw_free(ipsw: *mut IpswHandle) {
    if !ipsw.is_null() {
        let _ = unsafe { Box::from_raw(ipsw) };
    }
}

// ---------------------------------------------------------------------------
// Build identity selection + component path
// ---------------------------------------------------------------------------

/// Selects the `BuildIdentity` matching `board_id`/`chip_id` (and, when non-NULL,
/// `restore_behavior`, e.g. "Erase"/"Update") from a `BuildManifest` plist.
///
/// # Safety
/// `build_manifest`, `out_identity` must be valid. The result plist must be freed
/// with `plist_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_select_build_identity(
    build_manifest: crate::plist_t,
    board_id: u64,
    chip_id: u64,
    restore_behavior: *const c_char,
    out_identity: *mut crate::plist_t,
) -> *mut IdeviceFfiError {
    if out_identity.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(manifest) = plist_to_dict(build_manifest) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_manifest is not a plist dictionary".into(),
        )));
    };
    let behavior = cstr_opt(restore_behavior);
    match select_build_identity(&manifest, board_id, chip_id, behavior.as_deref()) {
        Ok(bi) => {
            unsafe { *out_identity = dict_to_plist(bi.clone()) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Resolves the archive path of a component from a build identity's `Manifest`.
///
/// # Safety
/// `build_identity`, `name`, `out_path` must be valid. Free the string with
/// `idevice_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_component_path(
    build_identity: crate::plist_t,
    name: *const c_char,
    out_path: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if name.is_null() || out_path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(bi) = plist_to_dict(build_identity) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_identity is not a plist dictionary".into(),
        )));
    };
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .to_string();
    match ipsw::component_path(&bi, &name) {
        Ok(p) => {
            unsafe { *out_path = CString::new(p).unwrap_or_default().into_raw() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

// ---------------------------------------------------------------------------
// TSS: AP ticket fetch
// ---------------------------------------------------------------------------

/// Fetches the AP `ApImg4Ticket` (IM4M) from Apple's TSS server for a build
/// identity and device, returning the ticket bytes.
///
/// `ap_nonce`/`sep_nonce` may be NULL (length 0); a NULL `sep_nonce` is signed
/// with a zeroed nonce.
///
/// # Safety
/// `build_identity`, `out_ticket`, `out_ticket_len` must be valid. Free the ticket
/// with `idevice_data_free`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn idevice_restore_fetch_ap_ticket(
    build_identity: crate::plist_t,
    board_id: u64,
    chip_id: u64,
    ecid: u64,
    ap_nonce: *const u8,
    ap_nonce_len: usize,
    sep_nonce: *const u8,
    sep_nonce_len: usize,
    out_ticket: *mut *mut u8,
    out_ticket_len: *mut usize,
) -> *mut IdeviceFfiError {
    if out_ticket.is_null() || out_ticket_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(bi) = plist_to_dict(build_identity) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_identity is not a plist dictionary".into(),
        )));
    };
    let ap_nonce = unsafe { opt_slice(ap_nonce, ap_nonce_len) };
    let sep_nonce = unsafe { opt_slice(sep_nonce, sep_nonce_len) };

    let res = run_sync_local(async move {
        let mut parameters = plist::Dictionary::new();
        parameters.insert("ApProductionMode".into(), true.into());
        parameters.insert("ApSecurityMode".into(), true.into());
        parameters.insert("ApSupportsImg4".into(), true.into());

        let mut request = TSSRequest::new();
        request.set_ap_img4_ticket(true);
        request.add_common_tags(
            board_id,
            chip_id,
            ecid,
            ap_nonce,
            sep_nonce.or(Some(vec![0u8; 20])),
        );
        request.add_ap_tags(&bi);
        request.add_ap_manifest_tags(&bi, &parameters)?;
        let response = match request.send().await? {
            plist::Value::Dictionary(d) => d,
            _ => {
                return Err(IdeviceError::Restore(RestoreError::TssResponse(
                    "response is not a dictionary".into(),
                )));
            }
        };
        extract_img4_ticket(&response)
    });

    match res {
        Ok(ticket) => {
            bytes_out(ticket, out_ticket, out_ticket_len);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

// ---------------------------------------------------------------------------
// IMG4 personalization
// ---------------------------------------------------------------------------

/// Stitches an `IM4P` component with the `ApImg4Ticket` into a personalized
/// `IMG4` the device will accept.
///
/// `fourcc` is either NULL (keep the payload's own type) or a pointer to exactly
/// four bytes to re-tag the payload with (required for some `Restore*` components;
/// see the library's `restore_fourcc_override`).
///
/// # Safety
/// `im4p`, `ticket`, `out_data`, `out_len` must be valid. If non-NULL, `fourcc`
/// must point to 4 readable bytes. Free the buffer with `idevice_data_free`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn idevice_img4_stitch_component(
    im4p: *const u8,
    im4p_len: usize,
    ticket: *const u8,
    ticket_len: usize,
    fourcc: *const u8,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> *mut IdeviceFfiError {
    if im4p.is_null() || ticket.is_null() || out_data.is_null() || out_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let im4p = unsafe { std::slice::from_raw_parts(im4p, im4p_len) };
    let ticket = unsafe { std::slice::from_raw_parts(ticket, ticket_len) };
    let fourcc = if fourcc.is_null() {
        None
    } else {
        let b = unsafe { std::slice::from_raw_parts(fourcc, 4) };
        Some([b[0], b[1], b[2], b[3]])
    };
    match idevice::restore::img4::stitch_component(im4p, ticket, fourcc, &[]) {
        Ok(bytes) => {
            bytes_out(bytes, out_data, out_len);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns the four-character code a `Restore*` component must be re-tagged with,
/// if any, writing four bytes to `out_fourcc`.
///
/// Returns `true` and fills `out_fourcc` when the component needs re-tagging;
/// returns `false` and leaves `out_fourcc` untouched otherwise.
///
/// # Safety
/// `component_name` must be a valid C string; `out_fourcc` must point to 4
/// writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_img4_restore_fourcc_override(
    component_name: *const c_char,
    out_fourcc: *mut u8,
) -> bool {
    if component_name.is_null() || out_fourcc.is_null() {
        return false;
    }
    let name = unsafe { CStr::from_ptr(component_name) }
        .to_string_lossy()
        .to_string();
    match idevice::restore::img4::restore_fourcc_override(&name) {
        Some(f) => {
            unsafe { std::ptr::copy_nonoverlapping(f.as_ptr(), out_fourcc, 4) };
            true
        }
        None => false,
    }
}

/// Returns the components iBoot loads during the restore boot, in manifest order,
/// as a newline-separated, NUL-terminated string (empty when none).
///
/// # Safety
/// `build_identity`, `out_names` must be valid. Free the string with
/// `idevice_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_boot_component_names(
    build_identity: crate::plist_t,
    out_names: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if out_names.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(bi) = plist_to_dict(build_identity) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_identity is not a plist dictionary".into(),
        )));
    };
    let joined = ipsw::components_loaded_by_iboot(&bi).join("\n");
    unsafe { *out_names = CString::new(joined).unwrap_or_default().into_raw() };
    null_mut()
}

// ---------------------------------------------------------------------------
// Preboard stashbag (data-preserving updates)
// ---------------------------------------------------------------------------

/// Builds the local (unsigned) `IM4M` preboard manifest for a stashbag request
/// from a build identity, into a caller-freed buffer.
///
/// # Safety
/// `build_identity`, `out_data`, `out_len` must be valid. Free the buffer with
/// `idevice_data_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_restore_build_preboard_manifest(
    build_identity: crate::plist_t,
    board_id: u64,
    chip_id: u64,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> *mut IdeviceFfiError {
    if out_data.is_null() || out_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let Some(bi) = plist_to_dict(build_identity) else {
        return ffi_err!(IdeviceError::Restore(RestoreError::Other(
            "build_identity is not a plist dictionary".into(),
        )));
    };
    match build_preboard_manifest(&bi, board_id, chip_id) {
        Ok(bytes) => {
            bytes_out(bytes, out_data, out_len);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

// ===========================================================================
// Recovery / DFU transport (feature `restore_recovery`)
// ===========================================================================

#[cfg(feature = "restore_recovery")]
mod recovery_ffi {
    use super::*;
    use std::sync::Arc;

    use idevice::restore::fdr::{FDR_CTRL_PORT, FdrClient, FdrConnector, run_fdr_listener};
    use idevice::restore::recovery::{
        ControlSetup, DeviceInfo, Mode, RecoveryDevice, RecoveryFuture, RecoveryTransport,
    };

    /// C delegate implementing the raw USB surface of a recovery/DFU device.
    ///
    /// The library implements the iBoot/DFU protocol on top of these calls, so the
    /// caller only supplies USB I/O (via nusb, libusb, etc) against the Apple device
    /// already opened in a recovery/DFU mode.
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct IdeviceRestoreRecoveryTransportFFI {
        pub context: *mut c_void,
        /// Host to device control transfer; writes the byte count to `out_transferred`.
        pub control_out: extern "C" fn(
            request_type: u8,
            request: u8,
            value: u16,
            index: u16,
            data: *const u8,
            data_len: usize,
            timeout_ms: u32,
            out_transferred: *mut usize,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
        /// Device to host control transfer into a system-allocated buffer (ownership
        /// transfers to the library).
        pub control_in: extern "C" fn(
            request_type: u8,
            request: u8,
            value: u16,
            index: u16,
            length: u16,
            timeout_ms: u32,
            out_data: *mut *mut u8,
            out_len: *mut usize,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
        /// Bulk OUT transfer; writes the byte count to `out_transferred`.
        pub bulk_out: extern "C" fn(
            endpoint: u8,
            data: *const u8,
            data_len: usize,
            timeout_ms: u32,
            out_transferred: *mut usize,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
        /// Writes the NUL-terminated USB serial-number string into `buf`
        /// (capacity `buf_len`).
        pub serial_number: extern "C" fn(
            buf: *mut c_char,
            buf_len: usize,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
        /// Returns the device descriptor's `idProduct`.
        pub product_id: extern "C" fn(context: *mut c_void) -> u16,
        /// Selects a configuration.
        pub set_configuration:
            extern "C" fn(configuration: u8, context: *mut c_void) -> *mut IdeviceFfiError,
        /// Claims an interface / alternate setting.
        pub claim_interface: extern "C" fn(
            interface: u8,
            alt_setting: u8,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
        /// Resets the device (it re-enumerates afterwards).
        pub reset: extern "C" fn(context: *mut c_void) -> *mut IdeviceFfiError,
    }

    unsafe impl Send for IdeviceRestoreRecoveryTransportFFI {}
    unsafe impl Sync for IdeviceRestoreRecoveryTransportFFI {}

    impl RecoveryTransport for IdeviceRestoreRecoveryTransportFFI {
        fn control_out<'a>(
            &'a mut self,
            setup: ControlSetup,
            data: &'a [u8],
            timeout_ms: u32,
        ) -> RecoveryFuture<'a, usize> {
            let cb = self.control_out;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let mut transferred = 0usize;
                let err = cb(
                    setup.request_type,
                    setup.request,
                    setup.value,
                    setup.index,
                    data.as_ptr(),
                    data.len(),
                    timeout_ms,
                    &mut transferred,
                    context,
                );
                if err.is_null() {
                    Ok(transferred)
                } else {
                    Err(ffi_err_to_idevice(err))
                }
            })
        }

        fn control_in<'a>(
            &'a mut self,
            setup: ControlSetup,
            length: u16,
            timeout_ms: u32,
        ) -> RecoveryFuture<'a, Vec<u8>> {
            let cb = self.control_in;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let mut data: *mut u8 = null_mut();
                let mut len = 0usize;
                let err = cb(
                    setup.request_type,
                    setup.request,
                    setup.value,
                    setup.index,
                    length,
                    timeout_ms,
                    &mut data,
                    &mut len,
                    context,
                );
                if err.is_null() {
                    Ok(unsafe { take_c_bytes(data, len) })
                } else {
                    Err(ffi_err_to_idevice(err))
                }
            })
        }

        fn bulk_out<'a>(
            &'a mut self,
            endpoint: u8,
            data: &'a [u8],
            timeout_ms: u32,
        ) -> RecoveryFuture<'a, usize> {
            let cb = self.bulk_out;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let mut transferred = 0usize;
                let err = cb(
                    endpoint,
                    data.as_ptr(),
                    data.len(),
                    timeout_ms,
                    &mut transferred,
                    context,
                );
                if err.is_null() {
                    Ok(transferred)
                } else {
                    Err(ffi_err_to_idevice(err))
                }
            })
        }

        fn serial_number(&mut self) -> RecoveryFuture<'_, String> {
            let cb = self.serial_number;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let mut buf = vec![0 as c_char; 512];
                let err = cb(buf.as_mut_ptr(), buf.len(), context);
                if !err.is_null() {
                    return Err(ffi_err_to_idevice(err));
                }
                let s = unsafe { CStr::from_ptr(buf.as_ptr()) }
                    .to_string_lossy()
                    .to_string();
                Ok(s)
            })
        }

        fn product_id(&self) -> u16 {
            (self.product_id)(self.context)
        }

        fn set_configuration(&mut self, configuration: u8) -> RecoveryFuture<'_, ()> {
            let cb = self.set_configuration;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let err = cb(configuration, context);
                if err.is_null() {
                    Ok(())
                } else {
                    Err(ffi_err_to_idevice(err))
                }
            })
        }

        fn claim_interface(&mut self, interface: u8, alt_setting: u8) -> RecoveryFuture<'_, ()> {
            let cb = self.claim_interface;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let err = cb(interface, alt_setting, context);
                if err.is_null() {
                    Ok(())
                } else {
                    Err(ffi_err_to_idevice(err))
                }
            })
        }

        fn reset(&mut self) -> RecoveryFuture<'_, ()> {
            let cb = self.reset;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let err = cb(context);
                if err.is_null() {
                    Ok(())
                } else {
                    Err(ffi_err_to_idevice(err))
                }
            })
        }
    }

    /// Opaque handle to a device in recovery/DFU mode.
    pub struct RecoveryDeviceHandle(pub RecoveryDevice);

    /// Opens a recovery/DFU device over a caller-supplied transport delegate.
    ///
    /// The `transport` struct is copied by value; the caller may free its own
    /// storage after this returns (the `context` pointer must stay valid).
    ///
    /// # Safety
    /// `transport` and `out_device` must be valid, non-null pointers.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_device_new(
        transport: *const IdeviceRestoreRecoveryTransportFFI,
        out_device: *mut *mut RecoveryDeviceHandle,
    ) -> *mut IdeviceFfiError {
        if transport.is_null() || out_device.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let transport = unsafe { *transport };
        let boxed: Box<dyn RecoveryTransport> = Box::new(transport);
        match run_sync_local(async move { RecoveryDevice::new(boxed).await }) {
            Ok(dev) => {
                unsafe { *out_device = Box::into_raw(Box::new(RecoveryDeviceHandle(dev))) };
                null_mut()
            }
            Err(e) => ffi_err!(e),
        }
    }

    /// Sends an iBoot command (NUL-terminated), with an explicit `bRequest`.
    ///
    /// # Safety
    /// `device`, `command` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_send_command(
        device: *mut RecoveryDeviceHandle,
        command: *const c_char,
        b_request: u8,
    ) -> *mut IdeviceFfiError {
        if device.is_null() || command.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        let cmd = unsafe { CStr::from_ptr(command) }
            .to_string_lossy()
            .to_string();
        match run_sync_local(async { device.0.send_command_with_request(&cmd, b_request).await }) {
            Ok(()) => null_mut(),
            Err(e) => ffi_err!(e),
        }
    }

    /// Uploads a firmware image (bulk in recovery mode, chunked control transfers
    /// in DFU mode).
    ///
    /// # Safety
    /// `device`, `data` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_send_buffer(
        device: *mut RecoveryDeviceHandle,
        data: *const u8,
        len: usize,
    ) -> *mut IdeviceFfiError {
        if device.is_null() || (data.is_null() && len != 0) {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        let buf = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
        match run_sync_local(async { device.0.send_buffer(&buf).await }) {
            Ok(()) => null_mut(),
            Err(e) => ffi_err!(e),
        }
    }

    /// Reads an environment variable via `getenv` into a caller-freed buffer.
    ///
    /// # Safety
    /// `device`, `name`, `out_data`, `out_len` must be valid. Free with
    /// `idevice_data_free`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_getenv(
        device: *mut RecoveryDeviceHandle,
        name: *const c_char,
        out_data: *mut *mut u8,
        out_len: *mut usize,
    ) -> *mut IdeviceFfiError {
        if device.is_null() || name.is_null() || out_data.is_null() || out_len.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        let name = unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .to_string();
        match run_sync_local(async { device.0.getenv(&name).await }) {
            Ok(bytes) => {
                bytes_out(bytes, out_data, out_len);
                null_mut()
            }
            Err(e) => ffi_err!(e),
        }
    }

    /// Sets an environment variable via `setenv`.
    ///
    /// # Safety
    /// `device`, `name`, `value` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_setenv(
        device: *mut RecoveryDeviceHandle,
        name: *const c_char,
        value: *const c_char,
    ) -> *mut IdeviceFfiError {
        if device.is_null() || name.is_null() || value.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        let name = unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .to_string();
        let value = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .to_string();
        match run_sync_local(async { device.0.setenv(&name, &value).await }) {
            Ok(()) => null_mut(),
            Err(e) => ffi_err!(e),
        }
    }

    /// Enables or disables auto-boot and persists it (`saveenv`).
    ///
    /// # Safety
    /// `device` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_set_autoboot(
        device: *mut RecoveryDeviceHandle,
        enable: bool,
    ) -> *mut IdeviceFfiError {
        if device.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        match run_sync_local(async { device.0.set_autoboot(enable).await }) {
            Ok(()) => null_mut(),
            Err(e) => ffi_err!(e),
        }
    }

    /// Issues the zero-length `finish_transfer` control request.
    ///
    /// # Safety
    /// `device` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_finish_transfer(
        device: *mut RecoveryDeviceHandle,
    ) -> *mut IdeviceFfiError {
        if device.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        match run_sync_local(async { device.0.finish_transfer().await }) {
            Ok(()) => null_mut(),
            Err(e) => ffi_err!(e),
        }
    }

    /// Reboots the device.
    ///
    /// # Safety
    /// `device` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_reboot(
        device: *mut RecoveryDeviceHandle,
    ) -> *mut IdeviceFfiError {
        if device.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &mut *device };
        match run_sync_local(async { device.0.reboot().await }) {
            Ok(()) => null_mut(),
            Err(e) => ffi_err!(e),
        }
    }

    /// Returns the device's USB `idProduct` (identifying its mode), and whether it
    /// is a recovery (iBoot) mode as opposed to DFU/WTF.
    ///
    /// # Safety
    /// `device`, `out_product_id`, `out_is_recovery` must be valid.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_get_mode(
        device: *mut RecoveryDeviceHandle,
        out_product_id: *mut u16,
        out_is_recovery: *mut bool,
    ) -> *mut IdeviceFfiError {
        if device.is_null() || out_product_id.is_null() || out_is_recovery.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &*device };
        let mode: Mode = device.0.mode();
        unsafe {
            *out_product_id = mode.product_id();
            *out_is_recovery = mode.is_recovery();
        }
        null_mut()
    }

    /// Fills device identifiers parsed from the recovery serial string.
    ///
    /// Each `has_*` output is set to whether the corresponding value was present;
    /// missing values leave their `out_*` untouched.
    ///
    /// # Safety
    /// All non-null out-pointers must be valid.
    #[unsafe(no_mangle)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe extern "C" fn idevice_recovery_get_info(
        device: *mut RecoveryDeviceHandle,
        out_cpid: *mut u64,
        out_has_cpid: *mut bool,
        out_bdid: *mut u64,
        out_has_bdid: *mut bool,
        out_ecid: *mut u64,
        out_has_ecid: *mut bool,
    ) -> *mut IdeviceFfiError {
        if device.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let device = unsafe { &*device };
        let info: &DeviceInfo = device.0.info();
        unsafe fn put(v: Option<u64>, out: *mut u64, has: *mut bool) {
            if let Some(v) = v {
                if !out.is_null() {
                    unsafe { *out = v };
                }
                if !has.is_null() {
                    unsafe { *has = true };
                }
            } else if !has.is_null() {
                unsafe { *has = false };
            }
        }
        unsafe {
            put(info.cpid, out_cpid, out_has_cpid);
            put(info.bdid, out_bdid, out_has_bdid);
            put(info.ecid, out_ecid, out_has_ecid);
        }
        null_mut()
    }

    /// Returns the AP nonce (`NONC`) from the recovery serial, if present, into a
    /// caller-freed buffer. Returns `true` when a nonce was present.
    ///
    /// # Safety
    /// `device`, `out_data`, `out_len` must be valid. Free with `idevice_data_free`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_get_ap_nonce(
        device: *mut RecoveryDeviceHandle,
        out_data: *mut *mut u8,
        out_len: *mut usize,
    ) -> bool {
        if device.is_null() || out_data.is_null() || out_len.is_null() {
            return false;
        }
        let device = unsafe { &*device };
        match &device.0.info().ap_nonce {
            Some(n) => {
                bytes_out(n.clone(), out_data, out_len);
                true
            }
            None => false,
        }
    }

    /// Frees a [`RecoveryDeviceHandle`].
    ///
    /// # Safety
    /// `device` must be a handle allocated by this library, or NULL.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_recovery_device_free(device: *mut RecoveryDeviceHandle) {
        if !device.is_null() {
            let _ = unsafe { Box::from_raw(device) };
        }
    }

    // ---- FDR trust channel ----

    /// C delegate opening FDR trust-channel connections to device ports.
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct IdeviceRestoreFdrConnectorFFI {
        pub context: *mut c_void,
        /// Connects to `port`, yielding a new [`IdeviceHandle`] (ownership
        /// transfers to the library).
        pub connect_device_port: extern "C" fn(
            port: u16,
            out_idevice: *mut *mut IdeviceHandle,
            context: *mut c_void,
        ) -> *mut IdeviceFfiError,
    }

    unsafe impl Send for IdeviceRestoreFdrConnectorFFI {}
    unsafe impl Sync for IdeviceRestoreFdrConnectorFFI {}

    impl FdrConnector for IdeviceRestoreFdrConnectorFFI {
        fn connect_device_port(
            &self,
            port: u16,
        ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
            let cb = self.connect_device_port;
            let context = self.context as usize;
            Box::pin(async move {
                let context = context as *mut c_void;
                let mut handle: *mut IdeviceHandle = null_mut();
                let err = cb(port, &mut handle, context);
                if !err.is_null() {
                    return Err(ffi_err_to_idevice(err));
                }
                if handle.is_null() {
                    return Err(IdeviceError::Restore(RestoreError::DataPortConnect(port)));
                }
                Ok(unsafe { Box::from_raw(handle) }.0)
            })
        }
    }

    /// Starts the FDR trust channel: control handshake, then a background listener
    /// running for the rest of the restore.
    ///
    /// The `connector` struct is copied by value (its `context` must stay valid for
    /// the duration of the restore).
    ///
    /// # Safety
    /// `connector` must be a valid, non-null pointer.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn idevice_restore_fdr_start(
        connector: *const IdeviceRestoreFdrConnectorFFI,
    ) -> *mut IdeviceFfiError {
        use std::time::Duration;

        if connector.is_null() {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
        let connector: Arc<dyn FdrConnector> = Arc::new(unsafe { *connector });

        let res = run_sync_local({
            let connector = connector.clone();
            async move {
                let mut last: Option<IdeviceError> = None;
                for _ in 0..3 {
                    let attempt = tokio::time::timeout(Duration::from_secs(3), async {
                        let ctrl = connector.connect_device_port(FDR_CTRL_PORT).await?;
                        let mut fdr = FdrClient::new(ctrl);
                        let conn_port = fdr.ctrl_handshake().await?;
                        Ok::<_, IdeviceError>((fdr, conn_port))
                    })
                    .await;
                    match attempt {
                        Ok(Ok(v)) => return Ok(v),
                        Ok(Err(e)) => last = Some(e),
                        Err(_) => {
                            last = Some(IdeviceError::Restore(RestoreError::Other(
                                "FDR ctrl handshake timed out".into(),
                            )))
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(last.unwrap_or_else(|| {
                    IdeviceError::Restore(RestoreError::Other("FDR failed to start".into()))
                }))
            }
        });

        match res {
            Ok((fdr, conn_port)) => {
                // Run the listener on the LOCAL runtime too, so its own
                // connect-device-port callbacks (GLOBAL) stay cross-runtime.
                crate::LOCAL_RUNTIME
                    .handle()
                    .spawn(run_fdr_listener(fdr, connector, conn_port));
                null_mut()
            }
            Err(e) => ffi_err!(e),
        }
    }
}

#[cfg(feature = "restore_recovery")]
pub use recovery_ffi::*;
