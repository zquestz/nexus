//! End-to-end TCP/WS + TLS + protocol tests for `nexus-tracker`: drives real
//! TLS + framed-JSON exchanges against the daemon's connection task and asserts
//! on the responses (handshake, register/refresh, list, role-lock, rate limits,
//! compat filtering, field validation).
//!
//! The client uses a permissive cert verifier (self-signed cert; TOFU not
//! exercised here).

use std::sync::Arc;
use std::time::Duration;

use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher};
use nexus_common::framing::{FrameError, FrameReader, FrameWriter, MessageId, RawFrame};
use nexus_common::io::{read_server_message, send_client_message, tracker_client_message_type};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use nexus_common::websocket::WebSocketAdapter;
use nexus_common::{EXPECT_SNI_SERVER_NAME_VALID_DNS, SNI_SERVER_NAME, TRACKER_PROTOCOL_VERSION};
use nexus_tracker::registry::Registry;
use nexus_tracker::state::TrackerState;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
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

/// Install the rustls crypto provider once; later calls return Err
/// (already installed), which we ignore.
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

/// Drive one accepted connection through the tracker's connection task.
/// Returns the cert fingerprint for comparison against the wire response.
async fn spawn_one_shot_tracker(data_dir: &std::path::Path) -> (std::net::SocketAddr, String) {
    let tls_config = nexus_common::tls::TlsCertConfig {
        cert_filename: nexus_tracker::constants::CERT_FILENAME,
        key_filename: nexus_tracker::constants::KEY_FILENAME,
        common_name: nexus_tracker::constants::TLS_CERT_COMMON_NAME,
    };
    let fingerprint = nexus_common::tls::ensure_cert(data_dir, tls_config).expect("ensure_cert");
    let acceptor = nexus_common::tls::build_acceptor(data_dir, tls_config).expect("build_acceptor");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let local_addr = listener.local_addr().expect("local_addr");

    // Open tracker (no passwords), unlimited capacity, default refresh.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));

    let fp_clone = fingerprint.clone();
    tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.expect("accept");
        // Handler returns Result; the harness asserts on the wire response, not this.
        let _ =
            nexus_tracker::connection::handle_connection(stream, peer, acceptor, fp_clone, state)
                .await;
    });

    (local_addr, fingerprint)
}

#[tokio::test]
async fn test_handshake_roundtrip_compatible_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let (server_addr, expected_fingerprint) = spawn_one_shot_tracker(tmp.path()).await;

    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("connect");
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
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
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
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
            // Fingerprint is sent even on failure (per spec).
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
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls connect");

    let (read, write) = tokio::io::split(tls);
    let mut reader = FrameReader::new(read);
    let mut writer = FrameWriter::new(write);

    // First message must be a Handshake; a Login here is a protocol violation.
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
        ServerMessage::Error {
            message, command, ..
        } => {
            assert!(
                message.contains("Handshake required"),
                "expected handshake-required message, got: {message}"
            );
            // `command` names the offending message type that triggered rejection.
            assert_eq!(
                command.as_deref(),
                Some("Login"),
                "command should name the offending message type"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

type ClientReader = FrameReader<ReadHalf<TlsStream<TcpStream>>>;
type ClientWriter = FrameWriter<WriteHalf<TlsStream<TcpStream>>>;

/// Drive a long-running tracker accepting multiple connections; tests
/// hold the shared `state` to inspect the registry directly.
async fn spawn_multi_tracker(
    data_dir: &std::path::Path,
    state: Arc<TrackerState>,
) -> (std::net::SocketAddr, String) {
    let tls_config = nexus_common::tls::TlsCertConfig {
        cert_filename: nexus_tracker::constants::CERT_FILENAME,
        key_filename: nexus_tracker::constants::KEY_FILENAME,
        common_name: nexus_tracker::constants::TLS_CERT_COMMON_NAME,
    };
    let fingerprint = nexus_common::tls::ensure_cert(data_dir, tls_config).expect("ensure_cert");
    let acceptor = nexus_common::tls::build_acceptor(data_dir, tls_config).expect("build_acceptor");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let local_addr = listener.local_addr().expect("local_addr");

    let fp_for_loop = fingerprint.clone();
    tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let acceptor = acceptor.clone();
            let fp = fp_for_loop.clone();
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                let _ =
                    nexus_tracker::connection::handle_connection(stream, peer, acceptor, fp, state)
                        .await;
            });
        }
    });

    (local_addr, fingerprint)
}

