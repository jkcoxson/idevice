//! iOS Device Pairing File Handling
//!
//! Provides functionality for reading, writing, and manipulating iOS device pairing files
//! which contain the cryptographic materials needed for secure communication with devices.

use std::path::Path;

use log::warn;
use plist::Data;
use rustls::pki_types::{pem::PemObject, CertificateDer};
use serde::{Deserialize, Serialize};

/// Represents a complete iOS device pairing record
///
/// Contains all cryptographic materials and identifiers needed for secure communication
/// with an iOS device, including certificates, private keys, and device identifiers.
#[derive(Clone, Debug)]
pub struct PairingFile {
    /// Device's certificate in DER format
    pub device_certificate: CertificateDer<'static>,
    /// Host's private key in DER format
    pub host_private_key: Vec<u8>,
    /// Host's certificate in DER format
    pub host_certificate: CertificateDer<'static>,
    /// Root CA's private key in DER format
    pub root_private_key: Vec<u8>,
    /// Root CA's certificate in DER format
    pub root_certificate: CertificateDer<'static>,
    /// System Build Unique Identifier
    pub system_buid: String,
    /// Host identifier
    pub host_id: String,
    /// Escrow bag allowing for access while locked
    pub escrow_bag: Vec<u8>,
    /// Device's WiFi MAC address
    pub wifi_mac_address: String,
    /// Device's Unique Device Identifier (optional)
    pub udid: Option<String>,
}

/// Internal representation of a pairing file for serialization/deserialization
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
    /// Reads a pairing file from disk
    ///
    /// # Arguments
    /// * `path` - Path to the pairing file (typically a .plist file)
    ///
    /// # Returns
    /// A parsed `PairingFile` on success
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - The file cannot be read
    /// - The contents are malformed
    /// - Cryptographic materials are invalid
    pub fn read_from_file(path: impl AsRef<Path>) -> Result<Self, crate::IdeviceError> {
        let f = std::fs::read(path)?;
        Self::from_bytes(&f)
    }

    /// Parses a pairing file from raw bytes
    ///
    /// # Arguments
    /// * `bytes` - Raw bytes of the pairing file (typically PLIST format)
    ///
    /// # Returns
    /// A parsed `PairingFile` on success
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - The data cannot be parsed as PLIST
    /// - Required fields are missing
    /// - Cryptographic materials are invalid
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

    /// Creates a pairing file from a plist value
    ///
    /// # Arguments
    /// * `v` - PLIST value containing pairing data
    ///
    /// # Returns
    /// A parsed `PairingFile` on success
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Required fields are missing
    /// - Cryptographic materials are invalid
    pub fn from_value(v: &plist::Value) -> Result<Self, crate::IdeviceError> {
        let raw: RawPairingFile = plist::from_value(v)?;
        let p = raw.try_into()?;
        Ok(p)
    }

    /// Serializes the pairing file to a PLIST-formatted byte vector
    ///
    /// # Returns
    /// A byte vector containing the serialized pairing file
    ///
    /// # Errors
    /// Returns `IdeviceError` if serialization fails
    pub fn serialize(self) -> Result<Vec<u8>, crate::IdeviceError> {
        let raw = RawPairingFile::from(self);

        let mut buf = Vec::new();
        plist::to_writer_xml(&mut buf, &raw)?;
        Ok(buf)
    }
}

impl TryFrom<RawPairingFile> for PairingFile {
    type Error = rustls::pki_types::pem::Error;

    /// Attempts to convert a raw pairing file into a structured pairing file
    ///
    /// Performs validation of cryptographic materials during conversion.
    fn try_from(value: RawPairingFile) -> Result<Self, Self::Error> {
        // Convert raw data into certificates and keys with proper PEM format
        let device_cert_data = Into::<Vec<u8>>::into(value.device_certificate);
        let host_private_key_data = Into::<Vec<u8>>::into(value.host_private_key);
        let host_cert_data = Into::<Vec<u8>>::into(value.host_certificate);
        let root_private_key_data = Into::<Vec<u8>>::into(value.root_private_key);
        let root_cert_data = Into::<Vec<u8>>::into(value.root_certificate);

        // Ensure device certificate has proper PEM headers
        let device_certificate_pem = ensure_pem_headers(&device_cert_data, "CERTIFICATE");
        
        // Ensure host certificate has proper PEM headers
        let host_certificate_pem = ensure_pem_headers(&host_cert_data, "CERTIFICATE");
        
        // Ensure root certificate has proper PEM headers
        let root_certificate_pem = ensure_pem_headers(&root_cert_data, "CERTIFICATE");
        
        Ok(Self {
            device_certificate: CertificateDer::from_pem_slice(&device_certificate_pem)?,
            host_private_key: host_private_key_data,
            host_certificate: CertificateDer::from_pem_slice(&host_certificate_pem)?,
            root_private_key: root_private_key_data,
            root_certificate: CertificateDer::from_pem_slice(&root_certificate_pem)?,
            system_buid: value.system_buid,
            host_id: value.host_id,
            escrow_bag: value.escrow_bag.into(),
            wifi_mac_address: value.wifi_mac_address,
            udid: value.udid,
        })
    }
}

