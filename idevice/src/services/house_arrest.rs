//! iOS Device HouseArrest Service Abstraction
//!
//! The HouseArrest service allows access to the container and Documents directory of apps
//! installed on an iOS device. This is typically used for file transfer and inspection of
//! app-specific data during development or diagnostics.

use plist::{Dictionary, Value};

use crate::{lockdown::LockdownClient, Idevice, IdeviceError, IdeviceService};

use super::afc::AfcClient;

/// Client for interacting with the iOS HouseArrest service
///
/// HouseArrest is used to expose the container or Documents directory of an app to a host machine
/// over AFC (Apple File Conduit).
pub struct HouseArrestClient {
    /// The underlying device connection with the HouseArrest service
    pub idevice: Idevice,
}

impl IdeviceService for HouseArrestClient {
    /// Returns the name of the HouseArrest service as registered with lockdownd
    fn service_name() -> &'static str {
        "com.apple.mobile.house_arrest"
    }

    /// Establishes a connection to the HouseArrest service
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `HouseArrestClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if any step of the connection process fails
    ///
    /// # Process
    /// 1. Connect to the lockdownd service
    /// 2. Start a lockdown session
    /// 3. Request the HouseArrest service
    /// 4. Connect to the returned service port
    /// 5. Start TLS if required by the service
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

impl HouseArrestClient {
    /// Creates a new HouseArrest client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - A pre-established device connection with the HouseArrest service
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Requests access to the app's full container (Documents, Library, etc.) over AFC
    ///
    /// # Arguments
    /// * `bundle_id` - The bundle identifier of the target app (e.g., "com.example.MyApp")
    ///
    /// # Returns
    /// An `AfcClient` for accessing the container of the specified app
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request or AFC setup fails
    pub async fn vend_container(
        self,
        bundle_id: impl Into<String>,
    ) -> Result<AfcClient, IdeviceError> {
        let bundle_id = bundle_id.into();
        self.vend(bundle_id, "VendContainer".into()).await
    }

    /// Requests access to the app's Documents directory over AFC
    ///
    /// # Arguments
    /// * `bundle_id` - The bundle identifier of the target app (e.g., "com.example.MyApp")
    ///
    /// # Returns
    /// An `AfcClient` for accessing the Documents directory of the specified app
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request or AFC setup fails
    pub async fn vend_documents(
        self,
        bundle_id: impl Into<String>,
    ) -> Result<AfcClient, IdeviceError> {
        let bundle_id = bundle_id.into();
        self.vend(bundle_id, "VendDocuments".into()).await
    }

    /// Sends a HouseArrest command to expose a specific directory over AFC
    ///
    /// This is an internal method used by `vend_container` and `vend_documents`.
    ///
    /// # Arguments
    /// * `bundle_id` - App bundle identifier
    /// * `cmd` - Command to send ("VendContainer" or "VendDocuments")
    ///
    /// # Returns
    /// A connected `AfcClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request or AFC setup fails
    async fn vend(mut self, bundle_id: String, cmd: String) -> Result<AfcClient, IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("Command".into(), cmd.into());
        req.insert("Identifier".into(), bundle_id.into());
        self.idevice.send_plist(Value::Dictionary(req)).await?;
        self.idevice.read_plist().await?;

        Ok(AfcClient::new(self.idevice))
    }
}
