// Jackson Coxson

use std::{io::SeekFrom, pin::Pin};

use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};

use super::inner_file::InnerFileDescriptor;
use crate::IdeviceError;

use super::{opcode::AfcOpcode, packet::AfcPacket};

pub struct FileDescriptor<'a> {
    inner: Pin<Box<InnerFileDescriptor<'a>>>,
}

impl<'a> FileDescriptor<'a> {
    pub(crate) fn new(inner: Pin<Box<InnerFileDescriptor<'a>>>) -> Self {
        Self { inner }
    }
}
impl FileDescriptor<'_> {
    /// Returns the current cursor position for the file
    pub async fn seek_tell(&mut self) -> Result<u64, IdeviceError> {
        self.inner.as_mut().seek_tell().await
    }

    /// Closes the file descriptor
    pub async fn close(&mut self) -> Result<(), IdeviceError> {
        self.inner.as_mut().close().await
    }

    /// Reads the entire contents of the file
    ///
    /// # Returns
    /// A vector containing the file's data
    pub async fn read_entire(&mut self) -> Result<Vec<u8>, IdeviceError> {
        self.inner.as_mut().read().await
    }

    /// Writes data to the file
    ///
    /// # Arguments
    /// * `bytes` - Data to write to the file
    pub async fn write_entire(&mut self, bytes: &[u8]) -> Result<(), IdeviceError> {
        self.inner.as_mut().write(bytes).await
    }
}

impl AsyncRead for FileDescriptor<'_> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let inner = self.inner.as_mut();
        inner.poll_read(cx, buf)
    }
}

impl AsyncWrite for FileDescriptor<'_> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let inner = self.inner.as_mut();
        inner.poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let inner = self.inner.as_mut();
        inner.poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let inner = self.inner.as_mut();
        inner.poll_shutdown(cx)
    }
}

impl AsyncSeek for FileDescriptor<'_> {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
        let this = self.inner.as_mut();
        this.start_seek(position)
    }

    fn poll_complete(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        let this = self.inner.as_mut();
        this.poll_complete(cx)
    }
}