/// Connect + TLS + successful `Handshake`, returning the post-handshake
/// reader/writer ready for tracker-protocol messages.
async fn connect_and_handshake(server_addr: std::net::SocketAddr) -> (ClientReader, ClientWriter) {
    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("connect");
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
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

    let response = read_server_message(&mut reader)
        .await
        .expect("read handshake response")
        .expect("frame");
    match response.message {
        ServerMessage::HandshakeResponse { success: true, .. } => {}
        other => panic!("expected successful HandshakeResponse, got {other:?}"),
    }

    (reader, writer)
}

/// Build a `TrackerServerRegister` for a server named `name`.
fn make_register(name: &str, user_count: u32) -> TrackerClientMessage {
    TrackerClientMessage::TrackerServerRegister {
        password: None,
        locale: "en".to_string(),
        name: name.to_string(),
        description: Some("integration test BBS".to_string()),
        address: Some("bbs.test.example".to_string()),
        port: 7500,
        websocket_port: None,
        version: "0.8.6".to_string(),
        fingerprint: "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
             AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"
            .to_string(),
        user_count,
        allows_guest: true,
    }
}

/// `send_client_message` for tracker-protocol payloads.
async fn send_tracker_client<W>(writer: &mut FrameWriter<W>, msg: &TrackerClientMessage)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(msg).expect("serialize");
    let message_type = tracker_client_message_type(msg);
    let frame = RawFrame::new(MessageId::new(), message_type.to_string(), payload);
    writer
        .write_frame(&frame)
        .await
        .expect("write tracker frame");
}

/// Poll until the registry length reaches `target`, asserting cleanup completed.
/// The 5s deadline accommodates the stale-eviction test (eviction lands ~2s in,
/// later on slow CI runners); fast cleanup paths return almost immediately.
async fn wait_for_registry_len(state: &Arc<TrackerState>, target: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let len = state.registry.lock().expect("registry mutex").len();
        if len == target {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("registry length didn't reach {target} within 5s (current: {len})");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn test_register_appears_in_list_then_unregister_on_disconnect() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Open tracker: unlimited entries, no passwords, default refresh.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // Phase 1: server connection registers.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Integration BBS", 7)).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success,
            refresh_interval,
            ..
        } => {
            assert!(success, "register should succeed on open tracker");
            assert_eq!(refresh_interval, Some(300));
        }
        other => panic!("expected TrackerServerRegisterResponse, got {other:?}"),
    }

    // Success response implies the entry is in the registry (mutex-coordinated).
    {
        let registry = state.registry.lock().expect("registry mutex");
        let listing = registry.list();
        assert_eq!(listing.len(), 1, "registry should have 1 entry");
        assert_eq!(listing[0].name, "Integration BBS");
        assert_eq!(listing[0].user_count, 7);
    }

    // Phase 2: client connection lists; sees the registered entry.
    let (mut c_reader, mut c_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut c_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: "0.8.6".to_string(),
        },
    )
    .await;
    let list_response = read_tracker_server(&mut c_reader).await;
    match list_response {
        TrackerServerMessage::TrackerServerListResponse {
            success,
            servers,
            error,
            ..
        } => {
            assert!(success, "list should succeed");
            assert!(error.is_none());
            assert_eq!(servers.len(), 1);
            assert_eq!(servers[0].name, "Integration BBS");
            assert_eq!(servers[0].user_count, 7);
        }
        other => panic!("expected TrackerServerListResponse, got {other:?}"),
    }
    // Tracker closes the client connection after List.
    drop(c_reader);
    drop(c_writer);

    // Phase 3: dropping the server connection delists it (disconnect = delist).
    drop(s_reader);
    drop(s_writer);
    wait_for_registry_len(&state, 0).await;

    // Phase 4: a fresh client now sees an empty list.
    let (mut c2_reader, mut c2_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut c2_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: "0.8.6".to_string(),
        },
    )
    .await;
    let list_response = read_tracker_server(&mut c2_reader).await;
    match list_response {
        TrackerServerMessage::TrackerServerListResponse {
            success, servers, ..
        } => {
            assert!(success);
            assert_eq!(
                servers.len(),
                0,
                "registry should be empty after server disconnect + cleanup"
            );
        }
        other => panic!("expected TrackerServerListResponse, got {other:?}"),
    }
}

