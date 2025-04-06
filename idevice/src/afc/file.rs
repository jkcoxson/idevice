// Jackson Coxson

use crate::IdeviceError;

use super::{
    opcode::AfcOpcode,
    packet::{AfcPacket, AfcPacketHeader},
};

const MAX_TRANSFER: u64 = 64 * 1024; // this is what go-ios uses

pub struct FileDescriptor<'a> {
    pub(crate) client: &'a mut super::AfcClient,
    pub(crate) fd: u64,
    pub(crate) path: String,
}

impl FileDescriptor<'_> {
    pub async fn close(self) -> Result<(), IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: super::MAGIC,
            entire_len: header_len, // it's the same since the payload is empty for this
            header_payload_len: header_len,
            packet_num: self.client.package_number,
            operation: AfcOpcode::FileClose,
        };
        self.client.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload: Vec::new(),
        };

        self.client.send(packet).await?;
        self.client.read().await?;
        Ok(())
    }

    pub async fn read(&mut self) -> Result<Vec<u8>, IdeviceError> {
        // Get the file size first
        let mut bytes_left = self.client.get_file_info(&self.path).await?.size;
        let mut collected_bytes = Vec::with_capacity(bytes_left);

        while bytes_left > 0 {
            let mut header_payload = self.fd.to_le_bytes().to_vec();
            header_payload.extend_from_slice(&MAX_TRANSFER.to_le_bytes());
            let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

            let header = AfcPacketHeader {
                magic: super::MAGIC,
                entire_len: header_len, // it's the same since the payload is empty for this
                header_payload_len: header_len,
                packet_num: self.client.package_number,
                operation: AfcOpcode::Read,
            };
            self.client.package_number += 1;

            let packet = AfcPacket {
                header,
                header_payload,
                payload: Vec::new(),
            };

            self.client.send(packet).await?;
            let res = self.client.read().await?;
            bytes_left -= res.payload.len();
            collected_bytes.extend(res.payload);
        }

        Ok(collected_bytes)
    }
}
