// Jackson Coxson

use std::{io::SeekFrom, pin::Pin};

use futures::{FutureExt, future::BoxFuture};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};

use crate::IdeviceError;

use super::{
    opcode::AfcOpcode,
    packet::{AfcPacket, AfcPacketHeader},
};

/// Maximum transfer size for file operations (64KB)
const MAX_TRANSFER: u64 = 64 * 1024; // this is what go-ios uses

fn chunk_number(n: usize, chunk_size: usize) -> impl Iterator<Item = usize> {
    (0..n)
        .step_by(chunk_size)
        .map(move |i| (n - i).min(chunk_size))
}

// Used to descripe what the future returns
#[derive(Debug)]
pub(crate) enum PendingResult {
    // writing
    Empty,
    // seeking
    SeekPos(u64),
    // reading
    Bytes(Vec<u8>),
}

/// Handle for an open file on the device.
/// Call close before dropping
pub(crate) struct InnerFileDescriptor<'a> {
    pub(crate) client: &'a mut super::AfcClient,
    pub(crate) fd: u64,
    pub(crate) path: String,

    pub(crate) pending_fut: Option<BoxFuture<'a, Result<PendingResult, IdeviceError>>>,

    pub(crate) _m: std::marker::PhantomPinned,
}

impl<'a> InnerFileDescriptor<'a> {
    pub(crate) fn new(client: &'a mut super::AfcClient, fd: u64, path: String) -> Pin<Box<Self>> {
        Box::pin(Self {
            client,
            fd,
            path,
            pending_fut: None,
            _m: std::marker::PhantomPinned,
        })
    }
}
impl InnerFileDescriptor<'_> {
    /// Generic helper to send an AFC packet and read the response
    pub async fn send_packet(
        self: Pin<&mut Self>,
        opcode: AfcOpcode,
        header_payload: Vec<u8>,
        payload: Vec<u8>,
    ) -> Result<AfcPacket, IdeviceError> {
        // SAFETY: we don't modify pinned fileds, it's ok
        let this = unsafe { self.get_unchecked_mut() };

        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;
        let header = AfcPacketHeader {
            magic: super::MAGIC,
            entire_len: header_len + payload.len() as u64,
            header_payload_len: header_len,
            packet_num: this.client.package_number,
            operation: opcode,
        };
        this.client.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload,
        };

        this.client.send(packet).await?;
        this.client.read().await
    }

    /// Returns the current cursor position for the file
    pub async fn seek_tell(self: Pin<&mut Self>) -> Result<u64, IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();
        let res = self
            .send_packet(AfcOpcode::FileTell, header_payload, Vec::new())
            .await?;

        let cur_pos = res
            .header_payload
            .get(..8)
            .ok_or(IdeviceError::UnexpectedResponse)?
            .try_into()
            .map(u64::from_le_bytes)
            .map_err(|_| IdeviceError::UnexpectedResponse)?;

        Ok(cur_pos)
    }

    /// Moves the file cursor
    async fn seek(mut self: Pin<&mut Self>, pos: SeekFrom) -> Result<u64, IdeviceError> {
        let (offset, whence) = match pos {
            SeekFrom::Start(off) => (off as i64, 0),
            SeekFrom::Current(off) => (off, 1),
            SeekFrom::End(off) => (off, 2),
        };

        let header_payload = [
            self.fd.to_le_bytes(),
            (whence as u64).to_le_bytes(),
            offset.to_le_bytes(),
        ]
        .concat();

        self.as_mut()
            .send_packet(AfcOpcode::FileSeek, header_payload, Vec::new())
            .await?;

        self.as_mut().seek_tell().await
    }

    /// Closes the file descriptor
    pub async fn close(mut self: Pin<Box<Self>>) -> Result<(), IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();

        self.as_mut()
            .send_packet(AfcOpcode::FileClose, header_payload, Vec::new())
            .await?;
        Ok(())
    }

    /// Reads n size of contents from the file
    ///
    /// # Arguments
    /// * `n` - amount of bytes to read
    /// # Returns
    /// A vector containing the file's data
    pub async fn read_n(mut self: Pin<&mut Self>, n: usize) -> Result<Vec<u8>, IdeviceError> {
        let mut collected_bytes = Vec::with_capacity(n);

        for chunk in chunk_number(n, MAX_TRANSFER as usize) {
            let header_payload = [self.fd.to_le_bytes(), chunk.to_le_bytes()].concat();
            let res = self
                .as_mut()
                .send_packet(AfcOpcode::Read, header_payload, Vec::new())
                .await?;

            collected_bytes.extend(res.payload);
        }
        Ok(collected_bytes)
    }

    /// Reads the entire contents of the file
    ///
    /// # Returns
    /// A vector containing the file's data
    pub async fn read(mut self: Pin<&mut Self>) -> Result<Vec<u8>, IdeviceError> {
        let seek_pos = self.as_mut().seek_tell().await? as usize;

        let file_info = unsafe {
            let this = self.as_mut().get_unchecked_mut();

            this.client.get_file_info(&this.path).await?
        };

        let mut bytes_left = file_info.size.saturating_sub(seek_pos);
        let mut collected_bytes = Vec::with_capacity(bytes_left);

        while bytes_left > 0 {
            let bytes = self.as_mut().read_n(MAX_TRANSFER as usize).await?;

            bytes_left -= bytes.len();
            collected_bytes.extend(bytes);
        }

        Ok(collected_bytes)
    }

    /// Writes data to the file
    ///
    /// # Arguments
    /// * `bytes` - Data to write to the file
    pub async fn write(mut self: Pin<&mut Self>, bytes: &[u8]) -> Result<(), IdeviceError> {
        for chunk in bytes.chunks(MAX_TRANSFER as usize) {
            let header_payload = self.as_ref().fd.to_le_bytes().to_vec();
            self.as_mut()
                .send_packet(AfcOpcode::Write, header_payload, chunk.to_vec())
                .await?;
        }
        Ok(())
    }

    fn store_pending_read(mut self: Pin<&mut Self>, buf_rem: usize) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut() as *mut InnerFileDescriptor;

            let fut = Some(
                // SAFETY: we already know that self is pinned
                Pin::new_unchecked(&mut *this)
                    .read_n(buf_rem)
                    .map(|r| r.map(PendingResult::Bytes))
                    .boxed(),
            );

            (&mut *this).pending_fut = fut;
        }
    }

    fn store_pending_seek(mut self: Pin<&mut Self>, position: std::io::SeekFrom) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut() as *mut InnerFileDescriptor;

            let fut = Some(
                Pin::new_unchecked(&mut *this)
                    .seek(position)
                    .map(|r| r.map(PendingResult::SeekPos))
                    .boxed(),
            );

            (&mut *this).pending_fut = fut;
        }
    }

    fn store_pending_write(mut self: Pin<&mut Self>, buf: &'_ [u8]) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut();

            let this = this as *mut InnerFileDescriptor;

            // move the entire buffer into the future so we don't have to store it somewhere
            let pined_this = Pin::new_unchecked(&mut *this);
            let buf = buf.to_vec();
            let fut =
                async move { pined_this.write(&buf).await.map(|_| PendingResult::Empty) }.boxed();

            (&mut *this).pending_fut = Some(fut);
        }
    }
}

