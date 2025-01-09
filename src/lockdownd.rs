// Jackson Coxson
// Abstractions for lockdownd

pub const LOCKDOWND_PORT: u16 = 62078;

use log::error;
use serde::{Deserialize, Serialize};

use crate::{pairing_file, Idevice, IdeviceError};

pub struct LockdowndClient {
    pub idevice: crate::Idevice,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LockdowndRequest {
    label: String,
    key: Option<String>,
    request: String,
}

impl LockdowndClient {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }
    pub fn get_value(&mut self, value: impl Into<String>) -> Result<String, IdeviceError> {
        let req = LockdowndRequest {
            label: self.idevice.label.clone(),
            key: Some(value.into()),
            request: "GetValue".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.idevice.send_plist(message)?;
        let message: plist::Dictionary = self.idevice.read_plist()?;
        match message.get("Value") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub fn get_all_values(&mut self) -> Result<plist::Dictionary, IdeviceError> {
        let req = LockdowndRequest {
            label: self.idevice.label.clone(),
            key: None,
            request: "GetValue".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.idevice.send_plist(message)?;
        let message: plist::Dictionary = self.idevice.read_plist()?;
        match message.get("Value") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Starts a TLS session with the client
    pub fn start_session(
        &mut self,
        pairing_file: pairing_file::PairingFile,
    ) -> Result<(), IdeviceError> {
        if self.idevice.socket.is_none() {
            return Err(IdeviceError::NoEstablishedConnection);
        }

        let mut request = plist::Dictionary::new();
        request.insert(
            "Label".to_string(),
            plist::Value::String(self.idevice.label.clone()),
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

        self.idevice.send_plist(plist::Value::Dictionary(request))?;

        let response = self.idevice.read_plist()?;
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

        self.idevice.start_session(pairing_file)?;
        Ok(())
    }

    /// Asks lockdownd to pretty please start a service for us
    /// # Arguments
    /// `identifier` - The identifier for the service you want to start
    /// # Returns
    /// The port number and whether to enable SSL on success, `IdeviceError` on failure
    pub fn start_service(
        &mut self,
        identifier: impl Into<String>,
    ) -> Result<(u16, bool), IdeviceError> {
        let identifier = identifier.into();
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "StartService".into());
        req.insert("Service".into(), identifier.into());
        self.idevice.send_plist(plist::Value::Dictionary(req))?;
        let response = self.idevice.read_plist()?;
        println!("{response:?}");
        match response.get("EnableServiceSSL") {
            Some(plist::Value::Boolean(ssl)) => match response.get("Port") {
                Some(plist::Value::Integer(port)) => {
                    if let Some(port) = port.as_unsigned() {
                        Ok((port as u16, *ssl))
                    } else {
                        error!("Port isn't an unsiged integer!");
                        Err(IdeviceError::UnexpectedResponse)
                    }
                }
                _ => {
                    error!("Response didn't contain an integer port");
                    Err(IdeviceError::UnexpectedResponse)
                }
            },
            _ => {
                error!("Response didn't contain EnableServiceSSL bool!");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}

impl From<Idevice> for LockdowndClient {
    fn from(value: Idevice) -> Self {
        Self::new(value)
    }
}
