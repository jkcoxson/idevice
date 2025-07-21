// Jackson Coxson

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit as _, Nonce,
};
use ed25519_dalek::{Signature, SigningKey};
use hkdf::Hkdf;
use json::{object, JsonValue};
use log::{debug, warn};
use rp_pairing_file::RpPairingFile;
use rsa::signature::SignerMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{IdeviceError, ReadWrite};

pub mod rp_pairing_file;
mod tlv;

const RP_MAGIC: &str = "RPPairing";

pub struct RPPairingClient<R: ReadWrite> {
    socket: R,
    sequence_number: usize,
}

impl<R: ReadWrite> RPPairingClient<R> {
    pub fn new(socket: R) -> Self {
        Self {
            socket,
            sequence_number: 0,
        }
    }

    pub async fn handshake(&mut self) -> Result<(), IdeviceError> {
        let req = object! {
            "request": {
                "_0": {
                    "handshake": {
                        "_0": {
                            "hostOptions": {
                                "attemptPairVerify": true
                            },
                            "wireProtocolVersion": 24
                        }
                    }
                }
            }
        };
        self.send_plain(req).await?;
        let res = self.read_json().await?;
        debug!("Handshake response: {res:#}");
        Ok(())
    }

    pub async fn pair(&mut self) -> Result<RpPairingFile, IdeviceError> {
        let pairing = RpPairingFile::generate();

        // M1 for a NEW pairing
        let t = vec![
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Method,
                data: vec![0x00],
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: vec![0x01],
            },
        ];
        let t = B64.encode(tlv::serialize_tlv8(&t));

        self.send_pairing_data(object! {
            "data": t,
            "kind": "setupManualPairing",
            "sendingHost": "Mac",
            "startNewSession": true,
        })
        .await?;

        let res = self.read_event_data().await?;
        debug!("Pair (M1) res: {res:#?}");

