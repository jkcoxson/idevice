//! Diagnostics Relay

use crate::{obf, Idevice, IdeviceError, IdeviceService};

/// Client for interacting with the Diagnostics Relay
pub struct DiagnosticsRelayClient {
    /// The underlying device connection with established service
    pub idevice: Idevice,
}

impl IdeviceService for DiagnosticsRelayClient {
    /// Returns the service name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.mobile.diagnostics_relay")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        Ok(Self::new(idevice))
    }
}

impl DiagnosticsRelayClient {
    /// Creates a new  client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Requests data from the IO registry
    ///
    /// # Arguments
    /// * `current_plane` - The plane to request the tree as
    /// * `entry_name` - The entry to get
    /// * `entry_class` - The class to filter by
    ///
    /// # Returns
    /// A plist of the tree on success
    pub async fn ioregistry(
        &mut self,
        current_plane: Option<impl Into<String>>,
        entry_name: Option<impl Into<String>>,
        entry_class: Option<impl Into<String>>,
    ) -> Result<Option<plist::Dictionary>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        if let Some(plane) = current_plane {
            let plane = plane.into();
            req.insert("CurrentPlane".into(), plane.into());
        }
        if let Some(name) = entry_name {
            let name = name.into();
            req.insert("EntryName".into(), name.into());
        }
        if let Some(class) = entry_class {
            let class = class.into();
            req.insert("EntryClass".into(), class.into());
        }
        req.insert("Request".into(), "IORegistry".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let mut res = self.idevice.read_plist().await?;

        match res.get("Status").and_then(|x| x.as_string()) {
            Some("Success") => {}
            _ => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        }

        let res = res
            .remove("Diagnostics")
            .and_then(|x| x.into_dictionary())
            .and_then(|mut x| x.remove("IORegistry"))
            .and_then(|x| x.into_dictionary());

        Ok(res)
    }
}
