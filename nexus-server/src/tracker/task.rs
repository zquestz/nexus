//! Per-tracker registration task: maintains one long-lived TLS connection
//! to a tracker and refreshes the registration on the tracker-supplied
//! interval.
//!
//! The task is spawned by [`super::manager::TrackerManager::spawn`] and
//! aborted via the `JoinHandle` it returns. The task is cancellation-
//! safe at every await point — drop semantics on the TLS stream and
//! the status `Arc<RwLock<TrackerStatus>>` handle cleanup.
//!
//! See `docs/TODO.md` § "Server-Side Tracker Registration Implementation Plan"
//! for the design rationale.

use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use chrono::Utc;
use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_server_message, read_tracker_server_message_with_full_timeout, send_client_message,
    send_tracker_client_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use nexus_common::{
    ERROR_KIND_TRACKER_ADDRESS_INVALID, ERROR_KIND_TRACKER_CONNECTION_FAILED,
    ERROR_KIND_TRACKER_CONNECTION_LOST, ERROR_KIND_TRACKER_DB_FAILED,
    ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED, ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
    ERROR_KIND_TRACKER_HANDSHAKE_FAILED, ERROR_KIND_TRACKER_PROTOCOL_ERROR,
    ERROR_KIND_TRACKER_TLS_FAILED, TRACKER_PROTOCOL_VERSION, is_unrecoverable_error_kind,
    is_valid_error_kind,
};
use rand::RngExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tracing::{debug, error, info, warn};

use super::context::TrackerContext;
use super::status::TrackerStatus;
use super::tls::TLS_CONNECTOR;
use crate::constants::{
    EXPECT_TRACKER_STATUS_LOCK_POISONED, LOG_TRACKER_REGISTRATION_BACKOFF,
    LOG_TRACKER_REGISTRATION_BUILD_PAYLOAD_FAILED,
    LOG_TRACKER_REGISTRATION_CLOSED_AWAITING_RESPONSE, LOG_TRACKER_REGISTRATION_CLOSED_MID_IDLE,
    LOG_TRACKER_REGISTRATION_CONNECTED, LOG_TRACKER_REGISTRATION_EXITING,
    LOG_TRACKER_REGISTRATION_HANDSHAKE_CLOSED, LOG_TRACKER_REGISTRATION_HANDSHAKE_REJECTED,
    LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR,
    LOG_TRACKER_REGISTRATION_HANDSHAKE_UNEXPECTED, LOG_TRACKER_REGISTRATION_INVALID_ERROR_KIND,
    LOG_TRACKER_REGISTRATION_INVALID_HOST, LOG_TRACKER_REGISTRATION_NO_PEER_CERTS,
    LOG_TRACKER_REGISTRATION_READ_ERROR_MID_IDLE, LOG_TRACKER_REGISTRATION_REFRESHED,
    LOG_TRACKER_REGISTRATION_REGISTER_REJECTED, LOG_TRACKER_REGISTRATION_RESPONSE_READ_ERROR,
    LOG_TRACKER_REGISTRATION_RESPONSE_TIMEOUT, LOG_TRACKER_REGISTRATION_SEND_HANDSHAKE_FAILED,
    LOG_TRACKER_REGISTRATION_SEND_REGISTER_FAILED, LOG_TRACKER_REGISTRATION_STAGE1_MISMATCH,
    LOG_TRACKER_REGISTRATION_STAGE2_MALFORMED, LOG_TRACKER_REGISTRATION_STAGE2_MISMATCH,
    LOG_TRACKER_REGISTRATION_TCP_FAILED, LOG_TRACKER_REGISTRATION_TLS_FAILED,
    LOG_TRACKER_REGISTRATION_TOFU_PINNED, LOG_TRACKER_REGISTRATION_TOFU_WRITE_FAILED,
    LOG_TRACKER_REGISTRATION_TRACKER_REPORTED_ERROR, LOG_TRACKER_REGISTRATION_UNEXPECTED_FRAME,
    LOG_TRACKER_REGISTRATION_WRONG_FLOW_RESPONSE, TRACKER_FINGERPRINT_MALFORMED_SENTINEL,
};
use crate::db::{TrackerRecord, is_transient_db_error};

/// Base backoff between connection attempts on failure.
///
/// In tests this collapses to 100ms so retry-then-success scenarios
/// run in well under the test's outer timeout, and existing tests
/// don't risk overlap between the tracker task's backoff deadline and
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

