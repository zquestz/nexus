//! `TrackerServerList` handler.
//!
//! A client connection sends one `TrackerServerList`; the tracker replies
//! once (success or typed failure) and closes — client connections are
//! short-lived by design. After field validation and (if gated) password
//! verification, the registry snapshot is filtered to entries
//! semver-compatible with the requesting client.

use std::io;
use std::net::SocketAddr;

use tokio::io::AsyncWrite;
use tracing::{debug, warn};

use nexus_common::framing::{FrameWriter, TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE};
use nexus_common::tracker_protocol::{ServerEntry, TrackerServerMessage};
use nexus_common::validators::{
    MAX_PASSWORD_LENGTH, MAX_VERSION_LENGTH, VersionError, validate_version,
};
use nexus_common::version::{self, Version};
use nexus_common::{ERROR_KIND_INVALID, ERROR_KIND_RATE_LIMITED, ERROR_KIND_UNAUTHORIZED};

use crate::auth::check_password;
use crate::connection_io::send_tracker_server_message_with_write_timeout;
use crate::constants::{
    ERR_REGISTRY_MUTEX_POISONED, LOG_AUTH_RATE_LIMITED, LOG_LIST_DROP_UNPARSEABLE_VERSION,
    LOG_LIST_REJECTED, LOG_LIST_RESPONSE, LOG_LIST_TRUNCATED, REASON_PASSWORD_TOO_LONG,
    REASON_RATE_LIMITED, REASON_UNAUTHORIZED, REASON_VERSION_INVALID, REASON_VERSION_TOO_LONG,
};
use crate::errors::{
    err_tracker_password_too_long, err_tracker_rate_limited, err_tracker_unauthorized,
    err_tracker_version_invalid, err_tracker_version_too_long,
};
use crate::state::TrackerState;
use nexus_common::rate_limiter::RateCheck;

/// Decoded `TrackerServerList` request fields.
pub struct ListParams {
    pub password: Option<String>,
    pub locale: String,
    /// Requesting client's `CARGO_PKG_VERSION`. Validated and parsed
    /// before the registry snapshot, then used to filter out entries
    /// the client cannot speak with.
    pub version: String,
}

impl std::fmt::Debug for ListParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListParams")
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("locale", &self.locale)
            .field("version", &self.version)
            .finish()
    }
}

