//! iOS Mobile Installation Agent (misagent) Client
//!
//! Provides functionality for interacting with the misagent service on iOS devices,
//! which manages provisioning profiles and certificates.

use log::warn;
use plist::Dictionary;

use crate::{lockdown::LockdownClient, obf, Idevice, IdeviceError, IdeviceService, RsdService};

/// Client for interacting with the iOS misagent service
///
/// The misagent service handles:
/// - Installation of provisioning profiles
/// - Removal of provisioning profiles
/// - Querying installed profiles
pub struct MisagentClient {
    /// The underlying device connection with established misagent service
    pub idevice: Idevice,
}

impl RsdService for MisagentClient {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.misagent.shim.remote")
    }

    async fn from_stream(stream: Box<dyn crate::ReadWrite>) -> Result<Self, IdeviceError> {
        let mut stream = Idevice::new(stream, "");
        stream.rsd_checkin().await?;
        Ok(Self::new(stream))
    }
}

impl IdeviceService for MisagentClient {
    /// Returns the misagent service name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.misagent")
    }

    /// Establishes a connection to the misagent service
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `MisagentClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if any step of the connection process fails
    ///
    /// # Process
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Requests the misagent service port
    /// 4. Establishes connection to the service port
    /// 5. Optionally starts TLS if required by service
    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let mut lockdown = LockdownClient::connect(provider).await?;
        lockdown
            .start_session(&provider.get_pairing_file().await?)
            .await?;
        let (port, ssl) = lockdown.start_service(Self::service_name()).await?;

        let mut idevice = provider.connect(port).await?;
        if ssl {
            idevice
                .start_session(&provider.get_pairing_file().await?)
                .await?;
        }

        Ok(Self::new(idevice))
    }
}

impl MisagentClient {
    /// Creates a new misagent client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Installs a provisioning profile on the device
    ///
    /// # Arguments
    /// * `profile` - The provisioning profile data to install
    ///
    /// # Returns
    /// `Ok(())` on successful installation
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The profile is invalid
    /// - Installation is not permitted
    ///
    /// # Example
    /// ```rust
    /// let profile_data = std::fs::read("profile.mobileprovision")?;
    /// client.install(profile_data).await?;
    /// ```
    pub async fn install(&mut self, profile: Vec<u8>) -> Result<(), IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "Install".into());
        req.insert("Profile".into(), plist::Value::Data(profile));
        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;

        match res.remove("Status") {
            Some(plist::Value::Integer(status)) => {
                if let Some(status) = status.as_unsigned() {
                    if status == 0 {
                        Ok(())
                    } else {
                        Err(IdeviceError::MisagentFailure)
                    }
                } else {
                    warn!("Misagent return status wasn't unsigned");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("Did not get integer status response");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    /// Removes a provisioning profile from the device
    ///
    /// # Arguments
    /// * `id` - The UUID of the profile to remove
    ///
    /// # Returns
    /// `Ok(())` on successful removal
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The profile doesn't exist
    /// - Removal is not permitted
    ///
    /// # Example
    /// ```rust
    /// client.remove("asdf").await?;
    /// ```
    pub async fn remove(&mut self, id: &str) -> Result<(), IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "Remove".into());
        req.insert("ProfileID".into(), id.into());
        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;

        match res.remove("Status") {
            Some(plist::Value::Integer(status)) => {
                if let Some(status) = status.as_unsigned() {
                    if status == 0 {
                        Ok(())
                    } else {
                        Err(IdeviceError::MisagentFailure)
                    }
                } else {
                    warn!("Misagent return status wasn't unsigned");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("Did not get integer status response");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    /// Retrieves all provisioning profiles from the device
    ///
    /// # Returns
    /// A vector containing raw profile data for each installed profile
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The response is malformed
    ///
    /// # Example
    /// ```rust
    /// let profiles = client.copy_all().await?;
    /// for profile in profiles {
    ///     println!("Profile size: {} bytes", profile.len());
    /// }
    /// ```
    pub async fn copy_all(&mut self) -> Result<Vec<Vec<u8>>, IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "CopyAll".into());
        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("Payload") {
            Some(plist::Value::Array(a)) => {
                let mut res = Vec::new();
                for profile in a {
                    if let Some(profile) = profile.as_data() {
                        res.push(profile.to_vec());
                    } else {
                        warn!("Misagent CopyAll did not return data plists");
                        return Err(IdeviceError::UnexpectedResponse);
                    }
                }
                Ok(res)
            }
            _ => {
                warn!("Did not get a payload of provisioning profiles as an array");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}