/// Floor for the tracker-supplied `refresh_interval`. Defends against
/// a buggy or hostile tracker that asks for `0` or any value below the
/// protocol's stated minimum, which would otherwise drive a tight
/// refresh loop and burn CPU.
///
/// In tests this collapses to 1s so propagation tests can observe
/// real refresh cycles without 2-minute waits. The shared
/// `nexus_common::MIN_REFRESH_INTERVAL_SECS` constant (120) is the
/// production floor and matches the tracker's CLI-side minimum.
#[cfg(not(test))]
const MIN_REFRESH_INTERVAL_SECS: u32 = nexus_common::MIN_REFRESH_INTERVAL_SECS;
#[cfg(test)]
const MIN_REFRESH_INTERVAL_SECS: u32 = 1;

/// Padding added to the mid-idle read's idle timeout so the refresh
/// sleep always wins the `tokio::select!` on the happy path. The idle
/// timeout is purely a backstop — its real value is letting the read
/// arm fire promptly when the peer closes the connection (e.g. tracker
/// restart), so we reconnect in seconds instead of waiting out the
/// full refresh interval. 15s is gracious; loopback latency is
/// microseconds and even satellite RTT is well under 1s.
const IDLE_READ_PADDING: Duration = Duration::from_secs(15);

/// Run the tracker task for one tracker. Loops forever (with
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
    context: Arc<TrackerContext>,
) {
    let mut backoff = BACKOFF_BASE;

    loop {
        // Reset transient connection state at the start of each attempt
        // and stamp `last_attempted_at` so the admin UI can surface
        // forward progress even when every recent attempt has failed.
        // We don't touch `pending_fingerprint` here — it's set only on
        // Unrecoverable Stage-1/Stage-2 mismatch (which `return`s out
        // of the task) and cleared on successful TOFU commit, so a
        // mid-loop reset would never apply.
        {
            let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
            s.connected = false;
            s.last_attempted_at = Some(Utc::now().timestamp());
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
    context: &Arc<TrackerContext>,
) -> CycleOutcome {
    // Phase 0: resolve the admin-supplied address into the form both
    // the system resolver and rustls's `ServerName::try_from` expect
    // — strips IPv6 URL brackets, passes IP literals through, and
    // Punycode-encodes Unicode hostnames. The validator at
    // `TrackerAdd`/`TrackerUpdate` already accepts the same set
    // (via `domain_to_ascii_strict`), so a failure here means the row
    // is structurally broken — admin must edit. Treat as Unrecoverable
    // so the tracker task exits instead of tight-looping on a busted row.
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
            set_status_error(status, ERROR_KIND_TRACKER_ADDRESS_INVALID);
            return CycleOutcome::Unrecoverable;
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
            set_status_error(status, ERROR_KIND_TRACKER_ADDRESS_INVALID);
            return CycleOutcome::Unrecoverable;
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
        Ok(Err(HandshakeReadError::Io(e))) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient;
        }
        Ok(Err(HandshakeReadError::Closed)) => {
            warn!(
                id = record.id,
                name = %record.name,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_CLOSED
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient;
        }
        Ok(Err(HandshakeReadError::Rejected { error })) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %sanitize_for_log(&error.unwrap_or_default()),
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_REJECTED
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient;
        }
        Ok(Err(HandshakeReadError::Unexpected { received })) => {
            warn!(
                id = record.id,
                name = %record.name,
                received = received,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_UNEXPECTED
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

    // Phase 6: Stage 2 — TLS-observed vs server-reported. The
    // `server_reported` value is tracker-supplied: validate canonical
    // form first (defends against terminal-control vandalism in logs
    // and distinguishes "tracker is broken" from "tracker is being
    // intercepted"). All log fields routed through tracker-supplied
    // strings pass through `sanitize_for_log`.
    if !is_canonical_fingerprint(&server_reported) {
        warn!(
            id = record.id,
            name = %record.name,
            tls_observed = %tls_observed,
            server_reported = TRACKER_FINGERPRINT_MALFORMED_SENTINEL,
            "{}", LOG_TRACKER_REGISTRATION_STAGE2_MALFORMED
        );
        set_status_error(status, ERROR_KIND_TRACKER_PROTOCOL_ERROR);
        return CycleOutcome::Unrecoverable;
    }
    if server_reported != tls_observed {
        warn!(
            id = record.id,
            name = %record.name,
            tls_observed = %tls_observed,
            server_reported = %sanitize_for_log(&server_reported),
            "{}", LOG_TRACKER_REGISTRATION_STAGE2_MISMATCH
        );
        // Stage 2 mismatch (TLS-observed cert disagrees with the
        // tracker's self-reported fingerprint in HandshakeResponse) is
        // an active-interception signal. We deliberately do NOT write
        // the observed fingerprint into `pending_fingerprint`: that
        // field is consumed by `TrackerAcceptFingerprint` as a
        // one-click promote-to-active-pin, and offering that path here
        // would let an admin pin the attacker's cert. The TLS-observed
        // fingerprint is captured in the operator log line above.
        // Recovery requires admin investigation (Edit / Remove), not a
        // one-click accept.
        set_status_error(status, ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED);
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
    context: &Arc<TrackerContext>,
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
        // responses to our requests; any frame read here means the
        // connection is closing). The read arm is a fast-path for
        // tracker-restart detection: catching a clean peer-close
        // immediately lets us reconnect within seconds instead of
        // waiting out the full refresh interval. The idle timeout is
        // padded past `sleep_for` so the sleep arm always wins on the
        // happy path; it's only a backstop for the read.
        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {}
            frame = read_tracker_server_message_with_full_timeout(
                reader,
                Some(sleep_for + IDLE_READ_PADDING),
                None,
            ) => {
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
                // Floor the tracker-supplied interval at the protocol's
                // minimum. A buggy or hostile tracker that asks for
                // `Some(0)` or anything below the floor would otherwise
                // drive a tight refresh loop here. The reference tracker
                // enforces the same minimum on its CLI; this is
                // defense-in-depth for the BBS-client side.
                let interval = refresh_interval
                    .unwrap_or(300)
                    .max(MIN_REFRESH_INTERVAL_SECS);
                let was_connected = status
                    .read()
                    .expect(EXPECT_TRACKER_STATUS_LOCK_POISONED)
                    .connected;
                {
                    let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
                    s.connected = true;
                    s.last_connected_at = Some(Utc::now().timestamp());
                    s.last_error_kind = None;
                    s.refresh_interval = Some(interval);
                }
                // First successful refresh after task start (or after a
                // reconnect post-error) gets info-level so an operator
                // running at info sees per-tracker confirmation. Steady-
                // state refreshes stay at debug — at scale they would
                // otherwise dominate the log volume.
                if was_connected {
                    debug!(
                        id = record.id,
                        name = %record.name,
                        refresh_interval = interval,
                        "{}", LOG_TRACKER_REGISTRATION_REFRESHED
                    );
                } else {
                    info!(
                        id = record.id,
                        name = %record.name,
                        refresh_interval = interval,
                        "{}", LOG_TRACKER_REGISTRATION_CONNECTED
                    );
                }
                sleep_for = Duration::from_secs(u64::from(interval));
            }
            TrackerServerMessage::TrackerServerRegisterResponse {
                success: false,
                error,
                error_kind,
                ..
            } => {
                // Wire-format gate: a tracker-supplied `error_kind`
                // must be ASCII snake_case bounded by
                // `MAX_ERROR_KIND_LENGTH`. Anything else (control
                // chars, embedded JSON, oversized blob) is a protocol
                // violation — substitute our own
                // `tracker_protocol_error` kind so junk never lands
                // in the wire-visible `TrackerInfo.last_error_kind`,
                // and exit Unrecoverable since a tracker that can't
                // emit valid error kinds isn't going to start.
                if let Some(raw) = error_kind.as_deref()
                    && !is_valid_error_kind(raw)
                {
                    warn!(
                        id = record.id,
                        name = %record.name,
                        rejected_kind = %sanitize_for_log(raw),
                        "{}", LOG_TRACKER_REGISTRATION_INVALID_ERROR_KIND
                    );
                    set_status_error(status, ERROR_KIND_TRACKER_PROTOCOL_ERROR);
                    return CycleOutcome::Unrecoverable;
                }
                // Outcome decision happens against the *raw* tracker
                // input: only an explicit, recognized unrecoverable
                // kind kills the task. A missing or format-valid-but-
                // unknown kind (e.g. forward-compat from a newer
                // tracker) is treated as transient so a misbehaving
                // tracker can't permanently take us out without admin
                // intervention.
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
                    err = %sanitize_for_log(&detail),
                    "{}", LOG_TRACKER_REGISTRATION_REGISTER_REJECTED
                );
                set_status_error(status, &kind);
                return outcome;
            }
            // Tracker explicitly reported a protocol-level error (e.g.
            // role-violation, unknown message type). The tracker has
            // diagnosed something we did wrong, so retrying isn't
            // going to help. Exit Unrecoverable; admin must fix.
            TrackerServerMessage::Error { message, command } => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    command = %sanitize_for_log(&command.unwrap_or_default()),
                    err = %sanitize_for_log(&message),
                    "{}", LOG_TRACKER_REGISTRATION_TRACKER_REPORTED_ERROR
                );
                set_status_error(status, ERROR_KIND_TRACKER_PROTOCOL_ERROR);
                return CycleOutcome::Unrecoverable;
            }
            // Tracker sent a client-flow response on our server
            // connection — itself a role violation by the tracker.
            // Exit Unrecoverable for the same reason as above.
            TrackerServerMessage::TrackerServerListResponse { .. } => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    "{}", LOG_TRACKER_REGISTRATION_WRONG_FLOW_RESPONSE
                );
                set_status_error(status, ERROR_KIND_TRACKER_PROTOCOL_ERROR);
                return CycleOutcome::Unrecoverable;
            }
        }
    }
}

