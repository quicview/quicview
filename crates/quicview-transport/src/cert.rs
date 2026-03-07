use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};

use crate::error::TransportError;

/// A self-signed TLS certificate for QUIC connections.
pub struct SelfSignedCert {
    cert_der: CertificateDer<'static>,
    key_der: PrivateKeyDer<'static>,
}

impl SelfSignedCert {
    /// Generate a new self-signed certificate for the given subject alternative names.
    pub fn generate(subject_alt_names: &[&str]) -> Result<Self, TransportError> {
        let names: Vec<String> = subject_alt_names.iter().map(|s| s.to_string()).collect();
        let certified_key = rcgen::generate_simple_self_signed(names)
            .map_err(|e| TransportError::Tls(e.to_string()))?;

        let cert_der = CertificateDer::from(certified_key.cert.der().as_ref().to_vec());
        let key_der =
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(certified_key.key_pair.serialize_der()));

        Ok(Self { cert_der, key_der })
    }

    /// Build a [`quinn::ServerConfig`] from this certificate.
    pub fn server_config(&self) -> Result<quinn::ServerConfig, TransportError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let rustls_config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| TransportError::Tls(e.to_string()))?
            .with_no_client_auth()
            .with_single_cert(vec![self.cert_der.clone()], self.key_der.clone_key())
            .map_err(|e| TransportError::Tls(e.to_string()))?;

        let quinn_config = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)
            .map_err(|e| TransportError::Tls(e.to_string()))?;

        Ok(quinn::ServerConfig::with_crypto(Arc::new(quinn_config)))
    }

    /// Build a [`quinn::ClientConfig`] that skips server certificate verification.
    ///
    /// Suitable for connecting to peers using self-signed certificates on
    /// trusted networks or during development.
    pub fn client_config() -> Result<quinn::ClientConfig, TransportError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let rustls_config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| TransportError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        let quinn_config = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
            .map_err(|e| TransportError::Tls(e.to_string()))?;

        Ok(quinn::ClientConfig::new(Arc::new(quinn_config)))
    }
}

/// Certificate verifier that accepts any server certificate.
/// Used for self-signed certificate scenarios on trusted networks.
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_self_signed_cert() {
        let cert = SelfSignedCert::generate(&["localhost"]).unwrap();
        assert!(!cert.cert_der.as_ref().is_empty());
    }

    #[test]
    fn build_server_config() {
        let cert = SelfSignedCert::generate(&["localhost"]).unwrap();
        assert!(cert.server_config().is_ok());
    }

    #[test]
    fn build_client_config() {
        assert!(SelfSignedCert::client_config().is_ok());
    }
}
