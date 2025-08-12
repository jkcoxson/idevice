//! iOS Mobile Installation Agent (misagent) Client
//!
//! Provides functionality for interacting with the misagent service on iOS devices,
//! which manages provisioning profiles and certificates.
//!
//! Based on libimobiledevice implementation from SideStore

use log::{debug, warn};
use plist::{Dictionary, Value};

use crate::{lockdown::LockdownClient, obf, Idevice, IdeviceError, IdeviceService};

/// Error codes returned by misagent service
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisagentError {
    Success = 0,
    InvalidArgument = -1,
    PlistError = -2,
    ConnectionFailed = -3,
    RequestFailed = -4,
    UnknownError = -256,
}

impl From<IdeviceError> for MisagentError {
    fn from(err: IdeviceError) -> Self {
        match err {
            IdeviceError::Plist(_) => MisagentError::PlistError,
            IdeviceError::Socket(_) => MisagentError::ConnectionFailed,
            _ => MisagentError::UnknownError,
        }
    }
}

impl From<MisagentError> for IdeviceError {
    fn from(err: MisagentError) -> Self {
        match err {
            MisagentError::Success => IdeviceError::UnexpectedResponse, // Shouldn't happen
            MisagentError::InvalidArgument => IdeviceError::FfiInvalidArg,
            MisagentError::PlistError => IdeviceError::UnexpectedResponse,
            MisagentError::ConnectionFailed => IdeviceError::NoEstablishedConnection,
            MisagentError::RequestFailed => IdeviceError::MisagentFailure,
            MisagentError::UnknownError => IdeviceError::MisagentFailure,
        }
    }
}

/// Client for interacting with the iOS misagent service
///
/// Handles provisioning profile installation, listing, and removal operations.
/// Uses traditional lockdown service connection (not RSD) for maximum compatibility.
pub struct MisagentClient {
    /// The underlying device connection with established misagent service
    pub idevice: Idevice,
    /// Last error code from the service
    pub last_error: i32,
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
    /// For USB connections:
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Requests the misagent service port
    /// 4. Establishes connection to the service port
    /// 5. Optionally starts TLS if required by service
    ///
    /// For network connections (TCP):
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Performs RSD check-in to get proper entitlements
    /// 4. Requests the misagent service port
    /// 5. Establishes connection to the service port
    /// 6. Starts TLS session
    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        // Check if this is a network connection by examining the provider label
        // Network connections typically have IP addresses in their labels
        let provider_label = provider.label();
        let is_network_connection = provider_label.contains('.') && 
            provider_label.chars().any(|c| c.is_ascii_digit());
        
        debug!("Connecting to misagent service (network: {})", is_network_connection);
        
        let mut lockdown = LockdownClient::connect(provider).await?;
        lockdown
            .start_session(&provider.get_pairing_file().await?)
            .await?;
            
        // For network connections, perform RSD check-in to get proper entitlements
        if is_network_connection {
            debug!("Network connection detected - performing RSD check-in for entitlements");
            
            // Perform RSD check-in on the lockdown connection
            lockdown.idevice.rsd_checkin().await?;
            debug!("RSD check-in completed successfully");
        }
        
        let (port, ssl) = lockdown.start_service(Self::service_name()).await?;
        debug!("Got misagent service port: {}, SSL: {}", port, ssl);

        let mut idevice = provider.connect(port).await?;
        if ssl {
            debug!("Starting TLS session for misagent");
            idevice
                .start_session(&provider.get_pairing_file().await?)
                .await?;
        }

        Ok(Self::new(idevice))
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        Ok(Self::new(idevice))
    }
}

