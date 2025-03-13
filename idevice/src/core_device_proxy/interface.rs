// Jackson Coxson

use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{IpAddr, Ipv6Addr},
    pin::Pin,
    str::FromStr,
    task::{Context, Poll},
};

use log::{debug, warn};
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    socket::tcp,
    time::Instant,
    wire::{IpAddress, IpCidr},
};
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::mpsc::{error::TryRecvError, unbounded_channel, UnboundedReceiver, UnboundedSender},
};

use crate::IdeviceError;

const TCP_BUFFER_SIZE: usize = 1024 * 1024;

pub struct TunnelInterface {
    thread_sender: UnboundedSender<ThreadMessage>,
}

enum ThreadMessage {
    NewTcp((u16, UnboundedSender<Vec<u8>>, UnboundedReceiver<Vec<u8>>)),
    // left for possible UDP/other in the future??
}

struct TunnelSocketHandle {
    handle: SocketHandle,
    sender: UnboundedSender<Vec<u8>>,
    receiver: UnboundedReceiver<Vec<u8>>,
}

#[derive(Debug)]
pub struct Channel {
    sender: UnboundedSender<Vec<u8>>,
    receiver: UnboundedReceiver<Vec<u8>>,
}

impl TunnelInterface {
    pub fn new(proxy: super::CoreDeviceProxy) -> Result<Self, IdeviceError> {
        let ip: IpAddr = match IpAddr::from_str(&proxy.handshake.client_parameters.address) {
            Ok(i) => i,
            Err(e) => {
                warn!("Failed to parse IP as IP address: {e:?}");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };
        let device_ip: Ipv6Addr = match proxy.handshake.server_address.parse() {
            Ok(d) => d,
            Err(_) => {
                warn!("Device IP isn't parsable as IPv6");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };
        let mut device = super::device::ProxyDevice::new(proxy);
        let config = Config::new(smoltcp::wire::HardwareAddress::Ip);

        // Create the interface
        let mut interface = Interface::new(config, &mut device, smoltcp::time::Instant::now());

        // Add the IPv6 address to the interface
        let ip = match ip {
            IpAddr::V4(_) => {
                warn!("IP was IPv4, not 6");
                return Err(IdeviceError::UnexpectedResponse);
            }
            IpAddr::V6(ipv6_addr) => ipv6_addr,
        };
        interface.update_ip_addrs(|addrs| {
            // Add the IPv6 address with a prefix length (e.g., 64 for typical IPv6 subnets)
            addrs.push(IpCidr::new(IpAddress::Ipv6(ip), 64)).unwrap();
        });

        let (thread_sender, mut thread_rx) = unbounded_channel();

        tokio::task::spawn_blocking(move || {
            // based on https://github.com/smoltcp-rs/smoltcp/blob/main/examples/client.rs
            let mut sockets = SocketSet::new(vec![]);
            let mut handles: HashMap<u16, TunnelSocketHandle> = HashMap::new();
            let mut last_port = 1024; // host to bind and index by
            loop {
                let timestamp = Instant::now();
                interface.poll(timestamp, &mut device, &mut sockets);

                let mut to_remove = Vec::new();
                let keys: Vec<u16> = handles.keys().cloned().collect();

                for key in keys {
                    if let Some(handle) = handles.get_mut(&key) {
                        let socket = sockets.get_mut::<tcp::Socket>(handle.handle);

                        if socket.may_recv() {
                            if let Err(e) = socket.recv(|data| {
                                if !data.is_empty() && handle.sender.send(data.to_owned()).is_err()
                                {
                                    warn!("Handle dropped");
                                    to_remove.push(key);
                                }
                                (data.len(), data)
                            }) {
                                warn!("Disconnected from socket: {e:?}");
                                to_remove.push(key);
                            }
                            if socket.can_send() {
                                match handle.receiver.try_recv() {
                                    Ok(data) => {
                                        let queued = socket.send_slice(&data[..]).unwrap();
                                        if queued < data.len() {
                                            log::error!("Failed to queue packet for send");
                                        }
                                    }
                                    Err(TryRecvError::Empty) => {} // wait
                                    Err(TryRecvError::Disconnected) => {
                                        debug!("handle is dropped");
                                    }
                                }
                            } else {
                                warn!("Can't send!!");
                            }
                        } else if socket.may_send() {
                            debug!("close");
                            socket.close();
                        }
                    }
                }

                // Remove the requested threads
                for t in to_remove {
                    if let Some(h) = handles.remove(&t) {
                        sockets.remove(h.handle);
                        // When the struct is dropped, the unbounded_channel will close.
                        // Because the channel is closed, the next time recv or send is called will
                        // error.
                    }
                }

                match thread_rx.try_recv() {
                    Ok(msg) => match msg {
                        ThreadMessage::NewTcp((port, tx, rx)) => {
                            // Create sockets
                            let tcp_rx_buffer = tcp::SocketBuffer::new(vec![0; TCP_BUFFER_SIZE]);
                            let tcp_tx_buffer = tcp::SocketBuffer::new(vec![0; TCP_BUFFER_SIZE]);
                            let tcp_socket = tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer);
                            let handle = sockets.add(tcp_socket);
                            let socket = sockets.get_mut::<tcp::Socket>(handle);
                            socket
                                .connect(interface.context(), (device_ip, port), last_port)
                                .unwrap();
                            handles.insert(
                                last_port,
                                TunnelSocketHandle {
                                    handle,
                                    sender: tx,
                                    receiver: rx,
                                },
                            );

                            last_port += 1;
                        }
                    },
                    Err(TryRecvError::Disconnected) => {
                        debug!("Thread sender dropped");
                        break;
                    }
                    Err(TryRecvError::Empty) => {
                        // noop
                    }
                }

                let poll_delay = interface
                    .poll_delay(timestamp, &sockets)
                    .unwrap_or(smoltcp::time::Duration::from_millis(100))
                    .millis();

                std::thread::sleep(std::time::Duration::from_millis(poll_delay));
            }
        });

        Ok(Self { thread_sender })
    }

    pub fn connect_tcp(&mut self, port: u16) -> Result<Channel, IdeviceError> {
        let (tx, rx) = unbounded_channel();
        let (rtx, rrx) = unbounded_channel();
        if self
            .thread_sender
            .send(ThreadMessage::NewTcp((port, rtx, rx)))
            .is_err()
        {
            return Err(IdeviceError::TunnelThreadClosed);
        }

        Ok(Channel {
            sender: tx,
            receiver: rrx,
        })
    }
}

impl Channel {
    pub async fn send(&self, data: Vec<u8>) -> Result<(), IdeviceError> {
        if let Err(e) = self.sender.send(data) {
            warn!("Failed to send: {e:?}");
            return Err(IdeviceError::ChannelClosed);
        }
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, IdeviceError> {
        match self.receiver.recv().await {
            Some(d) => Ok(d),
            None => {
                warn!("Failed to recv");
                Err(IdeviceError::ChannelClosed)
            }
        }
    }
}

impl AsyncRead for Channel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.receiver).poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                let len = data.len().min(buf.remaining());
                buf.put_slice(&data[..len]);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                warn!("unexpected eof");
                Poll::Ready(Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "Channel closed",
                )))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for Channel {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let data = buf.to_vec();
        match self.sender.send(data) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(e) => {
                warn!("Failed to send data: {e:?}");
                Poll::Ready(Err(io::Error::new(ErrorKind::BrokenPipe, "Channel closed")))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
