//! End-to-end TCP + TLS + protocol tests for `nexus-tracker`.
//!
//! Spins up the daemon's connection task on a TCP listener, drives
//! real TLS + framed-JSON exchanges against it, and asserts on the
//! responses. Covers:
//!
//! - Handshake (success, version mismatch, non-Handshake first message)
//! - `TrackerServerRegister` + `TrackerServerList` roundtrip and disconnect-cleanup
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
    use nexus_tracker::registry::Registry;
    use nexus_tracker::state::TrackerState;
    use std::sync::Arc;

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
        // The handler now returns Result; tests don't care about the
        // error path here (the test harness asserts on the wire response).
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

// =============================================================================
// TrackerServerRegister + TrackerServerList roundtrip
// =============================================================================

use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use nexus_tracker::registry::Registry;
use nexus_tracker::state::TrackerState;
use std::time::Duration;
use tokio::io::{ReadHalf, WriteHalf};
use tokio_rustls::client::TlsStream;

type ClientReader = FrameReader<ReadHalf<TlsStream<TcpStream>>>;
type ClientWriter = FrameWriter<WriteHalf<TlsStream<TcpStream>>>;

/// Drive a long-running tracker that accepts multiple connections.
/// Returns the bound address, the cert fingerprint, and the shared
/// state so tests can inspect the registry directly.
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

/// Connect, do TLS, send a `Handshake`, verify success, and return the
/// post-handshake reader/writer ready for tracker-protocol messages.
async fn connect_and_handshake(server_addr: std::net::SocketAddr) -> (ClientReader, ClientWriter) {
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
        version: "0.8.2".to_string(),
        fingerprint: "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
             AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"
            .to_string(),
        user_count,
        allows_guest: true,
    }
}

/// Send a tracker client message via the underlying frame writer. This
/// mirrors `send_client_message` but for tracker-protocol payloads.
async fn send_tracker_client<W>(writer: &mut FrameWriter<W>, msg: &TrackerClientMessage)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use nexus_common::framing::{MessageId, RawFrame};
    use nexus_common::io::tracker_client_message_type;
    let payload = serde_json::to_vec(msg).expect("serialize");
    let message_type = tracker_client_message_type(msg);
    let frame = RawFrame::new(MessageId::new(), message_type.to_string(), payload);
    writer
        .write_frame(&frame)
        .await
        .expect("write tracker frame");
}

/// Spin (poll) until the registry length reaches `target`, with a
/// bounded timeout. Used to assert post-disconnect cleanup completed.
///
/// The 5-second deadline accommodates the stale-eviction test, where
/// eviction lands ~2 s after registration (idle timeout fires once
/// `refresh_interval * 2` elapses). On slower CI runners — Windows in
/// particular — the eviction can land at T≈2.1–2.3 s, and a tighter
/// deadline produced flaky failures. Fast post-disconnect cleanup
/// paths return almost immediately, so the longer deadline only
/// matters when the test is genuinely broken.
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

    // Phase 1: open a server connection and register.
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

    // Tracker is mutex-coordinated; once we've received the success
    // response the entry is in the registry.
    {
        let registry = state.registry.lock().expect("registry mutex");
        let listing = registry.list();
        assert_eq!(listing.len(), 1, "registry should have 1 entry");
        assert_eq!(listing[0].name, "Integration BBS");
        assert_eq!(listing[0].user_count, 7);
    }

    // Phase 2: open a client connection and TrackerServerList.
    let (mut c_reader, mut c_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut c_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
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
            let servers = servers.expect("servers field present on success");
            assert_eq!(servers.len(), 1);
            assert_eq!(servers[0].name, "Integration BBS");
            assert_eq!(servers[0].user_count, 7);
        }
        other => panic!("expected TrackerServerListResponse, got {other:?}"),
    }
    // Client connection closes after List (no further messages).
    drop(c_reader);
    drop(c_writer);

    // Phase 3: drop the server connection and wait for cleanup.
    drop(s_reader);
    drop(s_writer);
    wait_for_registry_len(&state, 0).await;

    // Phase 4: a fresh client connection sees an empty list.
    let (mut c2_reader, mut c2_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(
        &mut c2_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
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
                servers.expect("servers field present").len(),
                0,
                "registry should be empty after server disconnect + cleanup"
            );
        }
        other => panic!("expected TrackerServerListResponse, got {other:?}"),
    }
}

