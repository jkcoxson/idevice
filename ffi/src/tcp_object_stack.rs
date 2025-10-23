//! Just to make things more complicated, some setups need an IP input from FFI. Or maybe a packet
//! input that is sync only. This is a stupid simple shim between callbacks and an input for the
//! legendary idevice TCP stack.

use std::{
    ffi::{CStr, c_char, c_void},
    ptr::null_mut,
    sync::Arc,
};

use tokio::sync::Mutex;
use tokio::{
    io::AsyncWriteExt,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::{IdeviceFfiError, core_device_proxy::AdapterHandle, ffi_err, run_sync, run_sync_local};

pub struct TcpFeedObject {
    sender: Arc<Mutex<OwnedWriteHalf>>,
}
pub struct TcpEatObject {
    receiver: Arc<Mutex<OwnedReadHalf>>,
}

#[repr(transparent)]
#[derive(Clone)]
pub struct UserContext(*mut c_void);
unsafe impl Send for UserContext {}
unsafe impl Sync for UserContext {}

/// # Safety
/// Pass valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_tcp_stack_into_sync_objects(
    our_ip: *const c_char,
    their_ip: *const c_char,
    feeder: *mut *mut TcpFeedObject, // feed the TCP stack with IP packets
    tcp_receiver: *mut *mut TcpEatObject,
    adapter_handle: *mut *mut AdapterHandle, // this object can be used throughout the rest of the
                                             // idevice ecosystem
) -> *mut IdeviceFfiError {
    if our_ip.is_null() || their_ip.is_null() || feeder.is_null() || adapter_handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let our_ip = unsafe { CStr::from_ptr(our_ip) }
        .to_string_lossy()
        .to_string();
    let our_ip = match our_ip.parse::<std::net::IpAddr>() {
        Ok(o) => o,
        Err(_) => {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
    };
    let their_ip = unsafe { CStr::from_ptr(their_ip) }
        .to_string_lossy()
        .to_string();
    let their_ip = match their_ip.parse::<std::net::IpAddr>() {
        Ok(o) => o,
        Err(_) => {
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
    };

    let res = run_sync(async {
        let mut port = 4000;
        loop {
            if port > 4050 {
                return None;
            }
            let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await {
                Ok(l) => l,
                Err(_) => {
                    port += 1;
                    continue;
                }
            };

            let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
                .await
                .ok()?;
            stream.set_nodelay(true).ok()?;
            let (stream2, _) = listener.accept().await.ok()?;
            stream2.set_nodelay(true).ok()?;
            break Some((stream, stream2));
        }
    });

    let (stream, stream2) = match res {
        Some(x) => x,
        None => {
            return ffi_err!(IdeviceError::NoEstablishedConnection);
        }
    };

    let (r, w) = stream2.into_split();
    let w = Arc::new(Mutex::new(w));
    let r = Arc::new(Mutex::new(r));

    // let w = Arc::new(Mutex::new(stream2));
    // let r = w.clone();

    let feed_object = TcpFeedObject { sender: w };
    let eat_object = TcpEatObject { receiver: r };

    // we must be inside the runtime for the inner function to spawn threads
    let new_adapter = run_sync_local(async {
        idevice::tcp::adapter::Adapter::new(Box::new(stream), our_ip, their_ip).to_async_handle()
    });
    // this object can now be used with the rest of the idevice FFI library

    unsafe {
        *feeder = Box::into_raw(Box::new(feed_object));
        *tcp_receiver = Box::into_raw(Box::new(eat_object));
        *adapter_handle = Box::into_raw(Box::new(AdapterHandle(new_adapter)));
    }

    null_mut()
}

/// Feed the TCP stack with data
/// # Safety
/// Pass valid pointers. Data is cloned out of slice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_tcp_feed_object_write(
    object: *mut TcpFeedObject,
    data: *const u8,
    len: usize,
) -> *mut IdeviceFfiError {
    if object.is_null() || data.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let object = unsafe { &mut *object };
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    run_sync_local(async move {
        let mut lock = object.sender.lock().await;
        match lock.write_all(data).await {
            Ok(_) => {
                lock.flush().await.ok();
                null_mut()
            }
            Err(e) => {
                ffi_err!(IdeviceError::Socket(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    format!("could not send: {e:?}")
                )))
            }
        }
    })
}

/// Block on getting a block of data to write to the underlying stream.
/// Write this to the stream as is, and free the data with idevice_data_free
///
/// # Safety
/// Pass valid pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_tcp_eat_object_read(
    object: *mut TcpEatObject,
    data: *mut *mut u8,
    len: *mut usize,
) -> *mut IdeviceFfiError {
    let object = unsafe { &mut *object };
    let mut buf = [0; 2048];
    run_sync_local(async {
        let lock = object.receiver.lock().await;
        match lock.try_read(&mut buf) {
            Ok(size) => {
                let bytes = buf[..size].to_vec();
                let mut res = bytes.into_boxed_slice();
                unsafe {
                    *len = res.len();
                    *data = res.as_mut_ptr();
                }
                std::mem::forget(res);
                std::ptr::null_mut()
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock => {
                    unsafe {
                        *len = 0;
                    }
                    std::ptr::null_mut()
                }
                _ => {
                    ffi_err!(IdeviceError::Socket(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "channel closed"
                    )))
                }
            },
        }
    })
}

/// # Safety
/// Pass a valid pointer allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_free_tcp_feed_object(object: *mut TcpFeedObject) {
    if object.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(object) };
}

/// # Safety
/// Pass a valid pointer allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_free_tcp_eat_object(object: *mut TcpEatObject) {
    if object.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(object) };
}
