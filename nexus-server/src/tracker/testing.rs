//! Mock tracker for tracker-task lifecycle tests.
//!
//! TLS listener on a random localhost port speaking just enough protocol
//! to drive the task's state machine: TLS handshake, BBS-style
//! `Handshake`/`HandshakeResponse`, one `TrackerServerRegister` cycle.
//! Self-reported fingerprint and register response are per-test configurable.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use nexus_common::TRACKER_PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_client_handshake_message_with_full_timeout, read_tracker_client_message_with_full_timeout,
    send_server_message, send_tracker_server_message,
};
use nexus_common::protocol::ServerMessage;
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use rcgen::{CertificateParams, KeyPair};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// What the mock should do during one connection cycle.
#[derive(Clone)]
pub struct MockBehavior {
    /// Fingerprint to self-report in `HandshakeResponse`. `None` = report
    /// the real TLS cert fp (Stage 2 passes); `Some(other)` simulates
    /// interception (TLS peer disagrees with self-report).
    pub reported_fingerprint: Option<String>,
    /// Default `TrackerServerRegister` response when `queued_responses` empty.
    pub register_response: RegisterPolicy,
    /// Per-connection response queue (front popped per connect, then falls
    /// back to `register_response`) — e.g. simulate rate_limited-then-success.
    /// `Arc<Mutex>` so handler clones share it.
    pub queued_responses: Arc<Mutex<VecDeque<RegisterPolicy>>>,
    /// Captures each received `TrackerServerRegister` payload in order, for
    /// propagation tests. `Arc<Mutex>` so the test can read post-spawn.
    pub captured_registers: Arc<Mutex<Vec<TrackerClientMessage>>>,
    /// Accept TLS then park (never read `Handshake`, never send
    /// `HandshakeResponse`), wedging the task in `read_handshake_response`.
    /// Used to verify shutdown aborts the deepest cycle await.
    pub wedge_after_tls: bool,
    /// Per-connection close controls. Each accepted connection pops one
    /// optional response count; when present, that connection closes after
    /// sending that many register responses.
    /// Leaves the listener alive so tests can simulate a tracker restart or
    /// mid-idle drop while allowing the task to reconnect to the same endpoint.
    pub close_after_register_responses: Arc<Mutex<VecDeque<usize>>>,
}

#[derive(Clone)]
pub enum RegisterPolicy {
    /// Reply with `success: true, refresh_interval`.
    Success { refresh_interval: u32 },
    /// Reply with `success: false` carrying the given error category.
    Failure { error_kind: String, error: String },
}

impl Default for MockBehavior {
    fn default() -> Self {
        Self {
            reported_fingerprint: None,
            register_response: RegisterPolicy::Success {
                refresh_interval: 300,
            },
            queued_responses: Arc::new(Mutex::new(VecDeque::new())),
            captured_registers: Arc::new(Mutex::new(Vec::new())),
            wedge_after_tls: false,
            close_after_register_responses: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

/// Running mock tracker. Drop the value (or call [`Self::stop`]) to
/// signal the listener task to stop accepting new connections.
pub struct MockTracker {
    pub addr: SocketAddr,
    pub fingerprint: String,
    /// Snapshot of the start behavior. Its `Arc` fields (`queued_responses`,
    /// `captured_registers`) are shared with handler tasks, so tests can
    /// mutate/read through this handle.
    pub behavior: MockBehavior,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl MockTracker {
    pub async fn start(behavior: MockBehavior) -> Self {
        // Install the rustls crypto provider (idempotent; Err if already
        // installed, ignored). Production does this in main.rs.
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

        let (cert_der, key_der, fingerprint) = generate_cert();

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("mock tracker: build TLS server config");
        let acceptor = TlsAcceptor::from(Arc::new(config));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock tracker: bind");
        let addr = listener.local_addr().expect("mock tracker: local_addr");

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let real_fp = fingerprint.clone();
        let exposed_behavior = behavior.clone();

        let join = tokio::spawn(async move {
            // JoinSet so shutdown aborts all handlers. Otherwise a wedged
            // handler (e.g. 600s read) outlives `stop`/`Drop` and only dies
            // with the runtime, producing stderr noise in shared-runtime tests.
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept = listener.accept() => {
                        let Ok((tcp, _)) = accept else { break };
                        let acceptor = acceptor.clone();
                        let behavior = behavior.clone();
                        let real_fp = real_fp.clone();
                        connections.spawn(async move {
                            // Best-effort; peer drops mid-protocol are expected.
                            let _ = handle_connection(tcp, acceptor, behavior, real_fp).await;
                        });
                    }
                }
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        });

        Self {
            addr,
            fingerprint,
            behavior: exposed_behavior,
            shutdown: Some(shutdown_tx),
            join: Some(join),
        }
    }

    /// Signal the listener task to stop and await its completion.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.await;
        }
    }
}

impl Drop for MockTracker {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        // Don't await; the listener exits on its next accept iteration.
    }
}

