//! iOS Lockdown Service Client
//!
//! Provides functionality for interacting with the lockdown service on iOS devices,
//! which is the primary service for device management and service discovery.

use log::error;
use plist::Value;
use serde::{Deserialize, Serialize};

use crate::{pairing_file, Idevice, IdeviceError, IdeviceService};

/// Client for interacting with the iOS lockdown service
///
/// This is the primary service for device management and provides:
/// - Access to device information and settings
/// - Service discovery and port allocation
/// - Session management and security
pub struct LockdownClient {
    /// The underlying device connection with established lockdown service
    pub idevice: crate::Idevice,
}

impl IdeviceService for LockdownClient {
    /// Returns the lockdown service name as registered with the device
    fn service_name() -> &'static str {
        "com.apple.mobile.lockdown"
    }

    /// Establishes a connection to the lockdown service
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `LockdownClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if connection fails
    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let idevice = provider.connect(Self::LOCKDOWND_PORT).await?;
        Ok(Self::new(idevice))
    }
}

/// Internal structure for lockdown protocol requests
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LockdownRequest {
    label: String,
    key: Option<String>,
    request: String,
}

impl LockdownClient {
    /// The default TCP port for the lockdown service
    pub const LOCKDOWND_PORT: u16 = 62078;

    /// Creates a new lockdown client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Retrieves a specific value from the device
    ///
    /// # Arguments
    /// * `value` - The name of the value to retrieve (e.g., "DeviceName")
    ///
    /// # Returns
    /// The requested value as a plist Value
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The requested value doesn't exist
    /// - The response is malformed
    ///
    /// # Example
    /// ```rust
    /// let device_name = client.get_value("DeviceName").await?;
    /// println!("Device name: {:?}", device_name);
    /// ```
    pub async fn get_value(&mut self, value: impl Into<String>) -> Result<Value, IdeviceError> {
        let req = LockdownRequest {
            label: self.idevice.label.clone(),
            key: Some(value.into()),
            request: "GetValue".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.idevice.send_plist(message).await?;
        let message: plist::Dictionary = self.idevice.read_plist().await?;
        match message.get("Value") {
            Some(m) => Ok(m.to_owned()),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Retrieves all available values from the device
    ///
    /// # Returns
    /// A dictionary containing all device values
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The response is malformed
    ///
    /// # Example
    /// ```rust
    /// let all_values = client.get_all_values().await?;
    /// for (key, value) in all_values {
    ///     println!("{}: {:?}", key, value);
    /// }
    /// ```
    pub async fn get_all_values(&mut self) -> Result<plist::Dictionary, IdeviceError> {
        let req = LockdownRequest {
            label: self.idevice.label.clone(),
            key: None,
            request: "GetValue".to_string(),
        };
        let message = plist::to_value(&req)?;
        self.idevice.send_plist(message).await?;
        let message: plist::Dictionary = self.idevice.read_plist().await?;
        match message.get("Value") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Starts a secure TLS session with the device
    ///
    /// # Arguments
    /// * `pairing_file` - Contains the device's identity and certificates
    ///
    /// # Returns
    /// `Ok(())` on successful session establishment
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - No connection is established
    /// - The session request is denied
    /// - TLS handshake fails
    pub async fn start_session(
        &mut self,
        pairing_file: &pairing_file::PairingFile,
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

        self.idevice
            .send_plist(plist::Value::Dictionary(request))
            .await?;

        let response = self.idevice.read_plist().await?;
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

        self.idevice.start_session(pairing_file).await?;
        Ok(())
    }

    /// Requests to start a service on the device
    ///
    /// # Arguments
    /// * `identifier` - The service identifier (e.g., "com.apple.debugserver")
    ///
    /// # Returns
    /// A tuple containing:
    /// - The port number where the service is available
    /// - A boolean indicating whether SSL should be used
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - The service cannot be started
    /// - The response is malformed
    /// - The requested service doesn't exist
    pub async fn start_service(
        &mut self,
        identifier: impl Into<String>,
    ) -> Result<(u16, bool), IdeviceError> {
        let identifier = identifier.into();
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "StartService".into());
        req.insert("Service".into(), identifier.into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let response = self.idevice.read_plist().await?;

        let ssl = match response.get("EnableServiceSSL") {
            Some(plist::Value::Boolean(ssl)) => ssl.to_owned(),
            _ => false, // over USB, this option won't exist
        };

        match response.get("Port") {
            Some(plist::Value::Integer(port)) => {
                if let Some(port) = port.as_unsigned() {
                    Ok((port as u16, ssl))
                } else {
                    error!("Port isn't an unsigned integer!");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                error!("Response didn't contain an integer port");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}

impl From<Idevice> for LockdownClient {
    /// Converts an existing device connection into a lockdown client
    fn from(value: Idevice) -> Self {
        Self::new(value)
    }
}
