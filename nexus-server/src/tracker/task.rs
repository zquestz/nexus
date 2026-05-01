//! Per-tracker publisher task: maintains one long-lived TLS connection
//! to a tracker and refreshes the registration on the tracker-supplied
//! interval.
//!
//! The task is spawned by [`super::manager::TrackerManager::spawn`] and
//! aborted via the `JoinHandle` it returns. The task is cancellation-
//! safe at every await point — drop semantics on the TLS stream and
//! the status `Arc<RwLock<TrackerStatus>>` handle cleanup.
//!
//! See `docs/TODO.md` § "Server-Side Publisher Implementation Plan"
//! for the design rationale.

use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use chrono::Utc;
use nexus_common::TRACKER_PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_server_message, read_tracker_server_message_with_full_timeout, send_client_message,
    send_tracker_client_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use rand::RngExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tracing::{debug, info, warn};

use super::context::PublisherContext;
use super::status::{TrackerStatus, is_unrecoverable_error_kind};
use super::tls::TLS_CONNECTOR;
use crate::db::TrackerRecord;

/// Base backoff between connection attempts on failure.
const BACKOFF_BASE: Duration = Duration::from_secs(5);
/// Cap on backoff growth — even after many consecutive failures, we
/// retry at this cadence.
const BACKOFF_CAP: Duration = Duration::from_secs(300);
/// Per-attempt jitter spread (±25%).
const BACKOFF_JITTER_PCT: f64 = 0.25;

/// Read timeout for tracker register responses. Generous enough for
/// normal latency; tight enough to recover from a wedged connection
/// instead of hanging until the outer 60s frame-completion timeout.
const REGISTER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Sentinel ServerName for the rustls handshake. We disable SNI in the
/// connector and TOFU-pin the cert ourselves, so the name doesn't
/// affect anything wire-visible — but rustls needs a syntactically-valid
/// name to construct the handshake.
const TLS_SERVER_NAME: &str = "tracker";

/// Run the publisher task for one tracker. Loops forever (with
/// backoff) until cancelled by the manager, or exits early on an
/// unrecoverable error. The manager's `JoinHandle` carries the exit.
///
/// The task owns:
/// - `record`: a snapshot of the tracker config at spawn time. If the
///   admin updates the row, the manager aborts this task and spawns a
///   fresh one with the new record — no in-place reconfiguration.
/// - `status`: shared with the manager (for admin reads). The task
///   updates fields as it transitions through phases.
/// - `context`: shared infrastructure (DB, UserManager, server fingerprint, ports).
pub async fn run(
    mut record: TrackerRecord,
    status: Arc<RwLock<TrackerStatus>>,
    context: Arc<PublisherContext>,
) {
    let mut backoff = BACKOFF_BASE;

    loop {
        // Reset connection-state fields at the start of each attempt;
        // preserve `pending_fingerprint` so admin-visible state from a
        // prior mismatch survives until the new attempt resolves it.
        {
            let mut s = status.write().expect("status lock poisoned");
            s.connected = false;
        }

        match attempt_connection_cycle(&mut record, &status, &context).await {
            CycleOutcome::Transient => {
                // Already logged + status-updated. Backoff and retry.
            }
            CycleOutcome::Unrecoverable => {
                // Already logged + status-updated. Exit the task; admin
                // intervention (TrackerUpdate or server restart) is the
                // recovery path.
                info!(
                    tracker_id = record.id,
                    "publisher: exiting task due to unrecoverable error"
                );
                return;
            }
        }

        // Backoff: jittered exponential, capped.
        let jittered = jitter(backoff);
        debug!(
            tracker_id = record.id,
            backoff_ms = jittered.as_millis() as u64,
            "publisher: backoff before retry"
        );
        tokio::time::sleep(jittered).await;
        backoff = (backoff * 2).min(BACKOFF_CAP);
    }
}

/// Outcome of one connection attempt.
enum CycleOutcome {
    /// Transient error (network blip, DNS miss, TLS hiccup, or a
    /// retryable tracker error like `rate_limited`). Backoff and retry.
    Transient,
    /// Permanent error (fingerprint mismatch, wrong password, malformed
    /// config). Exit the task; admin needs to fix the row.
    Unrecoverable,
}

