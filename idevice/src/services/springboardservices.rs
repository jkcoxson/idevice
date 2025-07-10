//! SpringBoard Services Client
//!
//! Provides functionality for interacting with the SpringBoard services on iOS devices,
//! which manages home screen and app icon related operations.

use crate::{lockdown::LockdownClient, obf, Idevice, IdeviceError, IdeviceService};

/// Client for interacting with the iOS SpringBoard services
///
/// This service provides access to home screen and app icon functionality,
/// such as retrieving app icons.
pub struct SpringBoardServicesClient {
    /// The underlying device connection with established SpringBoard services
    pub idevice: Idevice,
}

impl IdeviceService for SpringBoardServicesClient {
    /// Returns the SpringBoard services name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.springboardservices")
    }

    /// Establishes a connection to the SpringBoard services
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `SpringBoardServicesClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if any step of the connection process fails
    ///
    /// # Process
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Requests the SpringBoard services port
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

        Ok(Self { idevice })
    }
}

impl SpringBoardServicesClient {
    /// Creates a new SpringBoard services client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Retrieves the PNG icon data for a specified app
    ///
    /// # Arguments
    /// * `bundle_identifier` - The bundle identifier of the app (e.g., "com.apple.Maps")
    ///
    /// # Returns
    /// The raw PNG data of the app icon
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The app doesn't exist
    /// - The response is malformed
    ///
    /// # Example
    /// ```rust
    /// let icon_data = client.get_icon_pngdata("com.apple.Maps".to_string()).await?;
    /// std::fs::write("maps_icon.png", icon_data)?;
    /// ```
    pub async fn get_icon_pngdata(
        &mut self,
        bundle_identifier: String,
    ) -> Result<Vec<u8>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("command".into(), "getIconPNGData".into());
        req.insert("bundleId".into(), bundle_identifier.into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("pngData") {
            Some(plist::Value::Data(res)) => Ok(res),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }
}
