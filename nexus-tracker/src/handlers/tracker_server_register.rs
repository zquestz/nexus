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
//! rate-limit gate, password verification, and (when `address` is
//! provided) the address-validation phase — structural checks, the
//! hard-reject classifier, the LAN-peer bypass, and DNS-vs-peer
//! matching for hostnames — are shared via the private
//! [`validate_and_authenticate`] helper.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use tokio::io::AsyncWrite;
use tracing::{debug, info, warn};

use nexus_common::address as common_address;
use nexus_common::framing::FrameWriter;
use nexus_common::io::send_tracker_server_message;
use nexus_common::tracker_protocol::{ServerEntry, TrackerServerMessage};
use nexus_common::validators::{
    MAX_LOCALE_LENGTH, MAX_PASSWORD_LENGTH, MAX_PUBLIC_ADDRESS_LENGTH,
    MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_NAME_LENGTH, MAX_VERSION_LENGTH, NormalizedAddress,
    validate_and_classify_public_address,
};
use nexus_common::{
    ERROR_KIND_CAPACITY, ERROR_KIND_INVALID, ERROR_KIND_RATE_LIMITED, ERROR_KIND_UNAUTHORIZED,
};

use crate::auth::check_password;
use crate::constants::{
    ADDRESS_LOOKUP_TIMEOUT, DEFAULT_LOCALE, ERR_REGISTRY_MUTEX_POISONED, LOG_ADDRESS_DNS_TRANSIENT,
    LOG_AUTH_RATE_LIMITED, LOG_REFRESH_GHOST_ID, LOG_REFRESH_TOO_SOON, LOG_REGISTER_NEW,
    LOG_REGISTER_REFRESH, LOG_REGISTER_REJECTED, REASON_ADDRESS_BROADCAST,
    REASON_ADDRESS_DOCUMENTATION, REASON_ADDRESS_HOSTNAME_DNS_FAILED,
    REASON_ADDRESS_HOSTNAME_NO_MATCH, REASON_ADDRESS_HOSTNAME_NOT_FOUND, REASON_ADDRESS_INVALID,
    REASON_ADDRESS_IP_LITERAL_MISMATCH, REASON_ADDRESS_LINK_LOCAL, REASON_ADDRESS_LOOPBACK,
    REASON_ADDRESS_MULTICAST, REASON_ADDRESS_TOO_LONG, REASON_ADDRESS_UNSPECIFIED, REASON_CAPACITY,
    REASON_DESCRIPTION_TOO_LONG, REASON_FINGERPRINT_INVALID, REASON_LOCALE_TOO_LONG,
    REASON_NAME_TOO_LONG, REASON_PASSWORD_TOO_LONG, REASON_PER_IP_CAPACITY, REASON_RATE_LIMITED,
    REASON_REFRESH_TOO_SOON, REASON_UNAUTHORIZED, REASON_VERSION_TOO_LONG,
};
use crate::errors::{
    err_tracker_address_invalid, err_tracker_address_too_long, err_tracker_capacity,
    err_tracker_description_too_long, err_tracker_fingerprint_invalid, err_tracker_locale_too_long,
    err_tracker_name_too_long, err_tracker_password_too_long, err_tracker_per_ip_capacity,
    err_tracker_rate_limited, err_tracker_unauthorized, err_tracker_version_too_long,
};
use crate::rate_limiter::RateCheck;
use crate::registry::{ConnectionId, RegisterError};
use crate::resolver::Resolver;
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

