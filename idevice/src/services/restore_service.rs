//! Restore Service

use log::warn;
use plist::Dictionary;

use crate::{IdeviceError, ReadWrite, RemoteXpcClient, RsdService, obf};

/// Client for interacting with the Restore Service
pub struct RestoreServiceClient {
    /// The underlying device connection with established Restore Service service
    pub stream: RemoteXpcClient<Box<dyn ReadWrite>>,
}

impl RsdService for RestoreServiceClient {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.RestoreRemoteServices.restoreserviced")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Self::new(stream).await
    }
}

impl RestoreServiceClient {
    /// Creates a new Restore Service client a socket connection,
    /// and connects to the RemoteXPC service.
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub async fn new(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        let mut stream = RemoteXpcClient::new(stream).await?;
        stream.do_handshake().await?;
        Ok(Self { stream })
    }

    /// Enter recovery
    pub async fn enter_recovery(&mut self) -> Result<(), IdeviceError> {
        let req = crate::plist!({
            "command": "recovery"
        });

        self.stream.send_object(req, true).await?;

        let res = self.stream.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("Did not receive dictionary response from XPC");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        match res.remove("result") {
            Some(plist::Value::String(r)) => {
                if r == "success" {
                    Ok(())
                } else {
                    warn!("Failed to enter recovery");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("XPC dictionary did not contain result");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    /// Reboot
    pub async fn reboot(&mut self) -> Result<(), IdeviceError> {
        let req = crate::plist!({
            "command": "reboot"
        });
        self.stream.send_object(req, true).await?;

        let res = self.stream.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("Did not receive dictionary response from XPC");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        match res.remove("result") {
            Some(plist::Value::String(r)) => {
                if r == "success" {
                    Ok(())
                } else {
                    warn!("Failed to enter recovery");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("XPC dictionary did not contain result");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    /// Get preflightinfo
    pub async fn get_preflightinfo(&mut self) -> Result<Dictionary, IdeviceError> {
        let req = crate::plist!({
            "command": "getpreflightinfo"
        });
        self.stream.send_object(req, true).await?;

        let res = self.stream.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("Did not receive dictionary response from XPC");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let res = match res.remove("preflightinfo") {
            Some(plist::Value::Dictionary(i)) => i,
            _ => {
                warn!("XPC dictionary did not contain preflight info");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        Ok(res)
    }

    /// Get nonces
    /// Doesn't seem to work
    pub async fn get_nonces(&mut self) -> Result<Dictionary, IdeviceError> {
        let req = crate::plist!({
            "command": "getnonces"
        });
        self.stream.send_object(req, true).await?;

        let res = self.stream.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("Did not receive dictionary response from XPC");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let res = match res.remove("nonces") {
            Some(plist::Value::Dictionary(i)) => i,
            _ => {
                warn!("XPC dictionary did not contain nonces");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        Ok(res)
    }

    /// Get app parameters
    /// Doesn't seem to work
    pub async fn get_app_parameters(&mut self) -> Result<Dictionary, IdeviceError> {
        let req = crate::plist!({
            "command": "getappparameters"
        });
        self.stream.send_object(req, true).await?;

        let res = self.stream.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("Did not receive dictionary response from XPC");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let res = match res.remove("appparameters") {
            Some(plist::Value::Dictionary(i)) => i,
            _ => {
                warn!("XPC dictionary did not contain parameters");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        Ok(res)
    }

    /// Restores the language
    /// Doesn't seem to work
    pub async fn restore_lang(&mut self, language: impl Into<String>) -> Result<(), IdeviceError> {
        let language = language.into();

        let req = crate::plist!({
            "command": "restorelang",
            "argument": language,
        });
        self.stream.send_object(req, true).await?;

        let res = self.stream.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("Did not receive dictionary response from XPC");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        match res.remove("result") {
            Some(plist::Value::String(r)) => {
                if r == "success" {
                    Ok(())
                } else {
                    warn!("Failed to restore language");
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => {
                warn!("XPC dictionary did not contain result");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}