#[tokio::test]
async fn test_register_omitted_address_substitutes_peer_ip() {
    // When `address` is None (or empty), the tracker substitutes the
    // peer's source IP — kernel-truthful, no validation needed. This
    // test pins the substitution behavior so a future change that,
    // e.g., applied `classify_invalid` to the substituted IP and
    // started rejecting loopback peers shows up immediately.
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
    // Build a register with `address: None`.
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
    // Integration tests connect over TCP to 127.0.0.1, so the peer IP
    // the tracker sees is the IPv4 loopback. The substituted address
    // is the stringified peer IP.
    assert_eq!(listing[0].address, "127.0.0.1");
}

#[tokio::test]
async fn test_register_unicode_idn_address_stored_as_typed() {
    // The IDN policy stores the address as-typed (Unicode preserved);
    // Punycode is only used for the DNS lookup. This test pins the
    // contract so a future refactor that, e.g., reuses the ASCII form
    // as the entry's stored address would surface as a failure.
    //
    // The integration harness uses a loopback peer, so the LAN-peer
    // bypass keeps the resolver out of this path entirely — the test
    // is purely about what gets stored. That's the right scope: the
    // resolver-vs-storage interaction is exercised by the unit test
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

    // Server connection: register, then refresh with new user_count.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Refresh Test BBS", 1)).await;
    let _ = read_tracker_server(&mut s_reader).await; // consume register response

    send_tracker_client(&mut s_writer, &make_register("Refresh Test BBS", 42)).await;
    let _ = read_tracker_server(&mut s_reader).await; // consume refresh response

    // Registry should reflect the refresh's new user_count.
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

    // Establish a server connection first.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Role Test BBS", 0)).await;
    let _ = read_tracker_server(&mut s_reader).await;

    // Send a TrackerServerList on the server connection — role violation.
    send_tracker_client(
        &mut s_writer,
        &TrackerClientMessage::TrackerServerList {
            password: None,
            locale: "en".to_string(),
        },
    )
    .await;
    let response = read_tracker_server(&mut s_reader).await;
    match response {
        TrackerServerMessage::Error { command, .. } => {
            // The server-side dispatcher logs the offending command as
            // `TrackerServerList` on the Error.
            assert_eq!(command.as_deref(), Some("TrackerServerList"));
        }
        other => panic!("expected Error, got {other:?}"),
    }

    // After the role violation the connection closes; cleanup runs.
    drop(s_reader);
    drop(s_writer);
    wait_for_registry_len(&state, 0).await;
}

#[tokio::test]
async fn test_unauthorized_register_when_password_required() {
    ensure_crypto_provider();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Set a registration hash directly (any valid PHC string for "secret").
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    use argon2::{Argon2, PasswordHasher};
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

    // Connect and try to register without a password.
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

/// Read a `TrackerServerMessage` from the wire, parsing the frame
/// payload directly. (`nexus-common`'s `read_server_message` decodes
/// BBS `ServerMessage`; we need the tracker-protocol parallel here.)
async fn read_tracker_server<R>(reader: &mut FrameReader<R>) -> TrackerServerMessage
where
    R: tokio::io::AsyncRead + Unpin,
{
    use nexus_common::framing::FrameError;
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

    // refresh_interval=1 ⇒ stale window = 2 × 1 = 2 seconds. The
    // refresh loop's idle timeout uses this same window, so a server
    // that registers and then never refreshes will get evicted on
    // idle-timeout from the refresh loop.
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

    // Hold the connection open but send no refresh. Within the stale
    // window the refresh loop's read times out, the loop exits, and
    // the drop guard removes the entry.
    wait_for_registry_len(&state, 0).await;

    // Sanity: the connection is still ours; we just never had to do
    // anything with it. Drop it now to keep clippy happy.
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

    // First register: succeeds.
    let (mut a_reader, mut a_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut a_writer, &make_register("First BBS", 0)).await;
    match read_tracker_server(&mut a_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
        other => panic!("expected first register to succeed, got {other:?}"),
    }

    // Second register from a different connection: tracker is full.
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

    // First connection's entry is unaffected by the rejection.
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

    // max_entries unlimited but max_per_ip=1. Both connections come
    // from 127.0.0.1 (the test fixture binds to loopback), so the
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
            // Per the design doc, per-IP rejection reuses `capacity` —
            // distinguished only by the human-readable `error` message.
            assert_eq!(error_kind.as_deref(), Some("capacity"));
        }
        other => panic!("expected capacity rejection, got {other:?}"),
    }
    drop(a_reader);
    drop(a_writer);
    drop(b_reader);
    drop(b_writer);
}

