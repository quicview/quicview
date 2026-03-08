use std::sync::Arc;

use rustls::crypto::ring as ring_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};

use crate::error::TransportError;

/// Maximum control message payload size (1 MiB) to prevent DoS via huge allocations.
pub const MAX_CONTROL_MESSAGE_SIZE: usize = 1024 * 1024;

/// SHA-256 fingerprint of a DER-encoded certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertFingerprint([u8; 32]);

impl CertFingerprint {
    /// Compute the SHA-256 fingerprint of a DER-encoded certificate.
    pub fn from_der(der: &[u8]) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, der);
        let mut fp = [0u8; 32];
        fp.copy_from_slice(digest.as_ref());
        Self(fp)
    }

    /// Return the fingerprint bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Display the fingerprint as a hex string.
    pub fn to_hex(&self) -> String {
        self.0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

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

    /// Compute the SHA-256 fingerprint of this certificate.
    pub fn fingerprint(&self) -> CertFingerprint {
        CertFingerprint::from_der(self.cert_der.as_ref())
    }

    /// Build a [`quinn::ServerConfig`] from this certificate.
    pub fn server_config(&self) -> Result<quinn::ServerConfig, TransportError> {
        let provider = Arc::new(ring_provider::default_provider());
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
    /// # Security
    ///
    /// This disables all certificate validation. Only use on trusted local
    /// networks or during development. For production, prefer
    /// [`Self::pinned_client_config`] with a known server fingerprint.
    pub fn client_config() -> Result<quinn::ClientConfig, TransportError> {
        tracing::warn!(
            "using SkipServerVerification — all server certificates are accepted without validation"
        );
        let provider = Arc::new(ring_provider::default_provider());
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

    /// Build a [`quinn::ClientConfig`] that validates the server certificate
    /// by SHA-256 fingerprint (certificate pinning).
    ///
    /// This is the recommended approach when connecting to a known host that
    /// uses self-signed certificates.
    pub fn pinned_client_config(
        expected_fingerprint: CertFingerprint,
    ) -> Result<quinn::ClientConfig, TransportError> {
        let provider = Arc::new(ring_provider::default_provider());
        let rustls_config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| TransportError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(FingerprintVerifier {
                expected: expected_fingerprint,
            }))
            .with_no_client_auth();

        let quinn_config = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
            .map_err(|e| TransportError::Tls(e.to_string()))?;

        Ok(quinn::ClientConfig::new(Arc::new(quinn_config)))
    }
}

/// Certificate verifier that accepts any server certificate.
///
/// # Security
///
/// **WARNING:** This disables all TLS server authentication. A man-in-the-middle
/// attacker can present any certificate and the connection will succeed.
/// Only use on trusted local networks or during development.
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
        ring_provider::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Certificate verifier that validates the server certificate by SHA-256 fingerprint.
#[derive(Debug)]
struct FingerprintVerifier {
    expected: CertFingerprint,
}

impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual = CertFingerprint::from_der(end_entity.as_ref());
        if actual == self.expected {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch: expected {}, got {}",
                self.expected.to_hex(),
                actual.to_hex()
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &ring_provider::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &ring_provider::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        ring_provider::default_provider()
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

    #[test]
    fn build_pinned_client_config() {
        let cert = SelfSignedCert::generate(&["localhost"]).unwrap();
        let fp = cert.fingerprint();
        assert!(SelfSignedCert::pinned_client_config(fp).is_ok());
    }

    #[test]
    fn fingerprint_deterministic() {
        let cert = SelfSignedCert::generate(&["localhost"]).unwrap();
        let fp1 = cert.fingerprint();
        let fp2 = cert.fingerprint();
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_hex_format() {
        let cert = SelfSignedCert::generate(&["localhost"]).unwrap();
        let hex = cert.fingerprint().to_hex();
        // SHA-256 = 32 bytes = 32 hex pairs + 31 colons = 95 chars
        assert_eq!(hex.len(), 95);
        assert!(hex.contains(':'));
    }
}
