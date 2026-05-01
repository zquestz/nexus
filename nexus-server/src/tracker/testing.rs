//! Mock tracker for publisher-task lifecycle tests.
//!
//! Spawns a TLS listener on a random localhost port that speaks just
//! enough of the tracker protocol to exercise the publisher's state
//! machine: TLS handshake, BBS-style `Handshake`/`HandshakeResponse`,
//! one `TrackerServerRegister` cycle. Behavior (the self-reported
//! fingerprint, the register response) is configurable per test.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use nexus_common::TRACKER_PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_client_message, read_tracker_client_message_with_full_timeout, send_server_message,
    send_tracker_server_message,
};
use nexus_common::protocol::ServerMessage;
use nexus_common::tracker_protocol::TrackerServerMessage;
use rcgen::{CertificateParams, KeyPair};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// What the mock should do during one connection cycle.
#[derive(Clone)]
pub struct MockBehavior {
    /// Fingerprint to self-report in `HandshakeResponse`. `None` means
    /// "report my actual TLS cert fingerprint" — i.e. Stage 2 passes.
    /// `Some(other)` simulates an interception scenario where the TLS
    /// peer disagrees with what the tracker self-reports.
    pub reported_fingerprint: Option<String>,
    /// Default response to the `TrackerServerRegister` frame, used for
    /// every connection unless `queued_responses` has an entry.
    pub register_response: RegisterPolicy,
    /// Per-connection response queue. Each new TLS connection pops one
    /// from the front; once empty, falls back to `register_response`.
    /// Use this to simulate a transient-then-success sequence where
    /// the first connect rejects with `rate_limited` and the next
    /// accepts. Wrapped in `Arc<Mutex>` so all connection-handler
    /// clones share the same queue.
    pub queued_responses: Arc<Mutex<VecDeque<RegisterPolicy>>>,
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
        }
    }
}

/// Running mock tracker. Drop the value (or call [`Self::stop`]) to
/// signal the listener task to stop accepting new connections.
pub struct MockTracker {
    pub addr: SocketAddr,
    pub fingerprint: String,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl MockTracker {
    pub async fn start(behavior: MockBehavior) -> Self {
        // Install the rustls crypto provider for this process. Idempotent
        // — `install_default` returns Err if one is already installed,
        // which we ignore. (Production binaries do this in main.rs;
        // tests need to do it themselves.)
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

        let join = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept = listener.accept() => {
                        let Ok((tcp, _)) = accept else { break };
                        let acceptor = acceptor.clone();
                        let behavior = behavior.clone();
                        let real_fp = real_fp.clone();
                        tokio::spawn(async move {
                            // Best-effort. Connection errors are expected
                            // when the peer drops mid-protocol.
                            let _ = handle_connection(tcp, acceptor, behavior, real_fp).await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            fingerprint,
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
        // Don't await the join here; the listener task is well-behaved
        // and will exit on the next accept loop iteration.
    }
}

async fn handle_connection(
    tcp: tokio::net::TcpStream,
    acceptor: TlsAcceptor,
    behavior: MockBehavior,
    real_fingerprint: String,
) -> std::io::Result<()> {
    let tls = acceptor.accept(tcp).await?;
    let (read_half, write_half) = tokio::io::split(tls);
    let mut reader = FrameReader::new(tokio::io::BufReader::new(read_half));
    let mut writer = FrameWriter::new(write_half);

    // BBS-style Handshake / HandshakeResponse (the tracker protocol
    // reuses these at the handshake layer).
    let _handshake = read_client_message(&mut reader).await?;
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

    // TrackerServerRegister / TrackerServerRegisterResponse.
    let _register = read_tracker_client_message_with_full_timeout(&mut reader, None, None)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
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

    // Hold the connection open until the publisher (or the mock's
    // shutdown drop) closes it. The publisher's refresh interval is
    // far longer than any test duration, so we just block reading.
    loop {
        match read_tracker_client_message_with_full_timeout(&mut reader, None, None).await {
            Ok(Some(_)) => continue,   // unexpected mid-idle frame; ignore
            Ok(None) => return Ok(()), // peer closed
            Err(_) => return Ok(()),   // connection torn down
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
