//! `TrackerServerRegister` handler.
//!
//! The first register on a connection inserts an entry and locks the
//! connection's role to "server"; subsequent messages are refreshes that
//! replace the entry idempotently and reset `last_refresh`. The two paths
//! are separate entry points ([`handle_initial_register`],
//! [`handle_refresh`]) so callers don't handle impossible variants.
//! Shared validation/auth lives in [`validate_and_authenticate`].

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
    MAX_PASSWORD_LENGTH, MAX_PUBLIC_ADDRESS_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH,
    MAX_SERVER_NAME_LENGTH, MAX_VERSION_LENGTH, NormalizedAddress, ServerDescriptionError,
    ServerNameError, VersionError, validate_and_classify_public_address,
    validate_server_description, validate_server_name, validate_version,
};
use nexus_common::{
    ERROR_KIND_CAPACITY, ERROR_KIND_INVALID, ERROR_KIND_RATE_LIMITED, ERROR_KIND_UNAUTHORIZED,
};

use crate::auth::check_password;
use crate::constants::{
    ADDRESS_LOOKUP_TIMEOUT, ERR_REGISTRY_MUTEX_POISONED, LOG_ADDRESS_DNS_TRANSIENT,
    LOG_AUTH_RATE_LIMITED, LOG_REFRESH_GHOST_ID, LOG_REFRESH_TOO_SOON, LOG_REGISTER_NEW,
    LOG_REGISTER_REFRESH, LOG_REGISTER_REJECTED, REASON_ADDRESS_BROADCAST,
    REASON_ADDRESS_DOCUMENTATION, REASON_ADDRESS_HOSTNAME_DNS_FAILED,
    REASON_ADDRESS_HOSTNAME_NO_MATCH, REASON_ADDRESS_HOSTNAME_NOT_FOUND, REASON_ADDRESS_INVALID,
    REASON_ADDRESS_IP_LITERAL_MISMATCH, REASON_ADDRESS_LINK_LOCAL, REASON_ADDRESS_LOOPBACK,
    REASON_ADDRESS_MULTICAST, REASON_ADDRESS_TOO_LONG, REASON_ADDRESS_UNSPECIFIED, REASON_CAPACITY,
    REASON_DESCRIPTION_CONTAINS_NEWLINES, REASON_DESCRIPTION_INVALID_CHARACTERS,
    REASON_DESCRIPTION_TOO_LONG, REASON_FINGERPRINT_INVALID, REASON_NAME_CONTAINS_NEWLINES,
    REASON_NAME_EMPTY, REASON_NAME_INVALID_CHARACTERS, REASON_NAME_TOO_LONG,
    REASON_PASSWORD_TOO_LONG, REASON_PER_IP_CAPACITY, REASON_PORT_ZERO, REASON_RATE_LIMITED,
    REASON_REFRESH_TOO_SOON, REASON_UNAUTHORIZED, REASON_VERSION_INVALID, REASON_VERSION_TOO_LONG,
    REASON_WEBSOCKET_PORT_ZERO,
};
use crate::errors::{
    err_tracker_address_invalid, err_tracker_address_too_long, err_tracker_capacity,
    err_tracker_description_contains_newlines, err_tracker_description_invalid_characters,
    err_tracker_description_too_long, err_tracker_fingerprint_invalid,
    err_tracker_name_contains_newlines, err_tracker_name_empty,
    err_tracker_name_invalid_characters, err_tracker_name_too_long, err_tracker_password_too_long,
    err_tracker_per_ip_capacity, err_tracker_port_zero, err_tracker_rate_limited,
    err_tracker_unauthorized, err_tracker_version_invalid, err_tracker_version_too_long,
    err_tracker_websocket_port_zero,
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

// Manual `Debug` redacts `password` (deriving would leak the plaintext
// to anything that `{:?}`-prints it). The `Some`/`None` distinction is
// preserved so a reader can still tell whether a password was supplied.
impl std::fmt::Debug for RegisterParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisterParams")
            .field("password", &self.password.as_ref().map(|_| "<REDACTED>"))
            .field("locale", &self.locale)
            .field("name", &self.name)
            .field("description", &self.description)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("websocket_port", &self.websocket_port)
            .field("version", &self.version)
            .field("fingerprint", &self.fingerprint)
            .field("user_count", &self.user_count)
            .field("allows_guest", &self.allows_guest)
            .finish()
    }
}

