// Jackson Coxson
// Incomplete implementation for installation_proxy

use std::collections::HashMap;

use crate::{Idevice, IdeviceError};

pub struct InstallationProxyClient {
    pub idevice: Idevice,
}

impl InstallationProxyClient {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Gets installed apps on the device
    /// # Arguments
    /// `application_type` - The application type to filter by
    /// `bundle_identifiers` - The identifiers to filter by
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
            options.insert("BundleIDs".into(), ids.into()).unwrap();
        }
        options.insert("ApplicationType".into(), application_type.into());

        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "Lookup".into());
        // req.insert("ClientOptions".into(), plist::Value::Dictionary(options));
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
