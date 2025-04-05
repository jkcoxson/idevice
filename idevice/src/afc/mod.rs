// Jackson Coxson

use errors::AfcError;
use opcode::AfcOpcode;
use packet::{AfcPacket, AfcPacketHeader};

use crate::{lockdownd::LockdowndClient, Idevice, IdeviceError, IdeviceService};

pub mod errors;
pub mod opcode;
pub mod packet;

pub const MAGIC: u64 = 0x4141504c36414643;

pub struct AfcClient {
    pub idevice: Idevice,
    package_number: u64,
}

impl IdeviceService for AfcClient {
    fn service_name() -> &'static str {
        "com.apple.afc"
    }

    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let mut lockdown = LockdowndClient::connect(provider).await?;
        lockdown
            .start_session(&provider.get_pairing_file().await?)
            .await?;

        let (port, ssl) = lockdown.start_service(Self::service_name()).await?;

        let mut idevice = provider.connect(port).await?;
        if ssl {
            idevice
                .start_session(&provider.get_pairing_file().await?)
                .await?;
        }

        Ok(Self {
            idevice,
            package_number: 0,
        })
    }
}

impl AfcClient {
    pub fn new(idevice: Idevice) -> Self {
        Self {
            idevice,
            package_number: 0,
        }
    }

    pub async fn list(&mut self, path: impl Into<String>) -> Result<Vec<String>, IdeviceError> {
        let path = path.into();
        let header_payload = path.as_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.package_number,
            operation: AfcOpcode::ReadDir,
        };
        self.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload: Vec::new(),
        };

        self.send(packet).await?;
        let res = self.read().await?;

        let strings: Vec<String> = res
            .payload
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        Ok(strings)
    }

    pub async fn read(&mut self) -> Result<AfcPacket, IdeviceError> {
        let res = AfcPacket::read(&mut self.idevice).await?;
        if res.header.operation == AfcOpcode::Status {
            if res.header_payload.len() < 8 {
                log::error!("AFC returned error opcode, but not a code");
                return Err(IdeviceError::UnexpectedResponse);
            }
            let code = u64::from_le_bytes(res.header_payload[..8].try_into().unwrap());
            return Err(IdeviceError::Afc(AfcError::from(code)));
        }
        Ok(res)
    }

    pub async fn send(&mut self, packet: AfcPacket) -> Result<(), IdeviceError> {
        let packet = packet.serialize();
        self.idevice.send_raw(&packet).await?;
        Ok(())
    }
}