/// Drive the `TrackerServerList` flow. Always sends exactly one
/// `TrackerServerListResponse` to the wire.
pub async fn handle_tracker_server_list<W>(
    params: ListParams,
    state: &TrackerState,
    writer: &mut FrameWriter<W>,
    peer_addr: SocketAddr,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let ListParams {
        password,
        locale,
        version,
    } = params;

    // Use the request's locale to translate when it's within bounds;
    // fall back to DEFAULT_LOCALE when the locale field itself is suspect.
    if let Some((reason, message)) = super::validate_locale(&locale) {
        warn!(ip = %peer_addr.ip(), reason = reason, "{}", LOG_LIST_REJECTED);
        return send_failure(writer, ERROR_KIND_INVALID, message).await;
    }
    if let Some(p) = &password
        && p.len() > MAX_PASSWORD_LENGTH
    {
        warn!(ip = %peer_addr.ip(), reason = REASON_PASSWORD_TOO_LONG, "{}", LOG_LIST_REJECTED);
        return send_failure(
            writer,
            ERROR_KIND_INVALID,
            err_tracker_password_too_long(&locale, MAX_PASSWORD_LENGTH),
        )
        .await;
    }

    // `version` is required (empty / over-cap / unparseable all reject);
    // a missing field rejects at the framing layer. The parsed `Version`
    // is reused below to filter entries to compat matches.
    let client_version: Version = match validate_version(&version) {
        Ok(v) => v,
        Err(e) => {
            let (reason, message) = match e {
                VersionError::TooLong => (
                    REASON_VERSION_TOO_LONG,
                    err_tracker_version_too_long(&locale, MAX_VERSION_LENGTH),
                ),
                VersionError::Empty | VersionError::InvalidSemver => {
                    (REASON_VERSION_INVALID, err_tracker_version_invalid(&locale))
                }
            };
            warn!(ip = %peer_addr.ip(), reason = reason, "{}", LOG_LIST_REJECTED);
            return send_failure(writer, ERROR_KIND_INVALID, message).await;
        }
    };

    // Password verification when gated. Two-phase rate limiting (see
    // TrackerServerRegister handler). Snapshot the hash once so a
    // SIGHUP-driven swap mid-handler can't make decisions disagree.
    let stored_hash = state.listing_password_snapshot();
    let gated = stored_hash.is_some();
    if gated && state.auth_failure_rate_limiter.check_only(peer_addr.ip()) == RateCheck::Limited {
        warn!(ip = %peer_addr.ip(), "{}", LOG_AUTH_RATE_LIMITED);
        warn!(ip = %peer_addr.ip(), reason = REASON_RATE_LIMITED, "{}", LOG_LIST_REJECTED);
        return send_failure(
            writer,
            ERROR_KIND_RATE_LIMITED,
            err_tracker_rate_limited(&locale),
        )
        .await;
    }
    if !check_password(password.as_deref(), stored_hash.as_deref()).await {
        if gated {
            state
                .auth_failure_rate_limiter
                .record_failure(peer_addr.ip());
        }
        warn!(ip = %peer_addr.ip(), reason = REASON_UNAUTHORIZED, "{}", LOG_LIST_REJECTED);
        return send_failure(
            writer,
            ERROR_KIND_UNAUTHORIZED,
            err_tracker_unauthorized(&locale),
        )
        .await;
    }

    // Snapshot; the mutex is held only for the clone.
    let mut servers = state
        .registry
        .lock()
        .expect(ERR_REGISTRY_MUTEX_POISONED)
        .list();
    let total = servers.len();

    // Tracker-side compat filter: keep entries whose registered `version`
    // is `Compatible` with the client per `check_compatibility`.
    // Registration guarantees parseability; the defensive warn-and-drop
    // guards against a buggy registrant that slipped past that gate.
    servers.retain(|entry| match Version::parse(&entry.version) {
        Ok(server_version) => {
            version::check_compatibility(&server_version, &client_version).is_compatible()
        }
        Err(_) => {
            warn!(
                ip = %peer_addr.ip(),
                name = %entry.name,
                version = %entry.version,
                "{}",
                LOG_LIST_DROP_UNPARSEABLE_VERSION
            );
            false
        }
    });
    let fit = truncate_servers_to_response_limit(&mut servers)?;
    if fit.truncated() {
        warn!(
            ip = %peer_addr.ip(),
            kept = fit.kept,
            compatible_total = fit.compatible_total,
            payload_bytes = fit.payload_size,
            limit = TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE,
            "{}",
            LOG_LIST_TRUNCATED
        );
    }

    debug!(
        ip = %peer_addr.ip(),
        count = servers.len(),
        compatible_total = fit.compatible_total,
        total = total,
        payload_bytes = fit.payload_size,
        client_version = %client_version,
        "{}",
        LOG_LIST_RESPONSE
    );

    let response = TrackerServerMessage::TrackerServerListResponse {
        success: true,
        servers,
        error: None,
        error_kind: None,
    };
    send_tracker_server_message_with_write_timeout(writer, &response).await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListResponseFit {
    compatible_total: usize,
    kept: usize,
    payload_size: usize,
}

impl ListResponseFit {
    fn truncated(self) -> bool {
        self.kept < self.compatible_total
    }
}

/// Trim a sorted compatible list to the largest prefix that serializes
/// under the client-side `TrackerServerListResponse` frame cap. Size by
/// actual JSON bytes instead of pessimistic field caps so ordinary short
/// listings can include far more entries than the worst-case estimate.
fn truncate_servers_to_response_limit(
    servers: &mut Vec<ServerEntry>,
) -> io::Result<ListResponseFit> {
    let compatible_total = servers.len();
    let mut payload_size = empty_success_response_payload_len()?;
    let mut kept = 0;

    for entry in servers.iter() {
        let entry_size = serialized_server_entry_len(entry)?;
        let comma = usize::from(kept > 0);
        let Some(next_size) = payload_size
            .checked_add(comma)
            .and_then(|size| size.checked_add(entry_size))
        else {
            break;
        };
        if next_size > TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE {
            break;
        }
        payload_size = next_size;
        kept += 1;
    }

    if kept < compatible_total {
        servers.truncate(kept);
    }

    Ok(ListResponseFit {
        compatible_total,
        kept,
        payload_size,
    })
}

fn empty_success_response_payload_len() -> io::Result<usize> {
    let response = TrackerServerMessage::TrackerServerListResponse {
        success: true,
        servers: Vec::new(),
        error: None,
        error_kind: None,
    };
    serde_json::to_vec(&response)
        .map(|payload| payload.len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn serialized_server_entry_len(entry: &ServerEntry) -> io::Result<usize> {
    serde_json::to_vec(entry)
        .map(|payload| payload.len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Build and send a failure-shaped `TrackerServerListResponse`.
async fn send_failure<W>(
    writer: &mut FrameWriter<W>,
    error_kind: &str,
    message: String,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::TrackerServerListResponse {
        success: false,
        servers: Vec::new(),
        error: Some(message),
        error_kind: Some(error_kind.to_string()),
    };
    send_tracker_server_message_with_write_timeout(writer, &response).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::validators::{
        MAX_PUBLIC_ADDRESS_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_NAME_LENGTH,
        MAX_VERSION_LENGTH,
    };

    fn fingerprint() -> String {
        let mut value = "AA:".repeat(31);
        value.push_str("AA");
        value
    }

    fn small_entry(index: usize) -> ServerEntry {
        ServerEntry {
            name: format!("server-{index:05}"),
            description: None,
            address: "bbs.example.com".to_string(),
            port: 23,
            websocket_port: None,
            version: "0.9.3".to_string(),
            fingerprint: fingerprint(),
            user_count: index as u32,
            allows_guest: false,
        }
    }

    fn large_entry(index: usize) -> ServerEntry {
        let prefix = format!("{index:05}");
        let name = format!(
            "{}{}",
            prefix,
            "\u{1F680}".repeat(MAX_SERVER_NAME_LENGTH - prefix.chars().count())
        );
        ServerEntry {
            name,
            description: Some("\u{1F680}".repeat(MAX_SERVER_DESCRIPTION_LENGTH)),
            address: "x".repeat(MAX_PUBLIC_ADDRESS_LENGTH),
            port: u16::MAX,
            websocket_port: Some(u16::MAX),
            version: "x".repeat(MAX_VERSION_LENGTH),
            fingerprint: fingerprint(),
            user_count: u32::MAX,
            allows_guest: false,
        }
    }

    #[test]
    fn truncation_uses_actual_serialized_size() {
        let mut servers = (0..20_000).map(small_entry).collect::<Vec<_>>();

        let fit = truncate_servers_to_response_limit(&mut servers).expect("fit response");

        assert_eq!(fit.compatible_total, 20_000);
        assert_eq!(fit.kept, 20_000);
        assert!(!fit.truncated());
        assert!(fit.payload_size <= TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE);
    }

    #[test]
    fn truncation_stops_before_frame_cap_and_preserves_order() {
        let sample = large_entry(0);
        let entry_size = serialized_server_entry_len(&sample).expect("entry serializes");
        let empty_size = empty_success_response_payload_len().expect("empty response serializes");
        let count = ((TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE - empty_size) / (entry_size + 1)) + 2;
        let mut servers = (0..count).map(large_entry).collect::<Vec<_>>();

        let fit = truncate_servers_to_response_limit(&mut servers).expect("fit response");

        assert!(fit.truncated());
        assert_eq!(fit.compatible_total, count);
        assert_eq!(fit.kept, servers.len());
        assert!(fit.payload_size <= TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE);
        for (index, entry) in servers.iter().enumerate() {
            assert!(entry.name.starts_with(&format!("{index:05}")));
        }

        let response = TrackerServerMessage::TrackerServerListResponse {
            success: true,
            servers,
            error: None,
            error_kind: None,
        };
        let final_payload_size = serde_json::to_vec(&response)
            .expect("response serializes")
            .len();
        assert_eq!(fit.payload_size, final_payload_size);
        assert!(final_payload_size <= TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE);
    }
}