/// Run one connection cycle: TCP → TLS → fingerprint stages →
/// tracker handshake → refresh loop. Returns when something exits.
async fn attempt_connection_cycle(
    record: &mut TrackerRecord,
    status: &Arc<RwLock<TrackerStatus>>,
    context: &Arc<PublisherContext>,
) -> CycleOutcome {
    // Phase 1: TCP connect.
    let tcp = match TcpStream::connect((record.address.as_str(), record.port)).await {
        Ok(s) => s,
        Err(e) => {
            warn!(
                tracker_id = record.id,
                err = %e,
                "publisher: TCP connect failed"
            );
            set_status_error(status, "connection_failed", e.to_string());
            return CycleOutcome::Transient;
        }
    };

    // Phase 2: TLS handshake.
    let server_name = match ServerName::try_from(TLS_SERVER_NAME) {
        Ok(n) => n,
        Err(_) => {
            // Unreachable — TLS_SERVER_NAME is a static valid name.
            // Treat as unrecoverable to avoid an infinite loop.
            set_status_error(
                status,
                "tls_failed",
                "internal: invalid sentinel ServerName".to_string(),
            );
            return CycleOutcome::Unrecoverable;
        }
    };
    let tls = match TLS_CONNECTOR.connect(server_name, tcp).await {
        Ok(s) => s,
        Err(e) => {
            warn!(
                tracker_id = record.id,
                err = %e,
                "publisher: TLS handshake failed"
            );
            set_status_error(status, "tls_failed", e.to_string());
            return CycleOutcome::Transient;
        }
    };

    // Phase 3: extract observed cert fingerprint.
    let tls_observed = match observed_fingerprint(&tls) {
        Some(fp) => fp,
        None => {
            warn!(
                tracker_id = record.id,
                "publisher: peer presented no certificates"
            );
            set_status_error(
                status,
                "tls_failed",
                "peer presented no certificates".to_string(),
            );
            return CycleOutcome::Transient;
        }
    };

    // Phase 4: Stage 1 — pinned fingerprint vs TLS-observed.
    if let Some(pinned) = &record.fingerprint
        && pinned != &tls_observed
    {
        warn!(
            tracker_id = record.id,
            pinned = %pinned,
            observed = %tls_observed,
            "publisher: Stage 1 fingerprint mismatch"
        );
        set_status_with_pending_fingerprint(
            status,
            "fingerprint_mismatch",
            "tracker certificate does not match the pinned fingerprint".to_string(),
            tls_observed.clone(),
        );
        return CycleOutcome::Unrecoverable;
    }

    // Phase 5: tracker handshake. Send Handshake, read HandshakeResponse.
    // Use BBS-protocol Handshake/HandshakeResponse — the tracker
    // protocol reuses those at the handshake layer.
    let (read_half, write_half) = tokio::io::split(tls);
    let mut reader = FrameReader::new(tokio::io::BufReader::new(read_half));
    let mut writer = FrameWriter::new(write_half);

    if let Err(e) = send_client_message(
        &mut writer,
        &ClientMessage::Handshake {
            version: TRACKER_PROTOCOL_VERSION.to_string(),
        },
    )
    .await
    {
        warn!(
            tracker_id = record.id,
            err = %e,
            "publisher: failed to send Handshake"
        );
        set_status_error(status, "handshake_failed", e.to_string());
        return CycleOutcome::Transient;
    }

    let server_reported = match read_handshake_response(&mut reader).await {
        Ok(fp) => fp,
        Err(e) => {
            warn!(
                tracker_id = record.id,
                err = %e,
                "publisher: HandshakeResponse error"
            );
            set_status_error(status, "handshake_failed", e);
            return CycleOutcome::Transient;
        }
    };

    // Phase 6: Stage 2 — TLS-observed vs server-reported.
    if server_reported != tls_observed {
        warn!(
            tracker_id = record.id,
            tls_observed = %tls_observed,
            server_reported = %server_reported,
            "publisher: Stage 2 fingerprint mismatch (interception suspicion)"
        );
        set_status_with_pending_fingerprint(
            status,
            "fingerprint_intercepted",
            "tracker self-reported fingerprint does not match its TLS certificate".to_string(),
            tls_observed.clone(),
        );
        return CycleOutcome::Unrecoverable;
    }

    // Phase 7: TOFU commit. Both stages passed; if no pin was stored,
    // this is the first connect — write the now-trusted fingerprint to
    // the DB and update the local record so subsequent iterations of
    // this task see it.
    if record.fingerprint.is_none() {
        if let Err(e) = context
            .db
            .trackers
            .update_fingerprint(record.id, &tls_observed)
            .await
        {
            warn!(
                tracker_id = record.id,
                err = %e,
                "publisher: failed to TOFU-write fingerprint"
            );
            set_status_error(status, "db_failed", e.to_string());
            return CycleOutcome::Transient;
        }
        record.fingerprint = Some(tls_observed.clone());
        info!(
            tracker_id = record.id,
            fingerprint = %tls_observed,
            "publisher: TOFU-pinned fingerprint"
        );
    }

    // Clear pending_fingerprint now that both stages agree (covers the
    // admin-accept-then-reconnect path, where a previous task left a
    // pending observation in status before being respawned).
    {
        let mut s = status.write().expect("status lock poisoned");
        s.pending_fingerprint = None;
    }

    // Phase 8: register / refresh loop.
    refresh_loop(record, status, context, &mut reader, &mut writer).await
}

