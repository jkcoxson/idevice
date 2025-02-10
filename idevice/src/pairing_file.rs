// Jackson Coxson

use std::path::Path;

use log::warn;
use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};
use plist::Data;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct PairingFile {
    pub device_certificate: X509,
    pub host_private_key: PKey<Private>,
    pub host_certificate: X509,
    pub root_private_key: PKey<Private>,
    pub root_certificate: X509,
    pub system_buid: String,
    pub host_id: String,
    pub escrow_bag: Vec<u8>,
    pub wifi_mac_address: String,
    pub udid: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct RawPairingFile {
    device_certificate: Data,
    host_private_key: Data,
    host_certificate: Data,
    root_private_key: Data,
    root_certificate: Data,
    #[serde(rename = "SystemBUID")]
    system_buid: String,
    #[serde(rename = "HostID")]
    host_id: String,
    escrow_bag: Data,
    #[serde(rename = "WiFiMACAddress")]
    wifi_mac_address: String,
    #[serde(rename = "UDID")]
    udid: Option<String>,
}

impl PairingFile {
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self, crate::IdeviceError> {
        let f = std::fs::read(path)?;
        Self::from_bytes(&f)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::IdeviceError> {
        let r = match ::plist::from_bytes::<RawPairingFile>(bytes) {
            Ok(r) => r,
            Err(e) => {
                warn!("Unable to convert bytes to raw pairing file: {e:?}");
                return Err(crate::IdeviceError::UnexpectedResponse);
            }
        };

        match r.try_into() {
            Ok(r) => Ok(r),
            Err(e) => {
                warn!("Unable to convert raw pairing file into pairing file: {e:?}");
                Err(crate::IdeviceError::UnexpectedResponse)
            }
        }
    }

    pub fn from_value(v: &plist::Value) -> Result<Self, crate::IdeviceError> {
        let raw: RawPairingFile = plist::from_value(v)?;
        let p = raw.try_into()?;
        Ok(p)
    }

    pub fn serialize(self) -> Result<Vec<u8>, crate::IdeviceError> {
        let raw = RawPairingFile::try_from(self)?;

        let mut buf = Vec::new();
        plist::to_writer_xml(&mut buf, &raw)?;
        Ok(buf)
    }
}

impl TryFrom<RawPairingFile> for PairingFile {
    type Error = openssl::error::ErrorStack;

    fn try_from(value: RawPairingFile) -> Result<Self, Self::Error> {
        Ok(Self {
            device_certificate: X509::from_pem(&Into::<Vec<u8>>::into(value.device_certificate))?,
            host_private_key: PKey::private_key_from_pem(&Into::<Vec<u8>>::into(
                value.host_private_key,
            ))?,
            host_certificate: X509::from_pem(&Into::<Vec<u8>>::into(value.host_certificate))?,
            root_private_key: PKey::private_key_from_pem(&Into::<Vec<u8>>::into(
                value.root_private_key,
            ))?,
            root_certificate: X509::from_pem(&Into::<Vec<u8>>::into(value.root_certificate))?,
            system_buid: value.system_buid,
            host_id: value.host_id,
            escrow_bag: value.escrow_bag.into(),
            wifi_mac_address: value.wifi_mac_address,
            udid: value.udid,
        })
    }
}

impl TryFrom<PairingFile> for RawPairingFile {
    type Error = openssl::error::ErrorStack;

    fn try_from(value: PairingFile) -> Result<Self, Self::Error> {
        Ok(Self {
            device_certificate: Data::new(value.device_certificate.to_pem()?),
            host_private_key: Data::new(value.host_private_key.private_key_to_pem_pkcs8()?),
            host_certificate: Data::new(value.host_certificate.to_pem()?),
            root_private_key: Data::new(value.root_private_key.private_key_to_pem_pkcs8()?),
            root_certificate: Data::new(value.root_certificate.to_pem()?),
            system_buid: value.system_buid,
            host_id: value.host_id.clone(),
            escrow_bag: Data::new(value.escrow_bag),
            wifi_mac_address: value.wifi_mac_address,
            udid: value.udid,
        })
    }
}

#[test]
fn f1() {
    let f = std::fs::read("/var/lib/lockdown/test.plist").unwrap();

    println!("{}", String::from_utf8_lossy(&f));

    let input = PairingFile::from_bytes(&f).unwrap();
    let output = input.serialize().unwrap();
    println!("{}", String::from_utf8_lossy(&output));

    assert_eq!(f[..output.len()], output);
}
