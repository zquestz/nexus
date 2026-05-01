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
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_server_message, read_tracker_server_message_with_full_timeout, send_client_message,
    send_tracker_client_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use nexus_common::{
    ERROR_KIND_TRACKER_CONNECTION_FAILED, ERROR_KIND_TRACKER_CONNECTION_LOST,
    ERROR_KIND_TRACKER_DB_FAILED, ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED,
    ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH, ERROR_KIND_TRACKER_HANDSHAKE_FAILED,
    ERROR_KIND_TRACKER_TLS_FAILED, TRACKER_PROTOCOL_VERSION, is_unrecoverable_error_kind,
};
use rand::RngExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tracing::{debug, error, info, warn};

use super::context::PublisherContext;
use super::status::TrackerStatus;
use super::tls::TLS_CONNECTOR;
use crate::constants::{
    EXPECT_TRACKER_STATUS_LOCK_POISONED, LOG_TRACKER_REGISTRATION_BACKOFF,
    LOG_TRACKER_REGISTRATION_BUILD_PAYLOAD_FAILED,
    LOG_TRACKER_REGISTRATION_CLOSED_AWAITING_RESPONSE, LOG_TRACKER_REGISTRATION_CLOSED_MID_IDLE,
    LOG_TRACKER_REGISTRATION_EXITING, LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR,
    LOG_TRACKER_REGISTRATION_INVALID_HOST, LOG_TRACKER_REGISTRATION_NO_PEER_CERTS,
    LOG_TRACKER_REGISTRATION_READ_ERROR_MID_IDLE, LOG_TRACKER_REGISTRATION_REFRESHED,
    LOG_TRACKER_REGISTRATION_REGISTER_REJECTED, LOG_TRACKER_REGISTRATION_RESPONSE_READ_ERROR,
    LOG_TRACKER_REGISTRATION_RESPONSE_TIMEOUT, LOG_TRACKER_REGISTRATION_SEND_HANDSHAKE_FAILED,
    LOG_TRACKER_REGISTRATION_SEND_REGISTER_FAILED, LOG_TRACKER_REGISTRATION_STAGE1_MISMATCH,
    LOG_TRACKER_REGISTRATION_STAGE2_MISMATCH, LOG_TRACKER_REGISTRATION_TCP_FAILED,
    LOG_TRACKER_REGISTRATION_TLS_FAILED, LOG_TRACKER_REGISTRATION_TOFU_PINNED,
    LOG_TRACKER_REGISTRATION_TOFU_WRITE_FAILED, LOG_TRACKER_REGISTRATION_UNEXPECTED_FRAME,
    LOG_TRACKER_REGISTRATION_UNEXPECTED_RESPONSE,
};
use crate::db::{TrackerRecord, is_transient_db_error};

/// Base backoff between connection attempts on failure.
///
/// In tests this collapses to 100ms so retry-then-success scenarios
/// run in well under the test's outer timeout, and existing tests
/// don't risk overlap between the publisher's backoff deadline and
/// the test's `wait_for_status` deadline. Production behavior is
/// unaffected.
#[cfg(not(test))]
const BACKOFF_BASE: Duration = Duration::from_secs(5);
#[cfg(test)]
const BACKOFF_BASE: Duration = Duration::from_millis(100);
/// Cap on backoff growth — even after many consecutive failures, we
/// retry at this cadence.
const BACKOFF_CAP: Duration = Duration::from_secs(300);
/// Per-attempt jitter spread (±25%).
const BACKOFF_JITTER_PCT: f64 = 0.25;