#[tokio::test]
async fn test_register_omitted_address_substitutes_peer_ip() {
    // Pins: when `address` is None/empty the tracker substitutes the peer's
    // source IP (kernel-truthful, no validation), so a future change that
    // started rejecting the substituted loopback IP would surface here.
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    let mut msg = make_register("Implicit Address BBS", 1);
    if let TrackerClientMessage::TrackerServerRegister {
        address: ref mut a, ..
    } = msg
    {
        *a = None;
    }
    send_tracker_client(&mut s_writer, &msg).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success, .. } => {
            assert!(success, "register with omitted address should succeed");
        }
        other => panic!("expected TrackerServerRegisterResponse, got {other:?}"),
    }

    let registry = state.registry.lock().expect("registry mutex");
    let listing = registry.list();
    assert_eq!(listing.len(), 1);
    // Tests connect over TCP to loopback, so the substituted peer IP is 127.0.0.1.
    assert_eq!(listing[0].address, "127.0.0.1");
}

#[tokio::test]
async fn test_register_unicode_idn_address_stored_as_typed() {
    // Pins the IDN policy: addresses are stored as-typed (Unicode preserved),
    // Punycode only for DNS lookup. Loopback peer bypasses the resolver, so this
    // is purely about storage; resolver interaction is in the unit test
    // `unicode_idn_hostname_resolves_via_punycode`.
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    let mut msg = make_register("Unicode IDN BBS", 1);
    if let TrackerClientMessage::TrackerServerRegister {
        address: ref mut a, ..
    } = msg
    {
        *a = Some("münchen.de".to_string());
    }
    send_tracker_client(&mut s_writer, &msg).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success, .. } => {
            assert!(success, "register with Unicode IDN address should succeed");
        }
        other => panic!("expected TrackerServerRegisterResponse, got {other:?}"),
    }

    let registry = state.registry.lock().expect("registry mutex");
    let listing = registry.list();
    assert_eq!(listing.len(), 1);
    assert_eq!(
        listing[0].address, "münchen.de",
        "stored address should be the Unicode form, not the Punycode `xn--mnchen-3ya.de`"
    );
}

#[tokio::test]
async fn test_register_refresh_updates_user_count() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // A re-register with the same dedup key (everything but user_count) is a
    // refresh, not a new entry: still one row, user_count updated to the latest.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Refresh Test BBS", 1)).await;
    let _ = read_tracker_server(&mut s_reader).await;

    send_tracker_client(&mut s_writer, &make_register("Refresh Test BBS", 42)).await;
    let _ = read_tracker_server(&mut s_reader).await;

    let registry = state.registry.lock().expect("registry mutex");
    let listing = registry.list();
    assert_eq!(listing.len(), 1, "still one entry after refresh");
    assert_eq!(
        listing[0].user_count, 42,
        "user_count should reflect the latest refresh"
    );
}

#[tokio::test]
async fn test_role_violation_on_server_connection_disconnects() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // First message (register) locks the connection to the server role.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Role Test BBS", 0)).await;
    let _ = read_tracker_server(&mut s_reader).await;

    // A TrackerServerList on the server-role connection violates the role lock.
    send_tracker_client(
        &mut s_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: "0.8.6".to_string(),
        },
    )
    .await;
    let response = read_tracker_server(&mut s_reader).await;
    match response {
        TrackerServerMessage::Error { command, .. } => {
            assert_eq!(command.as_deref(), Some("TrackerServerList"));
        }
        other => panic!("expected Error, got {other:?}"),
    }

    // The role violation closes the connection, which delists the entry.
    drop(s_reader);
    drop(s_writer);
    wait_for_registry_len(&state, 0).await;
}