/// Build the `TrackerServerRegister` payload from the record + the
/// per-refresh field bundle (server name, description, public address,
/// guest enabled) plus the live user_count from `UserManager`.
async fn build_register_payload(
    record: &TrackerRecord,
    context: &Arc<TrackerContext>,
) -> Result<TrackerClientMessage, String> {
    // DO NOT CACHE this read — it is the propagation path for
    // `ServerInfoUpdate`. See `Database::tracker_registration_fields`.
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
        fingerprint: context.server_fingerprint.to_string(),
        user_count,
        allows_guest: fields.allows_guest,
    })
}

/// Failure modes for [`read_handshake_response`]. Each variant carries
/// only operator-log context — never raw `Debug` of arbitrary tracker
/// payloads, which would let a malicious tracker amplify into the log
/// stream. The caller maps each variant to a fixed log constant +
/// the same admin-facing kind (`tracker_handshake_failed`).
enum HandshakeReadError {
    /// Frame/IO read failed. Carries the underlying error display.
    Io(String),
    /// Tracker closed the connection cleanly without sending a
    /// `HandshakeResponse`.
    Closed,
    /// Tracker replied `HandshakeResponse { success: false }`. The
    /// optional `error` is the tracker-supplied (length-bounded by
    /// frame limits, English-localized by the tracker) text.
    Rejected { error: Option<String> },
    /// Tracker sent a different message type than `HandshakeResponse`.
    /// Carries only the bounded message-type name — never the payload.
    Unexpected { received: &'static str },
}

/// Read the tracker's `HandshakeResponse` and return its
/// server-reported fingerprint. Errors are typed; the caller picks the
/// log constant per variant so operator logs use fixed strings rather
/// than `format!`-built English with unbounded `Debug` content.
async fn read_handshake_response<R>(
    reader: &mut FrameReader<R>,
) -> Result<String, HandshakeReadError>
where
    R: AsyncReadExt + Unpin,
{
    let received = read_server_message(reader)
        .await
        .map_err(|e| HandshakeReadError::Io(e.to_string()))?
        .ok_or(HandshakeReadError::Closed)?;

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
        } => Err(HandshakeReadError::Rejected { error }),
        other => Err(HandshakeReadError::Unexpected {
            received: nexus_common::io::server_message_type(&other),
        }),
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

// =============================================================================
// Status update helpers
// =============================================================================
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

// =============================================================================
// Log-sanitization helper
// =============================================================================

/// Replace ASCII control characters with `?` so a tracker-supplied
/// string can be safely embedded in operator logs without leaking
/// terminal escape sequences (color, cursor moves, line clears) or
/// other display vandalism. The tracker is in our trust boundary, but
/// a compromised tracker the admin already added shouldn't be able to
/// muck with operator pagers reading server logs.
///
/// Returns `Cow::Borrowed(s)` on the common path where no substitution
/// is needed (no control characters present); only the malicious /
/// vandalism path allocates. `Cow<str>: Display`, so `%`-formatted
/// tracing fields write the borrowed slice directly without ever
/// materializing a `String`.
fn sanitize_for_log(s: &str) -> std::borrow::Cow<'_, str> {
    if s.chars().any(|c| c.is_control()) {
        std::borrow::Cow::Owned(
            s.chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

// =============================================================================
// Backoff helpers
// =============================================================================

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
    fn sanitize_replaces_control_characters() {
        assert_eq!(sanitize_for_log("hello"), "hello");
        // ANSI escape: \x1b[31m (red foreground)
        assert_eq!(
            sanitize_for_log("ok\x1b[31mlying\x1b[0m"),
            "ok?[31mlying?[0m"
        );
        // Newlines and tabs
        assert_eq!(sanitize_for_log("line1\nline2\tend"), "line1?line2?end");
        // Null byte
        assert_eq!(sanitize_for_log("a\0b"), "a?b");
        // Bell, backspace
        assert_eq!(sanitize_for_log("\x07\x08"), "??");
        // Unicode passes through
        assert_eq!(sanitize_for_log("héllo 🌍"), "héllo 🌍");
    }

    #[test]
    fn sanitize_borrows_on_common_path() {
        // The common path (no control chars) must not allocate. Lock in
        // the invariant so a future regression to "always allocate"
        // would fail this test.
        use std::borrow::Cow;
        assert!(matches!(sanitize_for_log("normal text"), Cow::Borrowed(_)));
        assert!(matches!(sanitize_for_log(""), Cow::Borrowed(_)));
        assert!(matches!(sanitize_for_log("héllo 🌍"), Cow::Borrowed(_)));
        // Control char triggers the owned path.
        assert!(matches!(sanitize_for_log("a\nb"), Cow::Owned(_)));
    }

    #[test]
    fn backoff_doubling_caps_at_max() {
        let mut backoff = BACKOFF_BASE;
        for _ in 0..20 {
            backoff = (backoff * 2).min(BACKOFF_CAP);
        }
        assert_eq!(backoff, BACKOFF_CAP);
    }

    /// Build a `TrackerContext` over a fresh in-memory DB. Sets a
    /// distinct `server_name` and `public_address` so payload tests can
    /// verify the right values flow through.
    async fn setup_context() -> (Database, Arc<TrackerContext>) {
        let pool = create_test_db().await;
        let db = Database::new(pool);
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
        let user_manager = UserManager::new();
        let context = Arc::new(TrackerContext {
            db: db.clone(),
            user_manager,
            server_fingerprint: TEST_FINGERPRINT,
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

        // Seed with a wrong pin so Stage 1 fails when the tracker task
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

        // Stage 2 must NOT populate pending_fingerprint — that field
        // gates the one-click `TrackerAcceptFingerprint` flow, and
        // accepting the TLS-observed cert here would let an admin pin
        // the attacker's certificate. Recovery requires Edit / Remove.
        assert!(snap.pending_fingerprint.is_none());
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
    async fn malformed_error_kind_is_substituted_and_unrecoverable() {
        // A hostile / buggy tracker ships an `error_kind` that fails
        // wire-format validation (uppercase + control chars + newline).
        // The tracker task must:
        //   1. NOT store the raw value in `last_error_kind` (which is
        //      a wire-visible field).
        //   2. Substitute `tracker_protocol_error` so the admin UI
        //      gets a clean kind to translate.
        //   3. Exit the task — a tracker that ships malformed kinds
        //      isn't going to start working.
        let raw_kind = "BAD\nKIND\0<script>";
        let mock = MockTracker::start(MockBehavior {
            register_response: RegisterPolicy::Failure {
                error_kind: raw_kind.to_string(),
                error: "broken tracker".to_string(),
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

        let exited = tokio::time::timeout(Duration::from_secs(5), task).await;
        assert!(exited.is_ok(), "task should exit on malformed kind");

        let snap = status.read().expect("status lock").clone();
        assert_eq!(
            snap.last_error_kind.as_deref(),
            Some(nexus_common::ERROR_KIND_TRACKER_PROTOCOL_ERROR),
            "raw kind must be replaced with the protocol-error sentinel"
        );
        assert!(!snap.connected);

        mock.stop().await;
    }

    #[tokio::test]
    async fn rate_limited_then_succeeds() {
        // First connection: tracker rejects with `rate_limited` (a
        // transient kind). Tracker task should backoff (~5s, jittered)
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

    #[tokio::test]
    async fn floor_clamps_low_refresh_interval() {
        // A buggy / hostile tracker asks for `Some(0)` — the publisher
        // must clamp to `MIN_REFRESH_INTERVAL_SECS` (1 in tests, 120 in
        // production) rather than hot-looping at 0s sleep.
        let mock = MockTracker::start(MockBehavior {
            register_response: RegisterPolicy::Success {
                refresh_interval: 0,
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

        let snap = wait_for_status(&status, Duration::from_secs(5), |s| s.connected)
            .await
            .expect("expected connected status");
        assert_eq!(
            snap.refresh_interval,
            Some(MIN_REFRESH_INTERVAL_SECS),
            "received 0 should be clamped to MIN_REFRESH_INTERVAL_SECS"
        );

        task.abort();
        let _ = task.await;
        mock.stop().await;
    }

    #[tokio::test]
    async fn server_info_changes_propagate_to_next_refresh() {
        // Verifies the propagation contract documented on
        // `Database::tracker_registration_fields`: every refresh reads
        // fresh values from the DB, so an admin's `ServerInfoUpdate`
        // (or guest-account toggle) reaches the tracker on the next
        // cycle without any explicit signal. Catches future regressions
        // that "optimize" by caching across refreshes.
        let mock = MockTracker::start(MockBehavior {
            register_response: RegisterPolicy::Success {
                refresh_interval: 1,
            },
            ..Default::default()
        })
        .await;
        let captured = Arc::clone(&mock.behavior.captured_registers);
        let (db, context) = setup_context().await;
        let record = seed_tracker(&db, mock.addr, None, None).await;

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task = tokio::spawn(run(
            record.clone(),
            Arc::clone(&status),
            Arc::clone(&context),
        ));

        // Wait for register #1 (with the original setup_context values).
        wait_for_capture_count(&captured, 1, Duration::from_secs(5))
            .await
            .expect("register #1 should arrive within 5s");
        {
            let registers = captured.lock().expect("capture lock");
            match &registers[0] {
                TrackerClientMessage::TrackerServerRegister {
                    name,
                    description,
                    address,
                    allows_guest,
                    ..
                } => {
                    assert_eq!(name, "Test BBS");
                    assert_eq!(description.as_deref(), Some("a test server"));
                    assert_eq!(address.as_deref(), Some("bbs.example.com"));
                    assert!(!allows_guest, "guest should be disabled by default");
                }
                other => panic!("expected TrackerServerRegister, got {other:?}"),
            }
        }

        // Mutate all four propagating fields.
        db.config
            .set_server_name("Renamed BBS")
            .await
            .expect("set server name");
        db.config
            .set_server_description("renamed description")
            .await
            .expect("set description");
        db.config
            .set_public_address("renamed.example.com")
            .await
            .expect("set public address");
        db.users
            .update_user(crate::db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
            })
            .await
            .expect("enable guest");

        // Wait for a register that arrived AFTER our mutations. The
        // refresh fires every 1s, so the next register reflects the
        // post-mutation DB state. We need at least 2 captures total.
        wait_for_capture_count(&captured, 2, Duration::from_secs(5))
            .await
            .expect("register #2 should arrive within 5s of refresh");

        // Inspect the most recent capture: every field should reflect
        // the updated DB values.
        {
            let registers = captured.lock().expect("capture lock");
            let last = registers.last().expect("at least one capture");
            match last {
                TrackerClientMessage::TrackerServerRegister {
                    name,
                    description,
                    address,
                    allows_guest,
                    ..
                } => {
                    assert_eq!(name, "Renamed BBS", "server name should propagate");
                    assert_eq!(
                        description.as_deref(),
                        Some("renamed description"),
                        "description should propagate"
                    );
                    assert_eq!(
                        address.as_deref(),
                        Some("renamed.example.com"),
                        "public address should propagate"
                    );
                    assert!(allows_guest, "guest enable should propagate");
                }
                other => panic!("expected TrackerServerRegister, got {other:?}"),
            }
        }

        task.abort();
        let _ = task.await;
        mock.stop().await;
    }

    /// Wait until `captured` contains at least `target` entries, or
    /// the timeout elapses. Used by the propagation test to wait for
    /// refresh-cycle captures without sleeping for fixed durations.
    async fn wait_for_capture_count(
        captured: &Arc<Mutex<Vec<TrackerClientMessage>>>,
        target: usize,
        timeout: Duration,
    ) -> Option<()> {
        tokio::time::timeout(timeout, async {
            loop {
                if captured.lock().expect("capture lock").len() >= target {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .ok()
    }
}
