//! `TrackerServerRegister` handler.
//!
//! A server connection sends `TrackerServerRegister` to register itself or to
//! refresh an existing entry. The first register on a connection inserts
//! a new entry and locks the connection's role to "server"; subsequent
//! `TrackerServerRegister` messages on the same connection are refreshes,
//! replacing the stored entry idempotently and resetting the entry's
//! `last_refresh`.
//!
//! The two paths are exposed as separate entry points so callers don't
//! need to handle impossible variants:
//!
//! - [`handle_initial_register`] returns [`InitialRegisterOutcome`]
//!   (`Registered(id)` or `Rejected`).
//! - [`handle_refresh`] returns [`RefreshOutcome`] (`Refreshed` or
//!   `Rejected`).
//!
//! Field-length validation, fingerprint format checks, the auth-failure
//! rate-limit gate, and password verification are shared via the
//! private [`validate_and_authenticate`] helper.

use std::io;
use std::net::SocketAddr;
use std::time::Instant;

use tokio::io::AsyncWrite;
use tracing::{debug, info, warn};

use nexus_common::framing::FrameWriter;
use nexus_common::io::send_tracker_server_message;
use nexus_common::tracker_protocol::{ServerEntry, TrackerServerMessage};
use nexus_common::validators::{
    MAX_LOCALE_LENGTH, MAX_PASSWORD_LENGTH, MAX_PUBLIC_ADDRESS_LENGTH,
    MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_NAME_LENGTH, MAX_VERSION_LENGTH,
};
use nexus_common::{
    ERROR_KIND_CAPACITY, ERROR_KIND_INVALID, ERROR_KIND_RATE_LIMITED, ERROR_KIND_UNAUTHORIZED,
};

use crate::auth::check_password;
use crate::constants::{
    DEFAULT_LOCALE, ERR_REGISTRY_MUTEX_POISONED, LOG_AUTH_RATE_LIMITED, LOG_REFRESH_GHOST_ID,
    LOG_REFRESH_TOO_SOON, LOG_REGISTER_NEW, LOG_REGISTER_REFRESH, LOG_REGISTER_REJECTED,
};
use crate::errors::{
    err_tracker_address_invalid, err_tracker_address_too_long, err_tracker_capacity,
    err_tracker_description_too_long, err_tracker_fingerprint_invalid, err_tracker_locale_too_long,
    err_tracker_name_too_long, err_tracker_password_too_long, err_tracker_per_ip_capacity,
    err_tracker_rate_limited, err_tracker_unauthorized, err_tracker_version_too_long,
};
use crate::rate_limiter::RateCheck;
use crate::registry::{ConnectionId, RegisterError};
use crate::state::TrackerState;

/// Decoded `TrackerServerRegister` request fields.
pub struct RegisterParams {
    pub password: Option<String>,
    pub locale: String,
    pub name: String,
    pub description: Option<String>,
    pub address: Option<String>,
    pub port: u16,
    pub websocket_port: Option<u16>,
    pub version: String,
    pub fingerprint: String,
    pub user_count: u32,
    pub allows_guest: bool,
}

/// Outcome of [`handle_initial_register`]. The caller (connection task)
/// uses this to decide whether to enter the refresh loop or close the
/// connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialRegisterOutcome {
    /// Accepted as a fresh registration. The id is the new connection's
    /// registry slot — caller stores it for refresh / cleanup.
    Registered(ConnectionId),
    /// Rejected with a typed failure response. Connection should close.
    Rejected,
}

/// Outcome of [`handle_refresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// Accepted as a refresh on the supplied id. Entry data is replaced
    /// and `last_refresh` reset.
    Refreshed,
    /// Rejected with a typed failure response. Connection should close.
    /// Refresh-floor violations land here too — a client refreshing
    /// faster than half the protocol minimum is broken or malicious,
    /// so we kick rather than tolerate.
    Rejected,
}