#[tokio::test]
async fn test_unauthorized_register_when_password_required() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Gate registration behind a password hash (PHC string for "secret").
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(b"secret", &salt)
        .expect("hash")
        .to_string();

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        Some(hash),
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // Register without a password against a gated tracker → unauthorized.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Should Fail", 0)).await;

    let response = read_tracker_server(&mut s_reader).await;
    match response {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success,
            error_kind,
            ..
        } => {
            assert!(!success);
            assert_eq!(error_kind.as_deref(), Some("unauthorized"));
        }
        other => panic!("expected TrackerServerRegisterResponse, got {other:?}"),
    }
}

/// Read a `TrackerServerMessage` off the wire — the tracker-protocol parallel
/// to `nexus-common`'s `read_server_message` (which decodes BBS `ServerMessage`).
async fn read_tracker_server<R>(reader: &mut FrameReader<R>) -> TrackerServerMessage
where
    R: tokio::io::AsyncRead + Unpin,
{
    let frame = match reader
        .read_frame_with_full_timeout(Duration::from_secs(5), Duration::from_secs(5))
        .await
    {
        Ok(Some(f)) => f,
        Ok(None) => panic!("connection closed before response"),
        Err(FrameError::IdleTimeout) => panic!("idle timeout waiting for response"),
        Err(e) => panic!("frame error: {e:?}"),
    };
    serde_json::from_slice::<TrackerServerMessage>(&frame.payload)
        .expect("parse TrackerServerMessage")
}

#[tokio::test]
async fn test_stale_entry_evicted_when_server_stops_refreshing() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // refresh_interval=1 ⇒ stale window = 2× = 2s. A server that registers but
    // never refreshes is evicted on the refresh loop's idle timeout.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        1,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Stale BBS", 0)).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
        other => panic!("expected successful register, got {other:?}"),
    }

    // Hold the connection open but send no refresh: the refresh loop's read
    // times out inside the stale window and the drop guard removes the entry.
    wait_for_registry_len(&state, 0).await;

    // Drop the still-open connection (otherwise clippy flags the unused binding).
    drop(s_reader);
    drop(s_writer);
}

#[tokio::test]
async fn test_register_rejected_when_global_capacity_reached() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // max_entries=1 ⇒ second registration must fail with `capacity`.
    let state = Arc::new(TrackerState::new(
        Registry::new(1, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut a_reader, mut a_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut a_writer, &make_register("First BBS", 0)).await;
    match read_tracker_server(&mut a_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
        other => panic!("expected first register to succeed, got {other:?}"),
    }

    // Second register (different connection): tracker is at capacity.
    let (mut b_reader, mut b_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut b_writer, &make_register("Second BBS", 0)).await;
    match read_tracker_server(&mut b_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => {
            assert_eq!(error_kind.as_deref(), Some("capacity"));
        }
        other => panic!("expected capacity rejection, got {other:?}"),
    }
    drop(b_reader);
    drop(b_writer);

    {
        let registry = state.registry.lock().expect("registry mutex");
        assert_eq!(
            registry.len(),
            1,
            "rejection must not displace the first entry"
        );
        assert_eq!(registry.list()[0].name, "First BBS");
    }
    drop(a_reader);
    drop(a_writer);
}

#[tokio::test]
async fn test_register_rejected_when_per_ip_cap_reached() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Unlimited entries but max_per_ip=1; both connections are loopback, so the
    // second register from the same source IP must fail.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 1),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut a_reader, mut a_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut a_writer, &make_register("A", 0)).await;
    match read_tracker_server(&mut a_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
        other => panic!("expected first register to succeed, got {other:?}"),
    }

    let (mut b_reader, mut b_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut b_writer, &make_register("B", 0)).await;
    match read_tracker_server(&mut b_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => {
            // Per-IP rejection reuses `capacity`, distinguished only by the
            // human-readable `error` message.
            assert_eq!(error_kind.as_deref(), Some("capacity"));
        }
        other => panic!("expected capacity rejection, got {other:?}"),
    }
    drop(a_reader);
    drop(a_writer);
    drop(b_reader);
    drop(b_writer);
}

