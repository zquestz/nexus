//! TLS client connector for outbound tracker registration connections.
//!
//! Trackers run self-signed certs, so there's no CA path: the connector
//! uses a permissive verifier and disables SNI, and TOFU pinning in
//! [`super::task`] does the actual post-handshake security check. Same
//! posture as the BBS client's `nexus-client::network::tls`.

use std::sync::Arc;
use std::sync::LazyLock;

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

/// Process-wide TLS connector for outbound tracker registration connections.
///
/// Lazy; the `ClientConfig` is shared across attempts (immutable shape state).
///
/// **Caller contract:** a rustls crypto provider must be installed before
/// first access — `main.rs` at startup (before any task spawns), tests in
/// `MockTracker::start`. Triggering init earlier leaves rustls on its
/// current default rather than the one this codebase chose.
pub static TLS_CONNECTOR: LazyLock<TlsConnector> = LazyLock::new(|| {
    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    // SNI off — we TOFU-pin manually post-handshake, so SNI would only
    // leak the destination hostname for no verification benefit.
    config.enable_sni = false;
    TlsConnector::from(Arc::new(config))
});

/// Permissive verifier — accepts any cert; no CA path. The task validates
/// the observed fingerprint vs the row's pin (Stage 1) and the
/// server-reported one in HandshakeResponse (Stage 2).
#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}
