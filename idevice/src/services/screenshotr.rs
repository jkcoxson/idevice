//! iOS screenshotr service client
//!
//! Provides functionality for interacting with the screenshot service on iOS devices below iOS 17,
//! which allows taking screenshots.

use crate::{Idevice, IdeviceError, IdeviceService, obf};
use log::{debug, warn};
use std::borrow::Cow;
use tokio::io::AsyncReadExt;
pub struct ScreenshotService {
    /// Underlying device connection
    pub idevice: Idevice,
}
impl IdeviceService for ScreenshotService {
    fn service_name() -> Cow<'static, str> {
        obf!("com.apple.mobile.screenshotr")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, IdeviceError> {
        let mut client = Self::new(idevice);
        // Perform DeviceLink handshake first
        client.dl_version_exchange().await?;
        Ok(client)
    }
}

impl ScreenshotService {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    async fn dl_version_exchange(&mut self) -> Result<(), IdeviceError> {
        debug!("Starting DeviceLink version exchange");
        // 1) Receive DLMessageVersionExchange
        let (msg, _arr) = self.receive_dl_message().await?;
        if msg != "DLMessageVersionExchange" {
            warn!("Expected DLMessageVersionExchange, got {msg}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        // 2) Send DLVersionsOk with version 400
        let out = vec![
            plist::Value::String("DLMessageVersionExchange".into()),
            plist::Value::String("DLVersionsOk".into()),
            plist::Value::Integer(400u64.into()),
        ];
        self.send_dl_array(out).await?;

        // 3) Receive DLMessageDeviceReady
        let (msg2, _arr2) = self.receive_dl_message().await?;
        if msg2 != "DLMessageDeviceReady" {
            warn!("Expected DLMessageDeviceReady, got {msg2}");
            return Err(IdeviceError::UnexpectedResponse);
        }
        Ok(())
    }

    /// Sends a raw DL array as binary plist
    async fn send_dl_array(&mut self, array: Vec<plist::Value>) -> Result<(), IdeviceError> {
        self.idevice.send_bplist(plist::Value::Array(array)).await
    }

    /// Receives any DL* message and returns (message_tag, full_array_value)
    pub async fn receive_dl_message(&mut self) -> Result<(String, plist::Value), IdeviceError> {
        if let Some(socket) = &mut self.idevice.socket {
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await?;
            let len = u32::from_be_bytes(buf);
            let mut body = vec![0; len as usize];
            socket.read_exact(&mut body).await?;
            let value: plist::Value = plist::from_bytes(&body)?;
            if let plist::Value::Array(arr) = &value
                && let Some(plist::Value::String(tag)) = arr.first()
            {
                return Ok((tag.clone(), value));
            }
            warn!("Invalid DL message format");
            Err(IdeviceError::UnexpectedResponse)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    pub async fn take_screenshot(&mut self) -> Result<Vec<u8>, IdeviceError> {
        // Send DLMessageTakeScreenshot

        let message_type_dict = crate::plist!(dict {
            "MessageType": "ScreenShotRequest"
        });

        let out = vec![
            plist::Value::String("DLMessageProcessMessage".into()),
            plist::Value::Dictionary(message_type_dict),
        ];
        self.send_dl_array(out).await?;

        // Receive DLMessageScreenshotData
        let (msg, value) = self.receive_dl_message().await?;
        if msg != "DLMessageProcessMessage" {
            warn!("Expected DLMessageProcessMessage, got {msg}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        if let plist::Value::Array(arr) = &value
            && arr.len() == 2
        {
            if let Some(plist::Value::Dictionary(dict)) = arr.get(1) {
                if let Some(plist::Value::Data(data)) = dict.get("ScreenShotData") {
                    Ok(data.clone())
                } else {
                    warn!("Invalid ScreenShotData format");
                    Err(IdeviceError::UnexpectedResponse)
                }
            } else {
                warn!("Invalid DLMessageScreenshotData format");
                Err(IdeviceError::UnexpectedResponse)
            }
        } else {
            warn!("Invalid DLMessageScreenshotData format");
            Err(IdeviceError::UnexpectedResponse)
        }
    }
}