/// Spawn a tracker accepting WebSocket connections (TLS + WS upgrade + protocol).
async fn spawn_ws_only_tracker(
    data_dir: &std::path::Path,
    state: Arc<TrackerState>,
) -> (std::net::SocketAddr, String) {
    let tls_config = nexus_common::tls::TlsCertConfig {
        cert_filename: nexus_tracker::constants::CERT_FILENAME,
        key_filename: nexus_tracker::constants::KEY_FILENAME,
        common_name: nexus_tracker::constants::TLS_CERT_COMMON_NAME,
    };
    let fingerprint = nexus_common::tls::ensure_cert(data_dir, tls_config).expect("ensure_cert");
    let acceptor = nexus_common::tls::build_acceptor(data_dir, tls_config).expect("build_acceptor");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let local_addr = listener.local_addr().expect("local_addr");

    let fp_for_loop = fingerprint.clone();
    tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let acceptor = acceptor.clone();
            let fp = fp_for_loop.clone();
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                let _ = nexus_tracker::websocket::handle_tracker_websocket_connection(
                    stream, peer, acceptor, fp, state,
                )
                .await;
            });
        }
    });

    (local_addr, fingerprint)
}

#[tokio::test]
async fn test_websocket_handshake_register_list_roundtrip() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _expected_fingerprint) =
        spawn_ws_only_tracker(tmp.path(), Arc::clone(&state)).await;

    // TCP → TLS → WebSocket upgrade. Once wrapped in nexus-common's generic
    // `WebSocketAdapter`, the rest of the protocol code is identical to the TCP path.
    let connector = build_test_client();
    let tcp = TcpStream::connect(server_addr).await.expect("connect");
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
    let tls = connector
        .connect(server_name, tcp)
        .await
        .expect("tls connect");

    let (ws_stream, _resp) = tokio_tungstenite::client_async("wss://localhost/", tls)
        .await
        .expect("ws upgrade");
    let adapter = WebSocketAdapter::new(ws_stream);
    let (read, write) = tokio::io::split(adapter);
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

    let response = read_server_message(&mut reader)
        .await
        .expect("read handshake response")
        .expect("frame");
    match response.message {
        ServerMessage::HandshakeResponse { success: true, .. } => {}
        other => panic!("expected successful HandshakeResponse, got {other:?}"),
    }

    send_tracker_client(&mut writer, &make_register("WS BBS", 9)).await;
    match read_tracker_server(&mut reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: true,
            refresh_interval,
            ..
        } => {
            assert_eq!(refresh_interval, Some(300));
        }
        other => panic!("expected successful register, got {other:?}"),
    }

    // Entry in the registry proves the server-side adapter delivered our bytes
    // intact through the WS layer.
    {
        let registry = state.registry.lock().expect("registry mutex");
        let listing = registry.list();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].name, "WS BBS");
        assert_eq!(listing[0].user_count, 9);
    }
}

/// Argon2id PHC hash of "secret" for tests needing a gated tracker.
fn hash_secret() -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"secret", &salt)
        .expect("hash")
        .to_string()
}

/// `make_register` with an explicit password (default is `None`).
fn make_register_with_password(name: &str, password: Option<&str>) -> TrackerClientMessage {
    let mut msg = make_register(name, 0);
    if let TrackerClientMessage::TrackerServerRegister {
        password: ref mut p,
        ..
    } = msg
    {
        *p = password.map(str::to_string);
    }
    msg
}

