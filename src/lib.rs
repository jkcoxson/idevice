// Jackson Coxson

const LOCKDOWND_PORT: u16 = 62078;

mod pairing_file;

use log::debug;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use serde::{Deserialize, Serialize};
use std::io::{self, BufWriter, Read, Write};
use thiserror::Error;

trait ReadWrite: Read + Write + std::fmt::Debug {}
impl<T: Read + Write + std::fmt::Debug> ReadWrite for T {}

pub struct LockdowndClient {
    socket: Option<Box<dyn ReadWrite>>, // in a box for now to use the ReadWrite trait for further uses
    label: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LockdowndRequest {
    label: String,
    key: Option<String>,
    request: String,
}

impl LockdowndClient {
    pub fn get_type(&mut self) -> Result<String, IdeviceError> {
        let req = LockdowndRequest {
            label: self.label.clone(),
            key: None,
            request: "QueryType".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.send_plist(message)?;
        let message: plist::Dictionary = self.read_plist()?;
        match message.get("Type") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub fn get_value(&mut self, value: impl Into<String>) -> Result<String, IdeviceError> {
        let req = LockdowndRequest {
            label: self.label.clone(),
            key: Some(value.into()),
            request: "GetValue".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.send_plist(message)?;
        let message: plist::Dictionary = self.read_plist()?;
        match message.get("Value") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub fn get_all_values(&mut self) -> Result<plist::Dictionary, IdeviceError> {
        let req = LockdowndRequest {
            label: self.label.clone(),
            key: None,
            request: "GetValue".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.send_plist(message)?;
        let message: plist::Dictionary = self.read_plist()?;
        match message.get("Value") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Sends a plist to the socket
    fn send_plist(&mut self, message: plist::Value) -> Result<(), IdeviceError> {
        if let Some(socket) = &mut self.socket {
            let buf = Vec::new();
            let mut writer = BufWriter::new(buf);
            message.to_writer_xml(&mut writer)?;
            let message = writer.into_inner().unwrap();
            let message = String::from_utf8(message)?;
            let len = message.len() as u32;
            socket.write_all(&len.to_be_bytes())?;
            socket.write_all(message.as_bytes())?;
            Ok(())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Read a plist from the socket
    fn read_plist(&mut self) -> Result<plist::Dictionary, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            debug!("Reading response size");
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf)?;
            let len = u32::from_be_bytes(buf);
            let mut buf = vec![0; len as usize];
            socket.read_exact(&mut buf)?;
            let res: plist::Dictionary = plist::from_bytes(&buf)?;
            debug!("Received plist: {res:#?}");

            if let Some(e) = res.get("Error") {
                let e: String = plist::from_value(e)?;
                if let Some(e) = IdeviceError::from_device_error_type(e.as_str()) {
                    return Err(e);
                } else {
                    return Err(IdeviceError::UnknownErrorType(e));
                }
            }
            Ok(res)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Starts a TLS session with the client
    pub fn start_session(
        &mut self,
        pairing_file: pairing_file::PairingFile,
    ) -> Result<(), IdeviceError> {
        if self.socket.is_none() {
            return Err(IdeviceError::NoEstablishedConnection);
        }

        let mut request = plist::Dictionary::new();
        request.insert(
            "Label".to_string(),
            plist::Value::String(self.label.clone()),
        );

        request.insert(
            "Request".to_string(),
            plist::Value::String("StartSession".to_string()),
        );
        request.insert(
            "HostID".to_string(),
            plist::Value::String(pairing_file.host_id.clone()),
        );
        request.insert(
            "SystemBUID".to_string(),
            plist::Value::String(pairing_file.system_buid.clone()),
        );

        self.send_plist(plist::Value::Dictionary(request))?;

        let response = self.read_plist()?;
        match response.get("EnableSessionSSL") {
            Some(plist::Value::Boolean(enable)) => {
                if !enable {
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            _ => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        }

        let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
        connector
            .set_certificate(&pairing_file.host_certificate)
            .unwrap();
        connector
            .set_private_key(&pairing_file.host_private_key)
            .unwrap();
        connector.set_verify(SslVerifyMode::empty());

        let connector = connector.build();
        let socket = self.socket.take().unwrap();
        let ssl_stream = connector.connect("ur mom", socket).unwrap();
        self.socket = Some(Box::new(ssl_stream));

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum IdeviceError {
    #[error("device socket io failed")]
    Socket(#[from] io::Error),
    #[error("io on plist")]
    Plist(#[from] plist::Error),
    #[error("can't convert bytes to utf8")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("unexpected response from device")]
    UnexpectedResponse,
    #[error("this request was prohibited")]
    GetProhibited,
    #[error("no established connection")]
    NoEstablishedConnection,
    #[error("unknown error `{0}` returned from device")]
    UnknownErrorType(String),
}

impl IdeviceError {
    fn from_device_error_type(e: &str) -> Option<Self> {
        match e {
            "GetProhibited" => Some(Self::GetProhibited),
            _ => None,
        }
    }
}
