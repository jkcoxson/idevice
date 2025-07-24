//! iOS Mobile Installation Agent (misagent) Client
//!
//! Provides functionality for interacting with the misagent service on iOS devices,
//! which manages provisioning profiles and certificates.

use log::warn;
use plist::Dictionary;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{lockdown::LockdownClient, obf, Idevice, IdeviceError, IdeviceService, ReadWrite, RsdService};

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

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        Ok(Self::new(idevice))
    }
}

/// RSD-based misagent client for network connections
pub struct MisagentRsdClient<R: ReadWrite> {
    /// The underlying socket connection to misagent service
    pub socket: R,
}

impl<R: ReadWrite> RsdService for MisagentRsdClient<R> {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.misagent.shim.remote")
    }

    async fn from_stream(mut stream: R) -> Result<Self, IdeviceError> {
        // Perform RSD check-in for the entitlement
        let mut req = plist::Dictionary::new();
        req.insert("Label".into(), "misagent-rsd".into());
        req.insert("ProtocolVersion".into(), "2".into());
        req.insert("Request".into(), "RSDCheckin".into());
        
        // Send the check-in request
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &plist::Value::Dictionary(req))?;
        
        let len = buf.len() as u32;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&buf).await?;
        stream.flush().await?;
        
        // Read the first response
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let response_len = u32::from_be_bytes(len_buf) as usize;
        
        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).await?;
        
        let response: plist::Value = plist::from_bytes(&response_buf)?;
        if let plist::Value::Dictionary(dict) = response {
            match dict.get("Request").and_then(|x| x.as_string()) {
                Some(r) if r == "RSDCheckin" => {},
                _ => return Err(IdeviceError::UnexpectedResponse),
            }
        } else {
            return Err(IdeviceError::UnexpectedResponse);
        }
        
        // Read the second response
        stream.read_exact(&mut len_buf).await?;
        let response_len = u32::from_be_bytes(len_buf) as usize;
        
        response_buf.resize(response_len, 0);
        stream.read_exact(&mut response_buf).await?;
        
        let response: plist::Value = plist::from_bytes(&response_buf)?;
        if let plist::Value::Dictionary(dict) = response {
            match dict.get("Request").and_then(|x| x.as_string()) {
                Some(r) if r == "StartService" => {},
                _ => return Err(IdeviceError::UnexpectedResponse),
            }
        } else {
            return Err(IdeviceError::UnexpectedResponse);
        }
        
        Ok(Self { socket: stream })
    }

    type Stream = R;
}

impl<R: ReadWrite> MisagentRsdClient<R> {
    /// Creates a new misagent RSD client from a socket connection
    ///
    /// # Arguments
    /// * `socket` - Pre-established socket connection to misagent service
    pub fn new(socket: R) -> Self {
        Self { socket }
    }

    /// Consumes the client and returns the underlying socket
    pub fn into_inner(self) -> R {
        self.socket
    }

    /// Send a plist message and read response
    async fn send_plist_request(&mut self, req: plist::Value) -> Result<plist::Dictionary, IdeviceError> {
        // Serialize the plist to binary format
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &req)?;
        
        // Write the length header (4 bytes, big-endian)
        let len = buf.len() as u32;
        self.socket.write_all(&len.to_be_bytes()).await?;
        
        // Write the plist data
        self.socket.write_all(&buf).await?;
        self.socket.flush().await?;

        // Read the response length header
        let mut len_buf = [0u8; 4];
        self.socket.read_exact(&mut len_buf).await?;
        let response_len = u32::from_be_bytes(len_buf) as usize;

        // Read the response data  
        let mut response_buf = vec![0u8; response_len];
        self.socket.read_exact(&mut response_buf).await?;

        // Parse the response plist
        let response: plist::Value = plist::from_bytes(&response_buf)?;
        match response {
            plist::Value::Dictionary(dict) => Ok(dict),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
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

        let mut res = self.send_plist_request(plist::Value::Dictionary(req)).await?;

        match res.remove("Status") {
            Some(plist::Value::Integer(status)) => {
                if let Some(status) = status.as_unsigned() {
                    if status == 1 {
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

        let mut res = self.send_plist_request(plist::Value::Dictionary(req)).await?;

        match res.remove("Status") {
            Some(plist::Value::Integer(status)) => {
                if let Some(status) = status.as_unsigned() {
                    if status == 1 {
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

        let mut res = self.send_plist_request(plist::Value::Dictionary(req)).await?;

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