/// Read timeout for tracker responses (both `HandshakeResponse` and
/// `TrackerServerRegisterResponse`). Generous enough for normal
/// latency; tight enough to recover from a wedged connection instead
/// of hanging until the outer 60s frame-completion timeout.
const TRACKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

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
            let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
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
                    id = record.id,
                    name = %record.name,
                    "{}", LOG_TRACKER_REGISTRATION_EXITING
                );
                return;
            }
        }

        // Backoff: jittered exponential, capped.
        let jittered = jitter(backoff);
        debug!(
            id = record.id,
            name = %record.name,
            backoff_ms = jittered.as_millis() as u64,
            "{}", LOG_TRACKER_REGISTRATION_BACKOFF
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
    // Phase 0: resolve the admin-supplied address into the form both
    // the system resolver and rustls's `ServerName::try_from` expect
    // — strips IPv6 URL brackets, passes IP literals through, and
    // Punycode-encodes Unicode hostnames. The validator at
    // `TrackerCreate`/`TrackerUpdate` already accepts the same set
    // (via `domain_to_ascii_strict`), so a failure here is rare and
    // operator-actionable: the row needs editing.
    let resolved_host = match nexus_common::address::resolve_host_for_connection(&record.address) {
        Ok(h) => h,
        Err(e) => {
            warn!(
                id = record.id,
                name = %record.name,
                address = %record.address,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_INVALID_HOST
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient;
        }
    };

    // Phase 1: TCP connect.
    let tcp = match TcpStream::connect((resolved_host.as_str(), record.port)).await {
        Ok(s) => s,
        Err(e) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_TCP_FAILED
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient;
        }
    };

    // Phase 2: TLS handshake. The resolved host doubles as the
    // `ServerName` rustls demands; SNI is off and we TOFU-pin the
    // cert post-handshake, so the value is otherwise unobservable on
    // the wire and uncompared against the cert.
    let server_name = match ServerName::try_from(resolved_host.clone()) {
        Ok(n) => n,
        Err(e) => {
            warn!(
                id = record.id,
                name = %record.name,
                address = %record.address,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_INVALID_HOST
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient;
        }
    };
    let tls = match TLS_CONNECTOR.connect(server_name, tcp).await {
        Ok(s) => s,
        Err(e) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_TLS_FAILED
            );
            set_status_error(status, ERROR_KIND_TRACKER_TLS_FAILED);
            return CycleOutcome::Transient;
        }
    };

    // Phase 3: extract observed cert fingerprint.
    let tls_observed = match observed_fingerprint(&tls) {
        Some(fp) => fp,
        None => {
            warn!(
                id = record.id,
                name = %record.name,
                "{}", LOG_TRACKER_REGISTRATION_NO_PEER_CERTS
            );
            set_status_error(status, ERROR_KIND_TRACKER_TLS_FAILED);
            return CycleOutcome::Transient;
        }
    };

    // Phase 4: Stage 1 — pinned fingerprint vs TLS-observed.
    if let Some(pinned) = &record.fingerprint
        && pinned != &tls_observed
    {
        warn!(
            id = record.id,
            name = %record.name,
            pinned = %pinned,
            observed = %tls_observed,
            "{}", LOG_TRACKER_REGISTRATION_STAGE1_MISMATCH
        );
        set_status_with_pending_fingerprint(
            status,
            ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
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
            id = record.id,
            name = %record.name,
            err = %e,
            "{}", LOG_TRACKER_REGISTRATION_SEND_HANDSHAKE_FAILED
        );
        set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
        return CycleOutcome::Transient;
    }

    // A wedged tracker that completes TLS but never sends the
    // `HandshakeResponse` would otherwise park us in `await` forever
    // — wrap the read in the same per-call deadline used for the
    // register response.
    let server_reported = match tokio::time::timeout(
        TRACKER_RESPONSE_TIMEOUT,
        read_handshake_response(&mut reader),
    )
    .await
    {
        Ok(Ok(fp)) => fp,
        Ok(Err(e)) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient;
        }
        Err(_elapsed) => {
            warn!(
                id = record.id,
                name = %record.name,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient;
        }
    };

    // Phase 6: Stage 2 — TLS-observed vs server-reported.
    if server_reported != tls_observed {
        warn!(
            id = record.id,
            name = %record.name,
            tls_observed = %tls_observed,
            server_reported = %server_reported,
            "{}", LOG_TRACKER_REGISTRATION_STAGE2_MISMATCH
        );
        set_status_with_pending_fingerprint(
            status,
            ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED,
            tls_observed.clone(),
        );
        return CycleOutcome::Unrecoverable;
    }

    // Phase 7: TOFU commit. Both stages passed; if no pin was stored,
    // this is the first connect — write the now-trusted fingerprint to
    // the DB and update the local record so subsequent iterations of
    // this task see it.
    if record.fingerprint.is_none() {
        match context
            .db
            .trackers
            .update_fingerprint(record.id, &tls_observed)
            .await
        {
            Ok(_) => {}
            Err(e) if is_transient_db_error(&e) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    err = %e,
                    "{}", LOG_TRACKER_REGISTRATION_TOFU_WRITE_FAILED
                );
                set_status_error(status, ERROR_KIND_TRACKER_DB_FAILED);
                return CycleOutcome::Transient;
            }
            Err(e) => {
                error!(
                    id = record.id,
                    name = %record.name,
                    err = %e,
                    "{}", LOG_TRACKER_REGISTRATION_TOFU_WRITE_FAILED
                );
                set_status_error(status, ERROR_KIND_TRACKER_DB_FAILED);
                return CycleOutcome::Unrecoverable;
            }
        }
        record.fingerprint = Some(tls_observed.clone());
        info!(
            id = record.id,
            name = %record.name,
            fingerprint = %tls_observed,
            "{}", LOG_TRACKER_REGISTRATION_TOFU_PINNED
        );
    }

    // Clear pending_fingerprint now that both stages agree (covers the
    // admin-accept-then-reconnect path, where a previous task left a
    // pending observation in status before being respawned).
    {
        let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
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
                            id = record.id,
                            name = %record.name,
                            "{}", LOG_TRACKER_REGISTRATION_UNEXPECTED_FRAME
                        );
                        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                        return CycleOutcome::Transient;
                    }
                    Ok(None) => {
                        // Clean close from the peer.
                        debug!(
                            id = record.id,
                            name = %record.name,
                            "{}", LOG_TRACKER_REGISTRATION_CLOSED_MID_IDLE
                        );
                        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                        return CycleOutcome::Transient;
                    }
                    Err(e) => {
                        warn!(
                            id = record.id,
                            name = %record.name,
                            err = %e,
                            "{}", LOG_TRACKER_REGISTRATION_READ_ERROR_MID_IDLE
                        );
                        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
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
                    id = record.id,
                    name = %record.name,
                    err = %e,
                    "{}", LOG_TRACKER_REGISTRATION_BUILD_PAYLOAD_FAILED
                );
                set_status_error(status, ERROR_KIND_TRACKER_DB_FAILED);
                return CycleOutcome::Transient;
            }
        };

        if let Err(e) = send_tracker_client_message(writer, &payload).await {
            warn!(
                id = record.id,
                name = %record.name,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_SEND_REGISTER_FAILED
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
            return CycleOutcome::Transient;
        }

        // Read the response with a tight per-refresh timeout.
        let response = match tokio::time::timeout(
            TRACKER_RESPONSE_TIMEOUT,
            read_tracker_server_message_with_full_timeout(reader, None, None),
        )
        .await
        {
            Ok(Ok(Some(r))) => r.message,
            Ok(Ok(None)) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    "{}", LOG_TRACKER_REGISTRATION_CLOSED_AWAITING_RESPONSE
                );
                set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                return CycleOutcome::Transient;
            }
            Ok(Err(e)) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    err = %e,
                    "{}", LOG_TRACKER_REGISTRATION_RESPONSE_READ_ERROR
                );
                set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                return CycleOutcome::Transient;
            }
            Err(_elapsed) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    "{}", LOG_TRACKER_REGISTRATION_RESPONSE_TIMEOUT
                );
                set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
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
                    let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
                    s.connected = true;
                    s.last_connected_at = Some(Utc::now().timestamp());
                    s.last_error_kind = None;
                    s.refresh_interval = Some(interval);
                }
                debug!(
                    id = record.id,
                    name = %record.name,
                    refresh_interval = interval,
                    "{}", LOG_TRACKER_REGISTRATION_REFRESHED
                );
                sleep_for = Duration::from_secs(u64::from(interval));
            }
            TrackerServerMessage::TrackerServerRegisterResponse {
                success: false,
                error,
                error_kind,
                ..
            } => {
                // Outcome decision happens against the *raw* tracker
                // input: only an explicit, recognized unrecoverable
                // kind kills the task. A missing or unknown kind is a
                // protocol violation by the tracker (or a forward-
                // compat kind we don't know yet) — treat as transient
                // so a misbehaving tracker can't permanently take us
                // out without admin intervention.
                let outcome = match error_kind.as_deref() {
                    Some(k) if is_unrecoverable_error_kind(k) => CycleOutcome::Unrecoverable,
                    _ => CycleOutcome::Transient,
                };
                // Default for `last_error_kind` when the tracker
                // omitted one: surface "connection lost" to the admin
                // (semantically the wire ate the response) rather
                // than "invalid", which would falsely imply the
                // tracker validated and rejected our payload.
                let kind =
                    error_kind.unwrap_or_else(|| ERROR_KIND_TRACKER_CONNECTION_LOST.to_string());
                // Tracker-supplied `error` text (already localized by
                // the tracker to whatever locale we sent in the
                // register; we hard-code `"en"`) is logged for the
                // operator only — it never reaches the admin's UI,
                // which renders the kind via the BBS server's own
                // i18n bundle in the admin's locale.
                let detail = error.unwrap_or_default();
                warn!(
                    id = record.id,
                    name = %record.name,
                    error_kind = %kind,
                    err = %detail,
                    "{}", LOG_TRACKER_REGISTRATION_REGISTER_REJECTED
                );
                set_status_error(status, &kind);
                return outcome;
            }
            // Any other variant on the register port is a protocol
            // violation by the tracker. Treat as transient — likely
            // version skew or daemon bug.
            other => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    response = ?other,
                    "{}", LOG_TRACKER_REGISTRATION_UNEXPECTED_RESPONSE
                );
                set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
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
/// server-reported fingerprint. The `Err(String)` carries operator-log
/// context (the underlying `io::Error` or framing error display);
/// admin-facing status uses only the kind, never this string.
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
//
// These set only the machine-readable `last_error_kind`. The
// human-readable message shown in admin UIs is translated at
// handler compose-time using the requesting admin's locale; the
// raw underlying error (if any) flows to operator logs separately
// via `warn!(err = %e, …)` at the call site.

