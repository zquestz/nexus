//! Tracker constants

/// Subdirectory name within the platform data directory
/// (e.g. `~/.local/share/nexus-trackerd/` on Linux).
pub const DATA_DIR_NAME: &str = "nexus-trackerd";

/// Log file prefix (used by daily rotation, e.g. `nexus-trackerd.2025-04-28`).
/// Daemon-specific; passed in to `nexus_common::logging::init` and
/// `purge_old_logs` so the shared logging stack picks the right files.
pub const LOG_FILE_PREFIX: &str = "nexus-trackerd";

// =============================================================================
// Startup banner
// =============================================================================

/// Tracker banner prefix (shown at startup; followed by the package version).
pub const MSG_BANNER: &str = "Nexus Tracker v";

/// Log level display
pub const MSG_LOG_LEVEL: &str = "Log level: ";

/// Log directory display
pub const MSG_LOG_DIR: &str = "Log directory: ";

/// Platform does not provide a data directory (extremely rare — Windows
/// without `%APPDATA%`, Linux without `HOME`, etc.)
pub const ERR_NO_DATA_DIR: &str = "Platform does not provide a data directory";

/// `--data-dir` rejected because the supplied path is relative
pub const ERR_DATA_DIR_NOT_ABSOLUTE: &str = "--data-dir must be an absolute path: ";

/// Tracing subscriber initialization failed; daemon falls back to
/// stderr-only output.
pub const LOG_LOGGING_INIT_FAILED: &str = "Logging init failed";

/// Data directory creation error
pub const ERR_CREATE_DATA_DIR: &str = "Failed to create data directory: ";

/// Data directory permissions error
#[cfg(unix)]
pub const ERR_SET_DATA_DIR_PERMS: &str = "Failed to set data directory permissions: ";

// =============================================================================
// Authentication (password hash files)
// =============================================================================

/// Filename for the registration password hash within the data directory.
/// Presence of this file gates `TrackerServerRegister`; absence means open registration.
pub const REGISTRATION_HASH_FILENAME: &str = "registration.hash";

/// Filename for the listing password hash within the data directory.
/// Presence of this file gates `TrackerServerList`; absence means open listing.
pub const LISTING_HASH_FILENAME: &str = "listing.hash";

/// Argon2 hashing failed (caller appends underlying error).
pub const ERR_HASH_PASSWORD: &str = "Failed to hash password: ";

/// Password hash file write failed (caller appends path and underlying error).
pub const ERR_WRITE_PASSWORD_FILE: &str = "Failed to write password file: ";

/// Password hash file read failed (caller appends path and underlying error).
pub const ERR_READ_PASSWORD_FILE: &str = "Failed to read password file: ";

/// Password hash file deletion failed (caller appends path and underlying error).
pub const ERR_DELETE_PASSWORD_FILE: &str = "Failed to delete password file: ";

/// Password prompt failed (caller appends underlying error).
pub const ERR_PROMPT_PASSWORD: &str = "Failed to prompt for password: ";

/// Reading a piped password from stdin failed (caller appends underlying error).
pub const ERR_READ_STDIN: &str = "Failed to read password from stdin: ";

/// Empty password rejected at set time (use `clear-password` to disable gating).
pub const ERR_PASSWORD_EMPTY: &str =
    "Password cannot be empty (use `clear-password` to disable gating)";

/// Password exceeds the maximum byte length (caller appends the limit).
pub const ERR_PASSWORD_TOO_LONG: &str = "Password exceeds maximum length of ";

/// Password confirmation did not match.
pub const ERR_PASSWORD_MISMATCH: &str = "Passwords do not match";

/// Stored password hash failed to parse as PHC (caller appends underlying error).
pub const ERR_PARSE_PASSWORD_HASH: &str = "Failed to parse stored password hash: ";

/// Log message: about to prompt for a new password (paired with `kind = %kind`).
pub const LOG_PASSWORD_SETTING: &str = "Setting password";

/// Log message: password successfully set (paired with `kind = %kind`).
pub const LOG_PASSWORD_SET: &str = "Password set";

/// Log message: password file successfully cleared (paired with `kind = %kind`).
pub const LOG_PASSWORD_CLEARED: &str = "Password cleared";

/// Log message: clear-password called but no file was present
/// (paired with `kind = %kind`).
pub const LOG_PASSWORD_NOT_PRESENT: &str = "No password configured";

/// Log message: SIGHUP received and we're about to reload password hashes
/// from disk.
#[cfg(unix)]
pub const LOG_SIGHUP_RECEIVED: &str = "SIGHUP received; reloading passwords";

