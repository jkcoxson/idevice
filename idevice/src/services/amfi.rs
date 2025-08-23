//! Abstraction for Apple Mobile File Integrity

use crate::{Idevice, IdeviceError, IdeviceService, obf};

/// Client for interacting with the AMFI service on the device
pub struct AmfiClient {
    /// The underlying device connection with established amfi service
    pub idevice: Idevice,
}

impl IdeviceService for AmfiClient {
    /// Returns the amfi service name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.amfi.lockdown")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        Ok(Self::new(idevice))
    }
}

impl AmfiClient {
    /// Creates a new amfi client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Shows the developer mode option in settings in iOS 18+
    /// Settings -> Privacy & Security -> Developer Mode
    pub async fn reveal_developer_mode_option_in_ui(&mut self) -> Result<(), IdeviceError> {
        let request = crate::plist!({
            "action": 0,
        });
        self.idevice.send_plist(request).await?;

        let res = self.idevice.read_plist().await?;
        if res.get("success").is_some() {
            Ok(())
        } else {
            Err(IdeviceError::UnexpectedResponse)
        }
    }

    /// Enables developer mode, triggering a reboot on iOS 18+
    pub async fn enable_developer_mode(&mut self) -> Result<(), IdeviceError> {
        let request = crate::plist!({
            "action": 1,
        });
        self.idevice.send_plist(request).await?;

        let res = self.idevice.read_plist().await?;
        if res.get("success").is_some() {
            Ok(())
        } else {
            Err(IdeviceError::UnexpectedResponse)
        }
    }

    /// Shows the accept dialogue for enabling developer mode
    pub async fn accept_developer_mode(&mut self) -> Result<(), IdeviceError> {
        let request = crate::plist!({
            "action": 2,
        });
        self.idevice.send_plist(request).await?;

        let res = self.idevice.read_plist().await?;
        if res.get("success").is_some() {
            Ok(())
        } else {
            Err(IdeviceError::UnexpectedResponse)
        }
    }

    /// Gets the developer mode status
    pub async fn get_developer_mode_status(&mut self) -> Result<bool, IdeviceError> {
        let request = crate::plist!({
            "action": 3,
        });
        self.idevice.send_plist(request).await?;

        let res = self.idevice.read_plist().await?;
        match res.get("success").and_then(|x| x.as_boolean()) {
            Some(true) => (),
            _ => return Err(IdeviceError::UnexpectedResponse),
        }

        match res.get("status").and_then(|x| x.as_boolean()) {
            Some(b) => Ok(b),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Trusts an app signer
    pub async fn trust_app_signer(
        &mut self,
        uuid: impl Into<String>,
    ) -> Result<bool, IdeviceError> {
        let request = crate::plist!({
            "action": 4,
            "input_profile_uuid": uuid.into(),
        });

        self.idevice.send_plist(request).await?;

        let res = self.idevice.read_plist().await?;
        match res.get("success").and_then(|x| x.as_boolean()) {
            Some(true) => (),
            _ => return Err(IdeviceError::UnexpectedResponse),
        }

        match res.get("status").and_then(|x| x.as_boolean()) {
            Some(b) => Ok(b),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }
}
