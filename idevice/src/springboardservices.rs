use crate::{lockdown::LockdowndClient, Idevice, IdeviceError, IdeviceService};

pub struct SpringBoardServicesClient {
    pub idevice: Idevice,
}

impl IdeviceService for SpringBoardServicesClient {
    fn service_name() -> &'static str {
        "com.apple.springboardservices"
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

        Ok(Self { idevice })
    }
}

impl SpringBoardServicesClient {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Gets the icon of a spceified app
    /// # Arguments
    /// `bundle_identifier` - The identifier of the app to get icon
    pub async fn get_icon_pngdata(
        &mut self,
        bundle_identifier: String,
    ) -> Result<Vec<u8>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("command".into(), "getIconPNGData".into());
        req.insert("bundleId".into(), bundle_identifier.into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("pngData") {
            Some(plist::Value::Data(res)) => Ok(res),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }
}