/// Log message: a single password kind was successfully reloaded
/// (paired with `kind = %kind, gated = %bool`). Logged for both
/// transitions (open → gated, gated → open) and for "still
/// gated, hash possibly changed" reloads.
pub const LOG_PASSWORD_RELOADED: &str = "Password reloaded";

/// Log message: reload of one password kind failed (paired with
/// `kind = %kind, err = %err`). Previous in-memory state is
/// preserved — the daemon does not crash on a typo in a hash file.
pub const LOG_PASSWORD_RELOAD_FAILED: &str = "Password reload failed; previous state preserved";

/// Auth-flow label: registration. Composed with a status label for the
/// startup status line, e.g. `format!("{LABEL_REGISTRATION}: {STATUS_OPEN}")`.
pub const LABEL_REGISTRATION: &str = "Registration";

/// Auth-flow label: listing.
pub const LABEL_LISTING: &str = "Listing";

/// Auth-flow status: open (no hash file present, password not required).
pub const STATUS_OPEN: &str = "open";

/// Auth-flow status: gated (hash file present, password required).
pub const STATUS_GATED: &str = "gated";

/// Prompt shown when setting a password (kind already announced via the
/// preceding `Setting password` log line). Used only when stdin is a TTY.
pub const PROMPT_NEW_PASSWORD: &str = "New password: ";

/// Prompt shown to confirm the password just entered. TTY-only.
pub const PROMPT_CONFIRM_PASSWORD: &str = "Confirm password: ";

// =============================================================================
// TLS certificate
// =============================================================================

/// TLS certificate filename within the data directory.
pub const CERT_FILENAME: &str = "tracker.crt";

/// TLS private key filename within the data directory.
pub const KEY_FILENAME: &str = "tracker.key";

/// Common Name embedded in the auto-generated self-signed certificate.
pub const TLS_CERT_COMMON_NAME: &str = "Nexus Tracker";

/// Status: certificate directory display (caller appends path; matches server style).
pub const MSG_CERTIFICATES: &str = "Certificates: ";

/// Rustls crypto provider installation failed (panics — required for any
/// TLS operation). Should never fire in practice.
pub const ERR_RUSTLS_PROVIDER: &str = "failed to install rustls crypto provider";

// =============================================================================
// Internationalization
// =============================================================================

/// Default locale (English) — re-exported from `nexus-common` so the
/// value is defined once for the workspace.
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

/// FluentResource construction failed (panics — indicates a malformed
/// `errors.ftl` baked into the binary, not an operator-actionable failure).
pub const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";

/// FluentBundle resource registration failed (panics — same character
/// as `ERR_I18N_PARSE_FTL`).
pub const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";

/// Translation key missing in English (panics — programming error, the
/// key was added to a call site without a corresponding `errors.ftl` entry).
pub const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

/// Panic message: `DEFAULT_LOCALE` failed to parse as a `LanguageIdentifier`.
/// Programmer-error: the constant is `"en"` and is hand-edited to be valid.
pub const ERR_DEFAULT_LOCALE_INVALID: &str = "DEFAULT_LOCALE is a valid locale";

/// Log message: Fluent reported recoverable formatting errors while
/// resolving a translation. Paired with `key = %key, errors = ?errors`.
pub const LOG_TRANSLATION_ERRORS: &str = "Translation errors";

/// Log message: a translation key was missing in the requested locale
/// (falls back to English). Paired with `key = %key, locale = %locale`.
pub const LOG_MISSING_TRANSLATION_KEY: &str = "Missing translation key";

// =============================================================================
// Listener / connection lifecycle
// =============================================================================

/// Tracker port listening display (caller appends bound `SocketAddr`).
pub const MSG_LISTENING: &str = "Tracker port: ";

/// WebSocket tracker port listening display (caller appends bound `SocketAddr`).
/// Only emitted when `--websocket` is enabled.
pub const MSG_WS_LISTENING: &str = "WebSocket tracker port: ";

/// Operator-facing message printed on graceful shutdown.
pub const MSG_SHUTDOWN_RECEIVED: &str = "\nShutdown signal received";

/// Listener bind failure prefix (caller appends `addr` and underlying error).
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";

