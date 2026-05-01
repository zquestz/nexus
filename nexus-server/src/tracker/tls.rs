//! TLS client connector for outbound publisher connections.
//!
//! The publisher task connects to trackers over TLS, but doesn't rely
//! on certificate-authority validation — trackers run with self-signed
//! certs, and the BBS-side TOFU pinning machinery validates the cert
//! manually after the handshake. So we configure a `rustls`
//! `ClientConfig` with a permissive verifier and disable SNI; the
//! pinning logic in [`super::task`] does the actual security check.
//!
//! Same posture as the BBS client's `nexus-client::network::tls`
//! connector.

use std::sync::Arc;
use std::sync::LazyLock;

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

/// Process-wide TLS connector for outbound publisher connections.
///
/// Lazy-initialized on first use; the rustls `ClientConfig` is shared
/// across all connection attempts (cheap to share — just shape state,
/// no per-connection mutable bits).
pub static TLS_CONNECTOR: LazyLock<TlsConnector> = LazyLock::new(|| {
    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    // SNI off — we accept any cert and TOFU-pin manually after the
    // handshake; SNI would leak the destination hostname for no
    // verification benefit.
    config.enable_sni = false;
    TlsConnector::from(Arc::new(config))
});

/// Permissive cert verifier — accepts any cert. The publisher task
/// validates the observed fingerprint against the row's pinned value
/// (Stage 1) and the server-reported fingerprint in HandshakeResponse
/// (Stage 2); no CA path is involved.
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
