//! Diagnostics Relay

use crate::{Idevice, IdeviceError, IdeviceService, obf};

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
    /// Creates a new client from an existing device connection
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

    /// Requests MobileGestalt information from the device
    ///
    /// # Arguments
    /// * `keys` - Optional list of specific keys to request. If None, requests all available keys
    ///
    /// # Returns
    /// A dictionary containing the requested MobileGestalt information
    pub async fn mobilegestalt(
        &mut self,
        keys: Option<Vec<String>>,
    ) -> Result<Option<plist::Dictionary>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "MobileGestalt".into());

        if let Some(keys) = keys {
            let keys_array: Vec<plist::Value> = keys.into_iter().map(|k| k.into()).collect();
            req.insert("MobileGestaltKeys".into(), plist::Value::Array(keys_array));
        }

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

        let res = res.remove("Diagnostics").and_then(|x| x.into_dictionary());

        Ok(res)
    }

    /// Requests gas gauge information from the device
    ///
    /// # Returns
    /// A dictionary containing gas gauge (battery) information
    pub async fn gasguage(&mut self) -> Result<Option<plist::Dictionary>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "GasGauge".into());

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

        let res = res.remove("Diagnostics").and_then(|x| x.into_dictionary());

        Ok(res)
    }

    /// Requests NAND information from the device
    ///
    /// # Returns
    /// A dictionary containing NAND flash information
    pub async fn nand(&mut self) -> Result<Option<plist::Dictionary>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "NAND".into());

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

        let res = res.remove("Diagnostics").and_then(|x| x.into_dictionary());

        Ok(res)
    }

    /// Requests all available diagnostics information
    ///
    /// # Returns
    /// A dictionary containing all diagnostics information
    pub async fn all(&mut self) -> Result<Option<plist::Dictionary>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "All".into());

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

        let res = res.remove("Diagnostics").and_then(|x| x.into_dictionary());

        Ok(res)
    }

    /// Restarts the device
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn restart(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "Restart".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let res = self.idevice.read_plist().await?;

        match res.get("Status").and_then(|x| x.as_string()) {
            Some("Success") => Ok(()),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Shuts down the device
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn shutdown(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "Shutdown".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let res = self.idevice.read_plist().await?;

        match res.get("Status").and_then(|x| x.as_string()) {
            Some("Success") => Ok(()),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Puts the device to sleep
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn sleep(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "Sleep".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let res = self.idevice.read_plist().await?;

        match res.get("Status").and_then(|x| x.as_string()) {
            Some("Success") => Ok(()),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Requests WiFi diagnostics from the device
    pub async fn wifi(&mut self) -> Result<Option<plist::Dictionary>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "WiFi".into());

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

        let res = res.remove("Diagnostics").and_then(|x| x.into_dictionary());

        Ok(res)
    }

    /// Sends Goodbye request signaling end of communication
    pub async fn goodbye(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Request".into(), "Goodbye".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let res = self.idevice.read_plist().await?;
        match res.get("Status").and_then(|x| x.as_string()) {
            Some("Success") => Ok(()),
            Some("UnknownRequest") => Err(IdeviceError::UnexpectedResponse),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }
}