#[tokio::test]
async fn test_register_auth_failure_rate_limit_rejects_after_burst() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Gated tracker, auth-failure capacity 2: two wrong passwords burn the
    // bucket; the third (even with the correct password) is rate_limited.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        Some(hash_secret()),
        None,
        300,
        0,              // connection rate disabled
        2,              // auth-failure rate
        Duration::ZERO, // refresh floor disabled (so successive registers are isolated)
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    for attempt in 1..=3 {
        let (mut r, mut w) = connect_and_handshake(server_addr).await;
        send_tracker_client(&mut w, &make_register_with_password("Brute", Some("wrong"))).await;
        match read_tracker_server(&mut r).await {
            TrackerServerMessage::TrackerServerRegisterResponse {
                success: false,
                error_kind,
                ..
            } => match attempt {
                1 | 2 => assert_eq!(
                    error_kind.as_deref(),
                    Some("unauthorized"),
                    "attempt {attempt} should be unauthorized (bucket still has tokens)",
                ),
                3 => assert_eq!(
                    error_kind.as_deref(),
                    Some("rate_limited"),
                    "attempt {attempt} should be rate-limited (bucket empty)",
                ),
                _ => unreachable!(),
            },
            other => panic!("attempt {attempt}: expected typed failure, got {other:?}"),
        }
    }

    // The correct password is also rate-limited until refill: an attacker who
    // triggered the limit can't sneak a guess through.
    let (mut r, mut w) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut w,
        &make_register_with_password("Brute", Some("secret")),
    )
    .await;
    match read_tracker_server(&mut r).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("rate_limited"),
            "correct password while rate-limited should still be rejected",
        ),
        other => panic!("expected rate_limited rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn test_correct_password_does_not_burn_auth_failure_tokens() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Gated tracker, auth-failure capacity 1: successful auths must NOT debit
    // the bucket, or the second correct attempt would already be rate-limited.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0), // unlimited entries
        Some(hash_secret()),
        None,
        300,
        0,              // connection rate disabled
        1,              // auth-failure rate (very low)
        Duration::ZERO, // refresh floor disabled
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // Three successful registrations on fresh connections, none consuming the
    // auth-failure bucket. Distinct names avoid the per-IP cap as a confound.
    for i in 1..=3 {
        let (mut r, mut w) = connect_and_handshake(server_addr).await;
        send_tracker_client(
            &mut w,
            &make_register_with_password(&format!("OK-{i}"), Some("secret")),
        )
        .await;
        match read_tracker_server(&mut r).await {
            TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
            other => panic!("attempt {i}: expected success, got {other:?}"),
        }
        drop(r);
        drop(w);
    }

    // The bucket still has its single token, so the next wrong password gets
    // unauthorized (not rate_limited, which would mean a success had burned it).
    let (mut r, mut w) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut w,
        &make_register_with_password("First-Failure", Some("wrong")),
    )
    .await;
    match read_tracker_server(&mut r).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("unauthorized"),
            "first failure after successes should be unauthorized, not rate_limited",
        ),
        other => panic!("expected unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn test_connection_rate_limit_drops_excess_connections() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Connection rate capacity 1: the first TCP connect completes TLS; the
    // second is dropped pre-TLS by the accept loop.
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        1, // connection rate
        0, // auth-failure rate disabled
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // First connection: normal handshake succeeds.
    let (mut _r, mut _w) = connect_and_handshake(server_addr).await;

    // Second connection: TCP accepts but the server drops the stream pre-TLS, so
    // the client's TLS handshake fails (exact error is platform-dependent).
    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("tcp connect");
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
    let tls_result = connector.connect(server_name, stream).await;
    assert!(
        tls_result.is_err(),
        "second connection should be dropped pre-TLS, got Ok",
    );
}

#[tokio::test]
async fn test_refresh_too_soon_rejected_and_disconnected() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Open tracker so the only gate is the per-entry refresh floor, set to the
    // production default here (other tests pass `Duration::ZERO` to disable it).
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        nexus_tracker::constants::REFRESH_FLOOR_INTERVAL,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // Phase 1: register on a long-lived connection.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Throttled BBS", 5)).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
        other => panic!("expected initial register to succeed, got {other:?}"),
    }

    // Phase 2: an immediate refresh is well under the floor, so the tracker
    // rejects with `rate_limited` AND closes the connection (such a client is
    // broken or malicious).
    send_tracker_client(&mut s_writer, &make_register("Throttled BBS", 6)).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("rate_limited"),
            "refresh inside the floor window should be rate-limited",
        ),
        other => panic!("expected rate_limited rejection, got {other:?}"),
    }

    // The closed connection delists the entry via the drop guard.
    drop(s_reader);
    drop(s_writer);
    wait_for_registry_len(&state, 0).await;
}

