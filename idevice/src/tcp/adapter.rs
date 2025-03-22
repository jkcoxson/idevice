// Jackson Coxson
// Simpler TCP stack

use std::{future::Future, net::IpAddr, path::Path, sync::Arc, task::Poll};

use log::trace;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::Mutex,
};

use crate::ReadWrite;

use super::packets::{Ipv4Packet, Ipv6Packet, ProtocolNumber, TcpFlags, TcpPacket};

#[derive(Clone, Debug, PartialEq)]
enum AdapterState {
    Connected,
    None,
}

/// An extremely naive, limited and dangerous stack
/// Only one connection can be live at a time
/// Acks aren't tracked, and are silently ignored
/// Only use when the underlying transport is 100% reliable
#[derive(Debug)]
pub struct Adapter {
    peer: Box<dyn ReadWrite>,
    host_ip: IpAddr,
    peer_ip: IpAddr,
    state: AdapterState,

    // TCP state
    seq: u32,
    ack: u32,
    host_port: u16,
    peer_port: u16,

    // Read buffer to cache unused bytes
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,

    // Logging
    pcap: Option<Arc<Mutex<tokio::fs::File>>>,
}

impl Adapter {
    pub fn new(peer: Box<dyn ReadWrite>, host_ip: IpAddr, peer_ip: IpAddr) -> Self {
        Self {
            peer,
            host_ip,
            peer_ip,
            state: AdapterState::None,
            seq: 0,
            ack: 0,
            host_port: 1024,
            peer_port: 1024,
            read_buffer: Vec::new(),
            write_buffer: Vec::new(),
            pcap: None,
        }
    }