/// Drive the initial-register flow (no `existing_id` — this is the
/// first `TrackerServerRegister` on a fresh connection). Always sends
/// exactly one `TrackerServerRegisterResponse` to the wire.
pub async fn handle_initial_register<W>(
    params: RegisterParams,
    state: &TrackerState,
    writer: &mut FrameWriter<W>,
    peer_addr: SocketAddr,
) -> io::Result<InitialRegisterOutcome>
where
    W: AsyncWrite + Unpin,
{
    let Validated { entry, locale } =
        match validate_and_authenticate(params, state, writer, peer_addr).await? {
            ValidationOutcome::Valid(v) => v,
            ValidationOutcome::Rejected => return Ok(InitialRegisterOutcome::Rejected),
        };

    let now = Instant::now();
    let entry_name = entry.name.clone();
    let result = state
        .registry
        .lock()
        .expect(ERR_REGISTRY_MUTEX_POISONED)
        .register(entry, peer_addr.ip(), now);

    match result {
        Ok(id) => {
            info!(
                ip = %peer_addr.ip(),
                id = id,
                name = %entry_name,
                "{}",
                LOG_REGISTER_NEW
            );
            send_success(writer, state.refresh_interval).await?;
            Ok(InitialRegisterOutcome::Registered(id))
        }
        Err(RegisterError::Capacity) => {
            reject(
                writer,
                ip(&peer_addr),
                "capacity",
                ERROR_KIND_CAPACITY,
                err_tracker_capacity(&locale),
            )
            .await?;
            Ok(InitialRegisterOutcome::Rejected)
        }
        Err(RegisterError::PerIpCapacity) => {
            reject(
                writer,
                ip(&peer_addr),
                "per_ip_capacity",
                ERROR_KIND_CAPACITY,
                err_tracker_per_ip_capacity(&locale),
            )
            .await?;
            Ok(InitialRegisterOutcome::Rejected)
        }
    }
}

/// Drive the refresh flow on an established server connection. The
/// `id` is the registry slot the connection registered itself in via
/// [`handle_initial_register`]; the drop guard on the connection task
/// keeps it alive until disconnect, so the [`Registry::refresh`]
/// `false` (id-not-found) path is normally unreachable. The handler
/// still treats it defensively — see [`LOG_REFRESH_GHOST_ID`] — so a
/// future stale-eviction worker can't crash a live connection.
///
/// Always sends exactly one `TrackerServerRegisterResponse` to the wire.
pub async fn handle_refresh<W>(
    params: RegisterParams,
    id: ConnectionId,
    state: &TrackerState,
    writer: &mut FrameWriter<W>,
    peer_addr: SocketAddr,
) -> io::Result<RefreshOutcome>
where
    W: AsyncWrite + Unpin,
{
    let now = Instant::now();

    // Per-entry refresh floor. Checking *before* validation /
    // password verification means a misbehaving server hammering
    // refreshes can't pin CPU on Argon2 hashing. We don't update
    // `last_refresh` on rejection — preserves the slide-protection
    // property (rapid rejected refreshes can't keep an entry alive).
    //
    // The lock is released before any `.await` (held only across the
    // `last_refresh` peek) so the future stays `Send`. A zero
    // `refresh_floor` (set by tests) skips the check entirely.
    if !state.refresh_floor.is_zero() {
        let last_refresh = state
            .registry
            .lock()
            .expect(ERR_REGISTRY_MUTEX_POISONED)
            .last_refresh(id);
        if let Some(last) = last_refresh
            && now.duration_since(last) < state.refresh_floor
        {
            warn!(ip = %peer_addr.ip(), id = id, "{}", LOG_REFRESH_TOO_SOON);
            reject(
                writer,
                ip(&peer_addr),
                "refresh_too_soon",
                ERROR_KIND_RATE_LIMITED,
                err_tracker_rate_limited(&params.locale),
            )
            .await?;
            return Ok(RefreshOutcome::Rejected);
        }
    }

    let Validated { entry, locale: _ } =
        match validate_and_authenticate(params, state, writer, peer_addr).await? {
            ValidationOutcome::Valid(v) => v,
            ValidationOutcome::Rejected => return Ok(RefreshOutcome::Rejected),
        };

    let user_count = entry.user_count;
    let updated = state
        .registry
        .lock()
        .expect(ERR_REGISTRY_MUTEX_POISONED)
        .refresh(id, entry, now);
    if !updated {
        // Drop-guard convention says this entry should still be live —
        // the handler holds a guard that only unregisters on
        // connection close. If it's gone anyway, something
        // out-of-band evicted it (future stale-eviction worker, or a
        // bug). Close the connection gracefully rather than
        // panicking; the drop guard's own unregister call is
        // idempotent.
        warn!(ip = %peer_addr.ip(), id = id, "{}", LOG_REFRESH_GHOST_ID);
        return Ok(RefreshOutcome::Rejected);
    }
    debug!(id = id, user_count = user_count, "{}", LOG_REGISTER_REFRESH);
    send_success(writer, state.refresh_interval).await?;
    Ok(RefreshOutcome::Refreshed)
}

