//! Device configuration over the `com.apple.coredevice.configuration`
//! RemoteXPC service.
//!
//! This is the service behind Xcode's appearance and accessibility toggles:
//! light/dark mode, liquid-glass opacity, color filters, dynamic type size,
//! Reduce Motion, Increase Contrast, Reduce Transparency, and the layout-debug
//! borders overlay.

use std::borrow::Cow;

use crate::{IdeviceError, ReadWrite, obf};

use super::{CoreDeviceError, CoreDeviceServiceClient};

#[derive(Debug)]
pub struct ConfigurationServiceClient<R: ReadWrite> {
    inner: CoreDeviceServiceClient<R>,
}

#[cfg(feature = "rsd")]
impl crate::RsdService for ConfigurationServiceClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> Cow<'static, str> {
        obf!("com.apple.coredevice.configuration")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: CoreDeviceServiceClient::new(stream).await?,
        })
    }
}

/// The system's light/dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserInterfaceStyle {
    Light,
    Dark,
}

impl UserInterfaceStyle {
    /// The wire value the device uses.
    pub fn as_str(self) -> &'static str {
        match self {
            UserInterfaceStyle::Light => "light",
            UserInterfaceStyle::Dark => "dark",
        }
    }

    fn from_wire(s: &str) -> Result<Self, IdeviceError> {
        match s {
            "light" => Ok(UserInterfaceStyle::Light),
            "dark" => Ok(UserInterfaceStyle::Dark),
            _ => Err(CoreDeviceError::MalformedField("style").into()),
        }
    }
}

/// The accessibility color filter's state.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorFilter {
    pub enabled: bool,
    pub filter_type: Option<String>,
    /// Filter strength, 0.0 to 1.0. Optional even when the filter is enabled.
    pub intensity: Option<f64>,
}