impl From<PairingFile> for RawPairingFile {
    /// Converts a structured pairing file into a raw pairing file for serialization
    fn from(value: PairingFile) -> Self {
        // Ensure certificates include proper PEM format
        let device_cert_data = ensure_pem_headers(&value.device_certificate.to_vec(), "CERTIFICATE");
        let host_cert_data = ensure_pem_headers(&value.host_certificate.to_vec(), "CERTIFICATE");
        let root_cert_data = ensure_pem_headers(&value.root_certificate.to_vec(), "CERTIFICATE");
        
        // Ensure private keys include proper PEM format
        let host_private_key_data = ensure_pem_headers(&value.host_private_key, "PRIVATE KEY");
        let root_private_key_data = ensure_pem_headers(&value.root_private_key, "PRIVATE KEY");

        Self {
            device_certificate: Data::new(device_cert_data),
            host_private_key: Data::new(host_private_key_data),
            host_certificate: Data::new(host_cert_data),
            root_private_key: Data::new(root_private_key_data),
            root_certificate: Data::new(root_cert_data),
            system_buid: value.system_buid,
            host_id: value.host_id.clone(),
            escrow_bag: Data::new(value.escrow_bag),
            wifi_mac_address: value.wifi_mac_address,
            udid: value.udid,
        }
    }
}

/// Helper function to ensure data has proper PEM headers
/// If the data already has headers, it returns it as is
/// If not, it adds the appropriate BEGIN and END headers
fn ensure_pem_headers(data: &[u8], pem_type: &str) -> Vec<u8> {
    if is_pem_formatted(data) {
        return data.to_vec();
    }
    
    // If it's just base64 data, add PEM headers
    let mut result = Vec::new();
    
    // Add header
    let header = format!("-----BEGIN {}-----\n", pem_type);
    result.extend_from_slice(header.as_bytes());
    
    // Add base64 content with line breaks every 64 characters
    let base64_content = if is_base64(data) {
        // Clean up any existing whitespace/newlines
        let data_str = String::from_utf8_lossy(data);
        data_str.replace('\n', "").replace('\r', "").replace(' ', "").into_bytes()
    } else {
        base64::encode(data).into_bytes()
    };
    
    // Format base64 content with proper line breaks (64 chars per line)
    for (i, chunk) in base64_content.chunks(64).enumerate() {
        if i > 0 {
            result.push(b'\n');
        }
        result.extend_from_slice(chunk);
    }
    
    // Add a final newline before the footer
    result.push(b'\n');
    
    // Add footer
    let footer = format!("-----END {}-----", pem_type);
    result.extend_from_slice(footer.as_bytes());
    
    result
}

/// Check if data is already in PEM format
fn is_pem_formatted(data: &[u8]) -> bool {
    if let Ok(data_str) = std::str::from_utf8(data) {
        data_str.contains("-----BEGIN") && data_str.contains("-----END")
    } else {
        false
    }
}

/// Check if data is already base64 encoded
fn is_base64(data: &[u8]) -> bool {
    if let Ok(data_str) = std::str::from_utf8(data) {
        // Simple check to see if string contains only valid base64 characters
        data_str.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c.is_whitespace()
        })
    } else {
        false
    }
}

#[test]
fn test_pairing_file_roundtrip() {
    let f = std::fs::read("/var/lib/lockdown/test.plist").unwrap();

    println!("{}", String::from_utf8_lossy(&f));

    let input = PairingFile::from_bytes(&f).unwrap();
    let output = input.serialize().unwrap();
    println!("{}", String::from_utf8_lossy(&output));

    assert_eq!(f[..output.len()], output);
}

