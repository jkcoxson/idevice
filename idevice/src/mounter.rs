// Jackson Coxson

use crate::{
    lockdownd::LockdowndClient, tss::TSSRequest, util::hashmap_to_dictionary, Idevice,
    IdeviceError, IdeviceService,
};

pub struct ImageMounter {
    idevice: Idevice,
}

impl IdeviceService for ImageMounter {
    fn service_name() -> &'static str {
        "com.apple.mobile.mobile_image_mounter"
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

impl ImageMounter {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    pub async fn copy_devices(&mut self) -> Result<Vec<plist::Value>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "CopyDevices".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;
        let mut res = self.idevice.read_plist().await?;

        match res.remove("EntryList") {
            Some(plist::Value::Array(i)) => Ok(i),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Looks up an image and returns the signature
    pub async fn lookup_image(
        &mut self,
        image_type: impl Into<String>,
    ) -> Result<Vec<u8>, IdeviceError> {
        let image_type = image_type.into();
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "LookupImage".into());
        req.insert("ImageType".into(), image_type.into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        match res.get("ImageSignature") {
            Some(plist::Value::Data(signature)) => Ok(signature.clone()),
            _ => Err(IdeviceError::NotFound),
        }
    }

    pub async fn upload_image(
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
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        match res.get("Status") {
            Some(plist::Value::String(s)) => {
                if s.as_str() != "ReceiveBytesAck" {
                    log::error!("Received bad response to SendBytes: {s:?}");
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            _ => return Err(IdeviceError::UnexpectedResponse),
        }

        self.idevice.send_raw(image).await?;

        let res = self.idevice.read_plist().await?;
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

    pub async fn mount_image(
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
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;

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

    /// Unmounts an image at a specified path.
    /// Use ``/Developer`` for pre-iOS 17 developer images.
    /// Use ``/System/Developer`` for personalized images.
    pub async fn unmount_image(
        &mut self,
        mount_path: impl Into<String>,
    ) -> Result<(), IdeviceError> {
        let mount_path = mount_path.into();
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "UnmountImage".into());
        req.insert("MountPath".into(), mount_path.into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        match res.get("Status") {
            Some(plist::Value::String(s)) if s.as_str() == "Complete" => Ok(()),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Queries the personalization manifest from the device.
    /// On failure, the socket must be closed and reestablished.
    pub async fn query_personalization_manifest(
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
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let mut res = self.idevice.read_plist().await?;
        match res.remove("ImageSignature") {
            Some(plist::Value::Data(i)) => Ok(i),
            _ => Err(IdeviceError::NotFound),
        }
    }

    pub async fn query_developer_mode_status(&mut self) -> Result<bool, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "QueryDeveloperModeStatus".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        match res.get("DeveloperModeStatus") {
            Some(plist::Value::Boolean(status)) => Ok(*status),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn query_nonce(
        &mut self,
        personalized_image_type: Option<String>,
    ) -> Result<Vec<u8>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "QueryNonce".into());
        if let Some(image_type) = personalized_image_type {
            req.insert("PersonalizedImageType".into(), image_type.into());
        }
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        match res.get("PersonalizationNonce") {
            Some(plist::Value::Data(nonce)) => Ok(nonce.clone()),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn query_personalization_identifiers(
        &mut self,
        image_type: Option<String>,
    ) -> Result<plist::Dictionary, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "QueryPersonalizationIdentifiers".into());
        if let Some(image_type) = image_type {
            req.insert("PersonalizedImageType".into(), image_type.into());
        }
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        let res = self.idevice.read_plist().await?;
        match res.get("PersonalizationIdentifiers") {
            Some(plist::Value::Dictionary(identifiers)) => Ok(identifiers.clone()),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn roll_personalization_nonce(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "RollPersonalizationNonce".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        Ok(())
    }

    pub async fn roll_cryptex_nonce(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "RollCryptexNonce".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req))
            .await?;

        Ok(())
    }

    pub async fn mount_developer(
        &mut self,
        image: &[u8],
        signature: Vec<u8>,
    ) -> Result<(), IdeviceError> {
        self.upload_image("Developer", &image, signature.clone())
            .await?;
        self.mount_image(
            "Developer",
            signature,
            Vec::new(),
            plist::Value::Dictionary(plist::Dictionary::new()),
        )
        .await?;

        Ok(())
    }

    pub async fn mount_personalized(
        &mut self,
        image: Vec<u8>,
        trust_cache: Vec<u8>,
        build_manifest: &[u8],
        info_plist: Option<plist::Value>,
        unique_chip_id: u64,
    ) -> Result<(), IdeviceError> {
        // Try to fetch personalization manifest
        let manifest = match self
            .query_personalization_manifest("DeveloperDiskImage", image.clone()) // TODO:
            .await
        {
            Ok(manifest) => manifest,
            Err(IdeviceError::NotFound) => {
                // Get manifest from TSS
                let manifest_dict: plist::Dictionary = plist::from_bytes(build_manifest)?;
                self.get_manifest_from_tss(&manifest_dict, unique_chip_id)
                    .await?
            }
            Err(e) => return Err(e),
        };

        self.upload_image("Personalized", &image, manifest.clone())
            .await?;

        let mut extras = plist::Dictionary::new();
        if let Some(info) = info_plist {
            extras.insert("ImageInfoPlist".into(), info);
        }
        extras.insert(
            "ImageTrustCache".into(),
            plist::Value::Data(trust_cache.clone()),
        );

        self.mount_image(
            "Personalized",
            manifest,
            trust_cache,
            plist::Value::Dictionary(extras),
        )
        .await?;

        Ok(())
    }

    #[cfg(feature = "tss")]
    pub async fn get_manifest_from_tss(
        &mut self,
        build_manifest: &plist::Dictionary,
        unique_chip_id: u64,
    ) -> Result<Vec<u8>, IdeviceError> {
        use log::{debug, warn};

        let mut request = TSSRequest::new();

        let personalization_identifiers = self.query_personalization_identifiers(None).await?;
        for (key, val) in &personalization_identifiers {
            if key.starts_with("Ap,") {
                request.insert(key, val.clone());
            }
        }

        let board_id = match personalization_identifiers.get("BoardId") {
            Some(plist::Value::Integer(b)) => match b.as_unsigned() {
                Some(b) => b,
                None => return Err(IdeviceError::UnexpectedResponse),
            },
            _ => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        };
        let chip_id = match personalization_identifiers.get("ChipID") {
            Some(plist::Value::Integer(b)) => match b.as_unsigned() {
                Some(b) => b,
                None => return Err(IdeviceError::UnexpectedResponse),
            },
            _ => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        request.insert("@ApImg4Ticket", true);
        request.insert("@BBTicket", true);
        request.insert("ApBoardID", board_id);
        request.insert("ApChipID", chip_id);
        request.insert("ApECID", unique_chip_id);
        request.insert(
            "ApNonce",
            plist::Value::Data(
                self.query_nonce(Some("DeveloperDiskImage".to_string()))
                    .await?,
            ),
        );
        request.insert("ApProductionMode", true);
        request.insert("ApSecurityDomain", 1);
        request.insert("ApSecurityMode", true);
        request.insert("SepNonce", plist::Value::Data(vec![0; 20]));
        request.insert("UID_MODE", false);

        let identities = match build_manifest.get("BuildIdentities") {
            Some(plist::Value::Array(i)) => i,
            _ => {
                return Err(IdeviceError::BadBuildManifest);
            }
        };
        let mut build_identity = None;
        for id in identities {
            let id = match id {
                plist::Value::Dictionary(id) => id,
                _ => {
                    debug!("build identity wasn't a dictionary");
                    continue;
                }
            };

            let ap_board_id = match id.get("ApBoardID") {
                Some(plist::Value::String(a)) => a,
                _ => {
                    debug!("Build identity contained no ApBoardID");
                    continue;
                }
            };
            let ap_board_id = match u64::from_str_radix(ap_board_id.trim_start_matches("0x"), 16) {
                Ok(a) => a,
                Err(_) => {
                    debug!("Could not parse {ap_board_id} as usize");
                    continue;
                }
            };
            if ap_board_id != board_id {
                continue;
            }
            let ap_chip_id = match id.get("ApChipID") {
                Some(plist::Value::String(a)) => a,
                _ => {
                    debug!("Build identity contained no ApChipID");
                    continue;
                }
            };
            let ap_chip_id = match u64::from_str_radix(ap_chip_id.trim_start_matches("0x"), 16) {
                Ok(a) => a,
                Err(_) => {
                    debug!("Could not parse {ap_board_id} as usize");
                    continue;
                }
            };
            if ap_chip_id != chip_id {
                continue;
            }
            build_identity = Some(id.to_owned());
            break;
        }

        let build_identity = match build_identity {
            Some(b) => b,
            None => {
                return Err(IdeviceError::BadBuildManifest);
            }
        };

        let manifest = match build_identity.get("Manifest") {
            Some(plist::Value::Dictionary(m)) => m,
            _ => {
                return Err(IdeviceError::BadBuildManifest);
            }
        };

        let mut parameters = plist::Dictionary::new();
        parameters.insert("ApProductionMode".into(), true.into());
        parameters.insert("ApSecurityDomain".into(), 1.into());
        parameters.insert("ApSecurityMode".into(), true.into());
        parameters.insert("ApSupportsImg4".into(), true.into());

        for (key, manifest_item) in manifest {
            let manifest_item = match manifest_item {
                plist::Value::Dictionary(m) => m,
                _ => {
                    debug!("Manifest item wasn't a dictionary");
                    continue;
                }
            };
            let info = match manifest_item.get("Info") {
                Some(plist::Value::Dictionary(i)) => i,
                _ => {
                    debug!("Manifest item didn't contain info");
                    continue;
                }
            };

            match info.get("Trusted") {
                Some(plist::Value::Boolean(t)) => {
                    if !t {
                        debug!("Info item isn't trusted");
                        continue;
                    }
                }
                _ => {
                    debug!("Info didn't contain trusted bool");
                    continue;
                }
            }

            let mut tss_entry = manifest_item.clone();
            tss_entry.remove("Info");

            if let Some(plist::Value::Dictionary(l)) = manifest.get("LoadableTrustCache") {
                if let Some(plist::Value::Dictionary(i)) = l.get("Info") {
                    if let Some(plist::Value::Array(rules)) = i.get("RestoreRequestRules") {
                        crate::tss::apply_restore_request_rules(&mut tss_entry, &parameters, rules);
                    }
                }
            }

            if manifest_item.get("Digest").is_none() {
                tss_entry.insert("Digest".into(), plist::Value::Data(vec![]));
            }

            request.insert(key, tss_entry);
        }
        let res = request.send().await?;
        let mut res = match res {
            plist::Value::Dictionary(r) => r,
            _ => {
                warn!("Apple returned a non-dictionary plist");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        match res.remove("ApImg4Ticket") {
            Some(plist::Value::Data(d)) => Ok(d),
            _ => {
                warn!("TSS response didn't contain ApImg4Ticket data");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}