impl<R: ReadWrite> ConfigurationServiceClient<R> {
    pub async fn new(inner: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: CoreDeviceServiceClient::new(inner).await?,
        })
    }

    /// The active appearance.
    pub async fn get_user_interface_style(&mut self) -> Result<UserInterfaceStyle, IdeviceError> {
        let res = self
            .invoke(obf!("com.apple.coredevice.action.getuserinterfacestyle"))
            .await?;
        let style = res
            .as_dictionary()
            .and_then(|d| d.get("style"))
            .and_then(|v| v.as_string())
            .ok_or(CoreDeviceError::MissingField("style"))?;
        UserInterfaceStyle::from_wire(style)
    }

    /// Switches the device between light and dark appearance.
    pub async fn set_user_interface_style(
        &mut self,
        style: UserInterfaceStyle,
    ) -> Result<(), IdeviceError> {
        self.invoke_with(
            obf!("com.apple.coredevice.action.setuserinterfacestyle"),
            crate::plist!({ "style": style.as_str() }),
        )
        .await?;
        Ok(())
    }

    /// Sets the system liquid-glass opacity, 0.0 to 1.0.
    pub async fn set_liquid_glass_opacity(&mut self, opacity: f32) -> Result<(), IdeviceError> {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(CoreDeviceError::InvalidArgument("opacity must be in [0.0, 1.0]").into());
        }
        self.invoke_with(
            obf!("com.apple.coredevice.action.setliquidglassconfiguration"),
            crate::plist!({ "configuration": { "opacity": opacity } }),
        )
        .await?;
        Ok(())
    }

    /// The accessibility color filter's state.
    pub async fn get_color_filter(&mut self) -> Result<ColorFilter, IdeviceError> {
        let res = self
            .invoke(obf!("com.apple.coredevice.action.getcolorfilter"))
            .await?;
        let filter = res
            .as_dictionary()
            .and_then(|d| d.get("colorFilter"))
            .and_then(|v| v.as_dictionary())
            .ok_or(CoreDeviceError::MissingField("colorFilter"))?;

        Ok(ColorFilter {
            enabled: filter
                .get("enabled")
                .and_then(|v| v.as_boolean())
                .ok_or(CoreDeviceError::MissingField("enabled"))?,
            filter_type: filter
                .get("filterType")
                .and_then(|v| v.as_dictionary())
                .and_then(|d| d.get("name"))
                .and_then(|v| v.as_string())
                .map(str::to_string),
            intensity: filter.get("intensity").and_then(|v| v.as_real()),
        })
    }

    /// Enables or disables the accessibility color filter.
    ///
    /// `filter_type` is required when enabling and names a preset, e.g.
    /// `Protanopia`. `intensity` (0.0 to 1.0) is always optional. As with
    /// [`set_liquid_glass_opacity`](Self::set_liquid_glass_opacity), the
    /// intensity goes out as an `f32`.
    pub async fn set_color_filter(
        &mut self,
        enabled: bool,
        filter_type: Option<&str>,
        intensity: Option<f32>,
    ) -> Result<(), IdeviceError> {
        let mut filter = plist::Dictionary::new();
        filter.insert("enabled".into(), enabled.into());
        if enabled {
            let Some(filter_type) = filter_type else {
                return Err(CoreDeviceError::InvalidArgument(
                    "filter_type is required when enabling the color filter",
                )
                .into());
            };
            filter.insert("filterType".into(), crate::plist!({ "name": filter_type }));
            if let Some(intensity) = intensity {
                filter.insert("intensity".into(), crate::plist!(intensity));
            }
        }

        self.invoke_with(
            obf!("com.apple.coredevice.action.setcolorfilter"),
            crate::plist!({ "colorFilter": plist::Value::Dictionary(filter) }),
        )
        .await?;
        Ok(())
    }

    /// The dynamic-type size's name, e.g. `medium` or `large`.
    pub async fn get_device_text_size(&mut self) -> Result<String, IdeviceError> {
        let res = self
            .invoke(obf!("com.apple.coredevice.action.getdevicetextsize"))
            .await?;
        // The size is the sole key of the `size` dictionary.
        res.as_dictionary()
            .and_then(|d| d.get("textSize"))
            .and_then(|v| v.as_dictionary())
            .and_then(|d| d.get("size"))
            .and_then(|v| v.as_dictionary())
            .and_then(|d| d.keys().next())
            .map(String::to_owned)
            .ok_or(CoreDeviceError::MissingField("textSize").into())
    }

    /// Sets the dynamic-type size by name, e.g. `medium` or `large`.
    pub async fn set_device_text_size(&mut self, size: &str) -> Result<(), IdeviceError> {
        self.invoke_with(
            obf!("com.apple.coredevice.action.setdevicetextsize"),
            crate::plist!({ "textSize": { "size": { size: {} } } }),
        )
        .await?;
        Ok(())
    }

    /// Whether Reduce Motion is on.
    pub async fn get_reduce_motion(&mut self) -> Result<bool, IdeviceError> {
        self.get_enabled(
            obf!("com.apple.coredevice.action.getreducemotion"),
            "reduceMotion",
        )
        .await
    }

    /// Toggles Reduce Motion.
    pub async fn set_reduce_motion(&mut self, enabled: bool) -> Result<(), IdeviceError> {
        self.set_enabled(
            obf!("com.apple.coredevice.action.setreducemotion"),
            "reduceMotion",
            enabled,
        )
        .await
    }

    /// Toggles Increase Contrast.
    pub async fn set_increase_contrast(&mut self, enabled: bool) -> Result<(), IdeviceError> {
        self.set_enabled(
            obf!("com.apple.coredevice.action.setdeviceincreasecontrast"),
            "increaseContrast",
            enabled,
        )
        .await
    }

    /// Whether the layout-debug borders overlay is on.
    pub async fn get_show_borders(&mut self) -> Result<bool, IdeviceError> {
        self.get_enabled(
            obf!("com.apple.coredevice.action.getshowborders"),
            "showBorders",
        )
        .await
    }

    /// Toggles the layout-debug borders overlay.
    pub async fn set_show_borders(&mut self, enabled: bool) -> Result<(), IdeviceError> {
        self.set_enabled(
            obf!("com.apple.coredevice.action.setshowborders"),
            "showBorders",
            enabled,
        )
        .await
    }

    /// Whether Reduce Transparency is on.
    pub async fn get_reduce_transparency(&mut self) -> Result<bool, IdeviceError> {
        self.get_enabled(
            obf!("com.apple.coredevice.action.getreducetransparency"),
            "reduceTransparency",
        )
        .await
    }

    /// Toggles Reduce Transparency.
    pub async fn set_reduce_transparency(&mut self, enabled: bool) -> Result<(), IdeviceError> {
        self.set_enabled(
            obf!("com.apple.coredevice.action.setreducetransparency"),
            "reduceTransparency",
            enabled,
        )
        .await
    }

    /// Reads one of the `{<knob>: {enabled: <bool>}}` knobs.
    async fn get_enabled(
        &mut self,
        action: Cow<'static, str>,
        knob: &'static str,
    ) -> Result<bool, IdeviceError> {
        let res = self.invoke(action).await?;
        res.as_dictionary()
            .and_then(|d| d.get(knob))
            .and_then(|v| v.as_dictionary())
            .and_then(|d| d.get("enabled"))
            .and_then(|v| v.as_boolean())
            .ok_or(CoreDeviceError::MissingField(knob).into())
    }

    /// Writes one of the `{<knob>: {enabled: <bool>}}` knobs.
    async fn set_enabled(
        &mut self,
        action: Cow<'static, str>,
        knob: &str,
        enabled: bool,
    ) -> Result<(), IdeviceError> {
        self.invoke_with(action, crate::plist!({ knob: { "enabled": enabled } }))
            .await?;
        Ok(())
    }

    async fn invoke(&mut self, action: Cow<'static, str>) -> Result<plist::Value, IdeviceError> {
        self.inner
            .invoke_action_with_plist(action.to_string(), plist::Dictionary::new())
            .await
    }

    async fn invoke_with(
        &mut self,
        action: Cow<'static, str>,
        input: plist::Value,
    ) -> Result<plist::Value, IdeviceError> {
        let input = input
            .into_dictionary()
            .ok_or(CoreDeviceError::MalformedField("(input)"))?;
        self.inner
            .invoke_action_with_plist(action.to_string(), input)
            .await
    }
}
