//! A simplified TCP network stack implementation.
//!
//! This module provides a naive TCP stack implementation designed for simple,
//! reliable network environments. It handles basic TCP operations while making
//! significant simplifying assumptions about the underlying transport.
//!
//! # Features
//! - Basic TCP connection establishment (3-way handshake)
//! - Data transmission with PSH flag
//! - Connection teardown
//! - Optional PCAP packet capture
//! - Implements `AsyncRead` and `AsyncWrite` for Tokio compatibility
//!
//! # Limitations (unecessary for CDTunnel)
//! - No proper sequence number tracking
//! - No retransmission or congestion control
//! - Requires 100% reliable underlying transport
//! - Minimal error handling
//!
//! # Example
//! ```rust,no_run
//! use std::net::{IpAddr, Ipv4Addr};
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//! use idevice::tcp::{adapter::Adapter, stream::AdapterStream};
//! use idevice::ReadWrite;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a transport connection (this would be your actual transport)
//!     let transport = /* your transport implementing ReadWrite */;
//!
//!     // Create TCP adapter
//!     let host_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));
//!     let peer_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
//!     let mut adapter = Adapter::new(Box::new(transport), host_ip, peer_ip);
//!
//!     // Optional: enable packet capture
//!     adapter.pcap("capture.pcap").await?;
//!
//!     // Connect to remote server
//!     let stream = AdapterStream::new(&mut adapter, 80).await?;
//!
//!     // Send HTTP request
//!     stream.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await?;
//!     stream.flush().await?;
//!
//!     // Read response
//!     let mut buf = vec![0; 1024];
//!     let n = stream.read(&mut buf).await?;
//!     println!("Received: {}", String::from_utf8_lossy(&buf[..n]));
//!
//!     // Close connection
//!     stream.close().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Warning
//! This implementation makes significant simplifications and should not be used
//! with unreliable network transports.

use std::{collections::HashMap, io::ErrorKind, net::IpAddr, path::Path, sync::Arc};

use log::{debug, trace, warn};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};

use crate::{ReadWrite, tcp::packets::IpParseError};

use super::packets::{Ipv4Packet, Ipv6Packet, ProtocolNumber, TcpFlags, TcpPacket};

#[derive(Debug, Clone)]
struct ConnectionState {
    seq: u32,
    ack: u32,
    host_port: u16,
    peer_port: u16,
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,
    status: ConnectionStatus,
    peer_seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ConnectionStatus {
    WaitingForSyn,
    Connected,
    Error(ErrorKind),
}

impl ConnectionState {
    fn new(host_port: u16, peer_port: u16) -> Self {
        Self {
            seq: rand::random(),
            ack: 0,
            host_port,
            peer_port,
            read_buffer: Vec::new(),
            write_buffer: Vec::new(),
            status: ConnectionStatus::WaitingForSyn,
            peer_seq: 0,
        }
    }
}

/// A simplified TCP network stack implementation.
///
/// This is an extremely naive, limited, and dangerous TCP stack implementation.
/// Key limitations:
/// - ACKs aren't properly tracked and are silently ignored
/// - Should only be used when the underlying transport is 100% reliable
#[derive(Debug)]
pub struct Adapter {
    /// The underlying transport connection
    peer: Box<dyn ReadWrite>,
    /// The local IP address
    host_ip: IpAddr,
    /// The remote peer's IP address
    peer_ip: IpAddr,

    /// The states of the connections
    states: HashMap<u16, ConnectionState>, // host port by state
    dropped: Vec<u16>,
    read_buf: [u8; 4096],
    bytes_in_buf: usize,

    /// Optional PCAP file for packet logging
    pcap: Option<Arc<Mutex<tokio::fs::File>>>,
}

impl Adapter {
    /// Creates a new TCP adapter instance.
    ///
    /// # Arguments
    /// * `peer` - The underlying transport connection implementing `ReadWrite`
    /// * `host_ip` - The local IP address to use
    /// * `peer_ip` - The remote IP address to connect to
    ///
    /// # Returns
    /// A new `Adapter` instance
    pub fn new(peer: Box<dyn ReadWrite>, host_ip: IpAddr, peer_ip: IpAddr) -> Self {
        Self {
            peer,
            host_ip,
            peer_ip,
            states: HashMap::new(),
            dropped: Vec::new(),
            read_buf: [0u8; 4096],
            bytes_in_buf: 0,
            pcap: None,
        }
    }

