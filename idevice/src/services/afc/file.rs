// Jackson Coxson

use std::{io::SeekFrom, pin::Pin};

use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};

use super::inner_file::InnerFileDescriptor;
use crate::IdeviceError;

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
    pub async fn close(self) -> Result<(), IdeviceError> {
        self.inner.close().await
    }

    /// Reads the entire contents of the file
    ///
    /// # Returns
    /// A vector containing the file's data
    pub async fn read_entire(&mut self) -> Result<Vec<u8>, IdeviceError> {
        self.inner.as_mut().read().await
    }

    pub async fn read_with_callback<Fut, S>(
        &mut self,
        callback: impl Fn(((usize, usize), S)) -> Fut,
        state: S,
    ) -> Result<Vec<u8>, IdeviceError>
    where
        Fut: std::future::Future<Output = ()>,
        S: Clone,
    {
        let seek_pos = self.seek_tell().await? as usize;
        let file_info = self.client.get_file_info(&self.path).await?;
        let mut bytes_left = file_info.size.saturating_sub(seek_pos);
        let mut collected_bytes = Vec::with_capacity(bytes_left);

        while bytes_left > 0 {
            let mut header_payload = self.fd.to_le_bytes().to_vec();
            header_payload.extend_from_slice(&MAX_TRANSFER.to_le_bytes());
            let res = self
                .send_packet(AfcOpcode::Read, header_payload, Vec::new())
                .await?;

            bytes_left -= res.payload.len();
            collected_bytes.extend(res.payload);
            callback(((file_info.size - bytes_left, file_info.size), state.clone())).await;
        }

        Ok(collected_bytes)
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

    pub async fn write_with_callback<Fut, S>(
        &mut self,
        bytes: &[u8],
        callback: impl Fn(((usize, usize), S)) -> Fut,
        state: S,
    ) -> Result<(), IdeviceError>
    where
        Fut: std::future::Future<Output = ()>,
        S: Clone,
    {
        let chunks = bytes.chunks(MAX_TRANSFER as usize);
        let chunks_len = chunks.len();
        for (i, chunk) in chunks.enumerate() {
            let header_payload = self.fd.to_le_bytes().to_vec();
            self.send_packet(AfcOpcode::Write, header_payload, chunk.to_vec())
                .await?;
            callback(((i, chunks_len), state.clone())).await;
        }
        Ok(())
    }
}