/// Successful output of the shared validation+authentication phase:
/// the validated `ServerEntry` plus the request's `locale` (which
/// downstream rejection paths in the call sites still need for
/// translated error messages — capacity rejections after registry
/// access).
struct Validated {
    entry: ServerEntry,
    locale: String,
}

/// Result of the shared validation+authentication phase. On rejection,
/// the failure response has already been written and logged; the
/// caller need only return its own rejection outcome.
enum ValidationOutcome {
    Valid(Validated),
    Rejected,
}

/// Run the shared length / fingerprint / password checks, send a typed
/// failure response on any rejection, and return the validated
/// [`ServerEntry`] (with `locale`) on success. Consumes `params` so
/// the field strings move into the resulting `ServerEntry` instead of
/// being cloned.
async fn validate_and_authenticate<W>(
    params: RegisterParams,
    state: &TrackerState,
    writer: &mut FrameWriter<W>,
    peer_addr: SocketAddr,
) -> io::Result<ValidationOutcome>
where
    W: AsyncWrite + Unpin,
{
    // Destructure upfront so individual fields move out without
    // cloning. Length/format validators reference `&locale`,
    // `&password`, etc.; the success path moves the strings into
    // `ServerEntry`.
    let RegisterParams {
        password,
        locale,
        name,
        description,
        address,
        port,
        websocket_port,
        version,
        fingerprint,
        user_count,
        allows_guest,
    } = params;

    // Length and format validation. Use the request's locale for
    // translation when it's within bounds; fall back to DEFAULT_LOCALE
    // when the locale field itself is suspect.
    if locale.len() > MAX_LOCALE_LENGTH {
        reject(
            writer,
            ip(&peer_addr),
            "locale_too_long",
            ERROR_KIND_INVALID,
            err_tracker_locale_too_long(DEFAULT_LOCALE, MAX_LOCALE_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if let Some(p) = &password
        && p.len() > MAX_PASSWORD_LENGTH
    {
        reject(
            writer,
            ip(&peer_addr),
            "password_too_long",
            ERROR_KIND_INVALID,
            err_tracker_password_too_long(&locale, MAX_PASSWORD_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if name.len() > MAX_SERVER_NAME_LENGTH {
        reject(
            writer,
            ip(&peer_addr),
            "name_too_long",
            ERROR_KIND_INVALID,
            err_tracker_name_too_long(&locale, MAX_SERVER_NAME_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if let Some(d) = &description
        && d.len() > MAX_SERVER_DESCRIPTION_LENGTH
    {
        reject(
            writer,
            ip(&peer_addr),
            "description_too_long",
            ERROR_KIND_INVALID,
            err_tracker_description_too_long(&locale, MAX_SERVER_DESCRIPTION_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if let Some(a) = &address
        && a.len() > MAX_PUBLIC_ADDRESS_LENGTH
    {
        reject(
            writer,
            ip(&peer_addr),
            "address_too_long",
            ERROR_KIND_INVALID,
            err_tracker_address_too_long(&locale, MAX_PUBLIC_ADDRESS_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if version.len() > MAX_VERSION_LENGTH {
        reject(
            writer,
            ip(&peer_addr),
            "version_too_long",
            ERROR_KIND_INVALID,
            err_tracker_version_too_long(&locale, MAX_VERSION_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if !nexus_common::fingerprint::is_canonical_fingerprint(&fingerprint) {
        reject(
            writer,
            ip(&peer_addr),
            "fingerprint_invalid",
            ERROR_KIND_INVALID,
            err_tracker_fingerprint_invalid(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }

    // Password check (gated registration). Two-phase rate limiting:
    // peek the auth-failure bucket BEFORE verification so an attacker
    // with too many recent failures can't sneak through with a guess;
    // record the failure only on actual mismatch so legitimate
    // operators with the correct password don't burn tokens.
    //
    // Snapshot the hash once and use it for every decision below so
    // that a SIGHUP-driven hash swap mid-handler doesn't make the
    // gated/open and verify decisions disagree.
    let stored_hash = state.registration_password_snapshot();
    let gated = stored_hash.is_some();
    if gated && state.auth_failure_rate_limiter.check_only(peer_addr.ip()) == RateCheck::Limited {
        warn!(ip = %peer_addr.ip(), "{}", LOG_AUTH_RATE_LIMITED);
        reject(
            writer,
            ip(&peer_addr),
            "rate_limited",
            ERROR_KIND_RATE_LIMITED,
            err_tracker_rate_limited(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if !check_password(password.as_deref(), stored_hash.as_deref()) {
        if gated {
            state
                .auth_failure_rate_limiter
                .record_failure(peer_addr.ip());
        }
        reject(
            writer,
            ip(&peer_addr),
            "unauthorized",
            ERROR_KIND_UNAUTHORIZED,
            err_tracker_unauthorized(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }

    // Resolve `address` — substitute the peer's IP when the field is
    // omitted or empty. The string form of an IpAddr is the canonical
    // form clients can plug into `nexus://`. Move out of `address`
    // when it's non-empty so we don't clone.
    let resolved_address = match address {
        Some(a) if !a.is_empty() => a,
        _ => peer_addr.ip().to_string(),
    };
    // Address-validation hook: empty after resolution would mean we
    // somehow ended up with an unrepresentable peer IP (shouldn't
    // happen), but we guard anyway.
    if resolved_address.is_empty() {
        reject(
            writer,
            ip(&peer_addr),
            "address_invalid",
            ERROR_KIND_INVALID,
            err_tracker_address_invalid(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }

    Ok(ValidationOutcome::Valid(Validated {
        entry: ServerEntry {
            name,
            description,
            address: resolved_address,
            port,
            websocket_port,
            version,
            fingerprint,
            user_count,
            allows_guest,
        },
        locale,
    }))
}

/// Send a successful `TrackerServerRegisterResponse`.
async fn send_success<W>(writer: &mut FrameWriter<W>, refresh_interval: u32) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::TrackerServerRegisterResponse {
        success: true,
        refresh_interval: Some(refresh_interval),
        error: None,
        error_kind: None,
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}

/// Send a typed failure `TrackerServerRegisterResponse`.
async fn send_failure<W>(
    writer: &mut FrameWriter<W>,
    error_kind: &str,
    message: String,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::TrackerServerRegisterResponse {
        success: false,
        refresh_interval: None,
        error: Some(message),
        error_kind: Some(error_kind.to_string()),
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}

/// Helper used by all rejection paths: log the rejection (with
/// structured `reason`) and send the failure response.
async fn reject<W>(
    writer: &mut FrameWriter<W>,
    peer_ip_str: String,
    reason: &str,
    error_kind: &str,
    error_msg: String,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    warn!(ip = %peer_ip_str, reason = %reason, "{}", LOG_REGISTER_REJECTED);
    send_failure(writer, error_kind, error_msg).await
}

/// Stringify the peer IP for log structured-field output.
fn ip(addr: &SocketAddr) -> String {
    addr.ip().to_string()
}
