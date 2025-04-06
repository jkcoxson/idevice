//! iOS Installation Proxy Service Client
//!
//! Provides functionality for interacting with the installation_proxy service on iOS devices,
//! which allows querying and managing installed applications.

use std::collections::HashMap;

use crate::{lockdown::LockdownClient, Idevice, IdeviceError, IdeviceService};

/// Client for interacting with the iOS installation proxy service
///
/// This service provides access to information about installed applications
/// and can perform application management operations.
pub struct InstallationProxyClient {
    /// The underlying device connection with established installation_proxy service
    pub idevice: Idevice,
}

impl IdeviceService for InstallationProxyClient {
    /// Returns the installation proxy service name as registered with lockdownd
    fn service_name() -> &'static str {
        "com.apple.mobile.installation_proxy"
    }

    /// Establishes a connection to the installation proxy service
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `InstallationProxyClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if any step of the connection process fails
    ///
    /// # Process
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Requests the installation proxy service port
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

impl InstallationProxyClient {
    /// Creates a new installation proxy client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Retrieves information about installed applications
    ///
    /// # Arguments
    /// * `application_type` - Optional filter for application type:
    ///   - "System" for system applications
    ///   - "User" for user-installed applications
    ///   - "Any" for all applications (default)
    /// * `bundle_identifiers` - Optional list of specific bundle IDs to query
    ///
    /// # Returns
    /// A HashMap mapping bundle identifiers to application information plist values
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The response is malformed
    /// - The service returns an error
    ///
    /// # Example
    /// ```rust
    /// let apps = client.get_apps(Some("User".to_string()), None).await?;
    /// for (bundle_id, info) in apps {
    ///     println!("{}: {:?}", bundle_id, info);
    /// }
    /// ```
    pub async fn get_apps(
        &mut self,
        application_type: Option<String>,
        bundle_identifiers: Option<Vec<String>>,
    ) -> Result<HashMap<String, plist::Value>, IdeviceError> {
        let application_type = application_type.unwrap_or("Any".to_string());
        let mut options = plist::Dictionary::new();
        if let Some(ids) = bundle_identifiers {
            let ids = ids
                .into_iter()
                .map(plist::Value::String)
                .collect::<Vec<plist::Value>>();
            options.insert("BundleIDs".into(), ids.into());
        }
        options.insert("ApplicationType".into(), application_type.into());

        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "Lookup".into());
        req.insert("ClientOptions".into(), plist::Value::Dictionary(options));
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("LookupResult") {
            Some(plist::Value::Dictionary(res)) => {
                Ok(res.into_iter().collect::<HashMap<String, plist::Value>>())
            }
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }
}

