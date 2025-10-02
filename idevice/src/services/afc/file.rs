// Jackson Coxson

use std::io::SeekFrom;

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
    /// Generic helper to send an AFC packet and read the response
    async fn send_packet(
        &mut self,
        opcode: AfcOpcode,
        header_payload: Vec<u8>,
        payload: Vec<u8>,
    ) -> Result<AfcPacket, IdeviceError> {
        let header_len = header_payload.len() as u64 + AfcPacketHeader::LEN;
        let header = AfcPacketHeader {
            magic: super::MAGIC,
            entire_len: header_len + payload.len() as u64,
            header_payload_len: header_len,
            packet_num: self.client.package_number,
            operation: opcode,
        };
        self.client.package_number += 1;

        let packet = AfcPacket {
            header,
            header_payload,
            payload,
        };

        self.client.send(packet).await?;
        self.client.read().await
    }

    /// Returns the current cursor position for the file
    pub async fn seek_tell(&mut self) -> Result<u64, IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();
        let res = self
            .send_packet(AfcOpcode::FileTell, header_payload, Vec::new())
            .await?;

        let cur_pos = res
            .header_payload
            .get(..8)
            .ok_or(IdeviceError::UnexpectedResponse)?
            .try_into()
            .map(u64::from_le_bytes)
            .map_err(|_| IdeviceError::UnexpectedResponse)?;

        Ok(cur_pos)
    }

    /// Moves the file cursor
    pub async fn seek(&mut self, pos: SeekFrom) -> Result<(), IdeviceError> {
        let (offset, whence) = match pos {
            SeekFrom::Start(off) => (off as i64, 0),
            SeekFrom::Current(off) => (off, 1),
            SeekFrom::End(off) => (off, 2),
        };

        let mut header_payload = Vec::new();
        header_payload.extend(self.fd.to_le_bytes());
        header_payload.extend((whence as u64).to_le_bytes());
        header_payload.extend(offset.to_le_bytes());

        self.send_packet(AfcOpcode::FileSeek, header_payload, Vec::new())
            .await?;

        Ok(())
    }

    /// Closes the file descriptor
    pub async fn close(mut self) -> Result<(), IdeviceError> {
        let header_payload = self.fd.to_le_bytes().to_vec();

        self.send_packet(AfcOpcode::FileClose, header_payload, Vec::new())
            .await?;
        Ok(())
    }

    /// Reads the entire contents of the file
    ///
    /// # Returns
    /// A vector containing the file's data
    pub async fn read(&mut self) -> Result<Vec<u8>, IdeviceError> {
        let seek_pos = self.seek_tell().await? as usize;
        let file_info = self.client.get_file_info(&self.path).await?;
        let mut bytes_left = file_info.size.saturating_sub(seek_pos);
        let mut collected_bytes = Vec::with_capacity(bytes_left);

        while bytes_left > 0 {
            let mut header_payload = self.fd.to_le_bytes().to_vec();
            header_payload.extend_from_slice(&MAX_TRANSFER.to_le_bytes());
            let res = self
                .send_packet(AfcOpcode::Read, header_payload, Vec::new())
                .await?;

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
        for chunk in bytes.chunks(MAX_TRANSFER as usize) {
            let header_payload = self.fd.to_le_bytes().to_vec();
            self.send_packet(AfcOpcode::Write, header_payload, chunk.to_vec())
                .await?;
        }
        Ok(())
    }
}