fn set_status_error(status: &Arc<RwLock<TrackerStatus>>, kind: &str) {
    let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
    s.connected = false;
    s.last_error_kind = Some(kind.to_string());
    s.refresh_interval = None;
}

fn set_status_with_pending_fingerprint(
    status: &Arc<RwLock<TrackerStatus>>,
    kind: &str,
    pending: String,
) {
    let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
    s.connected = false;
    s.last_error_kind = Some(kind.to_string());
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
    use std::collections::VecDeque;
    use std::sync::Mutex;

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
    /// of *tokio time* elapses. Returns the final snapshot when `pred`
    /// matched, else `None`.
    ///
    /// Uses `tokio::time::timeout` (not `std::time::Instant`) so the
    /// deadline respects `#[tokio::test(start_paused = true)]`. Under
    /// paused time, both the inner 25ms poll-sleep and the outer
    /// timeout advance the virtual clock together, so backoff-driven
    /// scenarios run in milliseconds of wallclock.
    async fn wait_for_status(
        status: &Arc<RwLock<TrackerStatus>>,
        timeout: Duration,
        pred: impl Fn(&TrackerStatus) -> bool,
    ) -> Option<TrackerStatus> {
        tokio::time::timeout(timeout, async {
            loop {
                {
                    let snap = status.read().expect("status lock").clone();
                    if pred(&snap) {
                        return snap;
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .ok()
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
            s.last_error_kind.as_deref() == Some(ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH)
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
    async fn fingerprint_intercepted_parks_pending_and_exits() {
        // Stage 2: TLS-observed cert disagrees with what the tracker
        // self-reports in HandshakeResponse. Under interception this is
        // the only signal — the attacker may hold a cert the client
        // would otherwise accept, but they can't forge the real
        // tracker's self-report.
        let lying_fingerprint = "11:22:33:44:55:66:77:88:99:00:AA:BB:CC:DD:EE:FF:\
            11:22:33:44:55:66:77:88:99:00:AA:BB:CC:DD:EE:FF";
        let mock = MockTracker::start(MockBehavior {
            reported_fingerprint: Some(lying_fingerprint.to_string()),
            ..Default::default()
        })
        .await;
        let mock_fp = mock.fingerprint.clone();
        let (db, context) = setup_context().await;
        // No pin → Stage 1 skipped; we reach the handshake and Stage 2
        // is what fires.
        let record = seed_tracker(&db, mock.addr, None, None).await;

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task = tokio::spawn(run(
            record.clone(),
            Arc::clone(&status),
            Arc::clone(&context),
        ));

        let snap = wait_for_status(&status, Duration::from_secs(5), |s| {
            s.last_error_kind.as_deref() == Some(ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED)
        })
        .await
        .expect("expected fingerprint_intercepted status within timeout");

        // Pending fingerprint records the *TLS-observed* cert (not the
        // self-reported lie) so the admin can compare.
        assert_eq!(snap.pending_fingerprint.as_deref(), Some(mock_fp.as_str()));
        assert!(!snap.connected);

        // Stage 2 mismatch is unrecoverable → task exits.
        let exited = tokio::time::timeout(Duration::from_secs(2), task).await;
        assert!(
            exited.is_ok(),
            "task should exit on fingerprint_intercepted"
        );

        // The DB row's fingerprint must NOT have been TOFU-pinned, since
        // Stage 2 failed before the pin commit.
        let stored = db
            .trackers
            .get_by_id(record.id)
            .await
            .expect("get_by_id")
            .expect("row present");
        assert!(
            stored.fingerprint.is_none(),
            "TOFU pin must not be written when Stage 2 fails"
        );

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
        assert_eq!(
            snap.last_error_kind.as_deref(),
            Some(nexus_common::ERROR_KIND_UNAUTHORIZED)
        );
        assert!(!snap.connected);

        mock.stop().await;
    }

    #[tokio::test]
    async fn rate_limited_then_succeeds() {
        // First connection: tracker rejects with `rate_limited` (a
        // transient kind). Publisher should backoff (~5s, jittered)
        // and reconnect. Second connection: tracker accepts. Status
        // should land at connected = true.
        let mut queue = VecDeque::new();
        queue.push_back(RegisterPolicy::Failure {
            error_kind: nexus_common::ERROR_KIND_RATE_LIMITED.to_string(),
            error: "slow down".to_string(),
        });
        let mock = MockTracker::start(MockBehavior {
            queued_responses: Arc::new(Mutex::new(queue)),
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

        // Test-mode `BACKOFF_BASE` is 100ms (cfg(test) override), so
        // the retry happens well within the standard 5s timeout —
        // backoff + reconnect + handshake all complete in <1s on
        // loopback, leaving generous headroom for slow CI.
        let snap = wait_for_status(&status, Duration::from_secs(5), |s| s.connected)
            .await
            .expect("expected eventual connected status after retry");
        assert!(snap.connected);
        assert!(snap.last_error_kind.is_none());

        task.abort();
        let _ = task.await;
        mock.stop().await;
    }
}
