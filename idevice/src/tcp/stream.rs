//! A stream for the adapter

use std::{future::Future, task::Poll};

use log::trace;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::tcp::adapter::ConnectionStatus;

use super::adapter::Adapter;

#[derive(Debug)]
pub struct AdapterStream<'a> {
    pub(crate) adapter: &'a mut Adapter,
    pub host_port: u16,
    pub peer_port: u16,
}

impl<'a> AdapterStream<'a> {
    /// Connect to the specified port
    pub async fn connect(adapter: &'a mut Adapter, port: u16) -> Result<Self, std::io::Error> {
        let host_port = adapter.connect(port).await?;
        Ok(Self {
            adapter,
            host_port,
            peer_port: port,
        })
    }

    /// Gracefully closes the stream
    pub async fn close(&mut self) -> Result<(), std::io::Error> {
        self.adapter.close(self.host_port).await
    }

    /// Sends data to the target
    pub async fn psh(&mut self, payload: &[u8]) -> Result<(), std::io::Error> {
        self.adapter.queue_send(payload, self.host_port)?;
        self.adapter.write_buffer_flush().await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, std::io::Error> {
        self.adapter.recv(self.host_port).await
    }
}

impl AsyncRead for AdapterStream<'_> {
    /// Attempts to read from the connection into the provided buffer.
    ///
    /// Uses an internal read buffer to cache any extra received data.
    ///
    /// # Returns
    /// * `Poll::Ready(Ok(()))` if data was read successfully
    /// * `Poll::Ready(Err(e))` if an error occurred
    /// * `Poll::Pending` if operation would block
    ///
    /// # Errors
    /// * Returns `NotConnected` if adapter isn't connected
    /// * Propagates any underlying transport errors
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.adapter.get_status(self.host_port) {
            Ok(ConnectionStatus::Error(e)) => {
                return std::task::Poll::Ready(Err(std::io::Error::new(e, "io error")));
            }
            Err(e) => {
                return std::task::Poll::Ready(Err(e));
            }
            _ => {}
        }

        // First, check if we have any cached data
        let p = self.host_port;
        let cache = match self.adapter.uncache(buf.remaining(), p) {
            Ok(c) => c,
            Err(e) => return std::task::Poll::Ready(Err(e)),
        };
        if !cache.is_empty() {
            buf.put_slice(&cache);
            return std::task::Poll::Ready(Ok(()));
        }

        // If no cached data, try to receive new data
        let future = async {
            match self.adapter.recv(p).await {
                Ok(data) => {
                    let len = std::cmp::min(buf.remaining(), data.len());
                    buf.put_slice(&data[..len]);

                    // If we received more data than needed, cache the rest
                    if len < data.len() {
                        self.adapter.cache_read(&data[len..], p)?
                    }

                    Ok(())
                }
                Err(e) => Err(e),
            }
        };

        // Pin the future and poll it
        futures::pin_mut!(future);
        future.poll(cx)
    }
}

impl AsyncWrite for AdapterStream<'_> {
    /// Attempts to write data to the connection.
    ///
    /// Data is buffered internally until flushed.
    ///
    /// # Returns
    /// * `Poll::Ready(Ok(n))` with number of bytes written
    /// * `Poll::Ready(Err(e))` if an error occurred
    /// * `Poll::Pending` if operation would block
    ///
    /// # Errors
    /// * Returns `NotConnected` if adapter isn't connected
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        trace!("poll psh {}", buf.len());
        match self.adapter.get_status(self.host_port) {
            Ok(ConnectionStatus::Error(e)) => {
                return std::task::Poll::Ready(Err(std::io::Error::new(e, "io error")));
            }
            Err(e) => {
                return std::task::Poll::Ready(Err(e));
            }
            _ => {}
        }
        let p = self.host_port;
        match self.adapter.queue_send(buf, p) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let future = async {
            match self.adapter.write_buffer_flush().await {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        };

        // Pin the future and poll it
        futures::pin_mut!(future);
        future.poll(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        // Create a future that can be polled
        let future = async { self.close().await };

        // Pin the future and poll it
        futures::pin_mut!(future);
        future.poll(cx)
    }
}

impl Drop for AdapterStream<'_> {
    fn drop(&mut self) {
        self.adapter.connection_drop(self.host_port);
    }
}
