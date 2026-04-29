//! `TrackerRegister` handler.
//!
//! A server connection sends `TrackerRegister` to register itself or to
//! refresh an existing entry. The first register on a connection inserts
//! a new entry and locks the connection's role to "server"; subsequent
//! `TrackerRegister` messages on the same connection are refreshes,
//! replacing the stored entry idempotently and resetting the entry's
//! `last_refresh`.
//!
//! Responsibilities:
//!
//! 1. Validate field lengths and fingerprint format (per spec).
//! 2. If registration is gated, verify the password.
//! 3. Resolve `address`: substitute the peer's IP when the field is
//!    omitted / empty.
//! 4. On first register: insert into the registry (capacity / per-IP
//!    cap may reject). On refresh: replace the existing entry by id.
//! 5. Send `TrackerRegisterResponse` carrying the operator-configured
//!    `refresh_interval` on success.

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
use nexus_common::{ERROR_KIND_CAPACITY, ERROR_KIND_INVALID, ERROR_KIND_UNAUTHORIZED};

use crate::auth::check_password;
use crate::constants::{
    DEFAULT_LOCALE, LOG_REGISTER_NEW, LOG_REGISTER_REFRESH, LOG_REGISTER_REJECTED,
};
use crate::errors::{
    err_tracker_address_invalid, err_tracker_address_too_long, err_tracker_capacity,
    err_tracker_description_too_long, err_tracker_fingerprint_invalid, err_tracker_locale_too_long,
    err_tracker_name_too_long, err_tracker_password_too_long, err_tracker_per_ip_capacity,
    err_tracker_unauthorized, err_tracker_version_too_long,
};
use crate::registry::{ConnectionId, RegistryError};
use crate::state::TrackerState;

/// Decoded `TrackerRegister` request fields.
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

/// Outcome of `handle_tracker_register`. The caller (connection task)
/// uses this to decide whether to keep the connection alive for refreshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// Accepted as a fresh registration. The id is the new connection's
    /// registry slot — caller must store it for refresh / cleanup.
    Registered(ConnectionId),
    /// Accepted as a refresh on the supplied id. Entry data is replaced
    /// and `last_refresh` reset.
    Refreshed,
    /// Rejected with a typed failure response. Connection should close.
    Rejected,
}