/// `make_register` with an explicit `version` (default `"0.8.6"`); plants
/// entries at varying versions for the compat-filter tests.
fn make_register_with_version(name: &str, version: &str) -> TrackerClientMessage {
    let mut msg = make_register(name, 0);
    if let TrackerClientMessage::TrackerServerRegister {
        version: ref mut v, ..
    } = msg
    {
        *v = version.to_string();
    }
    msg
}

#[tokio::test]
async fn test_register_rejects_malformed_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    // "garbage" is under MAX_VERSION_LENGTH but not valid semver; the handler
    // validates semver shape end-to-end and rejects with `error_kind: invalid`.
    send_tracker_client(
        &mut s_writer,
        &make_register_with_version("Garbage Version BBS", "garbage"),
    )
    .await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("invalid"),
            "malformed semver should be rejected with error_kind=invalid",
        ),
        other => panic!("expected typed register failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_list_rejects_malformed_client_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut c_reader, mut c_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut c_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: "not-semver".to_string(),
        },
    )
    .await;
    match read_tracker_server(&mut c_reader).await {
        TrackerServerMessage::TrackerServerListResponse {
            success: false,
            error_kind,
            servers,
            ..
        } => {
            assert_eq!(
                error_kind.as_deref(),
                Some("invalid"),
                "malformed client version must reject with error_kind=invalid",
            );
            assert!(servers.is_empty(), "failure path returns no entries");
        }
        other => panic!("expected typed list failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_list_filters_incompatible_versions() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    // Plant three BBSes at 0.7.x / 0.8.x (client's minor) / 0.9.x. Pre-1.0,
    // only same-minor is compatible per `check_compatibility`.
    let registers = [
        make_register_with_version("Old BBS", "0.7.5"),
        make_register_with_version("Match BBS", "0.8.6"),
        make_register_with_version("New BBS", "0.9.0"),
    ];
    let mut server_conns = Vec::new();
    for msg in &registers {
        let (mut s_reader, s_writer) = connect_and_handshake(server_addr).await;
        let mut s_writer = s_writer;
        send_tracker_client(&mut s_writer, msg).await;
        match read_tracker_server(&mut s_reader).await {
            TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
            other => panic!("expected register success, got {other:?}"),
        }
        // Hold the connection open so the entry stays registered.
        server_conns.push((s_reader, s_writer));
    }

    wait_for_registry_len(&state, 3).await;

    // Query as a 0.8.x client; only the same-minor "Match BBS" should return.
    let (mut c_reader, mut c_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut c_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: "0.8.6".to_string(),
        },
    )
    .await;
    match read_tracker_server(&mut c_reader).await {
        TrackerServerMessage::TrackerServerListResponse {
            success: true,
            servers,
            ..
        } => {
            assert_eq!(servers.len(), 1, "expected only same-minor entry to pass");
            assert_eq!(servers[0].name, "Match BBS");
            assert_eq!(servers[0].version, "0.8.6");
        }
        other => panic!("expected list success, got {other:?}"),
    }
}

/// `make_register` with a custom `name`.
fn make_register_with_name(name: &str) -> TrackerClientMessage {
    let mut msg = make_register("placeholder", 0);
    if let TrackerClientMessage::TrackerServerRegister {
        name: ref mut n, ..
    } = msg
    {
        *n = name.to_string();
    }
    msg
}

/// `make_register` with a custom `description`.
fn make_register_with_description(description: Option<&str>) -> TrackerClientMessage {
    let mut msg = make_register("Description Test BBS", 0);
    if let TrackerClientMessage::TrackerServerRegister {
        description: ref mut d,
        ..
    } = msg
    {
        *d = description.map(str::to_string);
    }
    msg
}

/// `make_register` with a custom `port`.
fn make_register_with_port(port: u16) -> TrackerClientMessage {
    let mut msg = make_register("Port Test BBS", 0);
    if let TrackerClientMessage::TrackerServerRegister {
        port: ref mut p, ..
    } = msg
    {
        *p = port;
    }
    msg
}

/// `make_register` with an explicit `websocket_port`.
fn make_register_with_websocket_port(websocket_port: Option<u16>) -> TrackerClientMessage {
    let mut msg = make_register("WS Port Test BBS", 0);
    if let TrackerClientMessage::TrackerServerRegister {
        websocket_port: ref mut wp,
        ..
    } = msg
    {
        *wp = websocket_port;
    }
    msg
}