/// The inner refresh loop. Sends `TrackerServerRegister` immediately,
/// then on each tracker-supplied refresh interval. `tokio::select!`
/// between sleep and read so a connection drop mid-sleep is detected
/// promptly.
async fn refresh_loop<R, W>(
    record: &TrackerRecord,
    status: &Arc<RwLock<TrackerStatus>>,
    context: &Arc<PublisherContext>,
    reader: &mut FrameReader<R>,
    writer: &mut FrameWriter<W>,
) -> CycleOutcome
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut sleep_for = Duration::ZERO;

    loop {
        // Wait until it's time to refresh, OR until the tracker sends
        // us something (which is unexpected — the tracker only sends
        // responses to our requests; any frame read here likely means
        // the connection is closing).
        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {}
            frame = read_tracker_server_message_with_full_timeout(reader, None, None) => {
                match frame {
                    Ok(Some(_)) => {
                        // Unexpected mid-idle frame; treat as connection
                        // anomaly and reconnect.
                        warn!(
                            tracker_id = record.id,
                            "publisher: unexpected mid-idle frame; reconnecting"
                        );
                        set_status_error(
                            status,
                            "connection_lost",
                            "unexpected frame from tracker mid-idle".to_string(),
                        );
                        return CycleOutcome::Transient;
                    }
                    Ok(None) => {
                        // Clean close from the peer.
                        debug!(
                            tracker_id = record.id,
                            "publisher: tracker closed connection mid-idle"
                        );
                        set_status_error(
                            status,
                            "connection_lost",
                            "tracker closed connection".to_string(),
                        );
                        return CycleOutcome::Transient;
                    }
                    Err(e) => {
                        warn!(
                            tracker_id = record.id,
                            err = %e,
                            "publisher: read error mid-idle"
                        );
                        set_status_error(status, "connection_lost", e.to_string());
                        return CycleOutcome::Transient;
                    }
                }
            }
        }

        // Build and send the register message.
        let payload = match build_register_payload(record, context).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    tracker_id = record.id,
                    err = %e,
                    "publisher: failed to build register payload"
                );
                set_status_error(status, "db_failed", e);
                return CycleOutcome::Transient;
            }
        };

        if let Err(e) = send_tracker_client_message(writer, &payload).await {
            warn!(
                tracker_id = record.id,
                err = %e,
                "publisher: failed to send TrackerServerRegister"
            );
            set_status_error(status, "connection_lost", e.to_string());
            return CycleOutcome::Transient;
        }

        // Read the response with a tight per-refresh timeout.
        let response = match tokio::time::timeout(
            REGISTER_RESPONSE_TIMEOUT,
            read_tracker_server_message_with_full_timeout(reader, None, None),
        )
        .await
        {
            Ok(Ok(Some(r))) => r.message,
            Ok(Ok(None)) => {
                warn!(
                    tracker_id = record.id,
                    "publisher: tracker closed connection awaiting register response"
                );
                set_status_error(
                    status,
                    "connection_lost",
                    "tracker closed connection".to_string(),
                );
                return CycleOutcome::Transient;
            }
            Ok(Err(e)) => {
                warn!(
                    tracker_id = record.id,
                    err = %e,
                    "publisher: error reading register response"
                );
                set_status_error(status, "connection_lost", e.to_string());
                return CycleOutcome::Transient;
            }
            Err(_elapsed) => {
                warn!(
                    tracker_id = record.id,
                    "publisher: timeout awaiting register response"
                );
                set_status_error(
                    status,
                    "connection_lost",
                    "timeout awaiting register response".to_string(),
                );
                return CycleOutcome::Transient;
            }
        };

        match response {
            TrackerServerMessage::TrackerServerRegisterResponse {
                success: true,
                refresh_interval,
                ..
            } => {
                let interval = refresh_interval.unwrap_or(300);
                {
                    let mut s = status.write().expect("status lock poisoned");
                    s.connected = true;
                    s.last_connected_at = Some(Utc::now().timestamp());
                    s.last_error = None;
                    s.last_error_kind = None;
                    s.refresh_interval = Some(interval);
                }
                debug!(
                    tracker_id = record.id,
                    refresh_interval = interval,
                    "publisher: refreshed"
                );
                sleep_for = Duration::from_secs(u64::from(interval));
            }
            TrackerServerMessage::TrackerServerRegisterResponse {
                success: false,
                error,
                error_kind,
                ..
            } => {
                let kind = error_kind.unwrap_or_else(|| "invalid".to_string());
                let msg = error.unwrap_or_else(|| kind.clone());
                warn!(
                    tracker_id = record.id,
                    error_kind = %kind,
                    err = %msg,
                    "publisher: tracker rejected register"
                );
                set_status_error(status, &kind, msg);
                return if is_unrecoverable_error_kind(&kind) {
                    CycleOutcome::Unrecoverable
                } else {
                    CycleOutcome::Transient
                };
            }
            // Any other variant on the register port is a protocol
            // violation by the tracker. Treat as transient — likely
            // version skew or daemon bug.
            other => {
                warn!(
                    tracker_id = record.id,
                    response = ?other,
                    "publisher: unexpected response to TrackerServerRegister"
                );
                set_status_error(
                    status,
                    "handshake_failed",
                    "tracker returned unexpected response type".to_string(),
                );
                return CycleOutcome::Transient;
            }
        }
    }
}

