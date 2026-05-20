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

use nexus_common::framing::FrameWriter;
use nexus_common::io::send_tracker_server_message;
use nexus_common::tracker_protocol::TrackerServerMessage;
use nexus_common::validators::{
    MAX_PASSWORD_LENGTH, MAX_VERSION_LENGTH, VersionError, validate_version,
};
use nexus_common::version::{self, Version};
use nexus_common::{ERROR_KIND_INVALID, ERROR_KIND_RATE_LIMITED, ERROR_KIND_UNAUTHORIZED};

use crate::auth::check_password;
use crate::constants::{
    ERR_REGISTRY_MUTEX_POISONED, LOG_AUTH_RATE_LIMITED, LOG_LIST_DROP_UNPARSEABLE_VERSION,
    LOG_LIST_REJECTED, LOG_LIST_RESPONSE, REASON_PASSWORD_TOO_LONG, REASON_RATE_LIMITED,
    REASON_UNAUTHORIZED, REASON_VERSION_INVALID, REASON_VERSION_TOO_LONG,
};
use crate::errors::{
    err_tracker_password_too_long, err_tracker_rate_limited, err_tracker_unauthorized,
    err_tracker_version_invalid, err_tracker_version_too_long,
};
use crate::rate_limiter::RateCheck;
use crate::state::TrackerState;

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

    debug!(
        ip = %peer_addr.ip(),
        count = servers.len(),
        total = total,
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
    send_tracker_server_message(writer, &response).await?;
    Ok(())
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
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}