    /// Wraps this handle in a new thread.
    /// Streams from this handle will be thread safe, with data sent through channels.
    /// The handle supports the trait for RSD provider.
    pub fn to_async_handle(self) -> super::handle::AdapterHandle {
        super::handle::AdapterHandle::new(self)
    }

    /// Initiates a TCP connection to the specified port.
    ///
    /// # Arguments
    /// * `port` - The remote port number to connect to
    ///
    /// # Returns
    /// * `Ok(u16)` the chosen host port if successful
    /// * `Err(std::io::Error)` if connection failed
    ///
    /// # Errors
    /// * Returns `InvalidData` if the SYN-ACK response is invalid
    /// * Returns other IO errors if underlying transport fails
    pub(crate) async fn connect(&mut self, port: u16) -> Result<u16, std::io::Error> {
        let host_port = loop {
            let host_port: u16 = rand::random();
            if self.states.contains_key(&host_port) {
                continue;
            } else {
                break host_port;
            }
        };
        let state = ConnectionState::new(host_port, port);

        // Create the TCP packet
        let tcp_packet = TcpPacket::create(
            self.host_ip,
            self.peer_ip,
            state.host_port,
            state.peer_port,
            state.seq,
            state.ack,
            TcpFlags {
                syn: true,
                ..Default::default()
            },
            u16::MAX - 1,
            &[],
        );
        let ip_packet = self.ip_wrap(&tcp_packet);
        self.peer.write_all(&ip_packet).await?;
        self.log_packet(&ip_packet)?;

        // Wait for the syn ack
        self.states.insert(host_port, state);
        let start_time = std::time::Instant::now();
        loop {
            self.process_tcp_packet().await?;
            if let Some(s) = self.states.get(&host_port) {
                match s.status {
                    ConnectionStatus::Connected => {
                        break;
                    }
                    ConnectionStatus::Error(e) => {
                        return Err(std::io::Error::new(e, "failed to connect"));
                    }
                    ConnectionStatus::WaitingForSyn => {
                        if start_time.elapsed() > std::time::Duration::from_secs(5) {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "didn't syn in time",
                            ));
                        }
                        continue;
                    }
                }
            }
        }

