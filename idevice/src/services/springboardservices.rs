//! SpringBoard Services Client
//!
//! Provides functionality for interacting with the SpringBoard services on iOS devices,
//! which manages home screen and app icon related operations.

use crate::{Idevice, IdeviceError, IdeviceService, obf, utils::plist::truncate_dates_to_seconds};

/// Client for interacting with the iOS SpringBoard services
///
/// This service provides access to home screen and app icon functionality,
/// such as retrieving app icons.
#[derive(Debug)]
pub struct SpringBoardServicesClient {
    /// The underlying device connection with established SpringBoard services
    pub idevice: Idevice,
}

impl IdeviceService for SpringBoardServicesClient {
    /// Returns the SpringBoard services name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.springboardservices")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        Ok(Self::new(idevice))
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
        let req = crate::plist!({
            "command": "getIconPNGData",
            "bundleId": bundle_identifier,
        });
        self.idevice.send_plist(req).await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("pngData") {
            Some(plist::Value::Data(res)) => Ok(res),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Retrieves the current icon state from the device
    ///
    /// The icon state contains the layout and organization of all apps on the home screen,
    /// including folder structures and icon positions. This is a read-only operation.
    ///
    /// # Arguments
    /// * `format_version` - Optional format version string for the icon state format
    ///
    /// # Returns
    /// A plist Value containing the complete icon state structure
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The response is malformed
    ///
    /// # Example
    /// ```rust
    /// use idevice::services::springboardservices::SpringBoardServicesClient;
    ///
    /// let mut client = SpringBoardServicesClient::connect(&provider).await?;
    /// let icon_state = client.get_icon_state(None).await?;
    /// println!("Icon state: {:?}", icon_state);
    /// ```
    ///
    /// # Notes
    /// This method successfully reads the home screen layout on all iOS versions.
    /// Note that modifying and setting icon state (setIconState) does not work
    /// on iOS 18+ due to Apple's security restrictions. See issue #62 for details.
    pub async fn get_icon_state(
        &mut self,
        format_version: Option<String>,
    ) -> Result<plist::Value, IdeviceError> {
        let mut req = crate::plist!({
            "command": "getIconState",
        });

        if let Some(version) = format_version {
            if let Some(dict) = req.as_dictionary_mut() {
                dict.insert("formatVersion".to_string(), plist::Value::String(version));
            }
        }

        self.idevice.send_plist(req).await?;
        let res = self.idevice.read_plist_value().await?;
        Ok(res)
    }

    /// Sets the icon state on the device
    ///
    /// This method allows you to modify the home screen layout by providing a new icon state.
    /// The icon state structure should match the format returned by `get_icon_state`.
    ///
    /// # Arguments
    /// * `icon_state` - A plist Value containing the complete icon state structure
    ///
    /// # Returns
    /// Ok(()) if the icon state was successfully set
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The icon state format is invalid
    /// - The device rejects the new layout
    ///
    /// # Example
    /// ```rust
    /// use idevice::services::springboardservices::SpringBoardServicesClient;
    ///
    /// let mut client = SpringBoardServicesClient::connect(&provider).await?;
    /// let mut icon_state = client.get_icon_state(None).await?;
    ///
    /// // Modify the icon state (e.g., swap two icons)
    /// // ... modify icon_state ...
    ///
    /// client.set_icon_state(icon_state).await?;
    /// println!("Icon state updated successfully");
    /// ```
    ///
    /// # Notes
    /// - This method does NOT work on iOS 18+ due to Apple's security restrictions
    /// - On supported iOS versions, changes take effect immediately
    /// - The device may validate the icon state structure before applying
    /// - Invalid icon states will be rejected by the device
    pub async fn set_icon_state(&mut self, icon_state: plist::Value) -> Result<(), IdeviceError> {
        let mut icon_state_normalized = icon_state.clone();
        truncate_dates_to_seconds(&mut icon_state_normalized);

        let req = crate::plist!({
            "command": "setIconState",
            "iconState": icon_state_normalized,
        });

        self.idevice.send_plist(req).await?;
        Ok(())
    }

    /// Sets the icon state with a specific format version
    ///
    /// This is similar to `set_icon_state` but allows specifying a format version.
    ///
    /// # Arguments
    /// * `icon_state` - A plist Value containing the complete icon state structure
    /// * `format_version` - Optional format version string
    ///
    /// # Returns
    /// Ok(()) if the icon state was successfully set
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - Communication fails
    /// - The icon state format is invalid
    /// - The device rejects the new layout
    pub async fn set_icon_state_with_version(
        &mut self,
        icon_state: plist::Value,
        format_version: Option<String>,
    ) -> Result<(), IdeviceError> {
        let mut icon_state_normalized = icon_state.clone();
        truncate_dates_to_seconds(&mut icon_state_normalized);

        let mut req = crate::plist!({
            "command": "setIconState",
            "iconState": icon_state_normalized,
        });

        if let Some(version) = format_version {
            if let Some(dict) = req.as_dictionary_mut() {
                dict.insert("formatVersion".to_string(), plist::Value::String(version));
            }
        }

        self.idevice.send_plist(req).await?;
        Ok(())
    }
}
