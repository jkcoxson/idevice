//! CoreDeviceProxy and CDTunnelPacket utilities for interacting with
//! iOS's CoreDeviceProxy service. This service starts an L3 (TUN) tunnel
//! for "trusted" services introduced in iOS 17.
//!
//! This module handles the construction and parsing of `CDTunnelPacket` messages
//! and manages a handshake and data tunnel to the CoreDeviceProxy daemon on iOS devices.
//!
//! # Overview
//! - `CDTunnelPacket` is used to parse and serialize packets sent over the CoreDeviceProxy channel.
//! - `CoreDeviceProxy` is a service client that initializes the tunnel, handles handshakes,
//!   and optionally supports creating a software-based TCP/IP tunnel (behind a feature flag).
//!
//! # Features
//! - `tunnel_tcp_stack`: Enables software TCP/IP tunnel creation using a virtual adapter. See the tcp moduel.

use crate::{Idevice, IdeviceError, IdeviceService, obf};

use byteorder::{BigEndian, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// A representation of a CDTunnel packet used in the CoreDeviceProxy protocol.
#[derive(Debug, PartialEq)]
pub struct CDTunnelPacket {
    /// The body of the packet, typically JSON-encoded data.
    body: Vec<u8>,
}

impl CDTunnelPacket {
    const MAGIC: &'static [u8] = b"CDTunnel";

    /// Parses a byte slice into a `CDTunnelPacket`.
    ///
    /// # Arguments
    ///
    /// * `input` - A byte slice containing the raw packet data.
    ///
    /// # Returns
    ///
    /// * `Ok(CDTunnelPacket)` if the input is a valid packet.
    /// * `Err(IdeviceError)` if parsing fails due to invalid magic, length, or size.
    pub fn parse(input: &[u8]) -> Result<Self, IdeviceError> {
        if input.len() < Self::MAGIC.len() + 2 {
            return Err(IdeviceError::CdtunnelPacketTooShort);
        }

        if &input[0..Self::MAGIC.len()] != Self::MAGIC {
            return Err(IdeviceError::CdtunnelPacketInvalidMagic);
        }

        let length_offset = Self::MAGIC.len();
        let body_length =
            u16::from_be_bytes([input[length_offset], input[length_offset + 1]]) as usize;

        if input.len() < length_offset + 2 + body_length {
            return Err(IdeviceError::PacketSizeMismatch);
        }

        let body_start = length_offset + 2;
        let body = input[body_start..body_start + body_length].to_vec();

        Ok(Self { body })
    }

    /// Serializes the `CDTunnelPacket` into a byte vector.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` containing the serialized packet.
    /// * `Err(io::Error)` if writing to the output buffer fails.
    pub fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();

        output.write_all(Self::MAGIC)?;
        output.write_u16::<BigEndian>(self.body.len() as u16)?;
        output.write_all(&self.body)?;

        Ok(output)
    }
}

/// A high-level client for the `com.apple.internal.devicecompute.CoreDeviceProxy` service.
///
/// Handles session negotiation, handshake, and tunnel communication.
pub struct CoreDeviceProxy {
    /// The underlying idevice connection used for communication.
    pub idevice: Idevice,
    /// The handshake response received during initialization.
    pub handshake: HandshakeResponse,
    /// The maximum transmission unit used for reading.
    pub mtu: u32,
}

impl IdeviceService for CoreDeviceProxy {
    /// Returns the name of the service used for launching the CoreDeviceProxy.
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.internal.devicecompute.CoreDeviceProxy")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        Self::new(idevice).await
    }
}

/// Request sent to initiate the handshake with the CoreDeviceProxy.
#[derive(Serialize)]
struct HandshakeRequest {
    #[serde(rename = "type")]
    packet_type: String,
    mtu: u32,
}

/// Parameters returned as part of the handshake response from the proxy server.
#[derive(Debug, Serialize, Deserialize)]
pub struct ClientParameters {
    /// The MTU (maximum transmission unit) for the connection.
    pub mtu: u16,
    /// The IP address assigned to the client.
    pub address: String,
    /// The subnet mask for the tunnel.
    pub netmask: String,
}

/// Handshake response structure received from the CoreDeviceProxy.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeResponse {
    #[serde(rename = "clientParameters")]
    pub client_parameters: ClientParameters,
    #[serde(rename = "serverAddress")]
    pub server_address: String,
    #[serde(rename = "type")]
    pub response_type: String,
    #[serde(rename = "serverRSDPort")]
    pub server_rsd_port: u16,
}

impl CoreDeviceProxy {
    const DEFAULT_MTU: u32 = 16000;

    /// Constructs a new `CoreDeviceProxy` by performing a handshake on the given `Idevice`.
    ///
    /// # Arguments
    ///
    /// * `idevice` - The connected `Idevice` socket.
    ///
    /// # Returns
    ///
    /// * `Ok(CoreDeviceProxy)` on successful handshake.
    /// * `Err(IdeviceError)` if the handshake fails.
    pub async fn new(mut idevice: Idevice) -> Result<Self, IdeviceError> {
        let req = HandshakeRequest {
            packet_type: "clientHandshakeRequest".to_string(),
            mtu: Self::DEFAULT_MTU,
        };

        let req = CDTunnelPacket::serialize(&CDTunnelPacket {
            body: serde_json::to_vec(&req)?,
        })?;

        idevice.send_raw(&req).await?;
        let recv = idevice.read_raw(CDTunnelPacket::MAGIC.len() + 2).await?;

        if recv.len() < CDTunnelPacket::MAGIC.len() + 2 {
            return Err(IdeviceError::CdtunnelPacketTooShort);
        }

        let len = u16::from_be_bytes([
            recv[CDTunnelPacket::MAGIC.len()],
            recv[CDTunnelPacket::MAGIC.len() + 1],
        ]) as usize;

        let recv = idevice.read_raw(len).await?;
        let res = serde_json::from_slice::<HandshakeResponse>(&recv)?;

        Ok(Self {
            idevice,
            handshake: res,
            mtu: Self::DEFAULT_MTU,
        })
    }

    /// Sends a raw data packet through the tunnel.
    ///
    /// # Arguments
    ///
    /// * `data` - The raw bytes to send.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the data is successfully sent.
    /// * `Err(IdeviceError)` if sending fails.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), IdeviceError> {
        self.idevice.send_raw(data).await?;
        Ok(())
    }

    /// Receives up to `mtu` bytes from the tunnel.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` containing the received data.
    /// * `Err(IdeviceError)` if reading fails.
    pub async fn recv(&mut self) -> Result<Vec<u8>, IdeviceError> {
        self.idevice.read_any(self.mtu).await
    }

    /// Creates a software-based TCP tunnel adapter, if the `tunnel_tcp_stack` feature is enabled.
    ///
    /// # Returns
    ///
    /// * `Ok(Adapter)` for the software TCP stack.
    /// * `Err(IdeviceError)` if IP parsing or socket extraction fails.
    #[cfg(feature = "tunnel_tcp_stack")]
    pub fn create_software_tunnel(self) -> Result<crate::tcp::adapter::Adapter, IdeviceError> {
        let our_ip = self
            .handshake
            .client_parameters
            .address
            .parse::<std::net::IpAddr>()?;
        let their_ip = self.handshake.server_address.parse::<std::net::IpAddr>()?;
        Ok(crate::tcp::adapter::Adapter::new(
            Box::new(self.idevice.socket.unwrap()),
            our_ip,
            their_ip,
        ))
    }
}