/// `make_register` with an explicit `locale`.
fn make_register_with_locale(locale: &str) -> TrackerClientMessage {
    let mut msg = make_register("Locale Test BBS", 0);
    if let TrackerClientMessage::TrackerServerRegister {
        locale: ref mut l, ..
    } = msg
    {
        *l = locale.to_string();
    }
    msg
}

#[tokio::test]
async fn test_register_rejects_empty_name() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    // Whitespace-only name passes length but `validate_server_name` rejects it
    // (Empty after trim).
    send_tracker_client(&mut s_writer, &make_register_with_name("   ")).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("invalid"),
            "whitespace-only name should reject with error_kind=invalid",
        ),
        other => panic!("expected typed register failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_register_rejects_newline_in_description() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    // Embedded newline passes length but `validate_server_description` rejects it
    // (ContainsNewlines).
    send_tracker_client(
        &mut s_writer,
        &make_register_with_description(Some("Line one\nLine two")),
    )
    .await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("invalid"),
            "newline in description should reject with error_kind=invalid",
        ),
        other => panic!("expected typed register failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_register_rejects_port_zero() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register_with_port(0)).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("invalid"),
            "port 0 should reject with error_kind=invalid",
        ),
        other => panic!("expected typed register failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_register_rejects_websocket_port_zero() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register_with_websocket_port(Some(0))).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("invalid"),
            "websocket_port 0 should reject with error_kind=invalid",
        ),
        other => panic!("expected typed register failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_register_rejects_locale_with_control_char() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    // Embedded null byte passes length but `validate_locale` rejects it
    // (InvalidCharacters).
    send_tracker_client(&mut s_writer, &make_register_with_locale("en\0US")).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            error_kind,
            ..
        } => assert_eq!(
            error_kind.as_deref(),
            Some("invalid"),
            "locale with control char should reject with error_kind=invalid",
        ),
        other => panic!("expected typed register failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_list_rejects_empty_client_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut c_reader, mut c_writer) = connect_and_handshake(server_addr).await;
    // Empty version → typed `TrackerServerListResponse` failure, distinct from a
    // missing `version` field (a frame-level deserialization error).
    send_tracker_client(
        &mut c_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: String::new(),
        },
    )
    .await;
    match read_tracker_server(&mut c_reader).await {
        TrackerServerMessage::TrackerServerListResponse {
            success: false,
            error_kind,
            servers,
            ..
        } => {
            assert_eq!(
                error_kind.as_deref(),
                Some("invalid"),
                "empty version should reject with error_kind=invalid",
            );
            assert!(servers.is_empty(), "failure path returns no entries");
        }
        other => panic!("expected typed list failure, got {other:?}"),
    }
}

#[tokio::test]
async fn test_list_rejects_over_cap_client_version() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = Arc::new(TrackerState::new(
        Registry::new(0, 0),
        None,
        None,
        300,
        0,
        0,
        Duration::ZERO,
    ));
    let (server_addr, _) = spawn_multi_tracker(tmp.path(), Arc::clone(&state)).await;

    let (mut c_reader, mut c_writer) = connect_and_handshake(server_addr).await;
    // Over `MAX_VERSION_LENGTH` but under the frame cap → `VersionError::TooLong`
    // → typed list failure with `error_kind: invalid`. Distinct from the missing-
    // field and unparseable-semver paths covered by other tests.
    let over_cap_version = "0.8.6".to_string() + &"a".repeat(64);
    send_tracker_client(
        &mut c_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
            version: over_cap_version,
        },
    )
    .await;
    match read_tracker_server(&mut c_reader).await {
        TrackerServerMessage::TrackerServerListResponse {
            success: false,
            error_kind,
            servers,
            ..
        } => {
            assert_eq!(
                error_kind.as_deref(),
                Some("invalid"),
                "over-cap version should reject with error_kind=invalid",
            );
            assert!(servers.is_empty(), "failure path returns no entries");
        }
        other => panic!("expected typed list failure, got {other:?}"),
    }
}
