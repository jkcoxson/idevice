// jkcoxson

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::muxer::TAG;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PairingFile {
    pair_record_data: ByteBuf,
}

pub async fn fetch(
    prog_name: impl Into<String>,
    udid: impl Into<String>,
) -> Result<Vec<u8>, std::io::Error> {
    let to_send = PairFileRequest {
        client_version_string: "idevice-rs v0.1.0".to_string(),
        message_type: "ReadPairRecord".to_string(),
        prog_name: prog_name.into(),
        k_lib_usbmux_version: crate::muxer::USBMUX_VERSION,
        pair_record_id: udid.into(),
    };

    let to_send = crate::connection::plist_to_binary(to_send)?;

    // Append the packet header to the beginning of the packet
    let version = (1 as u32).to_le_bytes();
    let message = (8 as u32).to_le_bytes();

    let tag = *TAG.lock().await;
    *TAG.lock().await += 1;
    let tag = tag.to_le_bytes();

    let mut buf = Vec::new();
    buf.extend_from_slice(&version);
    buf.extend_from_slice(&message);
    buf.extend_from_slice(&tag);
    buf.extend_from_slice(&to_send);

    let mut connection = crate::muxer::connect().await?;
    connection.write(&buf).await?;

    let buf = connection.read().await?;
    let pair_file: PairingFile = crate::connection::binary_to_plist(&buf[12..].to_vec())?;

    Ok(pair_file.pair_record_data.to_vec())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PairFileRequest {
    pub client_version_string: String,
    pub message_type: String,
    pub prog_name: String,
    #[serde(rename = "kLibUSBMuxVersion")]
    pub k_lib_usbmux_version: u8,
    #[serde(rename = "PairRecordID")]
    pub pair_record_id: String,
}
