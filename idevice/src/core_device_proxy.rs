// Jackson Coxson

use crate::{lockdownd::LockdowndClient, Idevice, IdeviceError, IdeviceService};

use byteorder::{BigEndian, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

#[derive(Debug, PartialEq)]
pub struct CDTunnelPacket {
    body: Vec<u8>,
}

impl CDTunnelPacket {
    const MAGIC: &'static [u8] = b"CDTunnel";

    /// Parses a byte slice into a `CDTunnelPacket`.
    pub fn parse(input: &[u8]) -> Result<Self, IdeviceError> {
        if input.len() < Self::MAGIC.len() + 2 {
            return Err(IdeviceError::CdtunnelPacketTooShort);
        }

        // Validate the magic bytes
        if &input[0..Self::MAGIC.len()] != Self::MAGIC {
            return Err(IdeviceError::CdtunnelPacketInvalidMagic);
        }

        // Parse the body length
        let length_offset = Self::MAGIC.len();
        let body_length =
            u16::from_be_bytes([input[length_offset], input[length_offset + 1]]) as usize;

        // Validate the body length
        if input.len() < length_offset + 2 + body_length {
            return Err(IdeviceError::PacketSizeMismatch);
        }

        // Extract the body
        let body_start = length_offset + 2;
        let body = input[body_start..body_start + body_length].to_vec();

        Ok(Self { body })
    }

    /// Serializes the `CDTunnelPacket` into a byte vector.
    pub fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();

        // Write the magic bytes
        output.write_all(Self::MAGIC)?;

        // Write the body length
        output.write_u16::<BigEndian>(self.body.len() as u16)?;

        // Write the body
        output.write_all(&self.body)?;

        Ok(output)
    }
}

pub struct CoreDeviceProxy {
    pub idevice: Idevice,
    pub mtu: u32,
}

impl IdeviceService for CoreDeviceProxy {
    fn service_name() -> &'static str {
        "com.apple.internal.devicecompute.CoreDeviceProxy"
    }

    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let mut lockdown = LockdowndClient::connect(provider).await?;
        let (port, ssl) = lockdown.start_service(Self::service_name()).await?;

        let mut idevice = provider.connect(port).await?;
        if ssl {
            idevice
                .start_session(&provider.get_pairing_file().await?)
                .await?;
        }

        Ok(Self::new(idevice))
    }
}

#[derive(Serialize)]
struct HandshakeRequest {
    #[serde(rename = "type")]
    packet_type: String,
    mtu: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientParameters {
    pub mtu: u16,
    pub address: String,
    pub netmask: String,
}

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

    pub fn new(idevice: Idevice) -> Self {
        Self {
            idevice,
            mtu: Self::DEFAULT_MTU,
        }
    }

    pub async fn establish_tunnel(&mut self) -> Result<HandshakeResponse, IdeviceError> {
        let req = HandshakeRequest {
            packet_type: "clientHandshakeRequest".to_string(),
            mtu: Self::DEFAULT_MTU,
        };

        let req = CDTunnelPacket::serialize(&CDTunnelPacket {
            body: serde_json::to_vec(&req)?,
        })?;

        self.idevice.send_raw(&req).await?;
        let recv = self
            .idevice
            .read_raw(CDTunnelPacket::MAGIC.len() + 2)
            .await?;

        if recv.len() < CDTunnelPacket::MAGIC.len() + 2 {
            return Err(IdeviceError::CdtunnelPacketTooShort);
        }

        let len = u16::from_be_bytes([
            recv[CDTunnelPacket::MAGIC.len()],
            recv[CDTunnelPacket::MAGIC.len() + 1],
        ]) as usize;

        let recv = self.idevice.read_raw(len).await?;
        let res = serde_json::from_slice::<HandshakeResponse>(&recv)?;

        Ok(res)
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<(), IdeviceError> {
        self.idevice.send_raw(data).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, IdeviceError> {
        self.idevice.read_any(self.mtu).await
    }
}
