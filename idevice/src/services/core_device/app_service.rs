// Jackson Coxson

use log::warn;
use serde::Deserialize;

use crate::{obf, IdeviceError, ReadWrite, RsdService};

use super::CoreDeviceServiceClient;

impl<R: ReadWrite> RsdService for AppServiceClient<R> {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.coredevice.appservice")
    }

    async fn from_stream(stream: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: CoreDeviceServiceClient::new(stream).await?,
        })
    }

    type Stream = R;
}

pub struct AppServiceClient<R: ReadWrite> {
    inner: CoreDeviceServiceClient<R>,
}

#[derive(Deserialize, Debug)]
pub struct AppListEntry {
    #[serde(rename = "isRemovable")]
    pub is_removable: bool,
    pub name: String,
    #[serde(rename = "isFirstParty")]
    pub is_first_party: bool,
    pub path: String,
    #[serde(rename = "bundleIdentifier")]
    pub bundle_identifier: String,
    #[serde(rename = "isDeveloperApp")]
    pub is_developer_app: bool,
    #[serde(rename = "bundleVersion")]
    pub bundle_version: Option<String>,
    #[serde(rename = "isInternal")]
    pub is_internal: bool,
    #[serde(rename = "isHidden")]
    pub is_hidden: bool,
    #[serde(rename = "isAppClip")]
    pub is_app_clip: bool,
    pub version: Option<String>,
}

impl<R: ReadWrite> AppServiceClient<R> {
    pub async fn new(stream: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            inner: CoreDeviceServiceClient::new(stream).await?,
        })
    }

    pub async fn list_apps(
        &mut self,
        app_clips: bool,
        removable_apps: bool,
        hidden_apps: bool,
        internal_apps: bool,
        default_apps: bool,
    ) -> Result<Vec<AppListEntry>, IdeviceError> {
        let mut options = plist::Dictionary::new();
        options.insert("includeAppClips".into(), app_clips.into());
        options.insert("includeRemovableApps".into(), removable_apps.into());
        options.insert("includeHiddenApps".into(), hidden_apps.into());
        options.insert("includeInternalApps".into(), internal_apps.into());
        options.insert("includeDefaultApps".into(), default_apps.into());
        let res = self
            .inner
            .invoke("com.apple.coredevice.feature.listapps", Some(options))
            .await?;

        let res = match res.as_array() {
            Some(a) => a,
            None => {
                warn!("CoreDevice result was not an array");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let mut desd = Vec::new();
        for r in res {
            let r: AppListEntry = match plist::from_value(r) {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to parse app entry: {e:?}");
                    return Err(IdeviceError::UnexpectedResponse);
                }
            };
            desd.push(r);
        }

        Ok(desd)
    }
}