/// Drive the `TrackerRegister` flow.
///
/// `existing_id` is `None` for the first register on a connection and
/// `Some(id)` for every subsequent refresh on the same connection.
///
/// Always sends exactly one `TrackerRegisterResponse` to the wire.
pub async fn handle_tracker_register<W>(
    params: RegisterParams,
    existing_id: Option<ConnectionId>,
    state: &TrackerState,
    writer: &mut FrameWriter<W>,
    peer_addr: SocketAddr,
) -> io::Result<RegisterOutcome>
where
    W: AsyncWrite + Unpin,
{
    // Length and format validation. Use the request's locale for
    // translation when it's within bounds; fall back to DEFAULT_LOCALE
    // when the locale field itself is suspect.
    if params.locale.len() > MAX_LOCALE_LENGTH {
        return reject(
            writer,
            ip(&peer_addr),
            "locale_too_long",
            ERROR_KIND_INVALID,
            err_tracker_locale_too_long(DEFAULT_LOCALE, MAX_LOCALE_LENGTH),
        )
        .await;
    }
    if let Some(p) = &params.password
        && p.len() > MAX_PASSWORD_LENGTH
    {
        return reject(
            writer,
            ip(&peer_addr),
            "password_too_long",
            ERROR_KIND_INVALID,
            err_tracker_password_too_long(&params.locale, MAX_PASSWORD_LENGTH),
        )
        .await;
    }
    if params.name.len() > MAX_SERVER_NAME_LENGTH {
        return reject(
            writer,
            ip(&peer_addr),
            "name_too_long",
            ERROR_KIND_INVALID,
            err_tracker_name_too_long(&params.locale, MAX_SERVER_NAME_LENGTH),
        )
        .await;
    }
    if let Some(d) = &params.description
        && d.len() > MAX_SERVER_DESCRIPTION_LENGTH
    {
        return reject(
            writer,
            ip(&peer_addr),
            "description_too_long",
            ERROR_KIND_INVALID,
            err_tracker_description_too_long(&params.locale, MAX_SERVER_DESCRIPTION_LENGTH),
        )
        .await;
    }
    if let Some(a) = &params.address
        && a.len() > MAX_PUBLIC_ADDRESS_LENGTH
    {
        return reject(
            writer,
            ip(&peer_addr),
            "address_too_long",
            ERROR_KIND_INVALID,
            err_tracker_address_too_long(&params.locale, MAX_PUBLIC_ADDRESS_LENGTH),
        )
        .await;
    }
    if params.version.len() > MAX_VERSION_LENGTH {
        return reject(
            writer,
            ip(&peer_addr),
            "version_too_long",
            ERROR_KIND_INVALID,
            err_tracker_version_too_long(&params.locale, MAX_VERSION_LENGTH),
        )
        .await;
    }
    if !is_canonical_fingerprint(&params.fingerprint) {
        return reject(
            writer,
            ip(&peer_addr),
            "fingerprint_invalid",
            ERROR_KIND_INVALID,
            err_tracker_fingerprint_invalid(&params.locale),
        )
        .await;
    }

    // Password check (gated registration).
    if !check_password(
        params.password.as_deref(),
        state.registration_password_hash.as_deref(),
    ) {
        return reject(
            writer,
            ip(&peer_addr),
            "unauthorized",
            ERROR_KIND_UNAUTHORIZED,
            err_tracker_unauthorized(&params.locale),
        )
        .await;
    }

    // Resolve `address` — substitute the peer's IP when the field is
    // omitted or empty. The string form of an IpAddr is the canonical
    // form clients can plug into `nexus://`.
    let resolved_address = match &params.address {
        Some(a) if !a.is_empty() => a.clone(),
        _ => peer_addr.ip().to_string(),
    };
    // Address-validation hook: empty after resolution would mean we
    // somehow ended up with an unrepresentable peer IP (shouldn't
    // happen), but we guard anyway.
    if resolved_address.is_empty() {
        return reject(
            writer,
            ip(&peer_addr),
            "address_invalid",
            ERROR_KIND_INVALID,
            err_tracker_address_invalid(&params.locale),
        )
        .await;
    }

    // Build the entry.
    let entry = ServerEntry {
        name: params.name,
        description: params.description,
        address: resolved_address,
        port: params.port,
        websocket_port: params.websocket_port,
        version: params.version,
        fingerprint: params.fingerprint,
        user_count: params.user_count,
        allows_guest: params.allows_guest,
    };
    let now = Instant::now();
    let locale = params.locale;

    // Register or refresh.
    let outcome = {
        let mut registry = state.registry.lock().expect("registry mutex poisoned");
        match existing_id {
            None => match registry.register(entry.clone(), peer_addr.ip(), now) {
                Ok(id) => Ok(RegisterOutcome::Registered(id)),
                Err(RegistryError::Capacity) => Err((
                    "capacity",
                    ERROR_KIND_CAPACITY,
                    err_tracker_capacity(&locale),
                )),
                Err(RegistryError::PerIpCapacity) => Err((
                    "per_ip_capacity",
                    ERROR_KIND_CAPACITY,
                    err_tracker_per_ip_capacity(&locale),
                )),
                Err(RegistryError::NotFound) => unreachable!("register doesn't return NotFound"),
            },
            Some(id) => match registry.refresh(id, entry.clone(), now) {
                Ok(()) => Ok(RegisterOutcome::Refreshed),
                Err(RegistryError::NotFound) => unreachable!(
                    "refresh on a connection-owned id should never NotFound; \
                    eviction only fires for stale entries, and refreshes reset that timer"
                ),
                Err(_) => unreachable!("refresh doesn't enforce caps"),
            },
        }
    };

    match outcome {
        Ok(RegisterOutcome::Registered(id)) => {
            info!(
                ip = %peer_addr.ip(),
                id = id,
                name = %entry.name,
                "{}",
                LOG_REGISTER_NEW
            );
            send_success(writer, state.refresh_interval).await?;
            Ok(RegisterOutcome::Registered(id))
        }
        Ok(RegisterOutcome::Refreshed) => {
            debug!(
                id = existing_id.expect("set when refreshing"),
                user_count = entry.user_count,
                "{}",
                LOG_REGISTER_REFRESH
            );
            send_success(writer, state.refresh_interval).await?;
            Ok(RegisterOutcome::Refreshed)
        }
        Ok(RegisterOutcome::Rejected) => unreachable!("Ok arm doesn't carry Rejected"),
        Err((reason, error_kind, error_msg)) => {
            warn!(ip = %peer_addr.ip(), reason = %reason, "{}", LOG_REGISTER_REJECTED);
            send_failure(writer, error_kind, error_msg).await?;
            Ok(RegisterOutcome::Rejected)
        }
    }
}

