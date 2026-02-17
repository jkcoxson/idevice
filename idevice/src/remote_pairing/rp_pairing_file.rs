// Jackson Coxson

use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};
use plist::Dictionary;
use plist_macro::plist_to_xml_bytes;
use rsa::rand_core::OsRng;
use serde::de::Error;
use tracing::{debug, warn};

use crate::IdeviceError;

#[derive(Clone)]
pub struct RpPairingFile {
    pub(crate) e_private_key: SigningKey,
    pub(crate) e_public_key: VerifyingKey,
    pub(crate) identifier: String,
}

impl RpPairingFile {
    pub fn generate(sending_host: &str) -> Self {
        // Ed25519 private key (persistent signing key)
        let ed25519_private_key = SigningKey::generate(&mut OsRng);
        let ed25519_public_key = VerifyingKey::from(&ed25519_private_key);

        let identifier =
            uuid::Uuid::new_v3(&uuid::Uuid::NAMESPACE_DNS, sending_host.as_bytes()).to_string();

        Self {
            e_private_key: ed25519_private_key,
            e_public_key: ed25519_public_key,
            identifier,
        }
    }

    pub(crate) fn recreate_signing_keys(&mut self) {
        let ed25519_private_key = SigningKey::generate(&mut OsRng);
        let ed25519_public_key = VerifyingKey::from(&ed25519_private_key);
        self.e_public_key = ed25519_public_key;
        self.e_private_key = ed25519_private_key;
    }

    pub async fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), IdeviceError> {
        let v = crate::plist!(dict {
            "public_key": self.e_public_key.to_bytes().to_vec(),
            "private_key": self.e_private_key.to_bytes().to_vec(),
            "identifier": self.identifier.as_str()
        });
        tokio::fs::write(path, plist_to_xml_bytes(&v)).await?;

        Ok(())
    }

    pub async fn read_from_file(path: impl AsRef<Path>) -> Result<Self, IdeviceError> {
        let s = tokio::fs::read_to_string(path).await?;
        let mut p: Dictionary = plist::from_bytes(s.as_bytes())?;
        debug!("Read dictionary for rppairingfile: {p:#?}");

        let public_key = match p
            .remove("public_key")
            .and_then(|x| x.into_data())
            .filter(|x| x.len() == 32)
            .and_then(|x| VerifyingKey::from_bytes(&x[..32].try_into().unwrap()).ok())
        {
            Some(p) => p,
            None => {
                warn!("plist did not contain valid public key bytes");
                return Err(IdeviceError::Plist(plist::Error::missing_field(
                    "public_key",
                )));
            }
        };

        let private_key = match p
            .remove("private_key")
            .and_then(|x| x.into_data())
            .filter(|x| x.len() == 32)
        {
            Some(p) => SigningKey::from_bytes(&p.try_into().unwrap()),
            None => {
                warn!("plist did not contain valid private key bytes");
                return Err(IdeviceError::Plist(plist::Error::missing_field(
                    "private_key",
                )));
            }
        };

        let identifier = match p.remove("identifier").and_then(|x| x.into_string()) {
            Some(i) => i,
            None => {
                warn!("plist did not contain identifier");
                return Err(IdeviceError::Plist(plist::Error::missing_field(
                    "identifier",
                )));
            }
        };

        Ok(Self {
            e_private_key: private_key,
            e_public_key: public_key,
            identifier,
        })
    }
}

impl std::fmt::Debug for RpPairingFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpPairingFile")
            .field("e_public_key", &self.e_public_key)
            .field("identifier", &self.identifier)
            .finish()
    }
}