/// Maximum time the tracker waits for a `Handshake` after TLS completes.
/// Spec §Timeouts: "TLS accepted, awaiting Handshake — 30 seconds."
pub const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum time the tracker waits for the first role-establishing
/// message (`TrackerServerRegister` or `TrackerServerList`) after a successful
/// handshake. Spec §Timeouts: "Awaiting first role-establishing
/// message after handshake — 30 seconds."
pub const ROLE_ESTABLISH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Log: TCP `accept()` returned an error (paired with `err = %e`).
pub const LOG_ACCEPT_ERROR: &str = "Accept error";

/// Log: connection error after TLS (frame, JSON, or unexpected disconnect).
pub const LOG_CONNECTION_ERROR: &str = "Connection error";

/// Log: TLS handshake itself failed (paired with `ip = %addr, err = %e`).
pub const LOG_CONNECTION_ERROR_TLS: &str = "Connection error (TLS handshake)";

/// Log: peer sent a non-Handshake message before completing the handshake.
pub const LOG_HANDSHAKE_REQUIRED: &str = "Handshake: required";

/// Log: handshake major version mismatch (paired with version fields).
pub const LOG_HANDSHAKE_MAJOR_MISMATCH: &str = "Handshake: major version mismatch";

/// Log: handshake minor version mismatch (paired with version fields).
pub const LOG_HANDSHAKE_MINOR_MISMATCH: &str = "Handshake: minor version mismatch";

/// Log: client minor version newer than server's (paired with version fields).
pub const LOG_HANDSHAKE_CLIENT_TOO_NEW: &str = "Handshake: client too new";

// Post-handshake dispatch / role-locking
/// Log: a server connection successfully registered a fresh entry.
/// Paired with `ip = %peer_addr.ip(), id = %connection_id, name = %name`.
pub const LOG_REGISTER_NEW: &str = "TrackerServerRegister: new entry";

/// Log: a server connection refreshed an existing entry.
/// Paired with `id = %connection_id, user_count = %user_count`.
pub const LOG_REGISTER_REFRESH: &str = "TrackerServerRegister: refresh";

/// Log: TrackerServerRegister rejected for an operator-actionable reason
/// (validation failure, capacity, unauthorized). Paired with
/// `ip = %peer_addr.ip(), reason = %short_str`.
pub const LOG_REGISTER_REJECTED: &str = "TrackerServerRegister: rejected";

// Address-validation rejection reasons (passed as `reason` to `LOG_REGISTER_REJECTED`).
//
// All address-related rejections share the single operator-facing
// `err_tracker_address_invalid` i18n string; the per-reason granularity
// is for tracker-operator log analysis (which check failed) rather than
// for the registrant.

/// `address` failed `validate_public_address` (malformed hostname /
/// embedded port / scheme / etc).
pub const REASON_ADDRESS_INVALID: &str = "address_invalid";

/// `address` parsed as an IP literal in the loopback range.
pub const REASON_ADDRESS_LOOPBACK: &str = "address_loopback";

/// `address` parsed as the unspecified IP (`0.0.0.0` / `::`) or
/// anywhere in the broader `0.0.0.0/8` "this network" range
/// (RFC 1122 §3.2.1.3) — both route to this telemetry bucket.
pub const REASON_ADDRESS_UNSPECIFIED: &str = "address_unspecified";

/// `address` parsed as an IP literal in the link-local range.
pub const REASON_ADDRESS_LINK_LOCAL: &str = "address_link_local";

/// `address` parsed as an IP literal in a multicast range.
pub const REASON_ADDRESS_MULTICAST: &str = "address_multicast";

/// `address` parsed as an IP literal in a documentation range.
pub const REASON_ADDRESS_DOCUMENTATION: &str = "address_documentation";

/// `address` parsed as the IPv4 limited broadcast (`255.255.255.255`).
pub const REASON_ADDRESS_BROADCAST: &str = "address_broadcast";

/// `address` parsed as an IP literal that didn't match the peer IP and
/// the peer was not on a private network (so the LAN bypass didn't apply).
pub const REASON_ADDRESS_IP_LITERAL_MISMATCH: &str = "address_ip_literal_mismatch";

/// Hostname `address` resolved to zero IPs (NXDOMAIN-equivalent).
pub const REASON_ADDRESS_HOSTNAME_NOT_FOUND: &str = "address_hostname_not_found";

/// Hostname `address` resolved successfully but none of the returned
/// IPs matched the peer source IP.
pub const REASON_ADDRESS_HOSTNAME_NO_MATCH: &str = "address_hostname_no_match";

