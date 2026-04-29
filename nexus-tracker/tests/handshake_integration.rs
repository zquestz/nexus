//! End-to-end TCP + TLS + handshake test for `nexus-tracker`.
//!
//! Spins up the daemon's connection task on a TCP listener, drives a
//! real TLS handshake against it, and exchanges the BBS-protocol
//! `Handshake` / `HandshakeResponse` over the wire. The intent is to
//! prove the listener + connection + handshake-handler stack works
//! together — the unit tests for `handlers::handshake` cover the
//! decision logic in isolation.
//!
//! The client uses a permissive cert verifier (accepts anything) since
//! the tracker's cert is self-signed and the test doesn't yet need to
//! exercise TOFU semantics.

use std::sync::Arc;

use nexus_common::TRACKER_PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{read_server_message, send_client_message};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme,
};

#[derive(Debug)]
struct AcceptAnyCert;

impl ServerCertVerifier for AcceptAnyCert {
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

/// Install the rustls crypto provider once. Safe to call from multiple
/// tests in the same process — second and subsequent calls return Err
/// (already installed) which we ignore.
fn ensure_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
}

fn build_test_client() -> TlsConnector {
    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
        .with_no_client_auth();
    config.enable_sni = false;
    TlsConnector::from(Arc::new(config))
}

/// Drive one accepted connection through the tracker's connection task
/// in a background tokio task. Returns the cert fingerprint for the
/// caller to compare against the wire response.
async fn spawn_one_shot_tracker(data_dir: &std::path::Path) -> (std::net::SocketAddr, String) {
    let fingerprint = nexus_tracker::tls::ensure_cert(data_dir).expect("ensure_cert");
    let acceptor = nexus_tracker::tls::build_acceptor(data_dir).expect("build_acceptor");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let local_addr = listener.local_addr().expect("local_addr");

    let fp_clone = fingerprint.clone();
    tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.expect("accept");
        // The handler now returns Result; tests don't care about the
        // error path here (the test harness asserts on the wire response).
        let _ =
            nexus_tracker::connection::handle_connection(stream, peer, acceptor, fp_clone).await;
    });

    (local_addr, fingerprint)
}

#[tokio::test]
async fn test_handshake_roundtrip_compatible_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let (server_addr, expected_fingerprint) = spawn_one_shot_tracker(tmp.path()).await;

    // Client side
    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("connect");
    let server_name = ServerName::try_from("localhost").expect("server name");
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls connect");

    let (read, write) = tokio::io::split(tls);
    let mut reader = FrameReader::new(read);
    let mut writer = FrameWriter::new(write);

    send_client_message(
        &mut writer,
        &ClientMessage::Handshake {
            version: TRACKER_PROTOCOL_VERSION.to_string(),
        },
    )
    .await
    .expect("send Handshake");

    let received = read_server_message(&mut reader)
        .await
        .expect("read response")
        .expect("frame");

    match received.message {
        ServerMessage::HandshakeResponse {
            success,
            version,
            fingerprint,
            error,
        } => {
            assert!(success, "compatible version should succeed");
            assert!(error.is_none(), "no error expected on success");
            assert_eq!(
                version,
                Some(TRACKER_PROTOCOL_VERSION.to_string()),
                "server should report its own version"
            );
            assert_eq!(
                fingerprint, expected_fingerprint,
                "server-reported fingerprint should match the cert on disk"
            );
        }
        other => panic!("expected HandshakeResponse, got {other:?}"),
    }
}

#[tokio::test]
async fn test_handshake_roundtrip_incompatible_major_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let (server_addr, expected_fingerprint) = spawn_one_shot_tracker(tmp.path()).await;

    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("connect");
    let server_name = ServerName::try_from("localhost").expect("server name");
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls connect");

    let (read, write) = tokio::io::split(tls);
    let mut reader = FrameReader::new(read);
    let mut writer = FrameWriter::new(write);

    // Tracker is currently 0.x; 1.0.0 is a major mismatch.
    send_client_message(
        &mut writer,
        &ClientMessage::Handshake {
            version: "1.0.0".to_string(),
        },
    )
    .await
    .expect("send Handshake");

    let received = read_server_message(&mut reader)
        .await
        .expect("read response")
        .expect("frame");

    match received.message {
        ServerMessage::HandshakeResponse {
            success,
            fingerprint,
            error,
            ..
        } => {
            assert!(!success, "major mismatch should fail");
            let err = error.expect("error message expected");
            assert!(
                err.contains("Incompatible tracker protocol version"),
                "expected mismatch message, got: {err}"
            );
            // Fingerprint must still be sent on failure (per spec).
            assert_eq!(fingerprint, expected_fingerprint);
        }
        other => panic!("expected HandshakeResponse, got {other:?}"),
    }
}

#[tokio::test]
async fn test_non_handshake_first_message_yields_error() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let (server_addr, _) = spawn_one_shot_tracker(tmp.path()).await;

    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("connect");
    let server_name = ServerName::try_from("localhost").expect("server name");
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls connect");

    let (read, write) = tokio::io::split(tls);
    let mut reader = FrameReader::new(read);
    let mut writer = FrameWriter::new(write);

    // Send a Login message before completing the handshake — protocol
    // violation, expecting Error in response.
    send_client_message(
        &mut writer,
        &ClientMessage::Login {
            username: "alice".to_string(),
            password: "hunter2".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: None,
        },
    )
    .await
    .expect("send Login");

    let received = read_server_message(&mut reader)
        .await
        .expect("read response")
        .expect("frame");

    match received.message {
        ServerMessage::Error { message, command } => {
            assert!(
                message.contains("Handshake required"),
                "expected handshake-required message, got: {message}"
            );
            // Server-style: `command` carries the offending message
            // type (the one the peer sent that triggered the rejection).
            assert_eq!(
                command.as_deref(),
                Some("Login"),
                "command should name the offending message type"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}
