// Jackson Coxson
// Incomplete implementation for installation_proxy

use log::warn;
use plist::{Dictionary, Value};

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

        // TODO: Determine if there are other types of profiles we can install
        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;

        todo!();
    }

    pub async fn remove(&mut self, id: &str) -> Result<(), IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "Remove".into());
        req.insert("ProfileID".into(), id.into());

        // TODO: Determine if there are other types of profiles we can install
        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        todo!()
    }

    pub async fn copy_all(&mut self) -> Result<Vec<Value>, IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("MessageType".into(), "CopyAll".into());

        // TODO: Determine if there are other types of profiles we can install
        req.insert("ProfileType".into(), "Provisioning".into());

        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        Ok(match res.remove("Payload") {
            // TODO: Determine if this is actually an array
            Some(plist::Value::Array(a)) => a,
            _ => {
                warn!("Did not get a payload of provisioning profiles as an array");
                return Err(IdeviceError::UnexpectedResponse);
            }
        })
    }
}