    pub async fn connect(&mut self, port: u16) -> Result<(), std::io::Error> {
        self.read_buffer = Vec::new();

        // Randomize seq
        self.seq = rand::random();
        self.ack = 0;

        // Choose a random port
        self.host_port = rand::random();
        self.peer_port = port;

        // Create the TCP packet
        let tcp_packet = TcpPacket::create(
            self.host_ip,
            self.peer_ip,
            self.host_port,
            self.peer_port,
            self.seq,
            self.ack,
            TcpFlags {
                syn: true,
                ..Default::default()
            },
            u16::MAX - 1,
            &[],
        );
        let ip_packet = self.ip_wrap(&tcp_packet);
        self.peer.write_all(&ip_packet).await?;
        self.log_packet(&ip_packet).await?;

        // Wait for the syn ack
        let res = self.read_tcp_packet().await?;
        if !(res.flags.syn && res.flags.ack) {
            log::error!("Didn't get syn ack");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No syn ack",
            ));
        }
        self.seq = self.seq.wrapping_add(1);

        // Ack back
        self.ack().await?;

        self.state = AdapterState::Connected;
        Ok(())
    }

    pub async fn pcap(&mut self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let mut file = tokio::fs::File::create(path).await?;

        // https://wiki.wireshark.org/Development/LibpcapFileFormat
        file.write_all(&0xa1b2c3d4_u32.to_le_bytes()).await?; // magic
        file.write_all(&2_u16.to_le_bytes()).await?; // major version
        file.write_all(&4_u16.to_le_bytes()).await?; // minor
        file.write_all(&0_i32.to_le_bytes()).await?; // timezone
        file.write_all(&0_u32.to_le_bytes()).await?; // accuracy
        file.write_all(&(u16::MAX as u32).to_le_bytes()).await?; // snaplen
                                                                 // https://www.tcpdump.org/linktypes.html
        file.write_all(&101_u32.to_le_bytes()).await?; // link type

        self.pcap = Some(Arc::new(Mutex::new(file)));
        Ok(())
    }

    async fn log_packet(&mut self, packet: &[u8]) -> Result<(), std::io::Error> {
        if let Some(file) = &self.pcap {
            super::log_packet(file, packet).await;
        }
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), std::io::Error> {
        let tcp_packet = TcpPacket::create(
            self.host_ip,
            self.peer_ip,
            self.host_port,
            self.peer_port,
            self.seq,
            self.ack,
            TcpFlags {
                fin: true,
                ..Default::default()
            },
            u16::MAX - 1,
            &[],
        );
        let ip_packet = self.ip_wrap(&tcp_packet);
        self.peer.write_all(&ip_packet).await?;
        self.log_packet(&ip_packet).await?;
        self.state = AdapterState::None;
        Ok(())
    }

    async fn ack(&mut self) -> Result<(), std::io::Error> {
        let tcp_packet = TcpPacket::create(
            self.host_ip,
            self.peer_ip,
            self.host_port,
            self.peer_port,
            self.seq,
            self.ack,
            TcpFlags {
                ack: true,
                ..Default::default()
            },
            u16::MAX - 1,
            &[],
        );
        let ip_packet = self.ip_wrap(&tcp_packet);
        self.peer.write_all(&ip_packet).await?;
        self.log_packet(&ip_packet).await?;

        Ok(())
    }

    /// Sends a packet
    pub async fn psh(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        trace!("pshing {} bytes", data.len());
        let tcp_packet = TcpPacket::create(
            self.host_ip,
            self.peer_ip,
            self.host_port,
            self.peer_port,
            self.seq,
            self.ack,
            TcpFlags {
                psh: true,
                ack: true,
                ..Default::default()
            },
            u16::MAX - 1,
            data,
        );
        let ip_packet = self.ip_wrap(&tcp_packet);
        self.peer.write_all(&ip_packet).await?;
        self.log_packet(&ip_packet).await?;

        self.seq = self.seq.wrapping_add(data.len() as u32);

        Ok(())
    }

    /// Flushes the packets
    async fn write_buffer_flush(&mut self) -> Result<(), std::io::Error> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }
        trace!("Flushing {} bytes", self.write_buffer.len());
        let write_buffer = self.write_buffer.clone();
        self.psh(&write_buffer).await?;
        self.write_buffer = Vec::new();
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, std::io::Error> {
        loop {
            let res = self.read_tcp_packet().await?;
            if res.flags.psh || !res.payload.is_empty() {
                self.ack().await?;
                break Ok(res.payload);
            }
            if res.flags.rst {
                self.state = AdapterState::None;
                break Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "Connection reset",
                ));
            }
            if res.flags.fin {
                self.ack().await?;
                self.state = AdapterState::None;
                break Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "Connection reset",
                ));
            }
        }
    }

    /// Reads a packet and returns the payload
    async fn read_ip_packet(&mut self) -> Result<Vec<u8>, std::io::Error> {
        self.write_buffer_flush().await?;
        Ok(loop {
            match self.host_ip {
                IpAddr::V4(_) => {
                    let packet = Ipv4Packet::from_reader(&mut self.peer, &self.pcap).await?;
                    trace!("IPv4 packet: {packet:#?}");
                    if packet.protocol == 6 {
                        break packet.payload;
                    }
                }
                IpAddr::V6(_) => {
                    let packet = Ipv6Packet::from_reader(&mut self.peer, &self.pcap).await?;
                    trace!("IPv6 packet: {packet:#?}");
                    if packet.next_header == 6 {
                        break packet.payload;
                    }
                }
            }
        })
    }

    async fn read_tcp_packet(&mut self) -> Result<TcpPacket, std::io::Error> {
        let ip_packet = self.read_ip_packet().await?;
        let tcp_packet = TcpPacket::parse(&ip_packet)?;
        trace!("TCP packet: {tcp_packet:#?}");
        self.ack = tcp_packet.sequence_number
            + if tcp_packet.payload.is_empty() {
                1
            } else {
                tcp_packet.payload.len() as u32
            };
        Ok(tcp_packet)
    }

    fn ip_wrap(&self, packet: &[u8]) -> Vec<u8> {
        match self.host_ip {
            IpAddr::V4(host_addr) => match self.peer_ip {
                IpAddr::V4(peer_addr) => {
                    Ipv4Packet::create(host_addr, peer_addr, ProtocolNumber::Tcp, 255, packet)
                }
                IpAddr::V6(_) => {
                    panic!("non matching IP versions");
                }
            },
            IpAddr::V6(host_addr) => match self.peer_ip {
                IpAddr::V4(_) => {
                    panic!("non matching IP versions")
                }
                IpAddr::V6(peer_addr) => {
                    Ipv6Packet::create(host_addr, peer_addr, ProtocolNumber::Tcp, 255, packet)
                }
            },
        }
    }
}

impl AsyncRead for Adapter {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // First, check if we have any cached data
        if !self.read_buffer.is_empty() {
            let to_copy = std::cmp::min(buf.remaining(), self.read_buffer.len());
            buf.put_slice(&self.read_buffer[..to_copy]);

            // Keep any remaining data in the buffer
            if to_copy < self.read_buffer.len() {
                self.read_buffer = self.read_buffer[to_copy..].to_vec();
            } else {
                self.read_buffer.clear();
            }

            return std::task::Poll::Ready(Ok(()));
        }

        // If no cached data and not connected, return error
        if self.state != AdapterState::Connected {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Adapter not connected",
            )));
        }

        // If no cached data, try to receive new data
        let future = async {
            match self.recv().await {
                Ok(data) => {
                    let len = std::cmp::min(buf.remaining(), data.len());
                    buf.put_slice(&data[..len]);

                    // If we received more data than needed, cache the rest
                    if len < data.len() {
                        self.read_buffer = data[len..].to_vec();
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

impl AsyncWrite for Adapter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        trace!("poll psh {}", buf.len());
        if self.state != AdapterState::Connected {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Adapter not connected",
            )));
        }
        self.write_buffer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let future = async {
            match self.write_buffer_flush().await {
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
