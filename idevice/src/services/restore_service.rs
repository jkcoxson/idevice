//! Restore Service

use log::warn;
use plist::Dictionary;

use crate::{obf, IdeviceError, ReadWrite, RemoteXpcClient, RsdService};

/// Client for interacting with the Restore Service
pub struct RestoreServiceClient<R: ReadWrite> {
    /// The underlying device connection with established Restore Service service
    pub stream: RemoteXpcClient<R>,
}

impl<R: ReadWrite> RsdService for RestoreServiceClient<R> {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.RestoreRemoteServices.restoreserviced")
    }

    async fn from_stream(stream: R) -> Result<Self, IdeviceError> {
        Self::new(stream).await
    }

    type Stream = R;
}

impl<'a, R: ReadWrite + 'a> RestoreServiceClient<R> {
    /// Creates a new Restore Service client a socket connection,
    /// and connects to the RemoteXPC service.
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub async fn new(stream: R) -> Result<Self, IdeviceError> {
        let mut stream = RemoteXpcClient::new(stream).await?;
        stream.do_handshake().await?;
        Ok(Self { stream })
    }

    pub fn box_inner(self) -> RestoreServiceClient<Box<dyn ReadWrite + 'a>> {
        RestoreServiceClient {
            stream: self.stream.box_inner(),
        }
    }

    /// Enter recovery
    pub async fn enter_recovery(&mut self) -> Result<(), IdeviceError> {
        let mut req = Dictionary::new();
        req.insert("command".into(), "recovery".into());

        self.stream
            .send_object(plist::Value::Dictionary(req), true)
            .await?;

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
        let mut req = Dictionary::new();
        req.insert("command".into(), "reboot".into());

        self.stream
            .send_object(plist::Value::Dictionary(req), true)
            .await?;

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
        let mut req = Dictionary::new();
        req.insert("command".into(), "getpreflightinfo".into());

        self.stream
            .send_object(plist::Value::Dictionary(req), true)
            .await?;

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
        let mut req = Dictionary::new();
        req.insert("command".into(), "getnonces".into());

        self.stream
            .send_object(plist::Value::Dictionary(req), true)
            .await?;

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
        let mut req = Dictionary::new();
        req.insert("command".into(), "getappparameters".into());

        self.stream
            .send_object(plist::Value::Dictionary(req), true)
            .await?;

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

        let mut req = Dictionary::new();
        req.insert("command".into(), "restorelang".into());
        req.insert("argument".into(), language.into());

        self.stream
            .send_object(plist::Value::Dictionary(req), true)
            .await?;

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