/// Build the `TrackerServerRegister` payload from the record + the
/// per-refresh field bundle (server name, description, public address,
/// guest enabled) plus the live user_count from `UserManager`.
async fn build_register_payload(
    record: &TrackerRecord,
    context: &Arc<PublisherContext>,
) -> Result<TrackerClientMessage, String> {
    let fields = context
        .db
        .tracker_registration_fields()
        .await
        .map_err(|e| e.to_string())?;
    let user_count = context.user_manager.user_count().await;
    Ok(TrackerClientMessage::TrackerServerRegister {
        password: record.password.clone(),
        locale: "en".to_string(),
        name: fields.server_name,
        description: fields.description,
        address: fields.public_address,
        port: context.server_port,
        websocket_port: context.server_websocket_port,
        version: env!("CARGO_PKG_VERSION").to_string(),
        fingerprint: context.server_fingerprint.clone(),
        user_count,
        allows_guest: fields.allows_guest,
    })
}

/// Read the tracker's `HandshakeResponse` and return its
/// server-reported fingerprint. Errors carry a string suitable for the
/// `last_error` status field.
async fn read_handshake_response<R>(reader: &mut FrameReader<R>) -> Result<String, String>
where
    R: AsyncReadExt + Unpin,
{
    let received = read_server_message(reader)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "tracker closed connection during handshake".to_string())?;

    match received.message {
        ServerMessage::HandshakeResponse {
            success: true,
            fingerprint,
            ..
        } => Ok(fingerprint),
        ServerMessage::HandshakeResponse {
            success: false,
            error,
            ..
        } => Err(error.unwrap_or_else(|| "tracker rejected handshake".to_string())),
        other => Err(format!(
            "tracker returned unexpected response to Handshake: {other:?}"
        )),
    }
}

/// Compute the canonical fingerprint of the peer's TLS cert.
fn observed_fingerprint(stream: &tokio_rustls::client::TlsStream<TcpStream>) -> Option<String> {
    let (_io, session) = stream.get_ref();
    let certs = session.peer_certificates()?;
    let first = certs.first()?;
    Some(nexus_common::fingerprint::format_certificate_fingerprint(
        first.as_ref(),
    ))
}

// ---- Status update helpers ----

fn set_status_error(status: &Arc<RwLock<TrackerStatus>>, kind: &str, message: String) {
    let mut s = status.write().expect("status lock poisoned");
    s.connected = false;
    s.last_error_kind = Some(kind.to_string());
    s.last_error = Some(message);
    s.refresh_interval = None;
}

fn set_status_with_pending_fingerprint(
    status: &Arc<RwLock<TrackerStatus>>,
    kind: &str,
    message: String,
    pending: String,
) {
    let mut s = status.write().expect("status lock poisoned");
    s.connected = false;
    s.last_error_kind = Some(kind.to_string());
    s.last_error = Some(message);
    s.pending_fingerprint = Some(pending);
    s.refresh_interval = None;
}