/// First register on a connection (`Initial`) vs a refresh (`Refresh`).
/// Threaded into validation for asymmetric DNS-failure handling: only
/// transient resolver failures differ (see [`transient_outcome`]); both
/// modes still hard-reject NXDOMAIN-equivalents and hostname no-match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterMode {
    Initial,
    Refresh,
}

/// Outcome of [`handle_initial_register`]; the caller decides whether to
/// enter the refresh loop or close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialRegisterOutcome {
    /// Accepted; the id is the registry slot to store for refresh/cleanup.
    Registered(ConnectionId),
    Rejected,
}

/// Outcome of [`handle_refresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Refreshed,
    /// Rejected (connection should close). Refresh-floor violations land
    /// here too — a too-fast refresh is broken/malicious, so we kick.
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

/// Drive the refresh flow on an established server connection. The drop
/// guard keeps `id` alive until disconnect, so a `refresh` returning
/// `false` (id-not-found) is normally unreachable; handled defensively
/// (see [`LOG_REFRESH_GHOST_ID`]) for a future stale-eviction worker.
/// Always sends exactly one `TrackerServerRegisterResponse`.
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

    // Per-entry refresh floor, checked *before* validation/password so a
    // server hammering refreshes can't pin CPU on Argon2. `last_refresh`
    // isn't updated on rejection, so rapid rejected refreshes can't keep
    // an entry alive. Lock released before any `.await` (Send). Zero
    // floor (tests) skips the check.
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
        // The drop guard should keep this entry live until close; if it's
        // gone, something out-of-band evicted it. Close gracefully rather
        // than panic — the guard's own unregister is idempotent.
        warn!(ip = %peer_addr.ip(), id = id, "{}", LOG_REFRESH_GHOST_ID);
        return Ok(RefreshOutcome::Rejected);
    }
    debug!(id = id, user_count = user_count, "{}", LOG_REGISTER_REFRESH);
    send_success(writer, state.refresh_interval).await?;
    Ok(RefreshOutcome::Refreshed)
}

/// Output of the shared validation+auth phase: the validated
/// `ServerEntry` plus `locale` (needed by post-registry capacity
/// rejections at the call sites).
struct Validated {
    entry: ServerEntry,
    locale: String,
}

/// Result of the shared validation+auth phase. On rejection the failure
/// response is already written and logged; the caller just returns.
enum ValidationOutcome {
    Valid(Validated),
    Rejected,
}

