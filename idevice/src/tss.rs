// Jackson Coxson
// Thanks pymobiledevice3

use log::{debug, warn};
use plist::Value;

use crate::{util::plist_to_bytes, IdeviceError};

const TSS_CLIENT_VERSION_STRING: &str = "libauthinstall-1033.0.2";
const TSS_CONTROLLER_ACTION_URL: &str = "http://gs.apple.com/TSS/controller?action=2";

#[derive(Debug)]
pub struct TSSRequest {
    inner: plist::Dictionary,
}

impl TSSRequest {
    pub fn new() -> Self {
        let mut inner = plist::Dictionary::new();
        inner.insert("@HostPlatformInfo".into(), "mac".into());
        inner.insert("@VersionInfo".into(), TSS_CLIENT_VERSION_STRING.into());
        inner.insert(
            "@UUID".into(),
            uuid::Uuid::new_v4().to_string().to_uppercase().into(),
        );
        Self { inner }
    }

    pub fn insert(&mut self, key: impl Into<String>, val: impl Into<Value>) {
        let key = key.into();
        let val = val.into();
        self.inner.insert(key, val);
    }

    pub async fn send(&self) -> Result<plist::Value, IdeviceError> {
        debug!(
            "Sending TSS request: {}",
            crate::pretty_print_dictionary(&self.inner)
        );
        let client = reqwest::Client::new();

        let res = client
            .post(TSS_CONTROLLER_ACTION_URL)
            .header("Cache-Control", "no-cache")
            .header("Content-type", "text/xml; charset=\"utf-8\"")
            .header("User-Agent", "InetURL/1.0")
            .header("Expect", "")
            .body(plist_to_bytes(&self.inner))
            .send()
            .await?
            .text()
            .await?;

        debug!("Apple responeded with {res}");
        let res = res.trim_start_matches("STATUS=0&");
        let res = res.trim_start_matches("MESSAGE=");
        if !res.starts_with("SUCCESS") {
            warn!("TSS responded with non-success value");
            return Err(IdeviceError::UnexpectedResponse);
        }
        let res = res.split("REQUEST_STRING=").collect::<Vec<&str>>();
        if res.len() < 2 {
            warn!("Response didn't contain a request string");
            return Err(IdeviceError::UnexpectedResponse);
        }
        Ok(plist::from_bytes(res[1].as_bytes())?)
    }
}

impl Default for TSSRequest {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_restore_request_rules(
    input: &mut plist::Dictionary,
    parameters: &plist::Dictionary,
    rules: &Vec<plist::Value>,
) {
    for rule in rules {
        if let plist::Value::Dictionary(rule) = rule {
            let conditions = match rule.get("Conditions") {
                Some(plist::Value::Dictionary(c)) => c,
                _ => {
                    warn!("Conditions doesn't exist or wasn't a dictionary!");
                    continue;
                }
            };

            let mut conditions_fulfilled = true;
            for (key, value) in conditions {
                let value2 = match key.as_str() {
                    "ApRawProductionMode" => parameters.get("ApProductionMode"),
                    "ApCurrentProductionMode" => parameters.get("ApProductionMode"),
                    "ApRawSecurityMode" => parameters.get("ApSecurityMode"),
                    "ApRequiresImage4" => parameters.get("ApSupportsImg4"),
                    "ApDemotionPolicyOverride" => parameters.get("DemotionPolicy"),
                    "ApInRomDFU" => parameters.get("ApInRomDFU"),
                    _ => {
                        warn!("Unhandled key {key}");
                        None
                    }
                };

                if value2.is_none() || value2 != Some(value) {
                    conditions_fulfilled = false;
                    break; // Stop checking other conditions immediately
                }
            }

            if !conditions_fulfilled {
                continue;
            }

            let actions = match rule.get("Actions") {
                Some(plist::Value::Dictionary(a)) => a,
                _ => {
                    warn!("Actions doesn't exist or wasn't a dictionary!");
                    continue;
                }
            };

            for (key, value) in actions {
                if let Some(i) = value.as_unsigned_integer() {
                    if i == 255 {
                        continue;
                    }
                }
                if let Some(i) = value.as_signed_integer() {
                    if i == 255 {
                        continue;
                    }
                }

                input.remove(key); // Explicitly remove before inserting, like Python
                input.insert(key.to_owned(), value.to_owned());
            }
        } else {
            warn!("Rule wasn't a dictionary");
        }
    }
}
