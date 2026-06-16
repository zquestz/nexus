//! One-shot tracker query for the client discovery panel.
//!
//! Connects to a configured tracker, performs the protocol handshake
//! (with two-stage fingerprint verification), sends one
//! `TrackerServerList` request, and returns the response. The tracker
//! closes its side after the response per spec — no explicit shutdown
//! is needed.
//!
//! Mirrors the connect → handshake → 2-stage-fingerprint flow used by
//! the BBS-server's tracker registration task (`nexus-server/src/
//! tracker/task.rs`) but without the long-lived registration loop,
//! retry/backoff, or DB writes — this is a single round-trip from the
//! caller's perspective.
//!
//! TOFU pin commit on first connect happens in the result-message
//! handler, not here, so this module stays read-only on `Config`.

use std::time::Duration;

use nexus_common::TRACKER_PROTOCOL_VERSION;
use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_server_handshake_response, read_tracker_server_message_with_progress_timeout,
    send_client_message, send_tracker_client_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use tokio::io::split;
use tokio::time::timeout;

use super::tls::establish_connection;
use super::types::{TrackerQueryError, TrackerQueryOk, TrackerQueryParams};
use crate::i18n::{t, t_args, translate_frame_error, translate_protocol_io_error};

/// Read timeout for tracker handshake and list responses. Matches the
/// BBS-server tracker task's `TRACKER_RESPONSE_TIMEOUT` so client and
/// server agree on what "wedged tracker" means in time.
const TRACKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Run one tracker query end-to-end.
///
/// Phases:
///
/// 1. TCP + TLS via the shared `establish_connection` helper.
/// 2. Stage 1 fingerprint check: TLS-observed vs stored pin (if any).
///    On mismatch returns `Stage1Mismatch`; the result handler stashes
///    the observed value in the cache's `pending_fingerprint` for the
///    AcceptFingerprint dialog.
/// 3. Send `Handshake` with `TRACKER_PROTOCOL_VERSION` (NOT the BBS
///    `PROTOCOL_VERSION` — the tracker has its own independent version).
/// 4. Read `HandshakeResponse` with a tight per-call timeout.
/// 5. Stage 2 fingerprint check: TLS-observed vs server-self-reported.
///    Mismatch indicates active interception (`Stage2Mismatch`).
///    Malformed self-reported value is `MalformedResponse` so the
///    panel can distinguish "tracker is broken" from "tracker is being
///    intercepted".
/// 6. Send `TrackerServerList` with the password, locale, and client
///    version (the version is required on the wire so the tracker can
///    filter to semver-compatible entries).
/// 7. Read `TrackerServerListResponse`. `success: true` → entries.
///    `success: false` → `Rejected { kind, message }`. Anything else
///    → `Protocol` or `MalformedResponse`.
pub async fn query_tracker(
    params: TrackerQueryParams,
) -> Result<TrackerQueryOk, TrackerQueryError> {
    // Phase 1: TCP + TLS. `establish_connection` returns a typed
    // `ConnectError` distinguishing phase (TCP / TLS / proxy /
    // fingerprint), so the panel can render phase-appropriate
    // messages — e.g. TLS-phase failures reliably indicate the
    // tracker accepted TCP then dropped (rate limit, refusing).
    let (stream, tls_observed) =
        establish_connection(&params.address, params.port, params.proxy.as_ref())
            .await
            .map_err(TrackerQueryError::Connection)?;

    // Phase 2: Stage 1 — TLS-observed vs stored pin. `None` is
    // first-connect TOFU; the pin is committed by the result handler
    // on `Ok` return so this module never writes to `Config`.
    if let Some(expected) = &params.expected_fingerprint
        && expected != &tls_observed
    {
        return Err(TrackerQueryError::Stage1Mismatch {
            received: tls_observed,
        });
    }

    // Split + frame the TLS stream.
    let (read_half, write_half) = split(stream);
    let mut reader = FrameReader::new(read_half);
    let mut writer = FrameWriter::new(write_half);

    // Phase 3: send Handshake. Tracker protocol version, not BBS.
    // Wrapped in a `TRACKER_RESPONSE_TIMEOUT` outer bound so a peer
    // that accepted TLS but then blackholes our writes can't park the
    // task indefinitely behind kernel TCP backpressure.
    timeout(
        TRACKER_RESPONSE_TIMEOUT,
        send_client_message(
            &mut writer,
            &ClientMessage::Handshake {
                version: TRACKER_PROTOCOL_VERSION.to_string(),
            },
        ),
    )
    .await
    .map_err(|_| TrackerQueryError::Handshake(t("err-tracker-timeout-sending-handshake")))?
    .map_err(|e| TrackerQueryError::Handshake(e.to_string()))?;

    // Phase 4: read HandshakeResponse with a tight per-call timeout.
    let (server_reported, server_version) = match timeout(
        TRACKER_RESPONSE_TIMEOUT,
        read_server_handshake_response(&mut reader),
    )
    .await
    {
        Ok(Ok(Some(received))) => match received.message {
            ServerMessage::HandshakeResponse {
                success,
                fingerprint,
                version,
                error,
            } => {
                if !success {
                    return Err(TrackerQueryError::Handshake(error.unwrap_or_default()));
                }
                (fingerprint, version)
            }
            ServerMessage::Error { message, .. } => {
                return Err(TrackerQueryError::Handshake(message));
            }
            _ => {
                return Err(TrackerQueryError::Handshake(t(
                    "err-unexpected-handshake-response",
                )));
            }
        },
        Ok(Ok(None)) => {
            return Err(TrackerQueryError::Handshake(t("err-connection-closed")));
        }
        Ok(Err(e)) => {
            return Err(TrackerQueryError::Handshake(translate_protocol_io_error(
                &e,
            )));
        }
        Err(_elapsed) => {
            return Err(TrackerQueryError::Handshake(t_args(
                "err-connection-timeout",
                &[("seconds", &TRACKER_RESPONSE_TIMEOUT.as_secs().to_string())],
            )));
        }
    };

    // Phase 5: Stage 2 — TLS-observed vs server-self-reported.
    // Validate canonical form first so a malformed self-report routes
    // to MalformedResponse (tracker broken) rather than Stage2Mismatch
    // (tracker being intercepted).
    if !is_canonical_fingerprint(&server_reported) {
        return Err(TrackerQueryError::MalformedResponse(t_args(
            "err-tracker-noncanonical-handshake-fingerprint",
            &[("bytes", &server_reported.len().to_string())],
        )));
    }
    if server_reported != tls_observed {
        return Err(TrackerQueryError::Stage2Mismatch);
    }

    // Trust-but-verify: the tracker's `success: true` indicates it
    // already filtered for compat server-side, but a buggy tracker
    // could erroneously accept us. Re-check the reported version
    // against ours via the same `check_compatibility` rules the BBS
    // handshake uses. Missing version is accepted (forward-compat
    // with older trackers); unparseable rejects as MalformedResponse.
    if let Some(reported) = server_version.as_deref() {
        let server_v = nexus_common::version::Version::parse(reported).map_err(|e| {
            TrackerQueryError::MalformedResponse(t_args(
                "err-tracker-version-unparseable",
                &[("version", reported), ("error", &e.to_string())],
            ))
        })?;
        let client_v = nexus_common::version::Version::parse(TRACKER_PROTOCOL_VERSION)
            .expect("TRACKER_PROTOCOL_VERSION is a canonical semver constant");
        let compat = nexus_common::version::check_compatibility(&server_v, &client_v);
        if !compat.is_compatible() {
            return Err(TrackerQueryError::Handshake(t_args(
                "err-tracker-version-incompatible",
                &[
                    ("server_version", reported),
                    ("client_version", TRACKER_PROTOCOL_VERSION),
                ],
            )));
        }
    }

    // Phase 6: send TrackerServerList. Tracker closes after responding.
    // Same `TRACKER_RESPONSE_TIMEOUT` outer bound as Phase 3 — defense
    // against a peer that stalls our send post-handshake.
    timeout(
        TRACKER_RESPONSE_TIMEOUT,
        send_tracker_client_message(
            &mut writer,
            &TrackerClientMessage::TrackerServerList {
                password: params.password.clone(),
                locale: params.locale.clone(),
                version: params.version.clone(),
            },
        ),
    )
    .await
    .map_err(|_| TrackerQueryError::Protocol(t("err-tracker-timeout-sending-server-list")))?
    .map_err(|e| TrackerQueryError::Protocol(e.to_string()))?;

    // Phase 7: read TrackerServerListResponse. The response can be large, so
    // keep a 30s wait-for-first-byte bound but use a progress timeout once the
    // frame starts instead of a whole-frame deadline.
    let response = read_tracker_server_message_with_progress_timeout(
        &mut reader,
        Some(TRACKER_RESPONSE_TIMEOUT),
        Some(TRACKER_RESPONSE_TIMEOUT),
    )
    .await
    .map_err(|e| TrackerQueryError::Protocol(translate_frame_error(&e)))?;

    match response {
        Some(received) => match received.message {
            TrackerServerMessage::TrackerServerListResponse {
                success: true,
                servers,
                ..
            } => {
                // Validate each entry's fingerprint shape on receipt —
                // the protocol guarantees the canonical 95-byte uppercase
                // form, but a buggy or hostile tracker could ship
                // malformed values that would otherwise reach the UI
                // (and the connect-form pre-fill) untouched. Reject the
                // whole response on first non-canonical fingerprint
                // since the tracker is definitionally broken at that
                // point.
                for entry in &servers {
                    if !nexus_common::fingerprint::is_canonical_fingerprint(&entry.fingerprint) {
                        return Err(TrackerQueryError::MalformedResponse(t_args(
                            "err-tracker-noncanonical-entry-fingerprint",
                            &[("name", &entry.name)],
                        )));
                    }
                }
                Ok(TrackerQueryOk {
                    entries: servers,
                    observed_fingerprint: tls_observed,
                })
            }
            TrackerServerMessage::TrackerServerListResponse {
                success: false,
                error,
                error_kind,
                ..
            } => Err(TrackerQueryError::Rejected {
                kind: error_kind,
                message: error.unwrap_or_default(),
            }),
            TrackerServerMessage::Error { message, .. } => {
                Err(TrackerQueryError::Protocol(message))
            }
            _ => Err(TrackerQueryError::Protocol(t(
                "err-tracker-unexpected-list-response",
            ))),
        },
        None => Err(TrackerQueryError::Protocol(t("err-connection-closed"))),
    }
}
