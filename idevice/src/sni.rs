// Jackson Coxson
// https://gist.github.com/doroved/2c92ddd5e33f257f901c763b728d1b61
//
// Why:
// https://github.com/rustls/rustls/issues/578
// We aren't in control of the certs served.
// Assuming that there's no use for unchecked certs is naive.

use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer, ServerName, UnixTime},
    ClientConfig, DigitallySignedStruct,
};
use std::sync::Arc;

use crate::{pairing_file::PairingFile, IdeviceError};

/// A completely permissive certificate verifier that bypasses all validation
/// This mimics OpenSSL's CERT_NONE behavior for iOS device compatibility
#[derive(Debug)]
pub struct NoServerNameVerification;

impl NoServerNameVerification {
    /// Create the most permissive verifier to match OpenSSL CERT_NONE behavior
    pub fn new_permissive() -> Self {
        Self
    }
}

impl ServerCertVerifier for NoServerNameVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        // Return all possible signature schemes for maximum iOS compatibility
        // This is especially important for iOS 18.5+ which may have changed signature requirements
        // Matches OpenSSL's ALL cipher behavior
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA1,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}

pub fn create_client_config(pairing_file: &PairingFile) -> Result<ClientConfig, IdeviceError> {
    log::debug!(
        "Creating TLS client config for iOS 18.5+ compatibility with maximum permissiveness"
    );
    // iOS 18.5 introduced stricter certificate validation that breaks with standard rustls configuration.
    // This configuration mimics OpenSSL's @SECLEVEL=0 behavior used by pymobiledevice3 for compatibility.

    // Create an empty root store - we'll bypass all certificate validation
    let _root_store = rustls::RootCertStore::empty();
    let private_key = PrivateKeyDer::from_pem_slice(&pairing_file.host_private_key)?;
    log::debug!("Successfully loaded client certificate and private key");

    // Use the most permissive configuration possible to match OpenSSL @SECLEVEL=0 behavior
    let mut config = ClientConfig::builder()
        .dangerous() // Enable dangerous configuration options
        .with_custom_certificate_verifier(Arc::new(NoServerNameVerification::new_permissive()))
        .with_client_auth_cert(vec![pairing_file.host_certificate.clone()], private_key)
        .unwrap();

    // Configure for maximum iOS compatibility, similar to OpenSSL @SECLEVEL=0
    config.resumption = rustls::client::Resumption::disabled();

    log::debug!("Configured rustls with maximum permissiveness for iOS 18.5 compatibility");

    Ok(config)
}