/// Run the shared length / fingerprint / password checks, sending a
/// typed failure response on any rejection. Consumes `params` so the
/// field strings move into the resulting `ServerEntry` rather than clone.
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
    // Destructure upfront so fields move into `ServerEntry` without cloning.
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

    // Translate with the request's locale when in bounds, else fall back
    // to DEFAULT_LOCALE. Each field uses the full `nexus_common`
    // validator — length-only checks would pass empty/newline/control input.
    if let Some((reason, message)) = super::validate_locale(&locale) {
        reject(writer, peer_addr, reason, ERROR_KIND_INVALID, message).await?;
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
    if let Err(e) = validate_server_name(&name) {
        let (reason, message) = match e {
            ServerNameError::Empty => (REASON_NAME_EMPTY, err_tracker_name_empty(&locale)),
            ServerNameError::TooLong => (
                REASON_NAME_TOO_LONG,
                err_tracker_name_too_long(&locale, MAX_SERVER_NAME_LENGTH),
            ),
            ServerNameError::ContainsNewlines => (
                REASON_NAME_CONTAINS_NEWLINES,
                err_tracker_name_contains_newlines(&locale),
            ),
            ServerNameError::InvalidCharacters => (
                REASON_NAME_INVALID_CHARACTERS,
                err_tracker_name_invalid_characters(&locale),
            ),
        };
        reject(writer, peer_addr, reason, ERROR_KIND_INVALID, message).await?;
        return Ok(ValidationOutcome::Rejected);
    }
    if let Some(d) = &description
        && let Err(e) = validate_server_description(d)
    {
        let (reason, message) = match e {
            ServerDescriptionError::TooLong => (
                REASON_DESCRIPTION_TOO_LONG,
                err_tracker_description_too_long(&locale, MAX_SERVER_DESCRIPTION_LENGTH),
            ),
            ServerDescriptionError::ContainsNewlines => (
                REASON_DESCRIPTION_CONTAINS_NEWLINES,
                err_tracker_description_contains_newlines(&locale),
            ),
            ServerDescriptionError::InvalidCharacters => (
                REASON_DESCRIPTION_INVALID_CHARACTERS,
                err_tracker_description_invalid_characters(&locale),
            ),
        };
        reject(writer, peer_addr, reason, ERROR_KIND_INVALID, message).await?;
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
    // Full semver-shape validation (distinct length-vs-format reasons).
    // Guarantees every stored entry has a parseable `version`, which the
    // listing-side compat filter relies on.
    if let Err(e) = validate_version(&version) {
        let (reason, message) = match e {
            VersionError::TooLong => (
                REASON_VERSION_TOO_LONG,
                err_tracker_version_too_long(&locale, MAX_VERSION_LENGTH),
            ),
            VersionError::Empty | VersionError::InvalidSemver => {
                (REASON_VERSION_INVALID, err_tracker_version_invalid(&locale))
            }
        };
        reject(writer, peer_addr, reason, ERROR_KIND_INVALID, message).await?;
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
    // Port 0 is unreachable; reject at the boundary to keep listings free
    // of advertisements clients could only fail to connect to.
    if port == 0 {
        reject(
            writer,
            peer_addr,
            REASON_PORT_ZERO,
            ERROR_KIND_INVALID,
            err_tracker_port_zero(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }
    // Optional, but when present must be non-zero like `port`.
    if let Some(ws) = websocket_port
        && ws == 0
    {
        reject(
            writer,
            peer_addr,
            REASON_WEBSOCKET_PORT_ZERO,
            ERROR_KIND_INVALID,
            err_tracker_websocket_port_zero(&locale),
        )
        .await?;
        return Ok(ValidationOutcome::Rejected);
    }

    // Password check when gated. Two-phase rate limiting: peek the
    // auth-failure bucket BEFORE verification, record a failure only on
    // actual mismatch (so correct operators don't burn tokens). Snapshot
    // the hash once so a SIGHUP swap mid-handler can't split decisions.
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

    // Substitute the peer's IP for an omitted/empty `address` (no
    // validation needed); otherwise run `validate_address`. The validated
    // string is stored as-typed to preserve IDN Unicode for display.
    // The substituted IPv6 form is bracket-less (matching the field's
    // bracket-rejecting contract); downstream URI assembly adds `[…]`.
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
/// On failure returns `Err(reason)`, a stable telemetry string for the
/// `LOG_REGISTER_REJECTED` `reason` field. Order:
///
/// 1. **Structural + classify** — `validate_and_classify_public_address`
///    rejects scheme/path/port/zone-id/whitespace and returns a typed
///    [`NormalizedAddress`] (parsed `IpAddr`, or Punycode ASCII for
///    hostnames — the form handed to the resolver in step 5).
/// 2. **Hard-reject** — IP literals in the `classify_invalid` set
///    (loopback / unspecified / link-local / multicast / documentation /
///    broadcast) are never valid public endpoints, regardless of context.
/// 3. **LAN-peer bypass** — a private-network peer (RFC 1918 / ULA /
///    loopback) can't make a public address match its source IP, so the
///    address-vs-peer check is skipped (after hard-reject).
/// 4. **IP literal match** — a public peer requires the literal to equal
///    the peer source IP.
/// 5. **Hostname resolution** — resolve the Punycode form (5s ceiling).
///    Includes-peer accepts; without-peer / NXDOMAIN reject. Transient
///    failures are mode-asymmetric (see [`transient_outcome`]).
async fn validate_address(
    address: &str,
    peer_ip: IpAddr,
    resolver: &dyn Resolver,
    mode: RegisterMode,
) -> Result<(), &'static str> {
    let classified =
        validate_and_classify_public_address(address).map_err(|_| REASON_ADDRESS_INVALID)?;

    match classified {
        NormalizedAddress::Empty => {
            // The caller substitutes the peer IP for empty `address`, so
            // this arm isn't expected to fire. Reject rather than panic.
            Err(REASON_ADDRESS_INVALID)
        }
        NormalizedAddress::Ip(ip) => {
            // Hard-reject invalid categories regardless of peer / bypass.
            if let Some(kind) = common_address::classify_invalid(ip) {
                return Err(invalid_kind_to_reason(kind));
            }
            // LAN-peer bypass.
            if common_address::is_private_network(peer_ip) {
                return Ok(());
            }
            // Public peer: require the literal to match the source IP.
            if ip == peer_ip {
                Ok(())
            } else {
                Err(REASON_ADDRESS_IP_LITERAL_MISMATCH)
            }
        }
        NormalizedAddress::Hostname(host) => {
            // LAN-peer bypass (same rationale as the IP-literal path).
            if common_address::is_private_network(peer_ip) {
                return Ok(());
            }
            // Public peer: resolve `host` (Punycode form) and require the
            // peer in the result set.
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

/// Mode-aware outcome for transient resolver failures. The asymmetry is a
/// deliberate security choice: `Initial` hard-rejects because a "transient"
/// DNS failure on first registration could be an attempt to bypass the
/// hostname-vs-peer check (no prior signal the host maps to this peer).
/// `Refresh` soft-passes — the entry already passed once, and a persistent
/// resolver outage is still caught by the stale-entry sweep.
fn transient_outcome(mode: RegisterMode) -> Result<(), &'static str> {
    match mode {
        RegisterMode::Initial => Err(REASON_ADDRESS_HOSTNAME_DNS_FAILED),
        RegisterMode::Refresh => Ok(()),
    }
}

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

/// Log the rejection (structured `reason`) and send the typed failure
/// `TrackerServerRegisterResponse`.
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
    let response = TrackerServerMessage::TrackerServerRegisterResponse {
        success: false,
        refresh_interval: None,
        error: Some(error_msg),
        error_kind: Some(error_kind.to_string()),
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Unit tests for `validate_address`. These reach what the
    //! integration harness can't: a public peer IP needs a synthetic
    //! `SocketAddr` (the harness is TLS-over-loopback, where the LAN
    //! bypass swallows everything), and `MockResolver` makes DNS
    //! outcomes deterministic.

    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory resolver keyed by `host → result`. Hosts not in the map
    /// default to `NotFound` (NXDOMAIN-equivalent).
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

    /// A "public" peer (8.8.8.8) — avoids the LAN-peer bypass.
    fn public_peer() -> IpAddr {
        ip("8.8.8.8")
    }

    /// `Initial`-mode wrapper around `validate_address`. Mode only
    /// affects the transient-failure branch; tests that care about mode
    /// call `validate_address` directly.
    async fn validate(
        address: &str,
        peer_ip: IpAddr,
        resolver: &dyn Resolver,
    ) -> Result<(), &'static str> {
        validate_address(address, peer_ip, resolver, RegisterMode::Initial).await
    }

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
        // `Unspecified` bucket, same as exact-`0.0.0.0`.
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
        // family but advertises the other is rejected as a literal
        // mismatch. Workaround is hostname registration with A+AAAA (the
        // multi-A path matches whichever family the kernel routed). Pins
        // the current behavior so a quiet relaxation fails the test.
        let r = empty_resolver();
        assert_eq!(
            validate("8.8.8.8", ip("2606:4700::1111"), &r).await,
            Err(REASON_ADDRESS_IP_LITERAL_MISMATCH)
        );
    }

    #[tokio::test]
    async fn yggdrasil_peer_treated_as_public_no_bypass() {
        // Yggdrasil (0200::/7) is "public-within-the-mesh": stable,
        // globally unique, routable, so it gets no LAN-peer bypass. Pins
        // the rule so an `is_private_network` change that includes
        // Yggdrasil fails the test.
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

    /// A resolver that hangs forever, exercising the timeout branch.
    /// Direct `Resolver` impl since `MockResolver` returns synchronously.
    struct HangingResolver;
    #[async_trait]
    impl Resolver for HangingResolver {
        async fn lookup(&self, _host: &str) -> io::Result<Vec<IpAddr>> {
            std::future::pending().await
        }
    }

    /// Run `validate_address` against `HangingResolver` under paused
    /// virtual time. The `select!` guard fails loudly if the 5s timeout
    /// doesn't fire (otherwise the test would hang forever).
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

    #[tokio::test]
    async fn unicode_idn_hostname_resolves_via_punycode() {
        // The Unicode form is what the operator typed; the lookup
        // happens against the Punycode form. Mock keys on Punycode.
        let r = MockResolver::new().with("xn--mnchen-3ya.de", vec![ip("8.8.8.8")]);
        assert!(validate("münchen.de", ip("8.8.8.8"), &r).await.is_ok());
    }
}
