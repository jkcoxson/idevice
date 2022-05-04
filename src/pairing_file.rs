// jkcoxson

use plist::Value;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::muxer::TAG;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PairingFile {
    device_certificate: ByteBuf,
    host_private_key: ByteBuf,
    host_certificate: ByteBuf,
    root_private_key: ByteBuf,
    root_certificate: ByteBuf,
    #[serde(rename = "HostID")]
    host_id: String,
    escrow_bag: ByteBuf,
    #[serde(rename = "WiFiMACAddress")]
    wifi_mac_address: String,
}

impl PairingFile {
    pub async fn fetch(
        prog_name: impl Into<String>,
        udid: impl Into<String>,
    ) -> Result<PairingFile, std::io::Error> {
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

        let upper_plist: Value = match plist::from_bytes(&buf[12..]) {
            Ok(v) => v,
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Unable to deserialize packet: {}", e),
                ));
            }
        };

        let upper_plist = match upper_plist.as_dictionary() {
            Some(v) => v,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Unable to deserialize packet: not a dictionary",
                ));
            }
        };

        let lower_plist: Value = match upper_plist.get("PairRecordData") {
            Some(v) => v.clone(),
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Unable to deserialize packet: no PairRecord",
                ));
            }
        };

        let lower_plist = match lower_plist.as_data() {
            Some(v) => v.to_vec(),
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Unable to deserialize packet: Not a data plist",
                ));
            }
        };

        Ok(crate::connection::binary_to_plist(&lower_plist)?)
    }
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
