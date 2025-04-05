// Jackson Coxson

use std::collections::HashMap;

use errors::AfcError;
use log::warn;
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

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub size: usize,
    pub blocks: usize,
    pub creation: chrono::NaiveDateTime,
    pub modified: chrono::NaiveDateTime,
    pub st_nlink: String,
    pub st_ifmt: String,
    pub st_link_target: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub model: String,
    pub total_bytes: usize,
    pub free_bytes: usize,
    pub block_size: usize,
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

    pub async fn list_dir(&mut self, path: impl Into<String>) -> Result<Vec<String>, IdeviceError> {
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

    pub async fn mk_dir(&mut self, path: impl Into<String>) -> Result<(), IdeviceError> {
        let path = path.into();
        let header_payload = path.as_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.package_number,
            operation: AfcOpcode::MakeDir,
        };
        self.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload: Vec::new(),
        };

        self.send(packet).await?;
        self.read().await?; // read a response to check for errors

        Ok(())
    }

    pub async fn get_file_info(
        &mut self,
        path: impl Into<String>,
    ) -> Result<FileInfo, IdeviceError> {
        let path = path.into();
        let header_payload = path.as_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.package_number,
            operation: AfcOpcode::GetFileInfo,
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

        let mut kvs: HashMap<String, String> = strings
            .chunks_exact(2)
            .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
            .collect();

        let size = kvs
            .remove("st_size")
            .and_then(|x| x.parse::<usize>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let blocks = kvs
            .remove("st_blocks")
            .and_then(|x| x.parse::<usize>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;

        let creation = kvs
            .remove("st_birthtime")
            .and_then(|x| x.parse::<i64>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let creation = chrono::DateTime::from_timestamp_nanos(creation).naive_local();

        let modified = kvs
            .remove("st_mtime")
            .and_then(|x| x.parse::<i64>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let modified = chrono::DateTime::from_timestamp_nanos(modified).naive_local();

        let st_nlink = kvs
            .remove("st_nlink")
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let st_ifmt = kvs
            .remove("st_ifmt")
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let st_link_target = kvs.remove("st_link_target");

        if !kvs.is_empty() {
            warn!("File info kvs not empty: {kvs:?}");
        }

        Ok(FileInfo {
            size,
            blocks,
            creation,
            modified,
            st_nlink,
            st_ifmt,
            st_link_target,
        })
    }

    pub async fn get_device_info(&mut self) -> Result<DeviceInfo, IdeviceError> {
        let header_len = AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.package_number,
            operation: AfcOpcode::GetDevInfo,
        };
        self.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload: Vec::new(),
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

        let mut kvs: HashMap<String, String> = strings
            .chunks_exact(2)
            .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
            .collect();

        let model = kvs
            .remove("Model")
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let total_bytes = kvs
            .remove("FSTotalBytes")
            .and_then(|x| x.parse::<usize>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let free_bytes = kvs
            .remove("FSFreeBytes")
            .and_then(|x| x.parse::<usize>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;
        let block_size = kvs
            .remove("FSBlockSize")
            .and_then(|x| x.parse::<usize>().ok())
            .ok_or(IdeviceError::AfcMissingAttribute)?;

        if !kvs.is_empty() {
            warn!("Device info kvs not empty: {kvs:?}");
        }

        Ok(DeviceInfo {
            model,
            total_bytes,
            free_bytes,
            block_size,
        })
    }

    pub async fn remove(&mut self, path: impl Into<String>) -> Result<(), IdeviceError> {
        let path = path.into();
        let header_payload = path.as_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.package_number,
            operation: AfcOpcode::RemovePath,
        };
        self.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload: Vec::new(),
        };

        self.send(packet).await?;
        self.read().await?; // read a response to check for errors

        Ok(())
    }

    pub async fn remove_all(&mut self, path: impl Into<String>) -> Result<(), IdeviceError> {
        let path = path.into();
        let header_payload = path.as_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.package_number,
            operation: AfcOpcode::RemovePathAndContents,
        };
        self.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload: Vec::new(),
        };

        self.send(packet).await?;
        self.read().await?; // read a response to check for errors

        Ok(())
    }

    pub async fn read(&mut self) -> Result<AfcPacket, IdeviceError> {
        let res = AfcPacket::read(&mut self.idevice).await?;
        if res.header.operation == AfcOpcode::Status {
            if res.header_payload.len() < 8 {
                log::error!("AFC returned error opcode, but not a code");
                return Err(IdeviceError::UnexpectedResponse);
            }
            let code = u64::from_le_bytes(res.header_payload[..8].try_into().unwrap());
            let e = AfcError::from(code);
            if e == AfcError::Success {
                return Ok(res);
            } else {
                return Err(IdeviceError::Afc(e));
            }
        }
        Ok(res)
    }

    pub async fn send(&mut self, packet: AfcPacket) -> Result<(), IdeviceError> {
        let packet = packet.serialize();
        self.idevice.send_raw(&packet).await?;
        Ok(())
    }
}