// =============================================================================
// WebSocket roundtrip
// =============================================================================

use nexus_common::websocket::WebSocketAdapter;

/// Spawn a tracker that accepts WebSocket connections (TLS + WS upgrade
/// + protocol). Returns the bound address and cert fingerprint.
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

    // Client side: TCP → TLS → WebSocket upgrade. The same
    // `WebSocketAdapter` from nexus-common works on both sides — it's
    // a generic Stream+Sink adapter — so once we have the WS stream
    // wrapped, the rest of the protocol code is identical to the TCP
    // path's.
    let connector = build_test_client();
    let tcp = TcpStream::connect(server_addr).await.expect("connect");
    let server_name = ServerName::try_from("localhost").expect("server name");
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

    // Drive the protocol exactly like the TCP tests.
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

    // Register over WS.
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

    // Verify the entry is in the registry — proving the server-side
    // adapter delivered our bytes intact through the WS layer.
    {
        let registry = state.registry.lock().expect("registry mutex");
        let listing = registry.list();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].name, "WS BBS");
        assert_eq!(listing[0].user_count, 9);
    }
}

// =============================================================================
// Rate limiting
// =============================================================================

/// Argon2id PHC hash of "secret", suitable for use as a tracker password
/// hash in tests that need a gated tracker.
fn hash_secret() -> String {
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    use argon2::{Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"secret", &salt)
        .expect("hash")
        .to_string()
}

/// Build a `TrackerServerRegister` with an explicit password (overriding
/// `make_register`'s `password: None`).
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

    // Gated tracker, auth-failure capacity 2. Two wrong-password attempts
    // burn the bucket; the third (and any further attempts, even with the
    // CORRECT password) get rate_limited until refill.
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

    // Three wrong-password attempts in succession.
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

    // Even the CORRECT password is rate-limited until the bucket refills:
    // an attacker who triggered the limit can't sneak through with a guess.
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

    // Gated tracker, auth-failure capacity 1. Successful auths must NOT
    // debit the bucket — otherwise the second correct attempt below
    // would already see an empty bucket and be rate-limited.
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

    // Three successful registrations across three fresh connections.
    // None should consume the auth-failure bucket. Use distinct names
    // so the per-IP cap (which we left at default 0 = unlimited via
    // Registry::new(0, 0)) isn't a confound either.
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
        // Drop the connection so the next iteration is a fresh registration.
        drop(r);
        drop(w);
    }

    // The bucket still has its single token: the next WRONG password
    // gets unauthorized (consumes the token). If any prior success had
    // burned a token, this would already be rate_limited.
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

    // Connection rate capacity 1: the first TCP connect succeeds (and
    // its TLS handshake completes), the second is dropped pre-TLS by
    // the accept loop.
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

    // First connection: succeeds and we can do a normal handshake.
    let (mut _r, mut _w) = connect_and_handshake(server_addr).await;

    // Second connection: TCP accepts, but the server drops the stream
    // before TLS, so the client's TLS handshake fails. The exact error
    // varies by platform (EOF / unexpected close); we only assert that
    // the TLS handshake doesn't *succeed*.
    let connector = build_test_client();
    let stream = TcpStream::connect(server_addr).await.expect("tcp connect");
    let server_name = ServerName::try_from("localhost").expect("server name");
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

    // Open tracker (no auth gates) so the only thing in our way is
    // the per-entry refresh floor — set to the production default here
    // (other tests pass `Duration::ZERO` to disable it).
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

    // Phase 1: open a long-lived connection and register.
    let (mut s_reader, mut s_writer) = connect_and_handshake(server_addr).await;
    send_tracker_client(&mut s_writer, &make_register("Throttled BBS", 5)).await;
    match read_tracker_server(&mut s_reader).await {
        TrackerServerMessage::TrackerServerRegisterResponse { success: true, .. } => {}
        other => panic!("expected initial register to succeed, got {other:?}"),
    }

    // Phase 2: immediate refresh on the same connection. Floor is 60s;
    // we're well under that. The tracker must reject with
    // `error_kind: rate_limited` AND close the connection — a client
    // refreshing > 2× the proper rate is broken or malicious.
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

    // The connection must be closed and the entry unregistered (drop
    // guard runs as the connection task exits). `wait_for_registry_len`
    // polls until the registry shows 0 entries.
    drop(s_reader);
    drop(s_writer);
    wait_for_registry_len(&state, 0).await;
}
