//! Per-tracker registration task: maintains one long-lived TLS connection
//! to a tracker and refreshes the registration on the tracker-supplied
//! interval.
//!
//! The task is spawned by [`super::manager::TrackerManager::spawn`] and
//! aborted via the `JoinHandle` it returns. The task is cancellation-
//! safe at every await point — drop semantics on the TLS stream and
//! the status `Arc<RwLock<TrackerStatus>>` handle cleanup.

use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use chrono::Utc;
use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_server_handshake_response, read_tracker_server_message_with_full_timeout,
    send_client_message, send_tracker_client_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use nexus_common::{
    ERROR_KIND_TRACKER_ADDRESS_INVALID, ERROR_KIND_TRACKER_CONNECTION_FAILED,
    ERROR_KIND_TRACKER_CONNECTION_LOST, ERROR_KIND_TRACKER_DB_FAILED,
    ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED, ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
    ERROR_KIND_TRACKER_HANDSHAKE_FAILED, ERROR_KIND_TRACKER_PROTOCOL_ERROR,
    ERROR_KIND_TRACKER_TLS_FAILED, EXPECT_SNI_SERVER_NAME_VALID_DNS, SNI_SERVER_NAME,
    TRACKER_PROTOCOL_VERSION, is_unrecoverable_error_kind, is_valid_error_kind,
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
    LOG_TRACKER_REGISTRATION_CONNECTED, LOG_TRACKER_REGISTRATION_DNS_FAILED,
    LOG_TRACKER_REGISTRATION_DNS_NO_RECORDS, LOG_TRACKER_REGISTRATION_DNS_TIMEOUT,
    LOG_TRACKER_REGISTRATION_EXITING, LOG_TRACKER_REGISTRATION_HANDSHAKE_CLOSED,
    LOG_TRACKER_REGISTRATION_HANDSHAKE_REJECTED, LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR,
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

/// Base backoff between connection attempts. In tests, 100ms so retry
/// scenarios finish well under the test timeout.
#[cfg(not(test))]
const BACKOFF_BASE: Duration = Duration::from_secs(5);
#[cfg(test)]
const BACKOFF_BASE: Duration = Duration::from_millis(100);
/// Cap on backoff growth — the slowest retry cadence.
const BACKOFF_CAP: Duration = Duration::from_secs(300);
/// Per-attempt jitter spread (±25%).
const BACKOFF_JITTER_PCT: f64 = 0.25;

/// Read timeout for tracker responses (`HandshakeResponse` and
/// `TrackerServerRegisterResponse`). Recovers from a wedged connection
/// instead of hanging until the outer 60s frame-completion timeout.
const TRACKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for connection establishment phases only: TCP connect and
/// TLS handshake. The long-lived registration refresh loop is governed
/// by the tracker-provided refresh interval, not this deadline.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Ceiling on a single `lookup_host`. Defends against a wedged/hostile
/// resolver hanging the task (the frame-completion timeout is pre-frame here).
#[cfg(not(test))]
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(1);

/// Floor for the tracker-supplied `refresh_interval`, against a buggy
/// or hostile tracker asking for `0` and driving a tight refresh loop.
/// In tests, 1s so propagation tests observe real cycles without long
/// waits; production floor is the shared `MIN_REFRESH_INTERVAL_SECS` (120).
#[cfg(not(test))]
const MIN_REFRESH_INTERVAL_SECS: u32 = nexus_common::MIN_REFRESH_INTERVAL_SECS;
#[cfg(test)]
const MIN_REFRESH_INTERVAL_SECS: u32 = 1;
const DEFAULT_REFRESH_INTERVAL_SECS: u32 = nexus_common::DEFAULT_REFRESH_INTERVAL_SECS;

/// Padding so the refresh sleep wins the `select!` on the happy path.
/// The idle timeout is a backstop: it fires the read arm promptly on
/// peer close (tracker restart) so we reconnect in seconds, not a full interval.
const IDLE_READ_PADDING: Duration = Duration::from_secs(15);

/// Test-only tracker task entrypoint without lifecycle serialization.
///
/// Production tasks are spawned through [`run_with_lifecycle_lock`] so TOFU
/// fingerprint writes serialize with admin lifecycle mutations.
#[cfg(test)]
pub async fn run(
    mut record: TrackerRecord,
    status: Arc<RwLock<TrackerStatus>>,
    context: Arc<TrackerContext>,
) {
    run_inner(&mut record, status, context, None).await;
}

/// Run the tracker task for one tracker. Loops with backoff until the
/// manager cancels it, or exits early on an unrecoverable error.
///
/// The task owns:
/// - `record`: config snapshot at spawn time. Admin row edits abort
///   and respawn the task — no in-place reconfiguration.
/// - `status`: shared with the manager for admin reads.
/// - `context`: shared infrastructure (DB, UserManager, server fingerprint, ports).
/// - `lifecycle`: shared with `TrackerManager` so task-side TOFU writes
///   serialize with admin DB mutation + spawn/replace/terminate sections.
pub(crate) async fn run_with_lifecycle_lock(
    mut record: TrackerRecord,
    status: Arc<RwLock<TrackerStatus>>,
    context: Arc<TrackerContext>,
    lifecycle: Arc<tokio::sync::Mutex<()>>,
) {
    run_inner(&mut record, status, context, Some(lifecycle)).await;
}

async fn run_inner(
    record: &mut TrackerRecord,
    status: Arc<RwLock<TrackerStatus>>,
    context: Arc<TrackerContext>,
    lifecycle: Option<Arc<tokio::sync::Mutex<()>>>,
) {
    let mut backoff = BACKOFF_BASE;

    loop {
        // Reset transient state and stamp `last_attempted_at` so the
        // admin UI shows forward progress even when attempts keep
        // failing. `pending_fingerprint` is left alone — it's set only
        // on Unrecoverable mismatch (which `return`s) and cleared on
        // TOFU commit, so a mid-loop reset would never apply.
        {
            let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
            s.connected = false;
            s.last_attempted_at = Some(Utc::now().timestamp());
        }

        match attempt_connection_cycle(record, &status, &context, lifecycle.as_ref()).await {
            CycleOutcome::Transient { connected } => {
                // Already logged + status-updated. Backoff and retry.
                reset_backoff_after_transient_cycle(&mut backoff, connected);
            }
            CycleOutcome::Unrecoverable => {
                // Already logged + status-updated. Exit; recovery needs
                // admin intervention (TrackerUpdate or server restart).
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
    Transient {
        /// Whether this cycle completed at least one successful tracker
        /// registration before the transient failure.
        connected: bool,
    },
    /// Permanent error (fingerprint mismatch, wrong password, malformed
    /// config). Exit the task; admin needs to fix the row.
    Unrecoverable,
}

/// Run one connection cycle: TCP → TLS → fingerprint stages →
/// tracker handshake → refresh loop.
async fn attempt_connection_cycle(
    record: &mut TrackerRecord,
    status: &Arc<RwLock<TrackerStatus>>,
    context: &Arc<TrackerContext>,
    lifecycle: Option<&Arc<tokio::sync::Mutex<()>>>,
) -> CycleOutcome {
    // Phase 0: normalize the address for the resolver and rustls
    // (strip IPv6 brackets, pass IP literals, Punycode Unicode hosts).
    // The Add/Update validator accepts the same set, so failure here
    // means a structurally broken row → Unrecoverable, not a tight loop.
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

    // Phase 1a: DNS resolution. Resolver failures are transient: at boot
    // the resolver/network may not be ready yet, and a typo costs only the
    // capped retry cadence. Structurally invalid rows fail above.
    let addrs: Vec<std::net::SocketAddr> = match tokio::time::timeout(
        DNS_LOOKUP_TIMEOUT,
        tokio::net::lookup_host((resolved_host.as_str(), record.port)),
    )
    .await
    {
        Ok(Ok(it)) => it.collect(),
        Ok(Err(e)) => {
            warn!(
                id = record.id,
                name = %record.name,
                address = %record.address,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_DNS_FAILED
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
        Err(_) => {
            warn!(
                id = record.id,
                name = %record.name,
                address = %record.address,
                timeout_secs = DNS_LOOKUP_TIMEOUT.as_secs(),
                "{}", LOG_TRACKER_REGISTRATION_DNS_TIMEOUT
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
    };
    if addrs.is_empty() {
        warn!(
            id = record.id,
            name = %record.name,
            address = %record.address,
            "{}", LOG_TRACKER_REGISTRATION_DNS_NO_RECORDS
        );
        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
        return CycleOutcome::Transient { connected: false };
    }

    // Phase 1b: TCP connect to a resolved address. Failure is
    // `Transient` (briefly unreachable); next cycle retries with backoff.
    let tcp = match tokio::time::timeout(CONNECTION_TIMEOUT, TcpStream::connect(&addrs[..])).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %e,
                "{}", LOG_TRACKER_REGISTRATION_TCP_FAILED
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
        Err(_) => {
            warn!(
                id = record.id,
                name = %record.name,
                timeout_secs = CONNECTION_TIMEOUT.as_secs(),
                "{}", LOG_TRACKER_REGISTRATION_TCP_FAILED
            );
            set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
    };

    // Phase 2: TLS handshake. SNI is disabled and we TOFU-pin the cert
    // via `AcceptAnyVerifier`, so `ServerName` is internal only — never
    // on the wire, never compared. Use the BBS client's literal convention.
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);
    let tls =
        match tokio::time::timeout(CONNECTION_TIMEOUT, TLS_CONNECTOR.connect(server_name, tcp))
            .await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    err = %e,
                    "{}", LOG_TRACKER_REGISTRATION_TLS_FAILED
                );
                set_status_error(status, ERROR_KIND_TRACKER_TLS_FAILED);
                return CycleOutcome::Transient { connected: false };
            }
            Err(_) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    timeout_secs = CONNECTION_TIMEOUT.as_secs(),
                    "{}", LOG_TRACKER_REGISTRATION_TLS_FAILED
                );
                set_status_error(status, ERROR_KIND_TRACKER_TLS_FAILED);
                return CycleOutcome::Transient { connected: false };
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
            return CycleOutcome::Transient { connected: false };
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

    // Phase 5: tracker handshake. The tracker protocol reuses the
    // BBS-protocol Handshake/HandshakeResponse at the handshake layer.
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
        return CycleOutcome::Transient { connected: false };
    }

    // Wrap the read in a deadline so a wedged tracker that completes
    // TLS but never replies can't park us in `await` forever.
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
            return CycleOutcome::Transient { connected: false };
        }
        Ok(Err(HandshakeReadError::Closed)) => {
            warn!(
                id = record.id,
                name = %record.name,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_CLOSED
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
        Ok(Err(HandshakeReadError::Rejected { error })) => {
            warn!(
                id = record.id,
                name = %record.name,
                err = %sanitize_for_log(&error.unwrap_or_default()),
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_REJECTED
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
        Ok(Err(HandshakeReadError::Unexpected { received })) => {
            warn!(
                id = record.id,
                name = %record.name,
                received = received,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_UNEXPECTED
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
        Err(_elapsed) => {
            warn!(
                id = record.id,
                name = %record.name,
                "{}", LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR
            );
            set_status_error(status, ERROR_KIND_TRACKER_HANDSHAKE_FAILED);
            return CycleOutcome::Transient { connected: false };
        }
    };

    // Phase 6: Stage 2 — TLS-observed vs server-reported. Validate
    // canonical form first: defends logs against control-char vandalism
    // and distinguishes "tracker broken" from "tracker intercepted".
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
        // Stage 2 mismatch is an active-interception signal. Do NOT
        // write `pending_fingerprint`: it feeds `TrackerAcceptFingerprint`
        // as a one-click promote-to-pin, which would let an admin pin
        // the attacker's cert. Recovery requires Edit / Remove.
        set_status_error(status, ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED);
        return CycleOutcome::Unrecoverable;
    }

    // Phase 7: TOFU commit. Both stages passed; if no pin was stored,
    // this is the first connect — persist the trusted fingerprint and
    // update the local record so later iterations see it.
    if record.fingerprint.is_none() {
        let _lifecycle_guard = match lifecycle {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };
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
                return CycleOutcome::Transient { connected: false };
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

    // Clear pending_fingerprint now both stages agree (covers the
    // accept-then-reconnect path where a prior task left one in status).
    {
        let mut s = status.write().expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
        s.pending_fingerprint = None;
    }

    // Phase 8: register / refresh loop.
    refresh_loop(record, status, context, &mut reader, &mut writer).await
}

/// The inner refresh loop. Sends `TrackerServerRegister` immediately,
/// then each refresh interval. `select!`s sleep against read so a
/// connection drop mid-sleep is detected promptly.
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
    let mut connected_once = false;

    loop {
        // Wait to refresh, OR for any inbound frame — unexpected, since
        // the tracker only replies to requests, so a frame here means
        // the connection is closing. The read arm fast-paths
        // tracker-restart detection (reconnect in seconds, not a full
        // interval). The idle timeout is padded past `sleep_for` so the
        // sleep arm wins the happy path; it's only a backstop.
        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {}
            frame = read_tracker_server_message_with_full_timeout(
                reader,
                Some(sleep_for + IDLE_READ_PADDING),
                None,
            ) => {
                match frame {
                    Ok(Some(_)) => {
                        // Unexpected mid-idle frame; reconnect.
                        warn!(
                            id = record.id,
                            name = %record.name,
                            "{}", LOG_TRACKER_REGISTRATION_UNEXPECTED_FRAME
                        );
                        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                        return CycleOutcome::Transient {
                            connected: connected_once,
                        };
                    }
                    Ok(None) => {
                        debug!(
                            id = record.id,
                            name = %record.name,
                            "{}", LOG_TRACKER_REGISTRATION_CLOSED_MID_IDLE
                        );
                        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                        return CycleOutcome::Transient {
                            connected: connected_once,
                        };
                    }
                    Err(e) => {
                        warn!(
                            id = record.id,
                            name = %record.name,
                            err = %e,
                            "{}", LOG_TRACKER_REGISTRATION_READ_ERROR_MID_IDLE
                        );
                        set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                        return CycleOutcome::Transient {
                            connected: connected_once,
                        };
                    }
                }
            }
        }

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
                return CycleOutcome::Transient {
                    connected: connected_once,
                };
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
            return CycleOutcome::Transient {
                connected: connected_once,
            };
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
                return CycleOutcome::Transient {
                    connected: connected_once,
                };
            }
            Ok(Err(e)) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    err = %e,
                    "{}", LOG_TRACKER_REGISTRATION_RESPONSE_READ_ERROR
                );
                set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                return CycleOutcome::Transient {
                    connected: connected_once,
                };
            }
            Err(_elapsed) => {
                warn!(
                    id = record.id,
                    name = %record.name,
                    "{}", LOG_TRACKER_REGISTRATION_RESPONSE_TIMEOUT
                );
                set_status_error(status, ERROR_KIND_TRACKER_CONNECTION_LOST);
                return CycleOutcome::Transient {
                    connected: connected_once,
                };
            }
        };

        match response {
            TrackerServerMessage::TrackerServerRegisterResponse {
                success: true,
                refresh_interval,
                ..
            } => {
                // Floor the interval at the protocol minimum so a buggy
                // or hostile `Some(0)` can't drive a tight refresh loop.
                // Defense-in-depth: the tracker enforces the same on its CLI.
                let interval = refresh_interval
                    .unwrap_or(DEFAULT_REFRESH_INTERVAL_SECS)
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
                connected_once = true;
                // First refresh after start/reconnect logs at info so an
                // operator sees per-tracker confirmation; steady-state
                // refreshes stay at debug to keep log volume down.
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
                // Wire-format gate: `error_kind` must be ASCII
                // snake_case within `MAX_ERROR_KIND_LENGTH`. Anything
                // else is a protocol violation — substitute
                // `tracker_protocol_error` so junk never reaches the
                // wire-visible `last_error_kind`, and exit Unrecoverable
                // (a tracker that can't emit valid kinds won't start).
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
                // Decide against the *raw* input: only a recognized
                // unrecoverable kind kills the task. Missing or
                // unknown-but-valid kinds (forward-compat) stay
                // transient so a tracker can't permanently take us out.
                let outcome = match error_kind.as_deref() {
                    Some(k) if is_unrecoverable_error_kind(k) => CycleOutcome::Unrecoverable,
                    _ => CycleOutcome::Transient {
                        connected: connected_once,
                    },
                };
                // When the tracker omits a kind, default to "connection
                // lost" (the wire ate the response) rather than
                // "invalid", which would imply our payload was rejected.
                let kind =
                    error_kind.unwrap_or_else(|| ERROR_KIND_TRACKER_CONNECTION_LOST.to_string());
                // Tracker-supplied `error` text (localized to the "en"
                // we send) is operator-log only — the admin UI renders
                // the kind via the BBS server's own i18n in their locale.
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
            // Tracker reported a protocol-level error (role violation,
            // unknown message type). It diagnosed something we did
            // wrong, so retrying won't help — exit Unrecoverable.
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
            // Client-flow response on our server connection — a tracker
            // role violation. Exit Unrecoverable as above.
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

/// Build the `TrackerServerRegister` payload from the record, the
/// per-refresh field bundle (name, description, public address, guest
/// enabled), and the live user_count from `UserManager`.
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
/// only operator-log context — never raw `Debug` of tracker payloads
/// (log amplification). The caller maps each to a fixed log constant +
/// the same admin-facing kind (`tracker_handshake_failed`).
enum HandshakeReadError {
    /// Frame/IO read failed. Carries the underlying error display.
    Io(String),
    /// Tracker closed cleanly without sending a `HandshakeResponse`.
    Closed,
    /// Tracker replied `HandshakeResponse { success: false }`; carries
    /// the optional tracker-supplied error text.
    Rejected { error: Option<String> },
    /// Tracker sent a different message type. Carries only the bounded
    /// message-type name — never the payload.
    Unexpected { received: &'static str },
}

/// Read the tracker's `HandshakeResponse` and return its
/// server-reported fingerprint. Errors are typed so the caller picks a
/// fixed log constant per variant, not `format!`-built unbounded content.
async fn read_handshake_response<R>(
    reader: &mut FrameReader<R>,
) -> Result<String, HandshakeReadError>
where
    R: AsyncReadExt + Unpin,
{
    let received = read_server_handshake_response(reader)
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

// These set only the machine-readable `last_error_kind`. The admin-UI
// message is translated at handler compose-time in the admin's locale;
// the raw error flows to operator logs separately at the call site.

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

/// Replace ASCII control characters with `?` so a tracker-supplied
/// string can't leak terminal escape sequences into operator logs.
/// A compromised tracker the admin added shouldn't muck with pagers
/// reading server logs.
///
/// Borrows on the common (no-control-char) path; only the vandalism
/// path allocates. `Cow<str>: Display` so tracing writes it directly.
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

/// Apply ±25% jitter to a backoff duration.
fn jitter(base: Duration) -> Duration {
    let mut rng = rand::rng();
    let factor = 1.0 + rng.random_range(-BACKOFF_JITTER_PCT..=BACKOFF_JITTER_PCT);
    let millis = base.as_millis() as f64 * factor;
    Duration::from_millis(millis.max(0.0) as u64)
}

fn reset_backoff_after_transient_cycle(backoff: &mut Duration, connected: bool) {
    if connected {
        *backoff = BACKOFF_BASE;
    }
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
        // Sample many to catch a misimplemented range (not a
        // statistical guarantee).
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
        // The common path (no control chars) must not allocate; locks
        // in the invariant against a "always allocate" regression.
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

    #[test]
    fn connected_transient_resets_backoff_to_base() {
        let mut backoff = BACKOFF_CAP;

        reset_backoff_after_transient_cycle(&mut backoff, false);
        assert_eq!(backoff, BACKOFF_CAP);

        reset_backoff_after_transient_cycle(&mut backoff, true);
        assert_eq!(backoff, BACKOFF_BASE);
    }

    /// Build a `TrackerContext` over a fresh in-memory DB with distinct
    /// `server_name` / `public_address` so payload tests can verify flow.
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

    /// Poll the status until `pred` is true or `timeout` of *tokio
    /// time* elapses. Returns the matching snapshot, else `None`.
    ///
    /// Uses `tokio::time::timeout` (not `Instant`) so the deadline
    /// respects `start_paused = true` — both the poll-sleep and the
    /// timeout advance virtual time together.
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

        // Wrong pin so Stage 1 fails against the mock's actual cert.
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
        // Stage 2: TLS-observed cert disagrees with the tracker's
        // self-report. The only interception signal — an attacker may
        // hold an acceptable cert but can't forge the real self-report.
        let lying_fingerprint = "11:22:33:44:55:66:77:88:99:00:AA:BB:CC:DD:EE:FF:\
            11:22:33:44:55:66:77:88:99:00:AA:BB:CC:DD:EE:FF";
        let mock = MockTracker::start(MockBehavior {
            reported_fingerprint: Some(lying_fingerprint.to_string()),
            ..Default::default()
        })
        .await;
        let (db, context) = setup_context().await;
        // No pin → Stage 1 skipped; Stage 2 is what fires.
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

        // Stage 2 must NOT populate pending_fingerprint — it gates the
        // one-click accept flow, which here would pin the attacker's
        // cert. Recovery requires Edit / Remove.
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
        // A tracker ships an `error_kind` that fails wire-format
        // validation. The task must: (1) not store the raw value in
        // wire-visible `last_error_kind`, (2) substitute
        // `tracker_protocol_error`, (3) exit (it won't start working).
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
        // First connection rejected with `rate_limited` (transient);
        // task should backoff and reconnect, second connection accepts.
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

        // Test-mode `BACKOFF_BASE` is 100ms, so the retry completes
        // well within the 5s timeout even on slow CI.
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
    async fn successful_registration_then_disconnect_reports_connected_transient() {
        let mock = MockTracker::start(MockBehavior {
            register_response: RegisterPolicy::Success {
                refresh_interval: 1,
            },
            ..Default::default()
        })
        .await;
        let addr = mock.addr;
        let fingerprint = mock.fingerprint.clone();
        let (db, context) = setup_context().await;
        let mut record = seed_tracker(&db, addr, Some(&fingerprint), None).await;

        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let status_for_stop = Arc::clone(&status);
        let stop_task = tokio::spawn(async move {
            wait_for_status(&status_for_stop, Duration::from_secs(5), |s| s.connected)
                .await
                .expect("expected connected status before stopping mock");
            mock.stop().await;
        });

        let outcome = tokio::time::timeout(
            Duration::from_secs(10),
            attempt_connection_cycle(&mut record, &status, &context, None),
        )
        .await
        .expect("cycle should return after mock tracker stops");

        stop_task.await.expect("stop task should complete");
        assert!(matches!(
            outcome,
            CycleOutcome::Transient { connected: true }
        ));
    }

    #[tokio::test]
    async fn tracker_restart_reconnects_before_refresh_interval() {
        let mut close_queue = VecDeque::new();
        close_queue.push_back(1);

        // The mock closes each connection immediately after the register
        // response while keeping its listener alive. That simulates a tracker
        // restart/drop during the task's long mid-idle sleep. The second
        // connection remains open, so the final connected status is stable.
        // The task should notice the clean close via the read arm and reconnect
        // on BACKOFF_BASE, not wait for the advertised 300s refresh interval.
        let mock = MockTracker::start(MockBehavior {
            register_response: RegisterPolicy::Success {
                refresh_interval: 300,
            },
            close_after_register_responses: Arc::new(Mutex::new(close_queue)),
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

        wait_for_capture_count(&captured, 2, Duration::from_secs(5))
            .await
            .expect("task should reconnect and re-register before the 300s refresh interval");
        wait_for_status(&status, Duration::from_secs(5), |s| {
            s.connected && s.last_error_kind.is_none()
        })
        .await
        .expect("task should mark the reconnect successful");

        task.abort();
        let _ = task.await;
        mock.stop().await;
    }

    #[tokio::test]
    async fn floor_clamps_low_refresh_interval() {
        // Tracker asks for `Some(0)` — must clamp to
        // `MIN_REFRESH_INTERVAL_SECS` rather than hot-loop at 0s sleep.
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
        // Propagation contract from `tracker_registration_fields`:
        // every refresh re-reads the DB, so an admin's `ServerInfoUpdate`
        // reaches the tracker next cycle with no explicit signal.
        // Catches regressions that "optimize" by caching across refreshes.
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
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: crate::db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .expect("enable guest");

        // Wait for a register after our mutations (refresh fires every
        // 1s, so the next one reflects post-mutation DB state).
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

    /// DNS lookup failures are transient even on the first attempt. A
    /// resolver/network boot race should not delist the tracker until an
    /// admin edit or server restart.
    #[tokio::test]
    async fn dns_failure_first_time_is_transient() {
        let (_db, context) = setup_context().await;
        let mut record = TrackerRecord {
            id: 1,
            // RFC 2606 reserves `.invalid` for guaranteed DNS failure.
            address: "tracker.dns-test.invalid".to_string(),
            port: 7510,
            fingerprint: None,
            password: None,
            name: "Invalid".to_string(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let status = Arc::new(RwLock::new(TrackerStatus::default()));

        let outcome = tokio::time::timeout(
            Duration::from_secs(10),
            attempt_connection_cycle(&mut record, &status, &context, None),
        )
        .await
        .expect("DNS lookup of an .invalid hostname must fail fast — the resolver hung");

        assert!(
            matches!(outcome, CycleOutcome::Transient { connected: false }),
            "first-ever DNS failure should be transient"
        );
        let snap = status.read().expect("status lock").clone();
        assert!(!snap.connected);
        assert_eq!(
            snap.last_error_kind.as_deref(),
            Some(ERROR_KIND_TRACKER_CONNECTION_FAILED),
            "DNS failure surfaces as connection-failed so the task retries"
        );
    }

    /// Repeated DNS failures remain transient.
    #[tokio::test]
    async fn repeated_dns_failure_is_transient() {
        let (_db, context) = setup_context().await;
        let mut record = TrackerRecord {
            id: 1,
            address: "tracker.dns-test.invalid".to_string(),
            port: 7510,
            fingerprint: None,
            password: None,
            name: "Invalid".to_string(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };
        let status = Arc::new(RwLock::new(TrackerStatus::default()));

        let first = tokio::time::timeout(
            Duration::from_secs(10),
            attempt_connection_cycle(&mut record, &status, &context, None),
        )
        .await
        .expect("first DNS lookup of an .invalid hostname must fail fast — the resolver hung");
        assert!(
            matches!(first, CycleOutcome::Transient { connected: false }),
            "first DNS failure should be transient"
        );

        let second = tokio::time::timeout(
            Duration::from_secs(10),
            attempt_connection_cycle(&mut record, &status, &context, None),
        )
        .await
        .expect("second DNS lookup of an .invalid hostname must fail fast — the resolver hung");

        assert!(
            matches!(second, CycleOutcome::Transient { connected: false }),
            "repeated DNS failure should stay transient"
        );
        let snap = status.read().expect("status lock").clone();
        assert!(!snap.connected);
        assert_eq!(
            snap.last_error_kind.as_deref(),
            Some(ERROR_KIND_TRACKER_CONNECTION_FAILED),
            "subsequent failure surfaces as connection-failed (transient)"
        );
    }

    /// Wait until `captured` holds at least `target` entries, or the
    /// timeout elapses. Lets the propagation test await captures without
    /// fixed sleeps.
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
