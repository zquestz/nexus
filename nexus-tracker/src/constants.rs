//! Tracker constants

pub const DATA_DIR_NAME: &str = "nexus-trackerd";

/// Daemon-specific prefix passed to the shared logging stack for daily
/// rotation (e.g. `nexus-trackerd.2025-04-28`).
pub const LOG_FILE_PREFIX: &str = "nexus-trackerd";

pub const MSG_BANNER: &str = "Nexus Tracker v";
pub const MSG_LOG_LEVEL: &str = "Log level: ";
pub const MSG_LOG_DIR: &str = "Log directory: ";

pub const ERR_NO_DATA_DIR: &str = "Platform does not provide a data directory";
pub const ERR_DATA_DIR_NOT_ABSOLUTE: &str = "--data-dir must be an absolute path: ";

/// Tracing subscriber init failed; daemon falls back to stderr-only output.
pub const LOG_LOGGING_INIT_FAILED: &str = "Logging init failed";

pub const ERR_CREATE_DATA_DIR: &str = "Failed to create data directory: ";

#[cfg(unix)]
pub const ERR_SET_DATA_DIR_PERMS: &str = "Failed to set data directory permissions: ";

/// Presence of this file gates `TrackerServerRegister`; absence means open registration.
pub const REGISTRATION_HASH_FILENAME: &str = "registration.hash";

/// Presence of this file gates `TrackerServerList`; absence means open listing.
pub const LISTING_HASH_FILENAME: &str = "listing.hash";

pub const ERR_HASH_PASSWORD: &str = "Failed to hash password: ";
pub const ERR_WRITE_PASSWORD_FILE: &str = "Failed to write password file: ";
pub const ERR_READ_PASSWORD_FILE: &str = "Failed to read password file: ";
pub const ERR_DELETE_PASSWORD_FILE: &str = "Failed to delete password file: ";
pub const ERR_PROMPT_PASSWORD: &str = "Failed to prompt for password: ";
pub const ERR_READ_STDIN: &str = "Failed to read password from stdin: ";

pub const ERR_PASSWORD_EMPTY: &str =
    "Password cannot be empty (use `clear-password` to disable gating)";

pub const ERR_PASSWORD_TOO_LONG: &str = "Password exceeds maximum length of ";
pub const ERR_PASSWORD_MISMATCH: &str = "Passwords do not match";
pub const ERR_PARSE_PASSWORD_HASH: &str = "Failed to parse stored password hash: ";

pub const LOG_PASSWORD_SETTING: &str = "Setting password";
pub const LOG_PASSWORD_SET: &str = "Password set";
pub const LOG_PASSWORD_CLEARED: &str = "Password cleared";
pub const LOG_PASSWORD_NOT_PRESENT: &str = "No password configured";

#[cfg(unix)]
pub const LOG_SIGHUP_RECEIVED: &str = "SIGHUP received; reloading passwords";

pub const LOG_PASSWORD_RELOADED: &str = "Password reloaded";

/// Reload of one password kind failed; previous in-memory state is
/// preserved — the daemon does not crash on a typo in a hash file.
pub const LOG_PASSWORD_RELOAD_FAILED: &str = "Password reload failed; previous state preserved";

pub const LABEL_REGISTRATION: &str = "Registration";
pub const LABEL_LISTING: &str = "Listing";

/// Auth-flow status: open (no hash file present, password not required).
pub const STATUS_OPEN: &str = "open";

/// Auth-flow status: gated (hash file present, password required).
pub const STATUS_GATED: &str = "gated";

pub const PROMPT_NEW_PASSWORD: &str = "New password: ";
pub const PROMPT_CONFIRM_PASSWORD: &str = "Confirm password: ";

pub const CERT_FILENAME: &str = "tracker.crt";
pub const KEY_FILENAME: &str = "tracker.key";
pub const TLS_CERT_COMMON_NAME: &str = "Nexus Tracker";
pub const MSG_CERTIFICATES: &str = "Certificates: ";

/// Panics — rustls crypto provider is required for any TLS operation.
pub const ERR_RUSTLS_PROVIDER: &str = "failed to install rustls crypto provider";

/// Re-exported from `nexus-common` so the value is defined once for the workspace.
pub use nexus_common::DEFAULT_LOCALE;

// Supported locale codes. Generic codes (`pt`, `zh`) normalize to a
// regional variant in `i18n::get_bundle`.
pub const LOCALE_SPANISH: &str = "es";
pub const LOCALE_JAPANESE: &str = "ja";
pub const LOCALE_FRENCH: &str = "fr";
pub const LOCALE_GERMAN: &str = "de";
pub const LOCALE_PORTUGUESE: &str = "pt";
pub const LOCALE_PORTUGUESE_PT: &str = "pt-PT";
pub const LOCALE_PORTUGUESE_BR: &str = "pt-BR";
pub const LOCALE_RUSSIAN: &str = "ru";
pub const LOCALE_CHINESE: &str = "zh";
pub const LOCALE_CHINESE_CN: &str = "zh-CN";
pub const LOCALE_CHINESE_TW: &str = "zh-TW";
pub const LOCALE_KOREAN: &str = "ko";
pub const LOCALE_ITALIAN: &str = "it";
pub const LOCALE_DUTCH: &str = "nl";

