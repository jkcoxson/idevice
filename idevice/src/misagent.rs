// Jackson Coxson
// Incomplete implementation for installation_proxy

use log::warn;
use plist::Dictionary;

use crate::{lockdownd::LockdowndClient, Idevice, IdeviceError, IdeviceService};

pub struct MisagentClient {
    pub idevice: Idevice,
}

impl IdeviceService for MisagentClient {
    fn service_name() -> &'static str {
        "com.apple.misagent"
    }

    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let mut lockdown = LockdowndClient::connect(provider).await?;
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

impl MisagentClient {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    pub async fn install(&mut self, profile: Vec<u8>) -> Result<(), IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "Install".into());
        req.insert("Profile".into(), plist::Value::Data(profile));

        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;

        match res.remove("Status") {
            Some(plist::Value::Integer(status)) => {
                if let Some(status) = status.as_unsigned() {
                    if status == 1 {
                        Ok(())
                    } else {
                        Err(IdeviceError::MisagentFailure)
                    }
                } else {
                    warn!("Misagent return status wasn't unsigned");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("Did not get integer status response");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    pub async fn remove(&mut self, id: &str) -> Result<(), IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "Remove".into());
        req.insert("ProfileID".into(), id.into());

        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;

        match res.remove("Status") {
            Some(plist::Value::Integer(status)) => {
                if let Some(status) = status.as_unsigned() {
                    if status == 1 {
                        Ok(())
                    } else {
                        Err(IdeviceError::MisagentFailure)
                    }
                } else {
                    warn!("Misagent return status wasn't unsigned");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("Did not get integer status response");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    pub async fn copy_all(&mut self) -> Result<Vec<Vec<u8>>, IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "CopyAll".into());

        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("Payload") {
            Some(plist::Value::Array(a)) => {
                let mut res = Vec::new();
                for profile in a {
                    if let Some(profile) = profile.as_data() {
                        res.push(profile.to_vec());
                    } else {
                        warn!("Misagent CopyAll did not return data plists");
                        return Err(IdeviceError::UnexpectedResponse);
                    }
                }
                Ok(res)
            }
            _ => {
                warn!("Did not get a payload of provisioning profiles as an array");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}
