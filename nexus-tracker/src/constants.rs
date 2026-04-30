//! Tracker constants

/// Subdirectory name within the platform data directory
/// (e.g. `~/.local/share/nexus-trackerd/` on Linux).
pub const DATA_DIR_NAME: &str = "nexus-trackerd";

/// Permissions mode for the data directory and its subdirectories on Unix.
/// Owner-only (`0o700`) so directory listings don't leak filenames or
/// existence to other local users.
#[cfg(unix)]
pub const DATA_DIR_MODE: u32 = 0o700;

/// Log file prefix (used by daily rotation, e.g. `nexus-trackerd.2025-04-28`).
pub const LOG_FILE_PREFIX: &str = "nexus-trackerd";

/// Subdirectory name for log files within the data directory.
pub const LOGS_DIR_NAME: &str = "logs";

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

/// `logging::init` called more than once (panics if it fires — indicates
/// a programming error, not an operator-actionable failure).
pub const ERR_LOG_LEVEL_ALREADY_SET: &str = "log level already initialized";

/// Log level parsing failed (caller appends the offending value).
pub const ERR_LOG_LEVEL_INVALID: &str =
    "Invalid log level (valid values: none, error, warn, info, debug): ";

/// Log retention parsing failed (caller appends the value and underlying error).
pub const ERR_LOG_RETENTION_INVALID: &str = "Invalid log retention: ";

/// Log retention below the 1-day minimum (caller appends the value).
pub const ERR_LOG_RETENTION_TOO_SHORT: &str =
    "Log retention must be 0 (disabled) or at least 1 day, got: ";

/// Log directory creation failed (caller appends the path and underlying error).
pub const ERR_CREATE_LOG_DIR: &str = "Failed to create log directory: ";

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

/// Permissions mode for password hash files on Unix (owner read/write only).
#[cfg(unix)]
pub const PASSWORD_FILE_MODE: u32 = 0o600;

/// Argon2 hashing failed (caller appends underlying error).
pub const ERR_HASH_PASSWORD: &str = "Failed to hash password: ";

/// Password hash file write failed (caller appends path and underlying error).
pub const ERR_WRITE_PASSWORD_FILE: &str = "Failed to write password file: ";

/// Atomic rename of `<file>.tmp` to the final path failed (caller appends
/// path and underlying error). Used by both auth.rs and tls.rs.
pub const ERR_RENAME_FILE: &str = "Failed to finalize file: ";

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

/// Permissions mode for the TLS cert and key on Unix (owner read/write only).
#[cfg(unix)]
pub const TLS_FILE_MODE: u32 = 0o600;

/// Status: about to generate a fresh self-signed certificate.
pub const MSG_GENERATING_CERT: &str = "Generating self-signed TLS certificate...";

/// Status: certificate file successfully written (caller appends path).
pub const MSG_CERT_GENERATED: &str = "Certificate generated: ";

/// Status: private key file successfully written (caller appends path).
pub const MSG_KEY_GENERATED: &str = "Private key generated: ";

/// Status: certificate fingerprint display (caller appends 95-char fingerprint).
pub const MSG_CERT_FINGERPRINT: &str = "Certificate fingerprint (SHA-256): ";

/// Status: certificate directory display (caller appends path; matches server style).
pub const MSG_CERTIFICATES: &str = "Certificates: ";

/// Rustls crypto provider installation failed (panics — required for any
/// TLS operation). Should never fire in practice.
pub const ERR_RUSTLS_PROVIDER: &str = "failed to install rustls crypto provider";

/// Key pair generation failed (caller appends underlying error).
pub const ERR_GENERATE_KEYPAIR: &str = "Failed to generate key pair: ";

/// Certificate parameter creation failed (caller appends underlying error).
pub const ERR_CREATE_CERT_PARAMS: &str = "Failed to create certificate parameters: ";

/// Certificate signing failed (caller appends underlying error).
pub const ERR_GENERATE_CERT: &str = "Failed to generate certificate: ";

/// Certificate file write failed (caller appends path and underlying error).
pub const ERR_WRITE_CERT_FILE: &str = "Failed to write certificate file: ";

/// Private key file write failed (caller appends path and underlying error).
pub const ERR_WRITE_KEY_FILE: &str = "Failed to write private key file: ";

/// Certificate file open failed (caller appends path and underlying error).
pub const ERR_OPEN_CERT_FILE: &str = "Failed to open certificate file: ";

/// Certificate PEM parsing failed (caller appends underlying error).
pub const ERR_PARSE_CERT: &str = "Failed to parse certificate: ";

/// Certificate file contained no certificates.
pub const ERR_NO_CERTS_FOUND: &str = "No certificates found in certificate file";

/// Private key file open failed (caller appends path and underlying error).
pub const ERR_OPEN_KEY_FILE: &str = "Failed to open private key file: ";

/// Private key PEM parsing failed (caller appends underlying error).
pub const ERR_PARSE_KEY: &str = "Failed to parse private key: ";

/// Private key file contained no keys.
pub const ERR_NO_KEY_FOUND: &str = "No private key found in key file";

/// rustls `ServerConfig` construction failed (caller appends underlying error).
pub const ERR_CREATE_TLS_CONFIG: &str = "Failed to create TLS configuration: ";

// =============================================================================
// Internationalization
// =============================================================================

/// Default locale (English). Used as the fallback when a peer-supplied
/// locale is unsupported or a key is missing in the requested locale.
pub const DEFAULT_LOCALE: &str = "en";

/// FluentResource construction failed (panics — indicates a malformed
/// `errors.ftl` baked into the binary, not an operator-actionable failure).
pub const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";

/// FluentBundle resource registration failed (panics — same character
/// as `ERR_I18N_PARSE_FTL`).
pub const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";

/// Translation key missing in English (panics — programming error, the
/// key was added to a call site without a corresponding `errors.ftl` entry).
pub const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

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

/// Prefix prepended to TLS handshake failures when bubbling them out of
/// `handle_connection`. The shared `log_connection_error` filters on this
/// to downgrade scanner / incompatible-client noise to debug level.
pub const TLS_HANDSHAKE_FAILED_PREFIX: &str = "TLS handshake failed:";

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

/// Log: a `TrackerServerRegister` refresh arrived too soon after the
/// previous accepted refresh on the same entry. Paired with
/// `ip = %peer_addr.ip(), id = %connection_id`.
pub const LOG_REFRESH_TOO_SOON: &str = "TrackerServerRegister: refresh too soon";

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