// ---- Backoff helpers ----

/// Apply ±25% jitter to a backoff duration.
fn jitter(base: Duration) -> Duration {
    let mut rng = rand::rng();
    let factor = 1.0 + rng.random_range(-BACKOFF_JITTER_PCT..=BACKOFF_JITTER_PCT);
    let millis = base.as_millis() as f64 * factor;
    Duration::from_millis(millis.max(0.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use crate::db::testing::create_test_db;
    use crate::db::{CreateTrackerParams, Database};
    use crate::tracker::testing::{MockBehavior, MockTracker, RegisterPolicy};
    use crate::users::UserManager;

    const TEST_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
        AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

    #[test]
    fn jitter_stays_within_25_percent_band() {
        // Sample many to verify the band; not a statistical guarantee
        // but tight enough to catch a misimplemented range.
        let base = Duration::from_millis(1000);
        let lower = Duration::from_millis(700); // 30% below band edge for slack
        let upper = Duration::from_millis(1300);
        for _ in 0..1000 {
            let j = jitter(base);
            assert!(
                j >= lower && j <= upper,
                "jitter {j:?} outside expected band [{lower:?}, {upper:?}]"
            );
        }
    }

    #[test]
    fn backoff_doubling_caps_at_max() {
        let mut backoff = BACKOFF_BASE;
        for _ in 0..20 {
            backoff = (backoff * 2).min(BACKOFF_CAP);
        }
        assert_eq!(backoff, BACKOFF_CAP);
    }

    /// Build a `PublisherContext` over a fresh in-memory DB. Sets a
    /// distinct `server_name` and `public_address` so payload tests can
    /// verify the right values flow through.
    async fn setup_context() -> (Arc<Database>, Arc<PublisherContext>) {
        let pool = create_test_db().await;
        let db = Arc::new(Database::new(pool));
        db.config
            .set_server_name("Test BBS")
            .await
            .expect("set server name");
        db.config
            .set_server_description("a test server")
            .await
            .expect("set description");
        db.config
            .set_public_address("bbs.example.com")
            .await
            .expect("set public address");
        let user_manager = Arc::new(UserManager::new());
        let context = Arc::new(PublisherContext {
            db: db.clone(),
            user_manager,
            server_fingerprint: TEST_FINGERPRINT.to_string(),
            server_port: 7500,
            server_websocket_port: Some(7502),
        });
        (db, context)
    }

    /// Insert one tracker row pointing at the mock's address. Returns
    /// the inserted record.
    async fn seed_tracker(
        db: &Database,
        addr: std::net::SocketAddr,
        fingerprint: Option<&str>,
        password: Option<&str>,
    ) -> TrackerRecord {
        db.trackers
            .create(CreateTrackerParams {
                address: &addr.ip().to_string(),
                port: addr.port(),
                fingerprint,
                password,
                name: "Mock",
                enabled: true,
            })
            .await
            .expect("seed tracker row")
    }

    /// Poll the status until `pred` returns true, or until `timeout`
    /// elapses. Returns the final snapshot when `pred` matched, else
    /// `None`.
    async fn wait_for_status(
        status: &Arc<RwLock<TrackerStatus>>,
        timeout: Duration,
        pred: impl Fn(&TrackerStatus) -> bool,
    ) -> Option<TrackerStatus> {
        let start = Instant::now();
        loop {
            {
                let snap = status.read().expect("status lock").clone();
                if pred(&snap) {
                    return Some(snap);
                }
            }
            if start.elapsed() >= timeout {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    async fn build_register_payload_includes_all_fields() {
        let (_db, context) = setup_context().await;

        let record = TrackerRecord {
            id: 1,
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: Some(TEST_FINGERPRINT.to_string()),
            password: Some("hunter2".to_string()),
            name: "Test".to_string(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };

        let payload = build_register_payload(&record, &context)
            .await
            .expect("payload");

        match payload {
            TrackerClientMessage::TrackerServerRegister {
                password,
                name,
                description,
                address,
                port,
                websocket_port,
                fingerprint,
                user_count,
                allows_guest,
                ..
            } => {
                assert_eq!(password.as_deref(), Some("hunter2"));
                assert_eq!(name, "Test BBS");
                assert_eq!(description.as_deref(), Some("a test server"));
                assert_eq!(address.as_deref(), Some("bbs.example.com"));
                assert_eq!(port, 7500);
                assert_eq!(websocket_port, Some(7502));
                assert_eq!(fingerprint, TEST_FINGERPRINT);
                assert_eq!(user_count, 0);
                // The bootstrap migration creates the guest account
                // with `enabled = false` by default.
                assert!(!allows_guest);
            }
            other => panic!("expected TrackerServerRegister, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn happy_path_marks_connected() {
        let mock = MockTracker::start(MockBehavior::default()).await;
        let (db, context) = setup_context().await;
        let record = seed_tracker(&db, mock.addr, None, None).await;

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task = tokio::spawn(run(
            record.clone(),
            Arc::clone(&status),
            Arc::clone(&context),
        ));

        let snap = wait_for_status(&status, Duration::from_secs(5), |s| s.connected)
            .await
            .expect("expected connected status within timeout");
        assert!(snap.connected);
        assert_eq!(snap.refresh_interval, Some(300));
        assert!(snap.last_error.is_none());
        assert!(snap.last_error_kind.is_none());

        task.abort();
        let _ = task.await;
        mock.stop().await;
    }

    #[tokio::test]
    async fn tofu_writes_pin_on_first_connect() {
        let mock = MockTracker::start(MockBehavior::default()).await;
        let mock_fp = mock.fingerprint.clone();
        let (db, context) = setup_context().await;
        // Seed with no pin → TOFU path.
        let record = seed_tracker(&db, mock.addr, None, None).await;
        assert!(record.fingerprint.is_none());

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task = tokio::spawn(run(
            record.clone(),
            Arc::clone(&status),
            Arc::clone(&context),
        ));

        wait_for_status(&status, Duration::from_secs(5), |s| s.connected)
            .await
            .expect("expected connected status within timeout");

        // The DB row's fingerprint should now be the mock's actual cert FP.
        let stored = db
            .trackers
            .get_by_id(record.id)
            .await
            .expect("get_by_id")
            .expect("row present");
        assert_eq!(stored.fingerprint.as_deref(), Some(mock_fp.as_str()));

        task.abort();
        let _ = task.await;
        mock.stop().await;
    }

    #[tokio::test]
    async fn fingerprint_mismatch_parks_pending() {
        let mock = MockTracker::start(MockBehavior::default()).await;
        let mock_fp = mock.fingerprint.clone();
        let (db, context) = setup_context().await;

        // Seed with a wrong pin so Stage 1 fails when the publisher
        // compares it against the mock's actual TLS cert.
        let wrong_pin = "11:22:33:44:55:66:77:88:99:00:AA:BB:CC:DD:EE:FF:\
            11:22:33:44:55:66:77:88:99:00:AA:BB:CC:DD:EE:FF";
        let record = seed_tracker(&db, mock.addr, Some(wrong_pin), None).await;

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task = tokio::spawn(run(
            record.clone(),
            Arc::clone(&status),
            Arc::clone(&context),
        ));

        let snap = wait_for_status(&status, Duration::from_secs(5), |s| {
            s.last_error_kind.as_deref() == Some("fingerprint_mismatch")
        })
        .await
        .expect("expected fingerprint_mismatch status within timeout");

        assert_eq!(snap.pending_fingerprint.as_deref(), Some(mock_fp.as_str()));
        assert!(!snap.connected);

        // Stage 1 mismatch is unrecoverable → task exits.
        let exited = tokio::time::timeout(Duration::from_secs(2), task).await;
        assert!(exited.is_ok(), "task should exit on fingerprint_mismatch");

        mock.stop().await;
    }

    #[tokio::test]
    async fn unrecoverable_error_kind_exits_task() {
        let mock = MockTracker::start(MockBehavior {
            register_response: RegisterPolicy::Failure {
                error_kind: "unauthorized".to_string(),
                error: "wrong password".to_string(),
            },
            ..Default::default()
        })
        .await;
        let (db, context) = setup_context().await;
        let record = seed_tracker(&db, mock.addr, None, None).await;

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task = tokio::spawn(run(
            record.clone(),
            Arc::clone(&status),
            Arc::clone(&context),
        ));

        // Task should exit shortly after receiving the rejection — no
        // backoff, no retry.
        let exited = tokio::time::timeout(Duration::from_secs(5), task).await;
        assert!(exited.is_ok(), "task should exit on unrecoverable kind");

        let snap = status.read().expect("status lock").clone();
        assert_eq!(snap.last_error_kind.as_deref(), Some("unauthorized"));
        assert!(!snap.connected);

        mock.stop().await;
    }
}