        Ok(host_port)
    }

    /// Enables packet capture to a PCAP file.
    ///
    /// # Arguments
    /// * `path` - The filesystem path to write the PCAP data to
    ///
    /// # Returns
    /// * `Ok(())` if PCAP file was successfully created
    /// * `Err(std::io::Error)` if file creation failed
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

    fn log_packet(&self, packet: &[u8]) -> Result<(), std::io::Error> {
        if let Some(file) = &self.pcap {
            super::log_packet(file, packet);
        }
        Ok(())
    }

    /// Closes the TCP connection.
    ///
    /// # Returns
    /// * `Ok(())` if connection was closed cleanly
    /// * `Err(std::io::Error)` if closing failed
    ///
    /// # Errors
    /// * Returns IO errors if underlying transport fails during close
    pub(crate) async fn close(&mut self, host_port: u16) -> Result<(), std::io::Error> {
        if let Some(state) = self.states.remove(&host_port) {
            let tcp_packet = TcpPacket::create(
                self.host_ip,
                self.peer_ip,
                state.host_port,
                state.peer_port,
                state.seq,
                state.ack,
                TcpFlags {
                    fin: true,
                    ack: true,
                    ..Default::default()
                },
                u16::MAX - 1,
                &[],
            );
            let ip_packet = self.ip_wrap(&tcp_packet);
            self.peer.write_all(&ip_packet).await?;
            self.log_packet(&ip_packet)?;

            Ok(())
        } else {
            Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ))
        }
    }

    async fn ack(&mut self, host_port: u16) -> Result<(), std::io::Error> {
        if let Some(state) = self.states.get_mut(&host_port) {
            let tcp_packet = TcpPacket::create(
                self.host_ip,
                self.peer_ip,
                state.host_port,
                state.peer_port,
                state.seq,
                state.ack,
                TcpFlags {
                    ack: true,
                    ..Default::default()
                },
                u16::MAX - 1,
                &[],
            );
            let ip_packet = self.ip_wrap(&tcp_packet);
            self.peer.write_all(&ip_packet).await?;
            self.log_packet(&ip_packet)?;

            Ok(())
        } else {
            Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ))
        }
    }

    /// Sends a TCP packet with PSH flag set (pushing data).
    ///
    /// # Arguments
    /// * `data` - The payload data to send
    ///
    /// # Returns
    /// * `Ok(())` if data was sent successfully
    /// * `Err(std::io::Error)` if sending failed
    ///
    /// # Errors
    /// * Returns IO errors if underlying transport fails
    async fn psh(&mut self, data: &[u8], host_port: u16) -> Result<(), std::io::Error> {
        let data_len = if let Some(state) = self.states.get(&host_port) {
            // Check to make sure we haven't closed since last operation
            if let ConnectionStatus::Error(e) = state.status {
                return Err(std::io::Error::new(e, "socket error"));
            }
            trace!("pshing {} bytes", data.len());
            let tcp_packet = TcpPacket::create(
                self.host_ip,
                self.peer_ip,
                state.host_port,
                state.peer_port,
                state.seq,
                state.ack,
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
            self.log_packet(&ip_packet)?;
            data.len() as u32
        } else {
            return Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ));
        };

        // We have to re-borrow, since we're mutating state
        if let Some(state) = self.states.get_mut(&host_port) {
            state.seq = state.seq.wrapping_add(data_len);
        }

        Ok(())
    }

    pub(crate) fn connection_drop(&mut self, host_port: u16) {
        self.dropped.push(host_port);
    }

    /// Flushes the packets
    pub(crate) async fn write_buffer_flush(&mut self) -> Result<(), std::io::Error> {
        for (_, state) in self.states.clone() {
            let writer_buffer = state.write_buffer.clone();
            if writer_buffer.is_empty() {
                continue;
            }

            self.psh(&writer_buffer, state.host_port).await.ok(); // don't care

            // we have to borrow mutably after self.psh
            if let Some(state) = self.states.get_mut(&state.host_port) {
                state.write_buffer.clear();
            }
        }

        // Since we have extra clocks and we haven't been cancelled by the runtime, let's reap the
        // dropped connections
        for d in self.dropped.clone() {
            if let Some(state) = self.states.remove(&d) {
                self.close(state.host_port).await.ok();
            }
        }
        // We can't clear until it's all done, since we can get cancelled by the runtime at any
        // point.
        self.dropped.clear();

        Ok(())
    }

    pub(crate) fn queue_send(
        &mut self,
        payload: &[u8],
        host_port: u16,
    ) -> Result<(), std::io::Error> {
        if let Some(state) = self.states.get_mut(&host_port) {
            state.write_buffer.extend_from_slice(payload);
        } else {
            return Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ));
        }
        Ok(())
    }

    pub(crate) fn uncache(
        &mut self,
        to_copy: usize,
        host_port: u16,
    ) -> Result<Vec<u8>, std::io::Error> {
        if let Some(state) = self.states.get_mut(&host_port) {
            let to_copy = if to_copy > state.read_buffer.len() {
                state.read_buffer.len()
            } else {
                to_copy
            };

            let res = state.read_buffer[..to_copy].to_vec();
            state.read_buffer = state.read_buffer[to_copy..].to_vec();
            Ok(res)
        } else {
            Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ))
        }
    }

    pub(crate) fn uncache_all(&mut self, host_port: u16) -> Result<Vec<u8>, std::io::Error> {
        if let Some(state) = self.states.get_mut(&host_port) {
            let res = state.read_buffer[..].to_vec();
            state.read_buffer.clear();
            Ok(res)
        } else {
            Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ))
        }
    }

    pub(crate) fn cache_read(
        &mut self,
        payload: &[u8],
        host_port: u16,
    ) -> Result<(), std::io::Error> {
        if let Some(state) = self.states.get_mut(&host_port) {
            state.read_buffer.extend_from_slice(payload);
            Ok(())
        } else {
            Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ))
        }
    }

    pub(crate) fn get_status(&self, host_port: u16) -> Result<ConnectionStatus, std::io::Error> {
        if let Some(state) = self.states.get(&host_port) {
            Ok(state.status.clone())
        } else {
            Err(std::io::Error::new(
                ErrorKind::NotConnected,
                "not connected",
            ))
        }
    }

    /// Receives data from the connection.
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` containing received data
    /// * `Err(std::io::Error)` if receiving failed
    ///
    /// # Errors
    /// * Returns `ConnectionReset` if connection was reset or closed
    /// * Returns other IO errors if underlying transport fails
    pub(crate) async fn recv(&mut self, host_port: u16) -> Result<Vec<u8>, std::io::Error> {
        loop {
            // Check to see if we already have some cached
            if let Some(state) = self.states.get_mut(&host_port) {
                if !state.read_buffer.is_empty() {
                    let res = state.read_buffer.clone();
                    state.read_buffer = Vec::new();
                    return Ok(res);
                }
                if let ConnectionStatus::Error(e) = state.status {
                    return Err(std::io::Error::new(e, "socket io error"));
                }
            } else {
                return Err(std::io::Error::new(
                    ErrorKind::NotConnected,
                    "not connected",
                ));
            }

            self.process_tcp_packet().await?;
        }
    }

    /// Reads a packet and returns the payload
    async fn read_ip_packet(&mut self) -> Result<Vec<u8>, std::io::Error> {
        self.write_buffer_flush().await?;
        Ok(loop {
            // try the data we already have
            match Ipv6Packet::parse(&self.read_buf[..self.bytes_in_buf], &self.pcap) {
                IpParseError::Ok {
                    packet,
                    bytes_consumed,
                } => {
                    // And remove it from the buffer by shifting the remaining bytes
                    self.read_buf
                        .copy_within(bytes_consumed..self.bytes_in_buf, 0);
                    self.bytes_in_buf -= bytes_consumed;
                    break packet.payload;
                }
                IpParseError::NotEnough => {
                    // Buffer doesn't have a full packet, wait for the next read
                }
                IpParseError::Invalid => {
                    // Corrupted data, close the connection
                    return Err(std::io::Error::new(
                        ErrorKind::InvalidData,
                        "invalid IPv6 parse",
                    ));
                }
            }
            // go get  more
            let s = self
                .peer
                .read(&mut self.read_buf[self.bytes_in_buf..])
                .await?;

            self.bytes_in_buf += s;
        })
    }

    pub(crate) async fn process_tcp_packet(&mut self) -> Result<(), std::io::Error> {
        tokio::select! {
            ip_packet = self.read_ip_packet() => {
                let ip_packet = ip_packet?;
                self.process_tcp_packet_from_payload(&ip_packet).await
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                Ok(())
            }
        }
    }

    pub(crate) async fn process_tcp_packet_from_payload(
        &mut self,
        payload: &[u8],
    ) -> Result<(), std::io::Error> {
        let res = TcpPacket::parse(payload)?;
        let mut ack_me = None;

        if let Some(state) = self.states.get(&res.destination_port) {
            // A keep-alive probe: ACK set, no payload, and seq == RCV.NXT - 1
            let is_keepalive = res.flags.ack
                && res.payload.is_empty()
                && res.sequence_number.wrapping_add(1) == state.ack;

            if is_keepalive {
                // Don't update any seq/ack state; just ACK what we already expect.
                debug!("responding to keep-alive probe");
                let port = res.destination_port;
                self.ack(port).await?;
                return Ok(());
            }
        }

        if let Some(state) = self.states.get_mut(&res.destination_port) {
            if state.peer_seq > res.sequence_number {
                // ignore retransmission
                return Ok(());
            }

            state.peer_seq = res.sequence_number + res.payload.len() as u32;
            state.ack = res.sequence_number
                + if res.payload.is_empty() && state.status != ConnectionStatus::Connected {
                    1
                } else {
                    res.payload.len() as u32
                };
            if res.flags.psh || !res.payload.is_empty() {
                ack_me = Some(res.destination_port);
                state.read_buffer.extend(res.payload);
            }
            if res.flags.rst {
                warn!("stream rst");
                state.status = ConnectionStatus::Error(ErrorKind::ConnectionReset);
            }
            if res.flags.fin {
                ack_me = Some(res.destination_port);
                state.status = ConnectionStatus::Error(ErrorKind::UnexpectedEof);
            }
            if res.flags.syn && res.flags.ack {
                ack_me = Some(res.destination_port);
                state.seq = state.seq.wrapping_add(1);
                state.status = ConnectionStatus::Connected;
            }
        }

        // we have to ack outside of the mutable state borrow
        if let Some(a) = ack_me {
            self.ack(a).await?;
        }
        Ok(())
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