impl<'a> InnerFileDescriptor<'a> {
    fn get_or_init_read_fut(
        mut self: Pin<&mut Self>,
        buf_rem: usize,
    ) -> &mut BoxFuture<'a, Result<PendingResult, IdeviceError>> {
        if self.as_ref().pending_fut.is_none() {
            self.as_mut().store_pending_read(buf_rem);
        }

        unsafe { self.get_unchecked_mut().pending_fut.as_mut().unwrap() }
    }

    fn get_or_init_write_fut(
        mut self: Pin<&mut Self>,
        buf: &'_ [u8],
    ) -> &mut BoxFuture<'a, Result<PendingResult, IdeviceError>> {
        if self.as_ref().pending_fut.is_none() {
            self.as_mut().store_pending_write(buf);
        }

        unsafe { self.get_unchecked_mut().pending_fut.as_mut().unwrap() }
    }

    fn get_seek_fut(
        self: Pin<&mut Self>,
    ) -> Option<&mut BoxFuture<'a, Result<PendingResult, IdeviceError>>> {
        unsafe { self.get_unchecked_mut().pending_fut.as_mut() }
    }

    fn remove_pending_fut(mut self: Pin<&mut Self>) {
        unsafe {
            self.as_mut().get_unchecked_mut().pending_fut.take();
        }
    }
}

impl AsyncRead for InnerFileDescriptor<'_> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let contents = {
            let read_func = self.as_mut().get_or_init_read_fut(buf.remaining());
            match std::task::ready!(read_func.as_mut().poll(cx)) {
                Ok(PendingResult::Bytes(c)) => {
                    self.as_mut().remove_pending_fut();
                    c
                }
                Err(e) => return std::task::Poll::Ready(Err(std::io::Error::other(e.to_string()))),

                _ => unreachable!("a non read future was stored, this shouldn't happen"),
            }
        };

        buf.put_slice(&contents);

        std::task::Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for InnerFileDescriptor<'_> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let write_func = self.as_mut().get_or_init_write_fut(buf);

        match std::task::ready!(write_func.as_mut().poll(cx)) {
            Ok(PendingResult::Empty) => self.as_mut().remove_pending_fut(),
            Err(e) => {
                println!("error: {e}");
                return std::task::Poll::Ready(Err(std::io::Error::other(e.to_string())));
            }

            _ => unreachable!("a non write future was stored, this shouldn't happen"),
        }

        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl AsyncSeek for InnerFileDescriptor<'_> {
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
        self.store_pending_seek(position);

        Ok(())
    }

    fn poll_complete(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        let Some(fut) = self.as_mut().get_seek_fut() else {
            // tokio runs the `poll_complete` before the `start_seek` to ensure no previous seek is in progress
            return std::task::Poll::Ready(Ok(0));
        };

        match std::task::ready!(fut.as_mut().poll(cx)) {
            Ok(PendingResult::SeekPos(pos)) => {
                self.as_mut().remove_pending_fut();
                std::task::Poll::Ready(Ok(pos))
            }
            Err(e) => std::task::Poll::Ready(Err(std::io::Error::other(e.to_string()))),
            _ => unreachable!("a non seek future was stored, this shouldn't happen"),
        }
    }
}