        // M2: Now you handle the SRP steps...
        todo!("Implement SRP steps using the device's public key and salt from the response");
    }

    pub async fn validate_pairing(&mut self, pairing: RpPairingFile) -> Result<(), IdeviceError> {
        let pairing_data = tlv::serialize_tlv8(&[
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::State,
                data: vec![0x01],
            },
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::PublicKey,
                data: pairing.x_public_key.to_bytes().to_vec(),
            },
        ]);
        let pairing_data = B64.encode(pairing_data);

        let req = object! {
            "event": {
                "_0": {
                    "pairingData": {
                        "_0": {
                            "data": pairing_data,
                            "kind": "verifyManualPairing",
                            "startNewSession": true
                        }
                    }
                }
            }
        };
        self.send_plain(req).await?;
        let res = self.read_json().await?;
        debug!("Public key response: {res:#}");
        let data =
            &res["message"]["plain"]["_0"]["event"]["_0"]["pairingData"]["_0"]["data"].as_str();
        let data = match data {
            Some(d) => d,
            None => {
                warn!("RPPairing validate pair message didn't contain pairingData -> _0 -> data");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };
        let data = B64.decode(data)?;
        let data = tlv::deserialize_tlv8(&data)?;
        println!("{data:#?}");

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
        let shared_secret = pairing.x_private_key.diffie_hellman(&device_public_key);

        // Derive encryption key with HKDF-SHA512
        let hk =
            Hkdf::<sha2::Sha512>::new(Some(b"Pair-Verify-Encrypt-Salt"), shared_secret.as_bytes());

        let mut okm = [0u8; 32];
        hk.expand(b"Pair-Verify-Encrypt-Info", &mut okm).unwrap();

        // ChaCha20Poly1305 AEAD cipher
        let cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&okm));

        let mut ed25519_signing_key = pairing.e_private_key;

        let mut signbuf = Vec::with_capacity(32 + pairing.identifier.len() + 32);
        signbuf.extend_from_slice(pairing.x_public_key.as_bytes()); // 32 bytes
        signbuf.extend_from_slice(pairing.identifier.as_bytes()); // variable
        signbuf.extend_from_slice(device_public_key.as_bytes()); // 32 bytes

        let signature: Signature = ed25519_signing_key.sign(&signbuf);

        let plaintext = vec![
            tlv::TLV8Entry {
                tlv_type: tlv::PairingDataComponentType::Identifier,
                data: pairing.identifier.as_bytes().to_vec(),
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

        let msg = object! {"event": {"_0": {"pairingData": {"_0": {
        "data": B64.encode(tlv::serialize_tlv8(&msg)),
        "kind": "verifyManualPairing",
        "startNewSession": false}}}}};

        self.send_plain(msg).await?;

        let res = self.read_json().await?;
        debug!("Verify response: {res:#}");

        let data =
            &res["message"]["plain"]["_0"]["event"]["_0"]["pairingData"]["_0"]["data"].as_str();
        let data = match data {
            Some(d) => d,
            None => {
                warn!("RPPairing validate pair message didn't contain pairingData -> _0 -> data");
                return Err(IdeviceError::UnexpectedResponse);
            }
        };
        let data = B64.decode(data)?;
        let data = tlv::deserialize_tlv8(&data)?;
        println!("{data:#?}");

        // Check if the device responded with an error (which is expected for a new pairing)
        if data
            .iter()
            .any(|x| x.tlv_type == tlv::PairingDataComponentType::ErrorResponse)
        {
            debug!("Verification failed, device reported an error. This is expected for a new pairing.");
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            // Tell the device we are aborting the verification attempt.
            let msg = object! {"event": {"_0": {"pairVerifyFailed": {}}}};
            self.send_plain(msg).await?;

            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            self.pair().await?;
            // Return a specific error to the caller.
            return Err(IdeviceError::PairVerifyFailed);
        }

        Ok(())
    }

    async fn send_pairing_data(&mut self, data: JsonValue) -> Result<(), IdeviceError> {
        self.send_event(object! {
            "pairingData": {
                "_0": data
            }
        })
        .await
    }

    async fn send_event(&mut self, data: JsonValue) -> Result<(), IdeviceError> {
        let req = object! {
            "event": {
                "_0": data
            }
        };
        self.send_plain(req).await
    }

    async fn read_event_data(&mut self) -> Result<Vec<tlv::TLV8Entry>, IdeviceError> {
        let res = self.read_json().await?;
        match &res["message"]["plain"]["_0"]["event"]["_0"]["pairingData"]["_0"]["data"].as_str() {
            Some(r) => Ok(tlv::deserialize_tlv8(&B64.decode(r)?)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    async fn send_plain(&mut self, data: JsonValue) -> Result<(), IdeviceError> {
        let req = object! {
            sequenceNumber: self.sequence_number,
            originatedBy: "host",
            message: {
                plain: {
                    _0: data
                }
            }
        };
        debug!("Sending {req:#}");

        self.sequence_number += 1;
        self.send_json(req).await?;
        Ok(())
    }

    async fn send_json(&mut self, data: JsonValue) -> Result<(), IdeviceError> {
        // Send the magic
        self.socket.write_all(RP_MAGIC.as_bytes()).await?;

        // Packet length
        let data = data.to_string().into_bytes();
        self.socket.write_u16(data.len() as u16).await?; // big endian

        self.socket.write_all(&data).await?;

        Ok(())
    }

    async fn read_json(&mut self) -> Result<JsonValue, IdeviceError> {
        // Read the magic
        let mut magic_buf = [0u8; RP_MAGIC.len()];
        self.socket.read_exact(&mut magic_buf).await?;

        // Read JSON length
        let len = self.socket.read_u16().await?;

        let mut buf = vec![0u8; len as usize];
        self.socket.read_exact(&mut buf).await?;

        let data = String::from_utf8_lossy(&buf);
        Ok(json::parse(&data)?)
    }
}