impl MisagentClient {
    /// Creates a new misagent client from an established device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self {
            idevice,
            last_error: 0,
        }
    }

    /// Checks the response from misagent to determine if the operation was successful
    fn check_result(&mut self, response: &Dictionary) -> Result<(), MisagentError> {
        // Look for Status field in response
        if let Some(status_value) = response.get("Status") {
            if let Some(status) = status_value.as_signed_integer() {
                self.last_error = status as i32;
                if status == 0 {
                    return Ok(());
                } else {
                    warn!("misagent operation failed with status: {}", status);
                    return Err(MisagentError::RequestFailed);
                }
            }
        }
        
        warn!("misagent response missing or invalid Status field");
        Err(MisagentError::PlistError)
    }

    /// Send a plist request and receive response
    async fn send_request(&mut self, request: Dictionary) -> Result<Dictionary, MisagentError> {
        // Convert to plist Value
        let plist_value = Value::Dictionary(request);
        
        debug!("Sending misagent request: {:#?}", plist_value);
        
        // Send the plist
        self.idevice.send_plist(plist_value).await
            .map_err(|e| {
                warn!("Failed to send misagent request: {:?}", e);
                MisagentError::from(e)
            })?;
        
        // Receive the response
        let response = self.idevice.read_plist().await
            .map_err(|e| {
                warn!("Failed to read misagent response: {:?}", e);
                MisagentError::from(e)
            })?;
        
        debug!("Received misagent response: {:#?}", response);
        
        // Response is already a dictionary
        Ok(response)
    }

    /// Installs a provisioning profile on the device
    ///
    /// # Arguments
    /// * `profile_data` - The provisioning profile data as bytes
    ///
    /// # Returns
    /// `Ok(())` on successful installation, error otherwise
    ///
    /// # Errors
    /// Returns `MisagentError` if installation fails
    pub async fn install_profile(&mut self, profile_data: &[u8]) -> Result<(), MisagentError> {
        debug!("Installing provisioning profile ({} bytes)", profile_data.len());
        
        let mut request = Dictionary::new();
        request.insert("MessageType".to_string(), Value::String("Install".to_string()));
        request.insert("ProfileType".to_string(), Value::String("Provisioning".to_string()));
        request.insert("Profile".to_string(), Value::Data(profile_data.to_vec()));
        
        let response = self.send_request(request).await?;
        self.check_result(&response)?;
        
        debug!("Provisioning profile installed successfully");
        Ok(())
    }

    /// Removes a provisioning profile from the device
    ///
    /// # Arguments
    /// * `profile_id` - The UUID of the profile to remove
    ///
    /// # Returns
    /// `Ok(())` on successful removal, error otherwise
    ///
    /// # Errors
    /// Returns `MisagentError` if removal fails
    pub async fn remove_profile(&mut self, profile_id: &str) -> Result<(), MisagentError> {
        debug!("Removing provisioning profile: {}", profile_id);
        
        let mut request = Dictionary::new();
        request.insert("MessageType".to_string(), Value::String("Remove".to_string()));
        request.insert("ProfileType".to_string(), Value::String("Provisioning".to_string()));
        request.insert("ProfileID".to_string(), Value::String(profile_id.to_string()));
        
        let response = self.send_request(request).await?;
        self.check_result(&response)?;
        
        debug!("Provisioning profile removed successfully");
        Ok(())
    }

    /// Lists all installed provisioning profiles
    ///
    /// # Returns
    /// A vector of profile data on success, error otherwise
    ///
    /// # Errors
    /// Returns `MisagentError` if listing fails
    pub async fn list_profiles(&mut self) -> Result<Vec<Value>, MisagentError> {
        debug!("Listing installed provisioning profiles");
        
        let mut request = Dictionary::new();
        request.insert("MessageType".to_string(), Value::String("Copy".to_string()));
        request.insert("ProfileType".to_string(), Value::String("Provisioning".to_string()));
        
        let response = self.send_request(request).await?;
        self.check_result(&response)?;
        
        // Extract payload containing profiles
        match response.get("Payload") {
            Some(Value::Array(profiles)) => {
                debug!("Found {} provisioning profiles", profiles.len());
                Ok(profiles.clone())
            }
            Some(_) => {
                warn!("Payload is not an array");
                Err(MisagentError::PlistError)
            }
            None => {
                debug!("No profiles found");
                Ok(Vec::new())
            }
        }
    }

    /// Lists all installed provisioning profiles (including system profiles)
    ///
    /// # Returns
    /// A vector of profile data on success, error otherwise
    ///
    /// # Errors
    /// Returns `MisagentError` if listing fails
    pub async fn list_all_profiles(&mut self) -> Result<Vec<Value>, MisagentError> {
        debug!("Listing all provisioning profiles (including system)");
        
        let mut request = Dictionary::new();
        request.insert("MessageType".to_string(), Value::String("CopyAll".to_string()));
        request.insert("ProfileType".to_string(), Value::String("Provisioning".to_string()));
        
        let response = self.send_request(request).await?;
        self.check_result(&response)?;
        
        // Extract payload containing profiles
        match response.get("Payload") {
            Some(Value::Array(profiles)) => {
                debug!("Found {} total provisioning profiles", profiles.len());
                Ok(profiles.clone())
            }
            Some(_) => {
                warn!("Payload is not an array");
                Err(MisagentError::PlistError)
            }
            None => {
                debug!("No profiles found");
                Ok(Vec::new())
            }
        }
    }

    /// Gets the last error code from the service
    ///
    /// # Returns
    /// The last error code returned by the misagent service
    pub fn get_last_error(&self) -> i32 {
        self.last_error
    }
}