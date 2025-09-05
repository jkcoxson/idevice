// So originally, streams wrote to the adapter via a mutable reference.
// This worked fine for most applications, but the lifetime requirement of the stream
// makes things difficult. This was especially apparent when trying to integrate with lockdown
// services that were swapped on the heap. This will also allow for usage across threads,
// especially in FFI. Judging the tradeoffs, we'll go forward with it.

use std::{collections::HashMap, path::PathBuf, sync::Mutex, task::Poll};

use crossfire::{AsyncRx, MTx, Tx, mpsc, spsc, stream::AsyncStream};
use futures::{StreamExt, stream::FuturesUnordered};
use log::trace;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::oneshot,
    time::timeout,
};

use crate::tcp::adapter::ConnectionStatus;

pub type ConnectToPortRes =
    oneshot::Sender<Result<(u16, AsyncRx<Result<Vec<u8>, std::io::Error>>), std::io::Error>>;

enum HandleMessage {
    /// Returns the host port
    ConnectToPort {
        target: u16,
        res: ConnectToPortRes,
    },
    Close {
        host_port: u16,
    },
    Send {
        host_port: u16,
        data: Vec<u8>,
        res: oneshot::Sender<Result<(), std::io::Error>>,
    },
    Pcap {
        path: PathBuf,
        res: oneshot::Sender<Result<(), std::io::Error>>,
    },
}

#[derive(Debug)]
pub struct AdapterHandle {
    sender: MTx<HandleMessage>,
}

impl AdapterHandle {
    pub fn new(mut adapter: super::adapter::Adapter) -> Self {
        let (tx, rx) = mpsc::unbounded_async();
        tokio::spawn(async move {
            let mut handles: HashMap<u16, Tx<Result<Vec<u8>, std::io::Error>>> = HashMap::new();
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(1));

            loop {
                tokio::select! {
                    // check for messages for us
                    msg = rx.recv() => {
                        match msg {
                            Ok(m) => match m {
                                HandleMessage::ConnectToPort { target, res } => {
                                    let connect_response = match adapter.connect(target).await {
                                        Ok(c) => {
                                            let (ptx, prx) = spsc::unbounded_async();
                                            handles.insert(c, ptx);
                                            Ok((c, prx))
                                        }
                                        Err(e) => Err(e),
                                    };
                                    res.send(connect_response).ok();
                                }
                                HandleMessage::Close { host_port } => {
                                    handles.remove(&host_port);
                                    adapter.close(host_port).await.ok();
                                }
                                HandleMessage::Send {
                                    host_port,
                                    data,
                                    res,
                                } => {
                                    if let Err(e) = adapter.queue_send(&data, host_port) {
                                        res.send(Err(e)).ok();
                                    } else {
                                        let response = adapter.write_buffer_flush().await;
                                        res.send(response).ok();
                                    }
                                }
                                HandleMessage::Pcap {
                                    path,
                                    res
                                } => {
                                    res.send(adapter.pcap(path).await).ok();
                                }
                            },
                            Err(_) => {
                                break;
                            },
                        }
                    }

                    r = adapter.process_tcp_packet() => {
                        if let Err(e) = r {
                            // propagate error to all streams; close them
                            for (hp, tx) in handles.drain() {
                                let _ = tx.send(Err(e.kind().into())); // or clone/convert
                                let _ = adapter.close(hp).await;
                            }
                            break;
                        }

                        // Push any newly available bytes to per-conn channels
                        let mut dead = Vec::new();
                        for (&hp, tx) in &handles {
                            match adapter.uncache_all(hp) {
                                Ok(buf) if !buf.is_empty() => {
                                    if tx.send(Ok(buf)).is_err() {
                                        dead.push(hp);
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(e));
                                    dead.push(hp);
                                }
                                _ => {}
                            }
                        }
                        for hp in dead {
                            handles.remove(&hp);
                            let _ = adapter.close(hp).await;
                        }

                        let mut to_close = Vec::new();
                        for (&hp, tx) in &handles {
                            if let Ok(ConnectionStatus::Error(kind)) = adapter.get_status(hp) {
                                if kind == std::io::ErrorKind::UnexpectedEof {
                                    to_close.push(hp);
                                } else {
                                    let _ = tx.send(Err(std::io::Error::from(kind)));
                                    to_close.push(hp);
                                }
                            }
                        }
                        for hp in to_close {
                            handles.remove(&hp);
                            // Best-effort close. For RST this just tidies state on our side
                            let _ = adapter.close(hp).await;
                        }
                    }

                    _ = tick.tick() => {
                        let _ = adapter.write_buffer_flush().await;
                    }
                }
            }
        });