async fn handle_connection(
    tcp: tokio::net::TcpStream,
    acceptor: TlsAcceptor,
    behavior: MockBehavior,
    real_fingerprint: String,
) -> std::io::Result<()> {
    let tls = acceptor.accept(tcp).await?;
    if behavior.wedge_after_tls {
        // Park forever; never send HandshakeResponse. Halves drop on abort.
        std::future::pending::<()>().await;
    }
    let (read_half, write_half) = tokio::io::split(tls);
    let mut reader = FrameReader::new(tokio::io::BufReader::new(read_half));
    let mut writer = FrameWriter::new(write_half);

    // BBS-style Handshake/HandshakeResponse (reused at the handshake layer).
    let _handshake =
        read_client_handshake_message_with_full_timeout(&mut reader, None, None).await?;
    let reported = behavior
        .reported_fingerprint
        .clone()
        .unwrap_or(real_fingerprint);
    send_server_message(
        &mut writer,
        &ServerMessage::HandshakeResponse {
            success: true,
            version: Some(TRACKER_PROTOCOL_VERSION.to_string()),
            fingerprint: reported,
            error: None,
        },
    )
    .await?;

    // TrackerServerRegister/Response plus refresh cycles. Each payload is
    // captured for propagation tests. Idle timeout set well past any
    // refresh interval; loop exits cleanly when the task drops the conn.
    let close_after_register_responses = behavior
        .close_after_register_responses
        .lock()
        .expect("mock close queue lock poisoned")
        .pop_front();
    let mut responses_sent = 0usize;
    loop {
        let received = match read_tracker_client_message_with_full_timeout(
            &mut reader,
            Some(std::time::Duration::from_secs(600)),
            None,
        )
        .await
        {
            Ok(Some(r)) => r,
            Ok(None) => return Ok(()), // peer closed
            Err(_) => return Ok(()),   // connection torn down
        };
        if matches!(
            &received.message,
            TrackerClientMessage::TrackerServerRegister { .. }
        ) {
            behavior
                .captured_registers
                .lock()
                .expect("mock capture lock poisoned")
                .push(received.message.clone());
        }
        // Queued response wins; otherwise default to `register_response`.
        let policy = behavior
            .queued_responses
            .lock()
            .expect("mock queue lock poisoned")
            .pop_front()
            .unwrap_or_else(|| behavior.register_response.clone());
        let response = match policy {
            RegisterPolicy::Success { refresh_interval } => {
                TrackerServerMessage::TrackerServerRegisterResponse {
                    success: true,
                    refresh_interval: Some(refresh_interval),
                    error: None,
                    error_kind: None,
                }
            }
            RegisterPolicy::Failure { error_kind, error } => {
                TrackerServerMessage::TrackerServerRegisterResponse {
                    success: false,
                    refresh_interval: None,
                    error: Some(error),
                    error_kind: Some(error_kind),
                }
            }
        };
        send_tracker_server_message(&mut writer, &response).await?;
        responses_sent += 1;
        if close_after_register_responses.is_some_and(|limit| responses_sent >= limit) {
            return Ok(());
        }
    }
}

fn generate_cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>, String) {
    let key_pair = KeyPair::generate().expect("rcgen: generate keypair");
    let mut params = CertificateParams::new(vec![]).expect("rcgen: cert params");
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "mock-tracker");
    let cert = params
        .self_signed(&key_pair)
        .expect("rcgen: self-sign cert");

    let der_bytes = cert.der().to_vec();
    let fingerprint = nexus_common::fingerprint::format_certificate_fingerprint(&der_bytes);

    let cert_der = CertificateDer::from(der_bytes);
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    (cert_der, key_der, fingerprint)
}