/// Panics — a malformed `errors.ftl` baked into the binary is a build error,
/// not operator-actionable.
pub const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";
pub const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";

/// Panics — programming error: a call site referenced a key with no `errors.ftl` entry.
pub const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

/// Panics — `DEFAULT_LOCALE` (`"en"`) failed to parse; hand-edited to be valid.
pub const ERR_DEFAULT_LOCALE_INVALID: &str = "DEFAULT_LOCALE is a valid locale";

pub const LOG_TRANSLATION_ERRORS: &str = "Translation errors";
pub const LOG_MISSING_TRANSLATION_KEY: &str = "Missing translation key";

pub const MSG_LISTENING: &str = "Tracker port: ";

/// Only emitted when `--websocket` is enabled.
pub const MSG_WS_LISTENING: &str = "WebSocket tracker port: ";

pub const MSG_SHUTDOWN_RECEIVED: &str = "Shutdown signal received";
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";

/// Spec §Timeouts: TLS accepted, awaiting Handshake — 30 seconds.
pub const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Max wait for the first role-establishing message (`TrackerServerRegister`
/// or `TrackerServerList`) after handshake. Spec §Timeouts: 30 seconds.
pub const ROLE_ESTABLISH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Max interval without write progress while sending a tracker response.
/// The response is written in chunks so slow readers are allowed as long
/// as each chunk can drain within this window.
pub const TRACKER_WRITE_PROGRESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Chunk size for progress-bounded tracker response writes.
pub const TRACKER_WRITE_CHUNK_SIZE: usize = 16 * 1024;

pub const ERR_TRACKER_WRITE_TIMEOUT: &str = "Tracker response write timed out";
pub const ERR_TRACKER_WRITE_ZERO: &str = "Tracker response write returned zero bytes";

pub const LOG_ACCEPT_ERROR: &str = "Accept error";

/// Connection error after TLS (frame, JSON, or unexpected disconnect).
pub const LOG_CONNECTION_ERROR: &str = "Connection error";

pub const LOG_CONNECTION_ERROR_TLS: &str = "Connection error (TLS handshake)";
pub const LOG_CONNECTION_ERROR_WS: &str = "Connection error (WebSocket handshake)";

/// Peer sent a non-Handshake message before completing the handshake.
pub const LOG_HANDSHAKE_REQUIRED: &str = "Handshake: required";

pub const LOG_HANDSHAKE_MAJOR_MISMATCH: &str = "Handshake: major version mismatch";
pub const LOG_HANDSHAKE_MINOR_MISMATCH: &str = "Handshake: minor version mismatch";
pub const LOG_HANDSHAKE_CLIENT_TOO_NEW: &str = "Handshake: client too new";

pub const LOG_REGISTER_NEW: &str = "TrackerServerRegister: new entry";

/// Logged at **debug** (vs info for `LOG_REGISTER_NEW`): refreshes fire every
/// `refresh_interval` per entry and would dominate operator logs at fleet
/// scale if elevated.
pub const LOG_REGISTER_REFRESH: &str = "TrackerServerRegister: refresh";

pub const LOG_REGISTER_REJECTED: &str = "TrackerServerRegister: rejected";

// Address-validation rejection reasons (the `reason` field on
// `LOG_REGISTER_REJECTED`). All share the single operator-facing
// `err_tracker_address_invalid` i18n string; this per-reason granularity is
// for operator log analysis (which check failed), not for the registrant.

pub const REASON_ADDRESS_INVALID: &str = "address_invalid";
pub const REASON_ADDRESS_LOOPBACK: &str = "address_loopback";

/// Unspecified IP (`0.0.0.0` / `::`) or the broader `0.0.0.0/8` range
/// (RFC 1122 §3.2.1.3) — both bucket here.
pub const REASON_ADDRESS_UNSPECIFIED: &str = "address_unspecified";

pub const REASON_ADDRESS_LINK_LOCAL: &str = "address_link_local";
pub const REASON_ADDRESS_MULTICAST: &str = "address_multicast";
pub const REASON_ADDRESS_DOCUMENTATION: &str = "address_documentation";
pub const REASON_ADDRESS_BROADCAST: &str = "address_broadcast";

/// IP literal that didn't match the peer IP, and peer wasn't on a private
/// network (LAN bypass didn't apply).
pub const REASON_ADDRESS_IP_LITERAL_MISMATCH: &str = "address_ip_literal_mismatch";

/// Hostname resolved to zero IPs (NXDOMAIN-equivalent).
pub const REASON_ADDRESS_HOSTNAME_NOT_FOUND: &str = "address_hostname_not_found";

/// Hostname resolved but none of the IPs matched the peer source IP.
pub const REASON_ADDRESS_HOSTNAME_NO_MATCH: &str = "address_hostname_no_match";

/// Transient resolver failure (timeout, network). Distinct from NXDOMAIN so
/// operators can tell DNS blips from "host doesn't exist." Only emitted on
/// initial register; refresh soft-passes so an established entry survives a blip.
pub const REASON_ADDRESS_HOSTNAME_DNS_FAILED: &str = "address_hostname_dns_failed";

// Non-address rejection reasons — the `reason` field on
// `LOG_REGISTER_REJECTED` / `LOG_LIST_REJECTED`. Values may equal an
// `ERROR_KIND_*` string but are separate constants so the log-side namespace
// stays distinct from the wire-side one.

pub const REASON_CAPACITY: &str = "capacity";
pub const REASON_PER_IP_CAPACITY: &str = "per_ip_capacity";

/// Refresh arrived before the per-entry refresh floor elapsed (defense against
/// a compromised tracker asking for floods).
pub const REASON_REFRESH_TOO_SOON: &str = "refresh_too_soon";

/// Refresh targeted an entry id that is no longer live.
pub const REASON_REFRESH_GHOST_ID: &str = "refresh_unknown_id";

pub const REASON_LOCALE_TOO_LONG: &str = "locale_too_long";
pub const REASON_LOCALE_INVALID: &str = "locale_invalid";
pub const REASON_PASSWORD_TOO_LONG: &str = "password_too_long";
pub const REASON_NAME_TOO_LONG: &str = "name_too_long";
pub const REASON_NAME_EMPTY: &str = "name_empty";
pub const REASON_NAME_CONTAINS_NEWLINES: &str = "name_contains_newlines";
pub const REASON_NAME_INVALID_CHARACTERS: &str = "name_invalid_characters";
pub const REASON_DESCRIPTION_TOO_LONG: &str = "description_too_long";
pub const REASON_DESCRIPTION_CONTAINS_NEWLINES: &str = "description_contains_newlines";
pub const REASON_DESCRIPTION_INVALID_CHARACTERS: &str = "description_invalid_characters";
pub const REASON_ADDRESS_TOO_LONG: &str = "address_too_long";
pub const REASON_VERSION_TOO_LONG: &str = "version_too_long";

/// `version` empty or not semver. Distinct from `REASON_VERSION_TOO_LONG` so
/// logs separate length from format failures.
pub const REASON_VERSION_INVALID: &str = "version_invalid";

/// `port` was zero. Port 0 is reserved / unreachable; rejecting at the boundary
/// keeps listings free of dead advertisements.
pub const REASON_PORT_ZERO: &str = "port_zero";

/// `websocket_port` was zero — same rationale as `REASON_PORT_ZERO`, distinct
/// constant so logs identify which port failed.
pub const REASON_WEBSOCKET_PORT_ZERO: &str = "websocket_port_zero";

pub const REASON_FINGERPRINT_INVALID: &str = "fingerprint_invalid";
pub const REASON_RATE_LIMITED: &str = "rate_limited";

/// Password (registration or listing) didn't verify against the stored hash.
pub const REASON_UNAUTHORIZED: &str = "unauthorized";

// Disconnect-cause values for `LOG_REGISTER_DISCONNECTED` — why an established
// server connection dropped (after a successful registration), distinct from
// rejection reasons (which fire instead of registration).

pub const REASON_DISCONNECT_CLEAN_CLOSE: &str = "clean_close";

/// Stale-timeout fired (no refresh within 2× refresh_interval).
pub const REASON_DISCONNECT_STALE_TIMEOUT: &str = "stale_timeout";

pub const REASON_DISCONNECT_FRAME_ERROR: &str = "frame_error";
pub const REASON_DISCONNECT_REJECTED: &str = "rejected";

/// Peer sent the wrong message type for its locked role (e.g.
/// `TrackerServerList` on an established server connection).
pub const REASON_DISCONNECT_ROLE_VIOLATION: &str = "role_violation";

/// Transient resolver failure during address validation; always warn-level.
/// Outcome is mode-dependent: refresh soft-passes (entry stays), initial
/// register hard-rejects (so a new entry can't slip in unverified during a
/// DNS blip) and additionally fires `LOG_REGISTER_REJECTED`.
pub const LOG_ADDRESS_DNS_TRANSIENT: &str =
    "TrackerServerRegister: address DNS lookup transient failure";

pub const LOG_LIST_RESPONSE: &str = "TrackerServerList: response sent";
pub const LOG_LIST_REJECTED: &str = "TrackerServerList: rejected";
pub const LOG_LIST_TRUNCATED: &str = "TrackerServerList: response truncated";

/// Stored entry's `version` failed to parse during the compat-filter pass and
/// was dropped. The registration-side `validate_version` gate makes this
/// unreachable in normal operation — fires only if a buggy registrant slipped past.
pub const LOG_LIST_DROP_UNPARSEABLE_VERSION: &str =
    "TrackerServerList: dropping entry with unparseable version";

/// Peer sent the wrong message type for its locked role. (List connections
/// close immediately, so register-on-client is impossible.)
pub const LOG_ROLE_VIOLATION: &str = "Role violation";

/// A registered server connection closed; its entry was removed from the registry.
pub const LOG_REGISTER_DISCONNECTED: &str =
    "TrackerServerRegister: connection closed; entry unregistered";

/// Benign rustls warning substring (client disconnected without TLS shutdown).
pub const TLS_CLOSE_NOTIFY_MSG: &str = "peer closed connection without sending TLS close_notify";

/// Panics — handler required for graceful shutdown.
#[cfg(unix)]
pub const ERR_SIGNAL_SIGTERM: &str = "Failed to setup SIGTERM handler";

#[cfg(unix)]
pub const ERR_SIGNAL_SIGINT: &str = "Failed to setup SIGINT handler";

#[cfg(not(unix))]
pub const ERR_SIGNAL_CTRLC: &str = "Failed to setup Ctrl+C handler";

/// Panics — handler required for password reload.
#[cfg(unix)]
pub const ERR_SIGNAL_SIGHUP: &str = "SIGHUP handler installation failed";

// Mutex/lock poisoning panic messages. A poisoned lock means a prior holder
// panicked mid-mutation, leaving the in-memory state unknown-shape and
// unrecoverable.

pub const ERR_REGISTRY_MUTEX_POISONED: &str = "registry mutex poisoned";
pub const ERR_REGISTRATION_HASH_LOCK_POISONED: &str = "registration password hash lock poisoned";
pub const ERR_LISTING_HASH_LOCK_POISONED: &str = "listing password hash lock poisoned";

/// Used by `TrackerState::reload_one` where the lock is selected dynamically;
/// the surrounding `kind = %kind` log field identifies which kind failed.
pub const ERR_PASSWORD_HASH_LOCK_POISONED: &str = "password hash lock poisoned";

/// Sweep interval for idle per-IP rate-limit buckets. Must be
/// `< RATE_LIMITER_IDLE_TTL`, else eviction lags and idle buckets pile up past
/// expiry. 60s balances cheap sweeps against stale-entry accumulation.
pub const RATE_LIMITER_GC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Bucket idle TTL. Set well above the 60s refill window so legit bursty
/// traffic doesn't thrash evict+recreate, but short enough that disposable-IP
/// spam doesn't waste memory.
pub const RATE_LIMITER_IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Debug-level — expected normal-operation noise.
pub const LOG_CONNECTION_RATE_LIMITED: &str = "Connection rate-limited; dropping";

pub const LOG_AUTH_RATE_LIMITED: &str = "Auth rate-limited";

/// Min interval between refreshes on one connection: half the protocol minimum
/// `refresh_interval` (120s) — anything faster is misbehavior. Bounds
/// Argon2/mutex-contention abuse. Hardcoded (protocol-derived, not a policy knob).
pub const REFRESH_FLOOR_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Bound on hostname resolution during address validation. Past this is treated
/// as transient (initial register rejects, refresh soft-passes). Matches the
/// workspace DNS deadline used by client and server connection paths.
pub const ADDRESS_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Per spec (`docs/protocol/18-trackers.md`, "Stale timeout"), an entry is
/// evicted after 2× its refresh_interval — one missed refresh of slack for
/// network blips. Hardcoded (protocol-derived, not a policy knob).
pub const STALE_TIMEOUT_REFRESH_MULTIPLIER: u32 = 2;

pub const LOG_REFRESH_TOO_SOON: &str = "TrackerServerRegister: refresh too soon";

/// Refresh targeted an `id` no longer in the registry. Can't happen normally
/// (the task's drop guard keeps the id alive); seeing it means an out-of-band
/// eviction cleaned the slot while the connection was still active.
pub const LOG_REFRESH_GHOST_ID: &str = "TrackerServerRegister: refresh on unregistered id";

pub const LOG_UPNP_SETUP_FAILED: &str = "UPnP setup failed";
pub const MSG_UPNP_CONTINUE: &str = "Tracker will continue without UPnP port forwarding.";
pub const MSG_UPNP_MANUAL: &str =
    "You may need to manually configure port forwarding on your router.";
pub const LOG_UPNP_REMOVE_FAILED: &str = "Failed to remove UPnP port mapping";