        Self { sender: tx }
    }

    pub async fn connect(&mut self, port: u16) -> Result<StreamHandle, std::io::Error> {
        let (res_tx, res_rx) = oneshot::channel();
        if self
            .sender
            .send(HandleMessage::ConnectToPort {
                target: port,
                res: res_tx,
            })
            .is_err()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NetworkUnreachable,
                "adapter closed",
            ));
        }

        match timeout(std::time::Duration::from_secs(8), res_rx).await {
            Ok(Ok(r)) => {
                let (host_port, recv_channel) = r?;
                Ok(StreamHandle {
                    host_port,
                    recv_channel: Mutex::new(recv_channel.into_stream()),
                    send_channel: self.sender.clone(),
                    read_buffer: Vec::new(),
                    pending_writes: FuturesUnordered::new(),
                })
            }
            Ok(Err(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "adapter closed",
            )),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "channel recv timeout",
            )),
        }
    }

    pub async fn pcap(&mut self, path: impl Into<PathBuf>) -> Result<(), std::io::Error> {
        let (res_tx, res_rx) = oneshot::channel();
        let path: PathBuf = path.into();

        if self
            .sender
            .send(HandleMessage::Pcap { path, res: res_tx })
            .is_err()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NetworkUnreachable,
                "adapter closed",
            ));
        }

        match res_rx.await {
            Ok(r) => r,
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "adapter closed",
            )),
        }
    }
}

#[derive(Debug)]
pub struct StreamHandle {
    host_port: u16,
    recv_channel: Mutex<AsyncStream<Result<Vec<u8>, std::io::Error>>>,
    send_channel: MTx<HandleMessage>,

    read_buffer: Vec<u8>,
    pending_writes: FuturesUnordered<oneshot::Receiver<Result<(), std::io::Error>>>,
}

impl StreamHandle {
    pub fn close(&mut self) {
        let _ = self.send_channel.send(HandleMessage::Close {
            host_port: self.host_port,
        });
    }
}

impl AsyncRead for StreamHandle {
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
        // 1) Serve from cache first.
        if !self.read_buffer.is_empty() {
            let n = buf.remaining().min(self.read_buffer.len());
            buf.put_slice(&self.read_buffer[..n]);
            self.read_buffer.drain(..n); // fewer allocs than to_vec + reassign
            return Poll::Ready(Ok(()));
        }

        // 2) Poll the channel directly; this registers the waker on Empty.
        let mut lock = self
            .recv_channel
            .lock()
            .expect("somehow the mutex was poisoned");
        // this should always return, since we're the only owner of the mutex. The mutex is only
        // used to satisfy the `Send` bounds of ReadWrite.
        let mut extend_slice = Vec::new();
        let res = match lock.poll_item(cx) {
            Poll::Pending => Poll::Pending,

            // Disconnected/ended: map to BrokenPipe
            Poll::Ready(None) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "channel closed",
            ))),

            // Got a chunk: copy what fits; cache the tail.
            Poll::Ready(Some(res)) => match res {
                Ok(data) => {
                    let n = buf.remaining().min(data.len());
                    buf.put_slice(&data[..n]);
                    if n < data.len() {
                        extend_slice = data[n..].to_vec();
                    }
                    Poll::Ready(Ok(()))
                }
                Err(e) => Poll::Ready(Err(e)),
            },
        };
        std::mem::drop(lock);
        self.read_buffer.extend(extend_slice);
        res
    }
}

impl AsyncWrite for StreamHandle {
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
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        trace!("poll psh {}", buf.len());
        let (tx, rx) = oneshot::channel();
        self.send_channel
            .send(HandleMessage::Send {
                host_port: self.host_port,
                data: buf.to_vec(),
                res: tx,
            })
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed"))?;
        self.pending_writes.push(rx);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        while let Poll::Ready(maybe) = self.pending_writes.poll_next_unpin(cx) {
            match maybe {
                Some(Ok(Ok(()))) => {}
                Some(Ok(Err(e))) => return Poll::Ready(Err(e)),
                Some(Err(_canceled)) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "channel closed",
                    )));
                }
                None => break, // nothing pending
            }
        }
        if self.pending_writes.is_empty() {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        // Just a drop will close the channel, which will trigger a close
        std::task::Poll::Ready(Ok(()))
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        let _ = self.send_channel.send(HandleMessage::Close {
            host_port: self.host_port,
        });
    }
}