/// Hostname `address` couldn't be resolved due to a transient
/// resolver failure (timeout, network error). Distinguished from
/// `REASON_ADDRESS_HOSTNAME_NOT_FOUND` (NXDOMAIN) so operators can
/// tell intermittent DNS issues apart from "the host doesn't exist."
/// Only emitted on initial register; refresh soft-passes transient
/// errors so an established entry isn't evicted by a brief DNS blip.
pub const REASON_ADDRESS_HOSTNAME_DNS_FAILED: &str = "address_hostname_dns_failed";

/// Log: hostname address validation hit a transient resolver failure
/// (timeout, network error). Always emitted at warn level when the
/// transient is encountered. The downstream outcome is mode-dependent:
/// refresh soft-passes (the entry stays registered), initial register
/// hard-rejects (so a brand-new entry can't slip in unverified during
/// a DNS blip) — so initial-register transients additionally fire
/// `LOG_REGISTER_REJECTED` with `REASON_ADDRESS_HOSTNAME_DNS_FAILED`.
/// Paired with `ip = %peer_addr.ip(), host = %ascii_host, err = %resolver_err`.
pub const LOG_ADDRESS_DNS_TRANSIENT: &str =
    "TrackerServerRegister: address DNS lookup transient failure";

/// Log: TrackerServerList received and a snapshot returned.
/// Paired with `ip = %peer_addr.ip(), count = %returned_count`.
pub const LOG_LIST_RESPONSE: &str = "TrackerServerList: response sent";

/// Log: TrackerServerList rejected for an operator-actionable reason.
/// Paired with `ip = %peer_addr.ip(), reason = %short_str`.
pub const LOG_LIST_REJECTED: &str = "TrackerServerList: rejected";

/// Log: peer sent the wrong message type for its locked role
/// (TrackerServerList on a server connection, or TrackerServerRegister on a
/// client connection — but the latter is impossible since List
/// connections close immediately). Paired with `ip = %peer_addr.ip()`.
pub const LOG_ROLE_VIOLATION: &str = "Role violation";

/// Log: a registered server connection closed; its entry has been
/// removed from the registry. Paired with `ip = %peer_addr.ip(), id = %id`.
pub const LOG_REGISTER_DISCONNECTED: &str =
    "TrackerServerRegister: connection closed; entry unregistered";

/// Substring of the rustls "close_notify" warning we treat as benign
/// (clients disconnecting without proper TLS shutdown).
pub const TLS_CLOSE_NOTIFY_MSG: &str = "peer closed connection without sending TLS close_notify";

// =============================================================================
// Signal handling
// =============================================================================

/// SIGTERM handler setup error (panics — required for graceful shutdown).
#[cfg(unix)]
pub const ERR_SIGNAL_SIGTERM: &str = "Failed to setup SIGTERM handler";

/// SIGINT handler setup error (panics — required for graceful shutdown).
#[cfg(unix)]
pub const ERR_SIGNAL_SIGINT: &str = "Failed to setup SIGINT handler";

/// Ctrl+C handler setup error on Windows (panics — required for graceful shutdown).
#[cfg(not(unix))]
pub const ERR_SIGNAL_CTRLC: &str = "Failed to setup Ctrl+C handler";

/// SIGHUP handler installation error on Unix (panics — required for
/// password reload). Should never fire in practice; if it does the
/// platform's signal subsystem itself is broken.
#[cfg(unix)]
pub const ERR_SIGNAL_SIGHUP: &str = "SIGHUP handler installation failed";

// =============================================================================
// Mutex poisoning panic messages (programmer-error invariants)
// =============================================================================

/// Panic message: the `Mutex<Registry>` on `TrackerState` is poisoned.
/// A poisoned mutex means a previous holder panicked while mutating
/// the registry; the in-memory state is unknown-shape and unrecoverable.
pub const ERR_REGISTRY_MUTEX_POISONED: &str = "registry mutex poisoned";

/// Panic message: the per-IP rate-limiter bucket map's `Mutex` is
/// poisoned. A poisoned mutex means a previous holder panicked while
/// updating the rate-limit state; the bucket counts are unknown-shape.
pub const ERR_RATE_LIMITER_MUTEX_POISONED: &str = "rate limiter mutex poisoned";

/// Panic message: the registration password hash `RwLock` is poisoned.
/// A poisoned lock means a prior holder panicked while reading or
/// swapping the hash, leaving the in-memory state unknown.
pub const ERR_REGISTRATION_HASH_LOCK_POISONED: &str = "registration password hash lock poisoned";

/// Panic message: the listing password hash `RwLock` is poisoned.
/// Same reasoning as [`ERR_REGISTRATION_HASH_LOCK_POISONED`].
pub const ERR_LISTING_HASH_LOCK_POISONED: &str = "listing password hash lock poisoned";

