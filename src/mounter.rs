// Jackson Coxson

use crate::{Idevice, IdeviceError};

pub struct ImageMounter {
    idevice: Idevice,
}

impl ImageMounter {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    pub fn copy_devices(&mut self) -> Result<Vec<plist::Value>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "CopyDevices".into());
        self.idevice.send_plist(plist::Value::Dictionary(req))?;
        let mut res = self.idevice.read_plist()?;

        match res.remove("EntryList") {
            Some(plist::Value::Array(i)) => Ok(i),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub fn upload_image(
        &mut self,
        image_type: impl Into<String>,
        image: &[u8],
        signature: Vec<u8>,
    ) -> Result<(), IdeviceError> {
        let image_type = image_type.into();

        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "ReceiveBytes".into());
        req.insert("ImageType".into(), image_type.into());
        req.insert("ImageSize".into(), (image.len() as u64).into());
        req.insert("ImageSignature".into(), plist::Value::Data(signature));
        self.idevice.send_plist(plist::Value::Dictionary(req))?;

        let res = self.idevice.read_plist()?;
        match res.get("Status") {
            Some(plist::Value::String(s)) => {
                if s.as_str() != "ReceiveBytesAck" {
                    log::error!("Received bad response to SendBytes: {s:?}");
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            _ => return Err(IdeviceError::UnexpectedResponse),
        }

        self.idevice.send_raw(image)?;

        let res = self.idevice.read_plist()?;
        match res.get("Status") {
            Some(plist::Value::String(s)) => {
                if s.as_str() != "Success" {
                    log::error!("Image send failure: {s:?}");
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            _ => return Err(IdeviceError::UnexpectedResponse),
        }

        Ok(())
    }

    pub fn mount_image(
        &mut self,
        image_type: impl Into<String>,
        signature: Vec<u8>,
        trust_cache: Vec<u8>,
        info_plist: plist::Value,
    ) -> Result<(), IdeviceError> {
        let image_type = image_type.into();

        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "MountImage".into());
        req.insert("ImageType".into(), image_type.into());
        req.insert("ImageSignature".into(), plist::Value::Data(signature));
        req.insert("ImageTrustCache".into(), plist::Value::Data(trust_cache));
        req.insert("ImageInfoPlist".into(), info_plist);
        self.idevice.send_plist(plist::Value::Dictionary(req))?;

        let res = self.idevice.read_plist()?;

        match res.get("Status") {
            Some(plist::Value::String(s)) => {
                if s.as_str() != "Success" {
                    log::error!("Image send failure: {s:?}");
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            _ => return Err(IdeviceError::UnexpectedResponse),
        }

        Ok(())
    }

    /// Queries the personalization manifest from the device.
    /// On failure, the socket must be closed and reestablished.
    pub fn query_personalization_manifest(
        &mut self,
        image_type: impl Into<String>,
        signature: Vec<u8>,
    ) -> Result<Vec<u8>, IdeviceError> {
        let image_type = image_type.into();

        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "QueryPersonalizationManifest".into());
        req.insert("PersonalizedImageType".into(), image_type.clone().into());
        req.insert("ImageType".into(), image_type.into());
        req.insert("ImageSignature".into(), plist::Value::Data(signature));
        self.idevice.send_plist(plist::Value::Dictionary(req))?;

        let mut res = self.idevice.read_plist()?;
        match res.remove("ImageSignature") {
            Some(plist::Value::Data(i)) => Ok(i),
            _ => Err(IdeviceError::NotFound),
        }
    }
}
