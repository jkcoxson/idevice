// Jackson Coxson

use crate::IdeviceError;

use super::{
    opcode::AfcOpcode,
    packet::{AfcPacket, AfcPacketHeader},
};

/// Maximum transfer size for file operations (64KB)
const MAX_TRANSFER: u64 = 64 * 1024; // this is what go-ios uses

/// Handle for an open file on the device.
/// Call close before dropping
pub struct FileDescriptor<'a> {
    pub(crate) client: &'a mut super::AfcClient,
    pub(crate) fd: u64,
    pub(crate) path: String,
}

impl FileDescriptor<'_> {
    /// Closes the file descriptor
    pub async fn close(self) -> Result<(), IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

        let header = AfcPacketHeader {
            magic: super::MAGIC,
            entire_len: header_len,
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

    /// Reads the entire contents of the file
    ///
    /// # Returns
    /// A vector containing the file's data
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
                entire_len: header_len,
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

    /// Writes data to the file
    ///
    /// # Arguments
    /// * `bytes` - Data to write to the file
    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), IdeviceError> {
        let chunks = bytes.chunks(MAX_TRANSFER as usize);

        for chunk in chunks {
            let header_payload = self.fd.to_le_bytes().to_vec();
            let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;

            let header = AfcPacketHeader {
                magic: super::MAGIC,
                entire_len: header_len + chunk.len() as u64,
                header_payload_len: header_len,
                packet_num: self.client.package_number,
                operation: AfcOpcode::Write,
            };
            self.client.package_number += 1;

            let packet = AfcPacket {
                header,
                header_payload,
                payload: chunk.to_vec(),
            };

            self.client.send(packet).await?;
            self.client.read().await?;
        }
        Ok(())
    }
}
