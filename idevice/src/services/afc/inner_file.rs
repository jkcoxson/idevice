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

/// Handle for an open file on the device.
/// Call close before dropping
pub(crate) struct InnerFileDescriptor<'a> {
    pub(crate) client: &'a mut super::AfcClient,
    pub(crate) fd: u64,
    pub(crate) path: String,

    pub(crate) pending_read_fut: Option<BoxFuture<'a, Result<Vec<u8>, IdeviceError>>>,

    pub(crate) pending_write_data: Option<Vec<u8>>,
    pub(crate) pending_write_fut: Option<BoxFuture<'a, Result<(), IdeviceError>>>,

    pub(crate) pending_seek_fut: Option<BoxFuture<'a, Result<u64, IdeviceError>>>,

    pub(crate) _m: std::marker::PhantomPinned,
}

impl<'a> InnerFileDescriptor<'a> {
    pub(crate) fn new(client: &'a mut super::AfcClient, fd: u64, path: String) -> Pin<Box<Self>> {
        Box::pin(Self {
            client,
            fd,
            path,
            pending_read_fut: None,
            pending_write_data: None,
            pending_write_fut: None,
            pending_seek_fut: None,
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
    pub async fn close(self: Pin<&mut Self>) -> Result<(), IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();

        self.send_packet(AfcOpcode::FileClose, header_payload, Vec::new())
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

            let fut = Some(Pin::new_unchecked(&mut *this).read_n(buf_rem).boxed());

            (&mut *this).pending_read_fut = fut;
        }
    }

    fn store_pending_seek(mut self: Pin<&mut Self>, position: std::io::SeekFrom) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut() as *mut InnerFileDescriptor;

            let fut = Some(Pin::new_unchecked(&mut *this).seek(position).boxed());

            (&mut *this).pending_seek_fut = fut;
        }
    }

    fn store_pending_write(mut self: Pin<&mut Self>, buf: &[u8]) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut();

            this.pending_write_data = Some(buf.to_vec());

            let this = this as *mut InnerFileDescriptor;

            let buf = (&*this).pending_write_data.as_ref().unwrap();

            let fut = Some(Pin::new_unchecked(&mut *this).write(buf).boxed());

            (&mut *this).pending_write_fut = fut;
        }
    }
}

impl<'a> InnerFileDescriptor<'a> {
    fn get_or_init_read_fut(
        mut self: Pin<&mut Self>,
        buf_rem: usize,
    ) -> &mut BoxFuture<'a, Result<Vec<u8>, IdeviceError>> {
        if self.as_ref().pending_read_fut.is_none() {
            self.as_mut().store_pending_read(buf_rem);
        }

        unsafe { self.get_unchecked_mut().pending_read_fut.as_mut().unwrap() }
    }

    fn get_or_init_write_fut(
        mut self: Pin<&mut Self>,
        buf: &'_ [u8],
    ) -> &mut BoxFuture<'a, Result<(), IdeviceError>> {
        if self.as_ref().pending_write_fut.is_none() {
            self.as_mut().store_pending_write(buf);
        }

        unsafe { self.get_unchecked_mut().pending_write_fut.as_mut().unwrap() }
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
                Ok(c) => {
                    unsafe {
                        self.as_mut().get_unchecked_mut().pending_read_fut.take();
                    }
                    c
                }
                Err(e) => return std::task::Poll::Ready(Err(std::io::Error::other(e.to_string()))),
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
            Ok(()) => unsafe {
                let this = self.get_unchecked_mut();
                this.pending_write_fut.take();
                this.pending_write_data.take();
            },
            Err(e) => {
                println!("error: {e}");
                return std::task::Poll::Ready(Err(std::io::Error::other(e.to_string())));
            }
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
        let fut = if self.pending_seek_fut.is_some() {
            unsafe {
                let this = self.as_mut().get_unchecked_mut();
                this.pending_seek_fut.as_mut().unwrap()
            }
        } else {
            // tokio run the `poll_complete` before the `start_seek` to ensure no seek in progress
            return std::task::Poll::Ready(Ok(0));
        };

        match std::task::ready!(fut.as_mut().poll(cx)) {
            Ok(pos) => unsafe {
                self.as_mut().get_unchecked_mut().pending_seek_fut.take();
                std::task::Poll::Ready(Ok(pos))
            },
            Err(e) => std::task::Poll::Ready(Err(std::io::Error::other(e.to_string()))),
        }
    }
}
