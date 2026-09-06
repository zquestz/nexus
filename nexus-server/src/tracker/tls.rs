//! TLS client connector for outbound tracker registration connections.
//!
//! Trackers run self-signed certs, so there's no CA path: the connector
//! verifies handshake signatures but defers certificate trust to TOFU
//! pinning in [`super::task`]. SNI is disabled. Same posture as the BBS
//! client's `nexus-client::network::tls`.

use std::sync::Arc;
use std::sync::LazyLock;

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature,
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
pub static TLS_CONNECTOR: LazyLock<TlsConnector> =
    LazyLock::new(|| TlsConnector::from(Arc::new(create_tls_config())));

fn create_tls_config() -> ClientConfig {
    let builder = ClientConfig::builder();
    let verifier = TofuVerifier {
        supported_algorithms: builder.crypto_provider().signature_verification_algorithms,
    };
    let mut config = builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    // SNI off — we TOFU-pin manually post-handshake, so SNI would only
    // leak the destination hostname for no verification benefit.
    config.enable_sni = false;
    config
}

/// Verifies possession of the certificate's private key without a CA path.
/// The task validates the observed fingerprint vs the row's pin (Stage 1)
/// and the server-reported one in HandshakeResponse (Stage 2).
#[derive(Debug)]
struct TofuVerifier {
    supported_algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for TofuVerifier {
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
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(message, cert, dss, &self.supported_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(message, cert, dss, &self.supported_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use rcgen::{CertificateParams, KeyPair};
    use tokio_rustls::rustls::crypto::ring;
    use tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer;
    use tokio_rustls::rustls::sign::{CertifiedKey, SingleCertAndKey};
    use tokio_rustls::rustls::version::{TLS12, TLS13};
    use tokio_rustls::rustls::{
        CertificateError, ClientConnection, InconsistentKeys, ServerConfig, ServerConnection,
        SupportedProtocolVersion,
    };

    use super::*;

    const MAX_HANDSHAKE_ROUNDS: usize = 8;

    fn handshake(
        version: &'static SupportedProtocolVersion,
        is_wrong_key: bool,
    ) -> Result<(), RustlsError> {
        let _ = ring::default_provider().install_default();
        let client_config = create_tls_config();
        assert!(!client_config.enable_sni);
        let provider = Arc::clone(client_config.crypto_provider());

        let certificate_key = KeyPair::generate().expect("generate certificate key");
        let certificate = CertificateParams::new(vec!["certificate.example".to_owned()])
            .expect("create certificate parameters")
            .self_signed(&certificate_key)
            .expect("generate self-signed certificate");
        let signing_key = if is_wrong_key {
            KeyPair::generate().expect("generate unrelated signing key")
        } else {
            certificate_key
        };
        let signing_key = provider
            .key_provider
            .load_private_key(PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into())
            .expect("load signing key");
        let certified_key = CertifiedKey::new(vec![certificate.der().clone()], signing_key);
        if is_wrong_key {
            assert_eq!(
                certified_key.keys_match(),
                Err(RustlsError::InconsistentKeys(InconsistentKeys::KeyMismatch))
            );
        } else {
            certified_key
                .keys_match()
                .expect("matching certificate key");
        }

        // The resolver deliberately permits a mismatched key, so the client
        // must reject the handshake rather than the server rejecting setup.
        let server_config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[version])
            .expect("enable requested TLS version")
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(SingleCertAndKey::from(certified_key)));
        let mut server = ServerConnection::new(Arc::new(server_config)).expect("create server");
        let mut client = ClientConnection::new(
            Arc::new(client_config),
            ServerName::try_from("different.example").expect("valid test hostname"),
        )
        .expect("create client");

        for _ in 0..MAX_HANDSHAKE_ROUNDS {
            let mut client_records = Vec::new();
            client
                .write_tls(&mut client_records)
                .expect("write client TLS records");
            let mut incoming = client_records.as_slice();
            while !incoming.is_empty() {
                assert!(server.read_tls(&mut incoming).expect("read client records") > 0);
                server
                    .process_new_packets()
                    .expect("server accepts client handshake");
            }

            let mut server_records = Vec::new();
            server
                .write_tls(&mut server_records)
                .expect("write server TLS records");
            let mut incoming = server_records.as_slice();
            while !incoming.is_empty() {
                assert!(client.read_tls(&mut incoming).expect("read server records") > 0);
                client.process_new_packets()?;
            }

            if !client.is_handshaking() && !server.is_handshaking() {
                assert_eq!(client.protocol_version(), Some(version.version));
                assert_eq!(server.protocol_version(), Some(version.version));
                assert!(server.server_name().is_none());
                assert_eq!(
                    client.peer_certificates(),
                    Some(std::slice::from_ref(certificate.der()))
                );
                return Ok(());
            }
        }
        panic!("handshake did not complete within the round limit");
    }

    #[test]
    fn test_tls12_accepts_self_signed_handshake() {
        handshake(&TLS12, false).expect("valid TLS 1.2 handshake");
    }

    #[test]
    fn test_tls13_accepts_self_signed_handshake() {
        handshake(&TLS13, false).expect("valid TLS 1.3 handshake");
    }

    #[test]
    fn test_tls12_rejects_wrong_signing_key() {
        assert_eq!(
            handshake(&TLS12, true),
            Err(RustlsError::InvalidCertificate(
                CertificateError::BadSignature
            ))
        );
    }

    #[test]
    fn test_tls13_rejects_wrong_signing_key() {
        assert_eq!(
            handshake(&TLS13, true),
            Err(RustlsError::InvalidCertificate(
                CertificateError::BadSignature
            ))
        );
    }

    #[test]
    fn test_verify_schemes_follow_supplied_algorithms() {
        let mut supported_algorithms = ring::default_provider().signature_verification_algorithms;
        supported_algorithms.mapping = &supported_algorithms.mapping[1..];
        let verifier = TofuVerifier {
            supported_algorithms,
        };
        assert_eq!(
            verifier.supported_verify_schemes(),
            supported_algorithms.supported_schemes()
        );
    }
}
