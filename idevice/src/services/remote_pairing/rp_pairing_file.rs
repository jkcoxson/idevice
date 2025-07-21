// Jackson Coxson

use ed25519_dalek::{SigningKey, VerifyingKey};
use rsa::rand_core::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};

pub struct RpPairingFile {
    pub(crate) x_private_key: EphemeralSecret,
    pub(crate) x_public_key: X25519PublicKey,
    pub(crate) e_private_key: SigningKey,
    pub(crate) e_public_key: VerifyingKey,
    pub(crate) identifier: String,
}

impl RpPairingFile {
    pub fn generate() -> Self {
        // X25519 private key (ephemeral)
        let x25519_private_key = EphemeralSecret::random_from_rng(OsRng);
        let x25519_public_key = X25519PublicKey::from(&x25519_private_key);

        // Ed25519 private key (persistent signing key)
        let ed25519_private_key = SigningKey::generate(&mut OsRng);
        let ed25519_public_key = VerifyingKey::from(&ed25519_private_key);

        let identifier = uuid::Uuid::new_v4().to_string().to_uppercase();

        Self {
            x_private_key: x25519_private_key,
            x_public_key: x25519_public_key,
            e_private_key: ed25519_private_key,
            e_public_key: ed25519_public_key,
            identifier,
        }
    }
}
