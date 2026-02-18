//! Remote Pairing

use crate::IdeviceError;

use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use ed25519_dalek::Signature;
use hkdf::Hkdf;
use idevice_srp::{client::SrpClient, groups::G_3072};
use plist_macro::plist;
use plist_macro::{PlistConvertible, PlistExt};
use rand::RngCore;
use rsa::{rand_core::OsRng, signature::SignerMut};
use serde::Serialize;
use sha2::Sha512;
use tracing::{debug, warn};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};

mod opack;
mod rp_pairing_file;
mod socket;
mod tlv;

// export
pub use rp_pairing_file::RpPairingFile;
pub use socket::{RpPairingSocket, RpPairingSocketProvider};

const RPPAIRING_MAGIC: &[u8] = b"RPPairing";
const WIRE_PROTOCOL_VERSION: u8 = 19;

pub struct RemotePairingClient<'a, R: RpPairingSocketProvider> {
    inner: R,
    sequence_number: usize,
    pairing_file: &'a mut RpPairingFile,
    sending_host: String,

    client_cipher: ChaCha20Poly1305,
    server_cipher: ChaCha20Poly1305,
}

impl<'a, R: RpPairingSocketProvider> RemotePairingClient<'a, R> {
    pub fn new(inner: R, sending_host: &str, pairing_file: &'a mut RpPairingFile) -> Self {
        let hk = Hkdf::<sha2::Sha512>::new(None, pairing_file.e_private_key.as_bytes());
        let mut okm = [0u8; 32];
        hk.expand(b"ClientEncrypt-main", &mut okm).unwrap();
        let client_cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&okm));

        let hk = Hkdf::<sha2::Sha512>::new(None, pairing_file.e_private_key.as_bytes());
        let mut okm = [0u8; 32];
        hk.expand(b"ServerEncrypt-main", &mut okm).unwrap();
        let server_cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&okm));

        Self {
            inner,
            sequence_number: 0,
            pairing_file,
            sending_host: sending_host.to_string(),

            client_cipher,
            server_cipher,
        }
    }

    pub async fn connect<Fut, S>(
        &mut self,
        pin_callback: impl Fn(S) -> Fut,
        state: S,
    ) -> Result<(), IdeviceError>
    where
        Fut: std::future::Future<Output = String>,
    {
        self.attempt_pair_verify().await?;

        if self.validate_pairing().await.is_err() {
            self.pair(pin_callback, state).await?;
        }
        Ok(())
    }

    pub async fn validate_pairing(&mut self) -> Result<(), IdeviceError> {
        let x_private_key = EphemeralSecret::random_from_rng(OsRng);
        let x_public_key = X25519PublicKey::from(&x_private_key);

        let pairing_data = tlv::serialize_tlv8(&[
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: vec![0x01],
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::PublicKey,
                data: x_public_key.to_bytes().to_vec(),
            },
        ]);
        let pairing_data = R::serialize_bytes(&pairing_data);
        self.send_pairing_data(plist!({
            "data": pairing_data,
            "kind": "verifyManualPairing",
            "startNewSession": true
        }))
        .await?;
        debug!("Waiting for response from verifyManualPairing");

        let pairing_data = self.receive_pairing_data().await?;

        let data = match R::deserialize_bytes(pairing_data) {
            Some(d) => d,
            None => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let data = tlv::deserialize_tlv8(&data)?;

        if data
            .iter()
            .any(|x| x.tlv_type == tlv::PairingDataComponentType::ErrorResponse)
        {
            self.send_pair_verified_failed().await?;
            return Err(IdeviceError::PairVerifyFailed);
        }

        let device_public_key = match data
            .iter()
            .find(|x| x.tlv_type == tlv::PairingDataComponentType::PublicKey)
        {
            Some(d) => d,
            None => {
                warn!("No public key in TLV data");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };
        let peer_pub_bytes: [u8; 32] = match device_public_key.data.as_slice().try_into() {
            Ok(d) => d,
            Err(_) => {
                warn!("Device public key isn't the expected size");
                return Err(IdeviceError::NotEnoughBytes(
                    32,
                    device_public_key.data.len(),
                ));
            }
        };
        let device_public_key = x25519_dalek::PublicKey::from(peer_pub_bytes);
        let shared_secret = x_private_key.diffie_hellman(&device_public_key);

        // Derive encryption key with HKDF-SHA512
        let hk =
            Hkdf::<sha2::Sha512>::new(Some(b"Pair-Verify-Encrypt-Salt"), shared_secret.as_bytes());

        let mut okm = [0u8; 32];
        hk.expand(b"Pair-Verify-Encrypt-Info", &mut okm).unwrap();

        // ChaCha20Poly1305 AEAD cipher
        let cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&okm));

        let ed25519_signing_key = &mut self.pairing_file.e_private_key;

        let mut signbuf = Vec::with_capacity(32 + self.pairing_file.identifier.len() + 32);
        signbuf.extend_from_slice(x_public_key.as_bytes()); // 32 bytes
        signbuf.extend_from_slice(self.pairing_file.identifier.as_bytes()); // variable
        signbuf.extend_from_slice(device_public_key.as_bytes()); // 32 bytes

        let signature: Signature = ed25519_signing_key.sign(&signbuf);

        let plaintext = vec![
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Identifier,
                data: self.pairing_file.identifier.as_bytes().to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Signature,
                data: signature.to_vec(),
            },
        ];
        let plaintext = tlv::serialize_tlv8(&plaintext);
        let nonce = Nonce::from_slice(b"\x00\x00\x00\x00PV-Msg03"); // 12-byte nonce
        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: &plaintext,
                    aad: &[],
                },
            )
            .expect("encryption should not fail");

        let msg = vec![
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: [0x03].to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::EncryptedData,
                data: ciphertext,
            },
        ];

        debug!("Waiting for signbuf response");
        self.send_pairing_data(plist! ({
            "data": R::serialize_bytes(&tlv::serialize_tlv8(&msg)),
            "kind": "verifyManualPairing",
            "startNewSession": false
        }))
        .await?;
        let res = self.receive_pairing_data().await?;

        let data = match R::deserialize_bytes(res) {
            Some(d) => d,
            None => return Err(IdeviceError::UnexpectedResponse),
        };
        let data = tlv::deserialize_tlv8(&data)?;
        debug!("Verify TLV: {data:#?}");

        // Check if the device responded with an error (which is expected for a new pairing)
        if data
            .iter()
            .any(|x| x.tlv_type == tlv::PairingDataComponentType::ErrorResponse)
        {
            debug!(
                "Verification failed, device reported an error. This is expected for a new pairing."
            );
            self.send_pair_verified_failed().await?;
            // Return a specific error to the caller.
            return Err(IdeviceError::PairVerifyFailed);
        }

        Ok(())
    }

    pub async fn send_pair_verified_failed(&mut self) -> Result<(), IdeviceError> {
        self.inner
            .send_plain(
                plist!({
                    "event": {
                        "_0": {
                            "pairVerifyFailed": {}
                        }
                    }
                }),
                self.sequence_number,
            )
            .await?;
        self.sequence_number += 1;
        Ok(())
    }

    pub async fn attempt_pair_verify(&mut self) -> Result<plist::Value, IdeviceError> {
        debug!("Sending attemptPairVerify");
        self.inner
            .send_plain(
                plist!({
                    "request": {
                        "_0": {
                            "handshake": {
                                "_0": {
                                    "hostOptions": {
                                        "attemptPairVerify": true
                                    },
                                    "wireProtocolVersion": plist::Value::Integer(WIRE_PROTOCOL_VERSION.into()),
                                }
                            }
                        }
                    }
                }),
                self.sequence_number,
            )
            .await?;
        self.sequence_number += 1;

        debug!("Waiting for attemptPairVerify response");
        let response = self.inner.recv_plain().await?;

        let response = response
            .as_dictionary()
            .and_then(|x| x.get("response"))
            .and_then(|x| x.as_dictionary())
            .and_then(|x| x.get("_1"))
            .and_then(|x| x.as_dictionary())
            .and_then(|x| x.get("handshake"))
            .and_then(|x| x.as_dictionary())
            .and_then(|x| x.get("_0"));

        match response {
            Some(v) => Ok(v.to_owned()),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn pair<Fut, S>(
        &mut self,
        pin_callback: impl Fn(S) -> Fut,
        state: S,
    ) -> Result<(), IdeviceError>
    where
        Fut: std::future::Future<Output = String>,
    {
        let (salt, public_key, pin) = self.request_pair_consent(pin_callback, state).await?;
        let key = self.init_srp_context(&salt, &public_key, &pin).await?;
        self.save_pair_record_on_peer(&key).await?;

        Ok(())
    }

    /// Returns salt and public key and pin
    async fn request_pair_consent<Fut, S>(
        &mut self,
        pin_callback: impl Fn(S) -> Fut,
        state: S,
    ) -> Result<(Vec<u8>, Vec<u8>, String), IdeviceError>
    where
        Fut: std::future::Future<Output = String>,
    {
        let tlv = tlv::serialize_tlv8(&[
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Method,
                data: vec![0x00],
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: vec![0x01],
            },
        ]);
        let tlv = R::serialize_bytes(&tlv);
        self.send_pairing_data(plist!({
            "data": tlv,
            "kind": "setupManualPairing",
            "sendingHost": &self.sending_host,
            "startNewSession": true
        }))
        .await?;

        let response = self.inner.recv_plain().await?;
        let response = match response
            .get_by("event")
            .and_then(|x| x.get_by("_0"))
            .and_then(|x| x.as_dictionary())
        {
            Some(r) => r,
            None => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let mut pin = None;

        let pairing_data = match if let Some(err) = response.get("pairingRejectedWithError") {
            let context = err
                .get_by("wrappedError")
                .and_then(|x| x.get_by("userInfo"))
                .and_then(|x| x.get_by("NSLocalizedDescription"))
                .and_then(|x| x.as_string())
                .map(|x| x.to_string());
            return Err(IdeviceError::PairingRejected(context.unwrap_or_default()));
        } else if response.get("awaitingUserConsent").is_some() {
            pin = Some("000000".to_string());
            Some(self.receive_pairing_data().await?)
        } else {
            // On Apple TV, we can get the pin now
            response
                .get_by("pairingData")
                .and_then(|x| x.get_by("_0"))
                .and_then(|x| x.get_by("data"))
                .map(|x| x.to_owned())
        } {
            Some(p) => p,
            None => {
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let tlv = tlv::deserialize_tlv8(&match R::deserialize_bytes(pairing_data) {
            Some(t) => t,
            None => return Err(IdeviceError::UnexpectedResponse),
        })?;
        debug!("Received pairingData response: {tlv:#?}");

        let mut salt = Vec::new();
        let mut public_key = Vec::new();
        for t in tlv {
            match t.tlv_type {
                tlv::PairingDataComponentType::Salt => {
                    salt = t.data;
                }
                tlv::PairingDataComponentType::PublicKey => {
                    public_key.extend(t.data);
                }
                tlv::PairingDataComponentType::ErrorResponse => {
                    warn!("Pairing data contained error response");
                    return Err(IdeviceError::UnexpectedResponse);
                }
                _ => {
                    continue;
                }
            }
        }

        let pin = match pin {
            Some(p) => p,
            None => pin_callback(state).await,
        };

        if salt.is_empty() || public_key.is_empty() {
            warn!("Pairing data did not contain salt or public key");
            return Err(IdeviceError::UnexpectedResponse);
        }

        Ok((salt, public_key, pin))
    }

    /// Returns the encryption key
    async fn init_srp_context(
        &mut self,
        salt: &[u8],
        public_key: &[u8],
        pin: &str,
    ) -> Result<Vec<u8>, IdeviceError> {
        let client = SrpClient::<Sha512>::new(
            &G_3072, // PRIME_3072 + generator
        );

        let mut a_private = [0u8; 32];
        rand::rng().fill_bytes(&mut a_private);

        let a_public = client.compute_public_ephemeral(&a_private);

        let verifier = match client.process_reply(
            &a_private,
            "Pair-Setup".as_bytes(),
            &pin.as_bytes()[..6],
            salt,
            public_key,
            false,
        ) {
            Ok(v) => v,
            Err(e) => {
                warn!("SRP verifier creation failed: {e:?}");
                return Err(IdeviceError::SrpAuthFailed);
            }
        };

        let client_proof = verifier.proof();

        let tlv = tlv::serialize_tlv8(&[
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: vec![0x03],
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::PublicKey,
                data: a_public[..254].to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::PublicKey,
                data: a_public[254..].to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Proof,
                data: client_proof.to_vec(),
            },
        ]);
        let tlv = R::serialize_bytes(&tlv);

        self.send_pairing_data(plist!({
            "data": tlv,
            "kind": "setupManualPairing",
            "sendingHost": &self.sending_host,
            "startNewSession": false,

        }))
        .await?;

        let response = self.receive_pairing_data().await?;
        let response = tlv::deserialize_tlv8(&match R::deserialize_bytes(response.to_owned()) {
            Some(r) => r,
            None => return Err(IdeviceError::UnexpectedResponse),
        })?;

        debug!("Proof response: {response:#?}");

        let proof = match response
            .iter()
            .find(|x| x.tlv_type == tlv::PairingDataComponentType::Proof)
        {
            Some(p) => &p.data,
            None => {
                warn!("Proof response did not contain server proof");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        match verifier.verify_server(proof) {
            Ok(_) => Ok(verifier.key().to_vec()),
            Err(e) => {
                warn!("Server auth failed: {e:?}");
                Err(IdeviceError::SrpAuthFailed)
            }
        }
    }

    async fn save_pair_record_on_peer(
        &mut self,
        encryption_key: &[u8],
    ) -> Result<Vec<tlv::TLV8Entry>, IdeviceError> {
        let salt = b"Pair-Setup-Encrypt-Salt";
        let info = b"Pair-Setup-Encrypt-Info";

        let hk = Hkdf::<Sha512>::new(Some(salt), encryption_key);
        let mut setup_encryption_key = [0u8; 32];
        hk.expand(info, &mut setup_encryption_key)
            .expect("HKDF expand failed");

        self.pairing_file.recreate_signing_keys();
        {
            // new scope, update our signing keys
            let hk = Hkdf::<sha2::Sha512>::new(None, self.pairing_file.e_private_key.as_bytes());
            let mut okm = [0u8; 32];
            hk.expand(b"ClientEncrypt-main", &mut okm).unwrap();
            self.client_cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&okm));

            let hk = Hkdf::<sha2::Sha512>::new(None, self.pairing_file.e_private_key.as_bytes());
            let mut okm = [0u8; 32];
            hk.expand(b"ServerEncrypt-main", &mut okm).unwrap();
            self.server_cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&okm));
        }

        let hk = Hkdf::<Sha512>::new(Some(b"Pair-Setup-Controller-Sign-Salt"), encryption_key);

        let mut signbuf = Vec::with_capacity(32 + self.pairing_file.identifier.len() + 32);

        let mut hkdf_out = [0u8; 32];
        hk.expand(b"Pair-Setup-Controller-Sign-Info", &mut hkdf_out)
            .expect("HKDF expand failed");

        signbuf.extend_from_slice(&hkdf_out);

        signbuf.extend_from_slice(self.pairing_file.identifier.as_bytes());
        signbuf.extend_from_slice(self.pairing_file.e_public_key.as_bytes());

        let signature = self.pairing_file.e_private_key.sign(&signbuf);

        let device_info = crate::plist!({
            "altIRK": b"\xe9\xe8-\xc0jIykVoT\x00\x19\xb1\xc7{".to_vec(),
            "btAddr": "11:22:33:44:55:66",
            "mac": b"\x11\x22\x33\x44\x55\x66".to_vec(),
            "remotepairing_serial_number": "AAAAAAAAAAAA",
            "accountID": self.pairing_file.identifier.as_str(),
            "model": "computer-model",
            "name": self.sending_host.as_str()
        });
        let device_info = opack::plist_to_opack(&device_info);

        let tlv = tlv::serialize_tlv8(&[
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Identifier,
                data: self.pairing_file.identifier.as_bytes().to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::PublicKey,
                data: self.pairing_file.e_public_key.to_bytes().to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Signature,
                data: signature.to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Info,
                data: device_info,
            },
        ]);

        let key = Key::from_slice(&setup_encryption_key); // 32 bytes
        let cipher = ChaCha20Poly1305::new(key);

        let nonce = Nonce::from_slice(b"\x00\x00\x00\x00PS-Msg05"); // 12 bytes

        let plaintext = &tlv;

        let ciphertext = match cipher.encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: b"",
            },
        ) {
            Ok(c) => c,
            Err(e) => {
                warn!("Chacha encryption failed: {e:?}");
                return Err(IdeviceError::ChachaEncryption(e));
            }
        };
        debug!("ciphertext len: {}", ciphertext.len());

        let tlv = tlv::serialize_tlv8(&[
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::EncryptedData,
                data: ciphertext[..254].to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::EncryptedData,
                data: ciphertext[254..].to_vec(),
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: vec![0x05],
            },
        ]);
        let tlv = R::serialize_bytes(&tlv);

        debug!("Sending encrypted data");
        self.send_pairing_data(plist!({
            "data": tlv,
            "kind": "setupManualPairing",
            "sendingHost": &self.sending_host,
            "startNewSession": false,
        }))
        .await?;

        debug!("Waiting for encrypted data");
        let response = match R::deserialize_bytes(self.receive_pairing_data().await?) {
            Some(r) => r,
            None => {
                warn!("Pairing data response was not deserializable");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };

        let tlv = tlv::deserialize_tlv8(&response)?;

        let mut encrypted_data = Vec::new();
        for t in tlv {
            match t.tlv_type {
                tlv::PairingDataComponentType::EncryptedData => encrypted_data.extend(t.data),
                tlv::PairingDataComponentType::ErrorResponse => {
                    warn!("TLV contained error response");
                    return Err(IdeviceError::UnexpectedResponse);
                }
                _ => {}
            }
        }

        let nonce = Nonce::from_slice(b"\x00\x00\x00\x00PS-Msg06");

        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &encrypted_data,
                    aad: b"",
                },
            )
            .expect("decryption failure!");

        let tlv = tlv::deserialize_tlv8(&plaintext)?;

        debug!("Decrypted plaintext TLV: {tlv:?}");
        Ok(tlv)
    }

    async fn send_pairing_data(
        &mut self,
        pairing_data: impl Serialize + PlistConvertible,
    ) -> Result<(), IdeviceError> {
        self.inner
            .send_plain(
                plist!({
                    "event": {
                        "_0": {
                            "pairingData": {
                                "_0": pairing_data
                            }
                        }
                    }
                }),
                self.sequence_number,
            )
            .await?;

        self.sequence_number += 1;
        Ok(())
    }

    async fn receive_pairing_data(&mut self) -> Result<plist::Value, IdeviceError> {
        let response = self.inner.recv_plain().await?;

        let response = match response.get_by("event").and_then(|x| x.get_by("_0")) {
            Some(r) => r,
            None => return Err(IdeviceError::UnexpectedResponse),
        };

        if let Some(data) = response
            .get_by("pairingData")
            .and_then(|x| x.get_by("_0"))
            .and_then(|x| x.get_by("data"))
        {
            Ok(data.to_owned())
        } else if let Some(err) = response.get_by("pairingRejectedWithError") {
            let context = err
                .get_by("wrappedError")
                .and_then(|x| x.get_by("userInfo"))
                .and_then(|x| x.get_by("NSLocalizedDescription"))
                .and_then(|x| x.as_string())
                .map(|x| x.to_string());
            Err(IdeviceError::PairingRejected(context.unwrap_or_default()))
        } else {
            Err(IdeviceError::UnexpectedResponse)
        }
    }
}

impl<R: RpPairingSocketProvider> std::fmt::Debug for RemotePairingClient<'_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemotePairingClient")
            .field("inner", &self.inner)
            .field("sequence_number", &self.sequence_number)
            .field("pairing_file", &self.pairing_file)
            .field("sending_host", &self.sending_host)
            .finish()
    }
}