/// Validate that `fp` is the canonical SHA-256 fingerprint form: 32
/// uppercase hex pairs separated by colons, exactly 95 ASCII bytes.
/// Matches the format produced by
/// `nexus_common::fingerprint::format_certificate_fingerprint`.
#[must_use]
pub fn is_canonical_fingerprint(fp: &str) -> bool {
    if fp.len() != 95 {
        return false;
    }
    for (i, &b) in fp.as_bytes().iter().enumerate() {
        if i % 3 == 2 {
            if b != b':' {
                return false;
            }
        } else if !(b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) {
            return false;
        }
    }
    true
}

/// Send a successful `TrackerRegisterResponse`.
async fn send_success<W>(writer: &mut FrameWriter<W>, refresh_interval: u32) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::TrackerRegisterResponse {
        success: true,
        refresh_interval: Some(refresh_interval),
        error: None,
        error_kind: None,
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}

/// Send a typed failure `TrackerRegisterResponse`.
async fn send_failure<W>(
    writer: &mut FrameWriter<W>,
    error_kind: &str,
    message: String,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::TrackerRegisterResponse {
        success: false,
        refresh_interval: None,
        error: Some(message),
        error_kind: Some(error_kind.to_string()),
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}

/// Helper for the validation paths above: log the rejection and send
/// the failure response, returning `RegisterOutcome::Rejected`.
async fn reject<W>(
    writer: &mut FrameWriter<W>,
    peer_ip_str: String,
    reason: &str,
    error_kind: &str,
    error_msg: String,
) -> io::Result<RegisterOutcome>
where
    W: AsyncWrite + Unpin,
{
    warn!(ip = %peer_ip_str, reason = %reason, "{}", LOG_REGISTER_REJECTED);
    send_failure(writer, error_kind, error_msg).await?;
    Ok(RegisterOutcome::Rejected)
}

fn ip(peer: &SocketAddr) -> String {
    peer.ip().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_canonical_fingerprint_accepts_valid() {
        // 32 uppercase hex pairs separated by colons = 95 chars.
        let fp = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
             AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
        assert_eq!(fp.len(), 95);
        assert!(is_canonical_fingerprint(fp));
    }

    #[test]
    fn test_is_canonical_fingerprint_rejects_lowercase() {
        // Lowercase hex is not canonical.
        let fp = "aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:\
             aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99";
        assert!(!is_canonical_fingerprint(fp));
    }

    #[test]
    fn test_is_canonical_fingerprint_rejects_wrong_length() {
        assert!(!is_canonical_fingerprint(""));
        assert!(!is_canonical_fingerprint("AA"));
        // 94 chars (one short).
        let short = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
                     AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:9";
        assert_eq!(short.len(), 94);
        assert!(!is_canonical_fingerprint(short));
    }

    #[test]
    fn test_is_canonical_fingerprint_rejects_wrong_separator() {
        // 95 chars but with `-` instead of `:`.
        let fp = "AA-BB-CC-DD-EE-FF-00-11-22-33-44-55-66-77-88-99-\
             AA-BB-CC-DD-EE-FF-00-11-22-33-44-55-66-77-88-99";
        assert_eq!(fp.len(), 95);
        assert!(!is_canonical_fingerprint(fp));
    }

    #[test]
    fn test_is_canonical_fingerprint_rejects_non_hex() {
        // Replace one hex char with `Z`.
        let fp = "ZA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
             AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
        assert_eq!(fp.len(), 95);
        assert!(!is_canonical_fingerprint(fp));
    }
}