/// Panic message used by `TrackerState::reload_one`, where the lock
/// is selected dynamically (registration or listing). Distinct from
/// the flow-specific constants above so the panic still tells the
/// operator *which kind* failed via the `kind = %kind` log field that
/// surrounds the panic site.
pub const ERR_PASSWORD_HASH_LOCK_POISONED: &str = "password hash lock poisoned";

// =============================================================================
// Rate limiting
// =============================================================================

/// How often the background task sweeps idle entries from the per-IP
/// rate-limit maps. 60s is a tradeoff: short enough that long-lived
/// daemons under disposable-IP attack don't accumulate too many stale
/// entries between sweeps, long enough that the sweep itself is cheap.
pub const RATE_LIMITER_GC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Bucket idle TTL — how long an IP's bucket sticks around after its
/// last touch. Set well above the 60s refill window so we don't thrash
/// (evict + re-create) under bursty but legitimate traffic, but not so
/// long that disposable-IP spam wastes memory.
pub const RATE_LIMITER_IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Log: a TCP connection was dropped pre-TLS because the source IP's
/// connection-rate bucket is empty. Paired with `ip = %peer_addr.ip()`.
/// Debug-level — this is expected, normal-operation noise.
pub const LOG_CONNECTION_RATE_LIMITED: &str = "Connection rate-limited; dropping";

/// Log: an auth attempt was rejected because the source IP's
/// auth-failure-rate bucket is empty. Paired with `ip = %peer_addr.ip()`.
pub const LOG_AUTH_RATE_LIMITED: &str = "Auth rate-limited";

/// Minimum elapsed time between successive `TrackerServerRegister`
/// refreshes on a single connection. Half the protocol-level minimum
/// `refresh_interval` (120s) — anything faster than this is misbehavior
/// by definition. Bounds Argon2 / mutex-contention abuse from a
/// long-lived connection that's already past the connection-rate gate.
/// Hardcoded (not operator-tunable) because it's a protocol-derived
/// value, not a policy knob.
pub const REFRESH_FLOOR_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Bound on the time spent resolving a registrant-supplied hostname
/// during address validation. Lookups past this point are treated as
/// transient — handler outcome is mode-asymmetric (initial register
/// rejects, refresh soft-passes). 5 seconds: generous enough that
/// healthy resolvers comfortably finish in milliseconds, short enough
/// that a stuck resolver can't pin a connection task.
pub const ADDRESS_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Stale-timeout multiplier applied to a server's `refresh_interval`.
/// Per the tracker protocol spec (`docs/protocol/18-trackers.md`,
/// "Stale timeout"), an entry is evicted after twice its refresh
/// interval has passed without a refresh — this gives one missed
/// refresh worth of slack for transient network blips before
/// declaring the server gone. Hardcoded (protocol-derived, not a
/// policy knob).
pub const STALE_TIMEOUT_REFRESH_MULTIPLIER: u32 = 2;

/// Log: a `TrackerServerRegister` refresh arrived too soon after the
/// previous accepted refresh on the same entry. Paired with
/// `ip = %peer_addr.ip(), id = %connection_id`.
pub const LOG_REFRESH_TOO_SOON: &str = "TrackerServerRegister: refresh too soon";

/// Log: a refresh targeted an `id` no longer present in the registry.
/// In normal operation this can't happen — the connection task's drop
/// guard keeps the id alive — so seeing this means an out-of-band
/// eviction (or a future stale-eviction worker) cleaned the slot
/// while the connection was still active. Paired with
/// `ip = %peer_addr.ip(), id = %connection_id`.
pub const LOG_REFRESH_GHOST_ID: &str = "TrackerServerRegister: refresh on unregistered id";

// =============================================================================
// UPnP (operator-facing)
// =============================================================================

/// UPnP setup failure log message (paired with structured `err = %e` field).
pub const LOG_UPNP_SETUP_FAILED: &str = "UPnP setup failed";

/// UPnP disabled continuation message printed alongside setup failure.
pub const MSG_UPNP_CONTINUE: &str = "Tracker will continue without UPnP port forwarding.";

/// UPnP manual configuration suggestion printed alongside setup failure.
pub const MSG_UPNP_MANUAL: &str =
    "You may need to manually configure port forwarding on your router.";

/// UPnP mapping removal failure log message (paired with `err = %e`).
pub const LOG_UPNP_REMOVE_FAILED: &str = "Failed to remove UPnP port mapping";
