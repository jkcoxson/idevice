// Jackson Coxson
// Abstractions for the heartbeat service on iOS

use crate::{Idevice, IdeviceError};

pub struct HeartbeatClient {
    pub idevice: Idevice,
}

impl HeartbeatClient {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    pub async fn get_marco(&mut self) -> Result<u64, IdeviceError> {
        let rec = self.idevice.read_plist().await?;
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
