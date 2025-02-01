// Jackson Coxson
// Abstractions for the heartbeat service on iOS

use crate::{lockdownd::LockdowndClient, Idevice, IdeviceError, IdeviceService};

pub struct HeartbeatClient {
    pub idevice: Idevice,
}

impl IdeviceService for HeartbeatClient {
    fn service_name() -> &'static str {
        "com.apple.mobile.heartbeat"
    }

    async fn connect(
        provider: &impl crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let mut lockdown = LockdowndClient::connect(provider).await?;
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

impl HeartbeatClient {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    pub async fn get_marco(&mut self, interval: u64) -> Result<u64, IdeviceError> {
        // Get a plist or wait for the interval
        let rec = tokio::select! {
            rec = self.idevice.read_plist() => rec?,
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval)) => {
                return Err(IdeviceError::HeartbeatTimeout)
            }
        };
        match rec.get("Interval") {
            Some(plist::Value::Integer(interval)) => {
                if let Some(interval) = interval.as_unsigned() {
                    Ok(interval)
                } else {
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => match rec.get("Command") {
                Some(plist::Value::String(command)) => {
                    if command.as_str() == "SleepyTime" {
                        Err(IdeviceError::HeartbeatSleepyTime)
                    } else {
                        Err(IdeviceError::UnexpectedResponse)
                    }
                }
                _ => Err(IdeviceError::UnexpectedResponse),
            },
        }
    }

    pub async fn send_polo(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "Polo".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req.clone()))
            .await?;
        Ok(())
    }
}