impl std::fmt::Debug for InnerFileDescriptor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InnerFileDescriptor")
            .field("client", &self.client)
            .field("fd", &self.fd)
            .field("path", &self.path)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

    use crate::usbmuxd::{UsbmuxdAddr, UsbmuxdConnection};

    use super::super::*;
    use super::*;

    async fn make_client() -> super::super::AfcClient {
        let mut u = UsbmuxdConnection::default()
            .await
            .expect("failed to connect to usbmuxd");
        let d = u
            .get_devices()
            .await
            .expect("no devices")
            .into_iter()
            .next()
            .expect("no devices connected")
            .to_provider(UsbmuxdAddr::default(), "idevice_afc_file_inner_tests");

        let mut ac = AfcClient::connect(&d)
            .await
            .expect("failed to connect to afc");
        ac.mk_dir("/tmp").await.unwrap();
        ac
    }

    #[tokio::test]
    async fn write_and_read_large_file() {
        let mut client = make_client().await;
        let path = "/tmp/large_file.txt";
        let data = vec![b'x'; 10_000_000]; // 10mb

        {
            let mut file = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
            file.write_all(&data).await.unwrap();
        }

        let mut file = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf.len(), data.len());

        drop(file);
        client.remove(path).await.unwrap();
    }

    #[should_panic]
    #[tokio::test]
    async fn panic_safety() {
        let mut client = make_client().await;
        client.list_dir("/invalid").await.unwrap();
    }

    #[tokio::test]
    async fn file_seek_and_append() {
        let mut client = make_client().await;
        let path = "/tmp/seek_append.txt";

        let mut f = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
        f.write_all(b"start").await.unwrap();
        f.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        f.write_all(b"over").await.unwrap();
        drop(f);

        let mut f = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"overt"); // “over” overwrites start

        drop(f);
        client.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn borrow_check_works() {
        let mut client = make_client().await;
        let fut = client.list_dir("/Downloads");
        // // This line should fail to compile if uncommented:
        // let fut2 = client.list_dir("/bar");
        fut.await.unwrap();
    }

    #[tokio::test]
    async fn not_send_across_threads() {
        let mut client = make_client().await;
        // // This should fail to compile if uncommented:
        // tokio::spawn(async move { client.list_dir("/").await });
        let _ = client.list_dir("/").await;
    }

    #[tokio::test]
    async fn write_and_read_roundtrip() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut client = make_client().await;

        // Create a test file in /tmp (AFC should allow this)
        let path = "/tmp/afc_test_file.txt";
        let contents = b"hello async afc world";

        {
            let mut file = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
            file.write_all(contents).await.unwrap();
        }

        {
            let mut file = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, contents);
        }

        client.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn write_multiple_chunks() {
        use tokio::io::AsyncWriteExt;

        let mut client = make_client().await;
        let path = "/tmp/afc_chunk_test.txt";
        let mut file = client.open(path, AfcFopenMode::WrOnly).await.unwrap();

        for i in 0..10 {
            let data = format!("chunk{}\n", i);
            file.write_all(data.as_bytes()).await.unwrap();
        }

        drop(file);

        let mut file = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut file, &mut buf)
            .await
            .unwrap();
        let s = String::from_utf8_lossy(&buf);

        for i in 0..10 {
            assert!(s.contains(&format!("chunk{}", i)));
        }
        drop(file);

        client.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn read_partial_and_resume() {
        use tokio::io::AsyncReadExt;

        let mut client = make_client().await;
        let path = "/tmp/afc_partial_read.txt";
        let contents = b"abcdefghijklmnopqrstuvwxyz";

        {
            let mut file = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
            file.write_all(contents).await.unwrap();
        }

        let mut file = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = [0u8; 5];
        let n = file.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"abcde");

        let mut rest = Vec::new();
        file.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, b"fghijklmnopqrstuvwxyz");
        drop(file);

        client.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn zero_length_file() {
        use tokio::io::AsyncReadExt;

        let mut client = make_client().await;
        let path = "/tmp/afc_empty.txt";

        {
            let _ = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
        }

        let mut file = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = Vec::new();
        let n = file.read_to_end(&mut buf).await.unwrap();
        assert_eq!(n, 0);
        drop(file);

        client.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn write_then_append() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut client = make_client().await;
        let path = "/tmp/afc_append.txt";

        {
            let mut file = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
            file.write_all(b"first\n").await.unwrap();
            file.flush().await.unwrap();
        }

        {
            let mut file = client.open(path, AfcFopenMode::Append).await.unwrap();
            file.write_all(b"second\n").await.unwrap();
            file.flush().await.unwrap();
        }

        let mut file = client.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.unwrap();

        assert_eq!(String::from_utf8_lossy(&buf), "first\nsecond\n");
        drop(file);

        client.remove(path).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_file_access_should_not_ub() {
        use std::sync::Arc;
        use tokio::task;

        let client = Arc::new(tokio::sync::Mutex::new(make_client().await));
        let path = "/tmp/afc_threaded.txt";

        let tasks: Vec<_> = (0..10)
            .map(|i| {
                let client = Arc::clone(&client);
                task::spawn(async move {
                    let mut guard = client.lock().await;
                    let mut f = guard.open(path, AfcFopenMode::Append).await.unwrap();
                    f.write_all(format!("{}\n", i).as_bytes()).await.unwrap();
                    f.flush().await.unwrap();
                })
            })
            .collect();

        for t in tasks {
            let _ = t.await;
        }

        let mut guard = client.lock().await;
        let mut f = guard.open(path, AfcFopenMode::RdOnly).await.unwrap();
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut f, &mut buf)
            .await
            .unwrap();
        let s = String::from_utf8_lossy(&buf);
        for i in 0..10 {
            assert!(s.contains(&i.to_string()));
        }
        drop(f);
        guard.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn panic_during_write_does_not_leak() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static COUNT: AtomicUsize = AtomicUsize::new(0);

        let mut client = make_client().await;
        let path = "/tmp/afc_panic.txt";

        let result = std::panic::AssertUnwindSafe(async {
            let _f = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
            COUNT.fetch_add(1, Ordering::SeqCst);
            panic!("simulate crash mid-write");
        })
        .catch_unwind()
        .await;

        assert!(result.is_err());
        // Reopen to ensure no handles leaked
        let _ = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn open_close_stress() {
        let mut client = make_client().await;
        let path = "/tmp/afc_stress.txt";

        for _ in 0..100 {
            let mut f = client.open(path, AfcFopenMode::WrOnly).await.unwrap();
            f.write_all(b"hi").await.unwrap();
            drop(f);
        }

        // Make sure handle cleanup didn’t break internal state
        client.remove(path).await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_access_stress() {
        let client = Arc::new(tokio::sync::Mutex::new(make_client().await));

        let mut handles = vec![];
        for i in 0..10 {
            let client = client.clone();
            handles.push(tokio::spawn(async move {
                let mut client = client.lock().await;
                let path = format!("/tmp/testfile_{}", i);
                client.mk_dir(&path).await.ok();
                let _ = client.list_dir("/tmp").await;
                client.remove(&path).await.ok();
            }));
        }

        for h in handles {
            let _ = h.await;
        }
    }

    #[tokio::test]
    async fn read_write_mode_works() {
        let mut client = make_client().await;

        // Clean up from previous runs
        let _ = client.remove("/tmp/rw_test.txt").await;

        // Open for read/write
        let mut file = client
            .open("/tmp/rw_test.txt", AfcFopenMode::Rw)
            .await
            .expect("failed to open file in rw mode");

        // Write some data
        let data = b"hello world";
        file.write_all(data).await.expect("failed to write");

        // Seek back to start
        file.seek(std::io::SeekFrom::Start(0))
            .await
            .expect("seek failed");

        // Read it back
        let mut buf = vec![0u8; data.len()];
        file.read_exact(&mut buf).await.expect("failed to read");
        assert_eq!(&buf, data);

        // Write again at end
        file.seek(std::io::SeekFrom::End(0)).await.unwrap();
        file.write_all(b"!").await.unwrap();

        // Verify new content
        file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        let mut final_buf = Vec::new();
        file.read_to_end(&mut final_buf).await.unwrap();
        assert_eq!(&final_buf, b"hello world!");

        file.close().await.expect("failed to close");

        // Double check via list/read
        let contents = client
            .open("/tmp/rw_test.txt", AfcFopenMode::RdOnly)
            .await
            .unwrap()
            .read_entire()
            .await
            .unwrap();
        assert_eq!(contents, b"hello world!");

        // Clean up
        client.remove("/tmp/rw_test.txt").await.unwrap();
    }
}
