//! Fetch rendered application icons as PNGs

use serde::Deserialize;
use tracing::warn;

use crate::{IdeviceError, ReadWrite, obf};

use super::{CoreDeviceError, CoreDeviceServiceClient};

#[derive(Debug)]
pub struct IconServiceClient<R: ReadWrite> {
    inner: CoreDeviceServiceClient<R>,
}

#[cfg(feature = "rsd")]
impl crate::RsdService for IconServiceClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.coredevice.iconservice")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: CoreDeviceServiceClient::new(stream).await?,
        })
    }
}

#[derive(Debug, Clone)]
pub enum AppIconTarget {
    BundleIdentifier(String),
    AppPath(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppIcon {
    /// The icon itself, PNG-encoded.
    #[serde(rename = "pngData")]
    pub png_data: plist::Data,
    /// Icon dimensions in pixels, i.e. `size` multiplied by `scale`.
    #[serde(rename = "pixelSize")]
    pub pixel_size: (f64, f64),
    /// Icon dimensions in points, as actually rendered. May be smaller than
    /// what was requested.
    pub size: (f64, f64),
    pub scale: f64,
    /// `true` when the device had no real icon for the app and rendered a
    /// generic placeholder instead.
    #[serde(rename = "isAppIconPlaceholder")]
    pub is_placeholder: bool,
}

impl<R: ReadWrite> IconServiceClient<R> {
    pub async fn new(inner: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: CoreDeviceServiceClient::new(inner).await?,
        })
    }

    /// Fetches one app's icon.
    pub async fn fetch_icon(
        &mut self,
        target: AppIconTarget,
        width: f32,
        height: f32,
        scale: f32,
        allow_placeholder: bool,
    ) -> Result<AppIcon, IdeviceError> {
        let (bundle_identifier, app_path) = match target {
            AppIconTarget::BundleIdentifier(b) => (Some(b), None),
            AppIconTarget::AppPath(p) => (None, Some(p)),
        };

        let feature = obf!("com.apple.coredevice.feature.fetchappicons");

        let res = self
            .inner
            .invoke_with_plist(
                feature,
                crate::plist!({
                    "bundleIdentifier":? bundle_identifier,
                    "appPath":? app_path,
                    "width": width,
                    "height": height,
                    "scale": scale,
                    "allowPlaceholder": allow_placeholder,
                })
                .into_dictionary()
                .unwrap(),
            )
            .await?;

        let info = res
            .as_dictionary()
            .and_then(|x| x.get("appIconInfo"))
            .ok_or(CoreDeviceError::MissingField("appIconInfo"))?;

        match plist::from_value(info) {
            Ok(icon) => Ok(icon),
            Err(e) => {
                warn!("Could not parse appIconInfo: {e:?}");
                Err(IdeviceError::UnexpectedResponse(
                    "failed to parse appIconInfo in fetch icon response".into(),
                ))
            }
        }
    }
}