/// Whether a `TrackerServerRegister` message is the first one on a
/// fresh connection (`Initial`) or a refresh on an already-registered
/// connection (`Refresh`). Threaded through validation so the address
/// step can apply asymmetric DNS-failure semantics: initial register
/// hard-rejects on transient resolver failures (so a brand-new entry
/// can't slip in unverified during a DNS blip), while refresh
/// soft-passes them (so an established entry isn't evicted by the
/// same blip). Both modes still hard-reject NXDOMAIN-equivalents and
/// hostname-vs-peer no-match — those aren't transient signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterMode {
    Initial,
    Refresh,
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
        match validate_and_authenticate(params, state, writer, peer_addr, RegisterMode::Initial)
            .await?
        {
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
                peer_addr,
                REASON_CAPACITY,
                ERROR_KIND_CAPACITY,
                err_tracker_capacity(&locale),
            )
            .await?;
            Ok(InitialRegisterOutcome::Rejected)
        }
        Err(RegisterError::PerIpCapacity) => {
            reject(
                writer,
                peer_addr,
                REASON_PER_IP_CAPACITY,
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
                peer_addr,
                REASON_REFRESH_TOO_SOON,
                ERROR_KIND_RATE_LIMITED,
                err_tracker_rate_limited(&params.locale),
            )
            .await?;
            return Ok(RefreshOutcome::Rejected);
        }
    }

    let Validated { entry, locale: _ } =
        match validate_and_authenticate(params, state, writer, peer_addr, RegisterMode::Refresh)
            .await?
        {
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
    mode: RegisterMode,
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
            peer_addr,
            REASON_LOCALE_TOO_LONG,
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
            peer_addr,
            REASON_PASSWORD_TOO_LONG,
            ERROR_KIND_INVALID,
            err_tracker_password_too_long(&locale, MAX_PASSWORD_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if name.len() > MAX_SERVER_NAME_LENGTH {
        reject(
            writer,
            peer_addr,
            REASON_NAME_TOO_LONG,
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
            peer_addr,
            REASON_DESCRIPTION_TOO_LONG,
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
            peer_addr,
            REASON_ADDRESS_TOO_LONG,
            ERROR_KIND_INVALID,
            err_tracker_address_too_long(&locale, MAX_PUBLIC_ADDRESS_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if version.len() > MAX_VERSION_LENGTH {
        reject(
            writer,
            peer_addr,
            REASON_VERSION_TOO_LONG,
            ERROR_KIND_INVALID,
            err_tracker_version_too_long(&locale, MAX_VERSION_LENGTH),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if !nexus_common::fingerprint::is_canonical_fingerprint(&fingerprint) {
        reject(
            writer,
            peer_addr,
            REASON_FINGERPRINT_INVALID,
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
            peer_addr,
            REASON_RATE_LIMITED,
            ERROR_KIND_RATE_LIMITED,
            err_tracker_rate_limited(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if !check_password(password.as_deref(), stored_hash.as_deref()).await {
        if gated {
            state
                .auth_failure_rate_limiter
                .record_failure(peer_addr.ip());
        }
        reject(
            writer,
            peer_addr,
            REASON_UNAUTHORIZED,
            ERROR_KIND_UNAUTHORIZED,
            err_tracker_unauthorized(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }

    // Resolve `address` — substitute the peer's IP when the field is
    // omitted or empty (kernel-supplied, no validation needed).
    // Otherwise run the address-validation phase: structural checks
    // (hard-reject categories), peer-IP match for IP literals, and
    // DNS-vs-peer-IP match for hostnames. The validated string is
    // stored as-typed to preserve IDN Unicode form for display.
    let resolved_address = match address {
        Some(a) if !a.is_empty() => {
            if let Err(reason) =
                validate_address(&a, peer_addr.ip(), state.resolver.as_ref(), mode).await
            {
                reject(
                    writer,
                    peer_addr,
                    reason,
                    ERROR_KIND_INVALID,
                    err_tracker_address_invalid(&locale),
                )
                .await?;
                return Ok(ValidationOutcome::Rejected);
            }
            a
        }
        _ => peer_addr.ip().to_string(),
    };

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

/// Validate an operator-supplied `address` against the peer's source IP.
///
/// On success, returns `Ok(())` and the caller stores the address
/// as-typed (preserving any IDN Unicode form). On failure, returns
/// `Err(reason)` where `reason` is a stable telemetry string suitable
/// for the `LOG_REGISTER_REJECTED` `reason` field.
///
/// The order is:
///
/// 1. **Structural + classify** — `validate_and_classify_public_address`
///    rejects scheme/path/port/zone-id/whitespace, verifies IDNA
///    well-formedness, and returns a typed [`NormalizedAddress`]: a
///    parsed `IpAddr` for IP literals, the Punycode ASCII form for
///    hostnames. The Punycode form is what we hand to the resolver in
///    step 5; we don't run `idna::domain_to_ascii_strict` separately,
///    and we don't reparse the canonical IP-literal string.
/// 2. **Hard-reject classification** — when the address is an IP
///    literal, the `classify_invalid` set rejects loopback / unspecified
///    / link-local / multicast / documentation / broadcast regardless
///    of context. These categories are never valid public unicast
///    endpoints.
/// 3. **LAN-peer bypass** — a peer on a private network (RFC 1918 /
///    ULA / loopback) bypasses the address-vs-peer match check. Local
///    DNS may not resolve advertised hostnames; an operator on a LAN
///    can't make a public address match their RFC 1918 source IP. The
///    address is accepted as-is once it's passed the hard-reject
///    classification.
/// 4. **IP literal match** — a public peer requires the IP literal to
///    equal the peer source IP.
/// 5. **Hostname resolution** — resolve the normalized Punycode form
///    with a 5s ceiling. Resolved-and-includes-peer accepts;
///    resolved-without-peer rejects as a no-match; NXDOMAIN-equivalents
///    always reject. Transient resolver failures (timeout, network
///    error) are mode-asymmetric: `Initial` rejects (so a brand-new
///    entry can't slip in unverified during a DNS blip), `Refresh`
///    soft-passes (so an established entry isn't evicted by the same
///    blip).
async fn validate_address(
    address: &str,
    peer_ip: IpAddr,
    resolver: &dyn Resolver,
    mode: RegisterMode,
) -> Result<(), &'static str> {
    // Validate + classify in one pass. The typed enum preserves the
    // kind (IP literal vs hostname) and the parsed `IpAddr`, so we
    // don't reparse the canonical string later.
    let classified =
        validate_and_classify_public_address(address).map_err(|_| REASON_ADDRESS_INVALID)?;

    match classified {
        NormalizedAddress::Empty => {
            // The caller substitutes the peer IP for empty `address`
            // before entering this function, so this arm isn't expected
            // to fire here. Treat as a structural rejection rather than
            // panicking — a future caller skipping the empty-substitute
            // guard shouldn't crash the daemon.
            Err(REASON_ADDRESS_INVALID)
        }
        NormalizedAddress::Ip(ip) => {
            // Hard-reject invalid IP categories regardless of peer / bypass.
            if let Some(kind) = common_address::classify_invalid(ip) {
                return Err(invalid_kind_to_reason(kind));
            }
            // LAN-peer bypass.
            if common_address::is_private_network(peer_ip) {
                return Ok(());
            }
            // Public peer with an IP literal: require match.
            if ip == peer_ip {
                Ok(())
            } else {
                Err(REASON_ADDRESS_IP_LITERAL_MISMATCH)
            }
        }
        NormalizedAddress::Hostname(host) => {
            // LAN-peer bypass: same as the IP-literal path. A LAN-coresident
            // operator can't generally make a public hostname resolve to
            // their RFC 1918 source IP, so we accept the hostname as-is.
            if common_address::is_private_network(peer_ip) {
                return Ok(());
            }
            // Public peer with a hostname: resolve and look for peer in
            // the result set. `host` is the Punycode ASCII form per the
            // validator's contract.
            match tokio::time::timeout(ADDRESS_LOOKUP_TIMEOUT, resolver.lookup(&host)).await {
                Err(_elapsed) => {
                    warn!(ip = %peer_ip, host = %host, "{}", LOG_ADDRESS_DNS_TRANSIENT);
                    transient_outcome(mode)
                }
                Ok(Err(e)) if e.kind() == io::ErrorKind::NotFound => {
                    Err(REASON_ADDRESS_HOSTNAME_NOT_FOUND)
                }
                Ok(Err(e)) => {
                    warn!(ip = %peer_ip, host = %host, err = %e, "{}", LOG_ADDRESS_DNS_TRANSIENT);
                    transient_outcome(mode)
                }
                Ok(Ok(ips)) if ips.is_empty() => Err(REASON_ADDRESS_HOSTNAME_NOT_FOUND),
                Ok(Ok(ips)) if ips.contains(&peer_ip) => Ok(()),
                Ok(Ok(_)) => Err(REASON_ADDRESS_HOSTNAME_NO_MATCH),
            }
        }
    }
}

/// Mode-aware outcome for transient resolver failures. Initial register
/// hard-rejects to keep unverified entries out; refresh soft-passes so
/// an already-validated entry survives a brief DNS blip.
fn transient_outcome(mode: RegisterMode) -> Result<(), &'static str> {
    match mode {
        RegisterMode::Initial => Err(REASON_ADDRESS_HOSTNAME_DNS_FAILED),
        RegisterMode::Refresh => Ok(()),
    }
}

/// Map an `InvalidAddressKind` to its telemetry-reason string.
fn invalid_kind_to_reason(kind: common_address::InvalidAddressKind) -> &'static str {
    match kind {
        common_address::InvalidAddressKind::Loopback => REASON_ADDRESS_LOOPBACK,
        common_address::InvalidAddressKind::Unspecified => REASON_ADDRESS_UNSPECIFIED,
        common_address::InvalidAddressKind::LinkLocal => REASON_ADDRESS_LINK_LOCAL,
        common_address::InvalidAddressKind::Multicast => REASON_ADDRESS_MULTICAST,
        common_address::InvalidAddressKind::Documentation => REASON_ADDRESS_DOCUMENTATION,
        common_address::InvalidAddressKind::Broadcast => REASON_ADDRESS_BROADCAST,
    }
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
    peer_addr: SocketAddr,
    reason: &str,
    error_kind: &str,
    error_msg: String,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    warn!(ip = %peer_addr.ip(), reason = %reason, "{}", LOG_REGISTER_REJECTED);
    send_failure(writer, error_kind, error_msg).await
}

#[cfg(test)]
mod tests {
    //! Unit tests for `validate_address`.
    //!
    //! These exercise the parts of the address-validation phase that
    //! the integration harness can't reach: a public peer IP requires
    //! a synthetic `SocketAddr`, since the harness only supports
    //! TLS-over-loopback (where the LAN bypass swallows everything).
    //! Hostname tests use `MockResolver` so DNS outcomes (NXDOMAIN,
    //! peer-in-set, peer-not-in-set, transient failure) are
    //! deterministic.

    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory resolver. Construct with a map of `host → result` and
    /// pass to `validate_address`. Hosts not in the map default to
    /// `io::ErrorKind::NotFound` (NXDOMAIN-equivalent), which lets
    /// tests assert on the "host wasn't even registered" path without
    /// special-casing it.
    struct MockResolver {
        responses: Mutex<HashMap<String, io::Result<Vec<IpAddr>>>>,
    }

    impl MockResolver {
        fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
            }
        }

        fn with(mut self, host: &str, ips: Vec<IpAddr>) -> Self {
            self.responses
                .get_mut()
                .expect("mock resolver mutex")
                .insert(host.to_string(), Ok(ips));
            self
        }

        fn with_error(mut self, host: &str, err: io::Error) -> Self {
            self.responses
                .get_mut()
                .expect("mock resolver mutex")
                .insert(host.to_string(), Err(err));
            self
        }
    }

    #[async_trait]
    impl Resolver for MockResolver {
        async fn lookup(&self, host: &str) -> io::Result<Vec<IpAddr>> {
            // Clone out of the map under lock, then return. The trait
            // is `&self` so we can't move; cloning IPs is cheap.
            let map = self.responses.lock().expect("mock resolver mutex");
            match map.get(host) {
                Some(Ok(ips)) => Ok(ips.clone()),
                Some(Err(e)) => Err(io::Error::new(e.kind(), e.to_string())),
                None => Err(io::Error::new(io::ErrorKind::NotFound, "not in map")),
            }
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("valid ip")
    }

    fn empty_resolver() -> MockResolver {
        MockResolver::new()
    }

    /// A "public" peer address (8.8.8.8) used everywhere we need to
    /// avoid the LAN-peer bypass.
    fn public_peer() -> IpAddr {
        ip("8.8.8.8")
    }

    /// Default-mode wrapper around `validate_address`. Mode only
    /// affects the transient-resolver-failure branch; tests that
    /// don't exercise that branch use this to keep the call sites
    /// terse. Tests that *do* care about mode call `validate_address`
    /// directly with an explicit `RegisterMode`.
    async fn validate(
        address: &str,
        peer_ip: IpAddr,
        resolver: &dyn Resolver,
    ) -> Result<(), &'static str> {
        validate_address(address, peer_ip, resolver, RegisterMode::Initial).await
    }

    // ---- Structural rejection ----

    #[tokio::test]
    async fn rejects_malformed_address() {
        let r = empty_resolver();
        // `validate_public_address` rejects embedded ports.
        assert_eq!(
            validate("example.com:7500", public_peer(), &r).await,
            Err(REASON_ADDRESS_INVALID)
        );
        // ...and schemes.
        assert_eq!(
            validate("https://example.com", public_peer(), &r).await,
            Err(REASON_ADDRESS_INVALID)
        );
    }

    // ---- Hard-reject categories (regardless of peer / bypass) ----

    #[tokio::test]
    async fn rejects_loopback_literal_from_lan_peer() {
        // Hard-reject runs before the LAN bypass, so even a private
        // peer can't register `127.0.0.1` as the advertised address.
        let r = empty_resolver();
        assert_eq!(
            validate("127.0.0.1", ip("192.168.1.5"), &r).await,
            Err(REASON_ADDRESS_LOOPBACK)
        );
        assert_eq!(
            validate("::1", ip("192.168.1.5"), &r).await,
            Err(REASON_ADDRESS_LOOPBACK)
        );
    }

    #[tokio::test]
    async fn rejects_unspecified_literal() {
        let r = empty_resolver();
        assert_eq!(
            validate("0.0.0.0", public_peer(), &r).await,
            Err(REASON_ADDRESS_UNSPECIFIED)
        );
        assert_eq!(
            validate("::", public_peer(), &r).await,
            Err(REASON_ADDRESS_UNSPECIFIED)
        );
    }

    #[tokio::test]
    async fn rejects_link_local_literal() {
        let r = empty_resolver();
        assert_eq!(
            validate("169.254.1.1", public_peer(), &r).await,
            Err(REASON_ADDRESS_LINK_LOCAL)
        );
        assert_eq!(
            validate("fe80::1", public_peer(), &r).await,
            Err(REASON_ADDRESS_LINK_LOCAL)
        );
    }

    #[tokio::test]
    async fn rejects_multicast_literal() {
        let r = empty_resolver();
        assert_eq!(
            validate("224.0.0.1", public_peer(), &r).await,
            Err(REASON_ADDRESS_MULTICAST)
        );
        assert_eq!(
            validate("ff00::1", public_peer(), &r).await,
            Err(REASON_ADDRESS_MULTICAST)
        );
    }

    #[tokio::test]
    async fn rejects_broadcast_literal() {
        let r = empty_resolver();
        assert_eq!(
            validate("255.255.255.255", public_peer(), &r).await,
            Err(REASON_ADDRESS_BROADCAST)
        );
    }

    #[tokio::test]
    async fn rejects_this_network_literal() {
        // 0.0.0.0/8 ("this network", RFC 1122) maps to the
        // `Unspecified` reason — the same telemetry bucket as the
        // exact-`0.0.0.0` case, since they're morphologically the
        // same family.
        let r = empty_resolver();
        assert_eq!(
            validate("0.1.2.3", public_peer(), &r).await,
            Err(REASON_ADDRESS_UNSPECIFIED)
        );
    }

    #[tokio::test]
    async fn rejects_documentation_literal() {
        let r = empty_resolver();
        assert_eq!(
            validate("192.0.2.1", public_peer(), &r).await,
            Err(REASON_ADDRESS_DOCUMENTATION)
        );
        assert_eq!(
            validate("198.51.100.42", public_peer(), &r).await,
            Err(REASON_ADDRESS_DOCUMENTATION)
        );
        assert_eq!(
            validate("203.0.113.7", public_peer(), &r).await,
            Err(REASON_ADDRESS_DOCUMENTATION)
        );
        assert_eq!(
            validate("2001:db8::1", public_peer(), &r).await,
            Err(REASON_ADDRESS_DOCUMENTATION)
        );
    }

    // ---- LAN-peer bypass ----

    #[tokio::test]
    async fn lan_peer_bypass_accepts_arbitrary_public_ip() {
        // Peer is on a private network, address is a public IP that
        // doesn't match. Bypass kicks in regardless.
        let r = empty_resolver();
        assert!(validate("8.8.8.8", ip("192.168.1.5"), &r).await.is_ok());
        assert!(validate("8.8.8.8", ip("10.0.0.5"), &r).await.is_ok());
        assert!(validate("8.8.8.8", ip("172.20.5.5"), &r).await.is_ok());
        assert!(validate("8.8.8.8", ip("::1"), &r).await.is_ok());
        assert!(validate("8.8.8.8", ip("fc00::1"), &r).await.is_ok());
    }

    #[tokio::test]
    async fn lan_peer_bypass_accepts_unresolvable_hostname() {
        // Bypass also applies to hostnames — local DNS may not resolve
        // the advertised name, and that's not a registration concern.
        // The resolver is never consulted on the bypass path.
        let r = empty_resolver();
        assert!(
            validate("bbs.example.com", ip("192.168.1.5"), &r)
                .await
                .is_ok()
        );
    }

    // ---- Public peer IP literal match ----

    #[tokio::test]
    async fn public_peer_ip_literal_match_accepts() {
        let r = empty_resolver();
        assert!(validate("8.8.8.8", ip("8.8.8.8"), &r).await.is_ok());
        assert!(
            validate("2606:4700::1111", ip("2606:4700::1111"), &r)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn public_peer_ip_literal_mismatch_rejects() {
        let r = empty_resolver();
        assert_eq!(
            validate("1.1.1.1", ip("8.8.8.8"), &r).await,
            Err(REASON_ADDRESS_IP_LITERAL_MISMATCH)
        );
    }

    #[tokio::test]
    async fn public_peer_dual_stack_literal_mismatch_rejects() {
        // Known limitation: a dual-stack operator who connects via one
        // address family but advertises the other (e.g., the kernel
        // routed the tracker connection over IPv6, but the operator
        // wants to advertise their IPv4 address for IPv4-only clients)
        // is rejected as a literal mismatch. The supported workaround
        // is hostname-based registration with both A and AAAA records
        // — the multi-A path matches whichever address family the
        // kernel routed. This test asserts the current rejecting
        // behavior so a future change that quietly relaxes the rule
        // shows up as a test failure rather than a silent semantics
        // drift.
        let r = empty_resolver();
        assert_eq!(
            validate("8.8.8.8", ip("2606:4700::1111"), &r).await,
            Err(REASON_ADDRESS_IP_LITERAL_MISMATCH)
        );
    }

    // ---- Yggdrasil peer (treated as public) ----

    #[tokio::test]
    async fn yggdrasil_peer_treated_as_public_no_bypass() {
        // Yggdrasil mesh addresses (0200::/7) are "public-within-the-
        // mesh" by design — they're stable, globally unique, and
        // routable, so they don't get the LAN-peer bypass that
        // RFC 1918 / ULA / loopback peers do. A Yggdrasil-only
        // operator must register either their own Yggdrasil IP
        // literal (peer match) or a hostname that resolves to it
        // (mesh-aware resolver required). This test pins the rule so
        // a future change to `is_private_network` that quietly
        // includes Yggdrasil shows up as a failure.
        let r = empty_resolver();
        let yggdrasil_peer = ip("0210:abcd:1234::5");
        // No-bypass: arbitrary public IP must mismatch.
        assert_eq!(
            validate("8.8.8.8", yggdrasil_peer, &r).await,
            Err(REASON_ADDRESS_IP_LITERAL_MISMATCH)
        );
        // Direct match: registering one's own Yggdrasil IP literal accepts.
        assert!(
            validate("0210:abcd:1234::5", yggdrasil_peer, &r)
                .await
                .is_ok()
        );
    }

    // ---- Public peer hostname resolution ----

    #[tokio::test]
    async fn public_peer_hostname_resolves_to_peer_accepts() {
        let r = MockResolver::new().with("bbs.example.com", vec![ip("8.8.8.8")]);
        assert!(validate("bbs.example.com", ip("8.8.8.8"), &r).await.is_ok());
    }

    #[tokio::test]
    async fn public_peer_hostname_resolves_to_set_containing_peer_accepts() {
        // Multi-A-record host: peer is one of several. Accept.
        let r = MockResolver::new().with(
            "bbs.example.com",
            vec![ip("203.0.113.5"), ip("8.8.8.8"), ip("1.2.3.4")],
        );
        assert!(validate("bbs.example.com", ip("8.8.8.8"), &r).await.is_ok());
    }

    #[tokio::test]
    async fn public_peer_hostname_resolves_without_peer_rejects() {
        let r = MockResolver::new().with("bbs.example.com", vec![ip("1.2.3.4")]);
        assert_eq!(
            validate("bbs.example.com", ip("8.8.8.8"), &r).await,
            Err(REASON_ADDRESS_HOSTNAME_NO_MATCH)
        );
    }

    #[tokio::test]
    async fn public_peer_hostname_nxdomain_rejects() {
        // Host not in mock map → NotFound-equivalent → reject.
        let r = empty_resolver();
        assert_eq!(
            validate("missing.example", ip("8.8.8.8"), &r).await,
            Err(REASON_ADDRESS_HOSTNAME_NOT_FOUND)
        );
    }

    #[tokio::test]
    async fn public_peer_hostname_empty_resolution_rejects() {
        // Resolver returns Ok(empty) — treat as NXDOMAIN-equivalent.
        let r = MockResolver::new().with("bbs.example.com", vec![]);
        assert_eq!(
            validate("bbs.example.com", ip("8.8.8.8"), &r).await,
            Err(REASON_ADDRESS_HOSTNAME_NOT_FOUND)
        );
    }

    fn transient_resolver() -> MockResolver {
        MockResolver::new().with_error(
            "bbs.example.com",
            io::Error::new(io::ErrorKind::ConnectionRefused, "dns unreachable"),
        )
    }

    #[tokio::test]
    async fn public_peer_hostname_transient_error_initial_rejects() {
        // ConnectionRefused (resolver dead) is transient. Initial
        // register hard-rejects so a brand-new entry can't slip in
        // unverified during a DNS blip.
        let r = transient_resolver();
        assert_eq!(
            validate_address("bbs.example.com", ip("8.8.8.8"), &r, RegisterMode::Initial).await,
            Err(REASON_ADDRESS_HOSTNAME_DNS_FAILED)
        );
    }

    #[tokio::test]
    async fn public_peer_hostname_transient_error_refresh_softpasses() {
        // Same resolver state, refresh mode: an established entry
        // shouldn't be evicted by a transient failure.
        let r = transient_resolver();
        assert!(
            validate_address("bbs.example.com", ip("8.8.8.8"), &r, RegisterMode::Refresh)
                .await
                .is_ok(),
            "transient resolver failure on refresh should soft-pass"
        );
    }

    /// A resolver that hangs forever — the timeout branch is the
    /// thing we're testing. Implements `Resolver` directly rather than
    /// extending `MockResolver`, since the latter returns synchronously.
    struct HangingResolver;
    #[async_trait]
    impl Resolver for HangingResolver {
        async fn lookup(&self, _host: &str) -> io::Result<Vec<IpAddr>> {
            std::future::pending().await
        }
    }

    /// Wait for `validate_address` against `HangingResolver` to elapse
    /// under paused virtual time. Returns the validation result. The
    /// `tokio::select!` outer guard exists to fail loudly if the
    /// 5s timeout doesn't fire (would otherwise hang the test forever).
    async fn validate_with_hanging_resolver(mode: RegisterMode) -> Result<(), &'static str> {
        let r = HangingResolver;
        tokio::select! {
            res = validate_address("bbs.example.com", ip("8.8.8.8"), &r, mode) => res,
            () = tokio::time::sleep(ADDRESS_LOOKUP_TIMEOUT + std::time::Duration::from_secs(1)) =>
                panic!("validate_address didn't return within timeout window"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn public_peer_hostname_timeout_initial_rejects() {
        // `start_paused = true` lets us advance virtual time past
        // ADDRESS_LOOKUP_TIMEOUT instantly.
        assert_eq!(
            validate_with_hanging_resolver(RegisterMode::Initial).await,
            Err(REASON_ADDRESS_HOSTNAME_DNS_FAILED)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn public_peer_hostname_timeout_refresh_softpasses() {
        assert!(
            validate_with_hanging_resolver(RegisterMode::Refresh)
                .await
                .is_ok(),
            "timeout on refresh should soft-pass"
        );
    }

    // ---- IDN-encoded hostname ----

    #[tokio::test]
    async fn unicode_idn_hostname_resolves_via_punycode() {
        // The Unicode form is what the operator typed; the lookup
        // happens against the Punycode form. Mock keys on Punycode.
        let r = MockResolver::new().with("xn--mnchen-3ya.de", vec![ip("8.8.8.8")]);
        assert!(validate("münchen.de", ip("8.8.8.8"), &r).await.is_ok());
    }
}
