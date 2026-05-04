//! Constants for server operator messages and configuration
//!
//! NOTE: User-facing error messages (sent to clients) are in handlers/errors.rs
//! This file contains only server operator messages (logs, startup, diagnostics)

use nexus_common::validators::PasswordStrength;

// =============================================================================
// File Area Configuration
// =============================================================================

/// File area root directory name (inside data directory)
pub const FILES_DIR_NAME: &str = "files";

/// Shared files directory name
pub const FILES_SHARED_DIR: &str = "shared";

/// User files directory name
pub const FILES_USERS_DIR: &str = "users";

/// Upload folder suffix (case-insensitive, includes leading space)
pub const FOLDER_SUFFIX_UPLOAD: &str = " [NEXUS-UL]";

/// Drop box folder suffix (case-insensitive, includes leading space)
pub const FOLDER_SUFFIX_DROPBOX: &str = " [NEXUS-DB]";

/// Drop box folder suffix prefix for user-specific drop boxes (includes leading space)
pub const FOLDER_SUFFIX_DROPBOX_PREFIX: &str = " [NEXUS-DB-";

/// Default filename when path has no filename or non-UTF-8 filename
pub const DEFAULT_FILENAME: &str = "file";

// =============================================================================
// Connection Limits
// =============================================================================

/// Configuration key for max connections per IP in the database
pub const CONFIG_KEY_MAX_CONNECTIONS_PER_IP: &str = "max_connections_per_ip";

/// Default maximum connections per IP address (matches migration default)
pub const DEFAULT_MAX_CONNECTIONS_PER_IP: usize = 5;

/// Configuration key for max transfers per IP in the database
pub const CONFIG_KEY_MAX_TRANSFERS_PER_IP: &str = "max_transfers_per_ip";

/// Default maximum file transfer connections per IP address (matches migration default)
pub const DEFAULT_MAX_TRANSFERS_PER_IP: usize = 3;

// =============================================================================
// File Reindex Configuration
// =============================================================================

/// Configuration key for file reindex interval in the database
pub const CONFIG_KEY_FILE_REINDEX_INTERVAL: &str = "file_reindex_interval";

/// Default file reindex interval in minutes (matches migration default)
/// A value of 0 disables automatic reindexing.
pub const DEFAULT_FILE_REINDEX_INTERVAL: u32 = 5;

/// Max age before a forced reindex runs even if nothing was marked dirty,
/// so external filesystem changes (e.g. admin adds files via SSH) are picked up.
pub const FILE_INDEX_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

// =============================================================================
// Database Validation Errors (defense-in-depth, operator-facing)
//
// These strings are returned from DB-layer setters via `io::Error::other(...)`
// and appear only in server logs. Handlers catch any DB error and send the
// generic translated `err-database` message to the client — users never see
// these strings. Do NOT add them to locale files; they are intentionally
// English-only for operator log readability.
// =============================================================================

/// Error when server name is empty
pub const ERR_SERVER_NAME_EMPTY: &str = "Server name cannot be empty";

/// Error when server name is too long
pub const ERR_SERVER_NAME_TOO_LONG: &str = "Server name is too long";

/// Error when server name contains newlines
pub const ERR_SERVER_NAME_NEWLINES: &str = "Server name cannot contain newlines";

/// Error when server name contains invalid characters
pub const ERR_SERVER_NAME_INVALID_CHARS: &str = "Server name contains invalid characters";

/// Error when server description is too long
pub const ERR_SERVER_DESC_TOO_LONG: &str = "Server description is too long";

/// Error when server description contains newlines
pub const ERR_SERVER_DESC_NEWLINES: &str = "Server description cannot contain newlines";

/// Error when server description contains invalid characters
pub const ERR_SERVER_DESC_INVALID_CHARS: &str = "Server description contains invalid characters";

/// Error when server image is too large
pub const ERR_SERVER_IMAGE_TOO_LARGE: &str = "Server image is too large";

/// Error when server image has invalid format
pub const ERR_SERVER_IMAGE_INVALID_FORMAT: &str = "Server image has invalid format";

/// Error when server image has unsupported type
pub const ERR_SERVER_IMAGE_UNSUPPORTED_TYPE: &str = "Server image has unsupported type";

/// Error when public address exceeds the maximum length
pub const ERR_PUBLIC_ADDRESS_TOO_LONG: &str = "Public address is too long";

/// Error when public address contains a URL scheme
pub const ERR_PUBLIC_ADDRESS_CONTAINS_SCHEME: &str = "Public address must not include a URL scheme";

/// Error when public address contains brackets
pub const ERR_PUBLIC_ADDRESS_CONTAINS_BRACKETS: &str = "Public address must not include brackets";

/// Error when public address contains a path
pub const ERR_PUBLIC_ADDRESS_CONTAINS_PATH: &str = "Public address must not include a path";

/// Error when public address contains userinfo
pub const ERR_PUBLIC_ADDRESS_CONTAINS_USERINFO: &str = "Public address must not include a username";

/// Error when public address contains whitespace
pub const ERR_PUBLIC_ADDRESS_CONTAINS_WHITESPACE: &str =
    "Public address must not contain whitespace";

/// Error when public address contains a port
pub const ERR_PUBLIC_ADDRESS_CONTAINS_PORT: &str = "Public address must not include a port";

/// Error when public address contains an IPv6 zone identifier
pub const ERR_PUBLIC_ADDRESS_CONTAINS_ZONE_ID: &str =
    "Public address must not include an IPv6 zone identifier";

/// Error when public address is not a valid hostname, IPv4, or IPv6
pub const ERR_PUBLIC_ADDRESS_INVALID_FORMAT: &str =
    "Public address is not a valid hostname or IP address";

/// Default server name (matches migration default)
pub const DEFAULT_SERVER_NAME: &str = "Nexus BBS";

/// Default server description (matches migration default)
pub const DEFAULT_SERVER_DESCRIPTION: &str = "";

/// Default server image (matches migration default)
pub const DEFAULT_SERVER_IMAGE: &str = "";

/// Default public address (empty = unset; admin must configure to enable URI sharing)
pub const DEFAULT_PUBLIC_ADDRESS: &str = "";

// =============================================================================
// Data Directory
// =============================================================================

/// Subdirectory name within the platform data directory
/// (e.g., `~/.local/share/nexusd/` on Linux). Hosts the database, TLS
/// certificate and key, file search index, and log files.
pub const DATA_DIR_NAME: &str = "nexusd";

/// Log file prefix (tracing-appender appends date, e.g. nexusd.2025-07-11).
/// Daemon-specific; passed in to `nexus_common::logging::init` and
/// `purge_old_logs` so the shared logging stack picks the right files.
pub const LOG_FILE_PREFIX: &str = "nexusd";

/// Database file name
pub const DATABASE_FILENAME: &str = "nexus.db";

// =============================================================================
// Database Configuration
// =============================================================================

/// Database configuration key for server name
pub const CONFIG_KEY_SERVER_NAME: &str = "server_name";

/// Database configuration key for server description
pub const CONFIG_KEY_SERVER_DESCRIPTION: &str = "server_description";

/// Database configuration key for server image
pub const CONFIG_KEY_SERVER_IMAGE: &str = "server_image";

/// Database configuration key for server public address
pub const CONFIG_KEY_PUBLIC_ADDRESS: &str = "public_address";

// =============================================================================
// Feature Names
// =============================================================================

/// Feature name for chat functionality
pub const FEATURE_CHAT: &str = "chat";

/// Feature name for news functionality
pub const FEATURE_NEWS: &str = "news";

/// Feature name for file area functionality
/// (Currently unused - will be used for file transfer broadcasts in future phases)
#[allow(dead_code)]
pub const FEATURE_FILES: &str = "files";

/// Config key for persistent channels (space-separated list)
/// These channels survive restart and can't be deleted when empty
pub const CONFIG_KEY_PERSISTENT_CHANNELS: &str = "persistent_channels";

/// Default persistent channels (survive restart, can't be deleted when empty)
pub const DEFAULT_PERSISTENT_CHANNELS: &str = nexus_common::validators::DEFAULT_CHANNEL;

/// Config key for auto-join channels (space-separated list)
/// These channels are automatically joined by users on login
pub const CONFIG_KEY_AUTO_JOIN_CHANNELS: &str = "auto_join_channels";

/// Default auto-join channels (joined on login)
/// By default, same as persistent channels for backward compatibility
pub const DEFAULT_AUTO_JOIN_CHANNELS: &str = nexus_common::validators::DEFAULT_CHANNEL;

/// Config key for minimum password strength
pub const CONFIG_KEY_MIN_PASSWORD_STRENGTH: &str = "min_password_strength";

/// Default minimum password strength
pub const DEFAULT_MIN_PASSWORD_STRENGTH: PasswordStrength = PasswordStrength::Good;

/// Config key for chat burst limit (max messages in a burst)
pub const CONFIG_KEY_CHAT_BURST_LIMIT: &str = "chat_burst_limit";

/// Default chat burst limit (0 = no burst, capacity is 1)
pub const DEFAULT_CHAT_BURST_LIMIT: u32 = 5;

/// Config key for chat rate limit (messages per minute)
pub const CONFIG_KEY_CHAT_RATE_LIMIT: &str = "chat_rate_limit";

/// Default chat rate limit (messages per minute, 0 = disabled)
pub const DEFAULT_CHAT_RATE_LIMIT: u32 = 20;

/// Maximum number of concurrent database connections in the pool
///
/// This value (5) is chosen to balance:
/// - Concurrent request handling (multiple users can access DB simultaneously)
/// - Resource usage (SQLite has limitations on concurrent writes)
/// - Typical BBS workload (small to medium number of simultaneous users)
///
/// SQLite uses WAL mode which allows multiple readers + one writer concurrently,
/// so 5 connections provides good throughput for read-heavy workloads while
/// keeping resource usage reasonable.
pub const MAX_DB_CONNECTIONS: u32 = 5;

// =============================================================================
// TLS Configuration
// =============================================================================

/// TLS certificate file name
pub const CERT_FILENAME: &str = "server.crt";

/// TLS private key file name
pub const KEY_FILENAME: &str = "server.key";

/// TLS certificate common name
pub const TLS_CERT_COMMON_NAME: &str = "Nexus BBS";

/// TLS close notify error pattern
pub const TLS_CLOSE_NOTIFY_MSG: &str = "peer closed connection without sending TLS close_notify";

// =============================================================================
// Server Startup Messages (operator-facing)
// =============================================================================

/// Server banner prefix
pub const MSG_BANNER: &str = "Nexus BBS Server v";

/// Database path display
pub const MSG_DATABASE: &str = "Database: ";

/// Certificates path display
pub const MSG_CERTIFICATES: &str = "Certificates: ";

/// BBS port listening display
pub const MSG_LISTENING: &str = "BBS port: ";

/// Transfer port listening display
pub const MSG_TRANSFER_LISTENING: &str = "Transfer port: ";

/// WebSocket port listening display
pub const MSG_WS_LISTENING: &str = "WebSocket port: ";

/// WebSocket transfer port listening display
pub const MSG_WS_TRANSFER_LISTENING: &str = "WebSocket transfer port: ";

/// Voice UDP port listening display
pub const MSG_VOICE_LISTENING: &str = "Voice UDP port: ";

/// Log level display
pub const MSG_LOG_LEVEL: &str = "Log level: ";

/// Log directory display
pub const MSG_LOG_DIR: &str = "Log directory: ";

/// Shutdown signal received message
pub const MSG_SHUTDOWN_RECEIVED: &str = "Shutdown signal received";

// =============================================================================
// Server Error Messages (operator-facing)
// =============================================================================

/// Rustls crypto provider initialization (panics if it fails — required for
/// any TLS or DTLS operation).
pub const ERR_RUSTLS_PROVIDER: &str = "failed to install rustls crypto provider";

/// IP rule cache lock poisoned (panics if it fires — indicates a panic in
/// another thread while holding the lock, unrecoverable).
pub const ERR_IP_CACHE_POISONED: &str = "ip rule cache lock poisoned";

/// Platform does not provide a data directory (extremely rare — Windows
/// without `%APPDATA%`, Linux without `HOME`, etc.)
pub const ERR_NO_DATA_DIR: &str = "Platform does not provide a data directory";

/// `--data-dir` rejected because the supplied path is relative
pub const ERR_DATA_DIR_NOT_ABSOLUTE: &str = "--data-dir must be an absolute path: ";

/// Data directory creation error
pub const ERR_CREATE_DATA_DIR: &str = "Failed to create data directory: ";

/// Data directory permissions error
#[cfg(unix)]
pub const ERR_SET_DATA_DIR_PERMS: &str = "Failed to set data directory permissions: ";

/// Database initialization error
pub const ERR_DATABASE_INIT: &str = "Failed to initialize database: ";

/// Tracker bootstrap error
pub const ERR_TRACKER_BOOTSTRAP_FAILED: &str = "Failed to bootstrap tracker manager: ";

/// TLS initialization error
pub const ERR_TLS_INIT: &str = "Failed to initialize TLS: ";

/// Server bind error
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";

/// File permissions error
#[cfg(unix)]
pub const ERR_SET_PERMISSIONS: &str = "Failed to set file permissions: ";

// =============================================================================
// Signal Handler Errors (operator-facing)
// =============================================================================

/// SIGTERM handler setup error
#[cfg(unix)]
pub const ERR_SIGNAL_SIGTERM: &str = "Failed to setup SIGTERM handler";

/// SIGINT handler setup error
#[cfg(unix)]
pub const ERR_SIGNAL_SIGINT: &str = "Failed to setup SIGINT handler";

/// Ctrl+C handler setup error (Windows)
#[cfg(not(unix))]
pub const ERR_SIGNAL_CTRLC: &str = "Failed to setup Ctrl+C handler";

// =============================================================================
// UPnP Messages (operator-facing)
// =============================================================================

/// UPnP setup failure log message (paired with structured `err = %e` field).
pub const LOG_UPNP_SETUP_FAILED: &str = "UPnP setup failed";

/// UPnP disabled continuation message
pub const MSG_UPNP_CONTINUE: &str = "Server will continue without UPnP port forwarding.";

/// UPnP manual configuration suggestion
pub const MSG_UPNP_MANUAL: &str =
    "You may need to manually configure port forwarding on your router.";

/// UPnP mapping removal failure log message (paired with structured `err = %e` field).
pub const LOG_UPNP_REMOVE_FAILED: &str = "Failed to remove UPnP port mapping";

// =============================================================================
// Internationalization Configuration and Error Messages (operator-facing)
// =============================================================================

/// Default locale (English) — re-exported from `nexus-common` so the
/// value is defined once for the workspace.
pub use nexus_common::DEFAULT_LOCALE;

/// Supported locale: Spanish
pub const LOCALE_SPANISH: &str = "es";

/// Supported locale: Japanese
pub const LOCALE_JAPANESE: &str = "ja";

/// Supported locale: French
pub const LOCALE_FRENCH: &str = "fr";

/// Supported locale: German
pub const LOCALE_GERMAN: &str = "de";

/// Supported locale: Portuguese (generic/Brazilian)
pub const LOCALE_PORTUGUESE: &str = "pt";

/// Supported locale: Portuguese (Portugal)
pub const LOCALE_PORTUGUESE_PT: &str = "pt-PT";

/// Supported locale: Portuguese (Brazil)
pub const LOCALE_PORTUGUESE_BR: &str = "pt-BR";

/// Supported locale: Russian
pub const LOCALE_RUSSIAN: &str = "ru";

/// Supported locale: Chinese (generic/Simplified)
pub const LOCALE_CHINESE: &str = "zh";

/// Supported locale: Chinese (Simplified)
pub const LOCALE_CHINESE_CN: &str = "zh-CN";

/// Supported locale: Chinese (Traditional)
pub const LOCALE_CHINESE_TW: &str = "zh-TW";

/// Supported locale: Korean
pub const LOCALE_KOREAN: &str = "ko";

/// Supported locale: Italian
pub const LOCALE_ITALIAN: &str = "it";

/// Supported locale: Dutch
pub const LOCALE_DUTCH: &str = "nl";

/// Error when translation key is missing in English (format: key)
pub const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

/// Error when FTL file parsing fails
pub const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";

/// Error when adding resource to bundle fails
pub const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";

// =============================================================================
// File Area Messages (operator-facing)
// =============================================================================

/// File area root path display
pub const MSG_FILE_ROOT: &str = "File area: ";

/// Error when initializing the file area fails (caller-side prefix wrapping
/// any `init_file_area` failure).
pub const ERR_INIT_FILE_AREA: &str = "Failed to initialize file area: ";

/// Error when path resolution fails due to invalid path
pub const ERR_FILE_INVALID_PATH: &str = "Invalid file path";

/// Error when path resolution fails due to access denied (path traversal attempt)
pub const ERR_FILE_ACCESS_DENIED: &str = "Access denied: path outside file area";

/// Error when path does not exist
pub const ERR_FILE_NOT_FOUND: &str = "File or directory not found";

/// Error when canonicalization fails
pub const ERR_FILE_CANONICALIZE: &str = "Failed to resolve path";

/// Error when file root canonicalization fails
pub const ERR_FILE_ROOT_CANONICALIZE: &str = "Failed to canonicalize file root: ";

/// Error when area root is not absolute
pub const ERR_FILE_INVALID_AREA_ROOT: &str = "Area root must be an absolute path";

// =============================================================================
// Channel Errors
// =============================================================================

/// Error when message channel is closed (connection dropped)
pub const ERR_CHANNEL_CLOSED: &str = "channel closed";

// =============================================================================
// Mutex / lock poisoning panic messages (programmer-error invariants)
// =============================================================================

/// Panic message: the transfer-registry mutex is poisoned. A poisoned
/// mutex means a previous holder panicked while updating the active
/// transfer map; the in-memory state is unknown-shape.
pub const ERR_TRANSFER_REGISTRY_LOCK_POISONED: &str = "transfer registry lock poisoned";

/// Panic message: the per-transfer `ban_tx` mutex is poisoned. A
/// poisoned mutex means a previous holder panicked while taking the
/// oneshot sender, leaving the ban-signal state unknown.
pub const ERR_BAN_TX_LOCK_POISONED: &str = "ban_tx lock poisoned";

/// Panic message: the connection-tracker per-IP-count mutex used for
/// limiting BBS connections.
pub const ERR_CONNECTION_TRACKER_LOCK: &str = "connection tracker lock";

/// Panic message: the connection-tracker per-IP-count mutex used for
/// limiting transfer connections.
pub const ERR_TRANSFER_TRACKER_LOCK: &str = "transfer tracker lock";

// =============================================================================
// Other panic messages (programmer-error invariants)
// =============================================================================

/// Panic message: `SystemTime::now().duration_since(UNIX_EPOCH)` failed
/// (system clock is set before 1970-01-01). Used everywhere the daemon
/// derives a Unix-epoch timestamp — bans, trusts, duration parsing,
/// IP-rule-cache, voice session timestamps, handler-level uptime /
/// status. The daemon can't sensibly continue with a clock skewed that
/// far back; the operator hint is actionable rather than a generic
/// "before epoch" message.
pub const ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK: &str =
    "system time is before Unix epoch — check system clock configuration";

/// Panic message: building an `IpNet` from a bare IP returned an error
/// even though the IP came directly from a parsed `IpAddr`. Used by
/// `db::bans::row_to_ban` and `db::trusts::row_to_trust` when
/// constructing the prefix from a known-valid IP.
pub const ERR_VALID_IP_PREFIX: &str = "valid prefix";

/// Panic message: a code path expected the `sessions` collection to be
/// non-empty (e.g., we just confirmed the user has at least one active
/// session in the line above). Used by `users::manager::helpers`.
pub const ERR_SESSIONS_NOT_EMPTY: &str = "sessions is not empty";

/// Panic message: the user-status code paths require at least one
/// active session for the target user. Used by handlers/user_away,
/// user_back, user_status.
pub const ERR_AT_LEAST_ONE_SESSION_EXISTS: &str = "at least one session exists";

/// Panic message: handlers/user_info expected `target_sessions` to be
/// non-empty — checked just upstream.
pub const ERR_TARGET_SESSIONS_NON_EMPTY: &str = "target_sessions is non-empty";

/// Panic message: `DEFAULT_LOCALE` failed to parse as a Fluent
/// `LanguageIdentifier`. Programmer-error: the constant is hand-edited
/// to be valid.
pub const ERR_DEFAULT_LOCALE_INVALID: &str = "DEFAULT_LOCALE is a valid locale";

/// `PasswordError::TaskJoin` Display message — the blocking task that ran
/// Argon2 work panicked or was cancelled. Surfaces here so callers fail
/// closed instead of treating the operation as a verify-success.
pub const ERR_PASSWORD_TASK_JOIN: &str = "password task did not complete";

// --- File Index errors ---
//
// Prefixed messages emitted by `files::index::FileIndex` plumbing.
// Composed via `format!("{}{}", PREFIX, err)` to surface the
// underlying I/O / regex error.
pub const ERR_FILE_INDEX_CREATE_TEMP: &str = "Failed to create temp index: ";
pub const ERR_FILE_INDEX_SET_PERMISSIONS: &str = "Failed to set index permissions: ";
pub const ERR_FILE_INDEX_WRITE_ENTRY: &str = "Failed to write index entry: ";
pub const ERR_FILE_INDEX_FLUSH: &str = "Failed to flush index: ";
pub const ERR_FILE_INDEX_SWAP: &str = "Failed to swap index file: ";
pub const ERR_FILE_INDEX_SEARCH_PATTERN: &str = "Invalid search pattern: ";

// --- File operation task errors ---
//
// `spawn_blocking` failures from `files::operations`. The task wrapper
// catches panics inside the blocking pool; the prefix identifies which
// blocking-pool call failed.
pub const ERR_FILE_OP_REMOVE_TASK: &str = "remove task failed: ";
pub const ERR_FILE_OP_COPY_TASK: &str = "copy task failed: ";
pub const ERR_FILE_OP_RENAME_TASK: &str = "rename task failed: ";

// --- Transfer-port handshake / login / DB errors ---
//
// Internal `io::Error` messages emitted by the transfer-port auth flow
// (port 7501 / WS 7503). Pure-static for connection-state assertions
// (e.g. "Connection closed") and prefix-style for wrapping I/O errors.
// These are operator-log strings, not localized — the transfer port
// has no locale negotiation before authentication completes.

// Pure-static
pub const ERR_TRANSFER_HANDSHAKE_CLOSED: &str = "Connection closed during handshake";
pub const ERR_TRANSFER_HANDSHAKE_EXPECTED: &str = "Expected Handshake message";
pub const ERR_TRANSFER_VERSION_INVALID: &str = "Invalid version string";
pub const ERR_TRANSFER_VERSION_MAJOR_MISMATCH: &str = "Major version mismatch";
pub const ERR_TRANSFER_LOGIN_CLOSED: &str = "Connection closed during login";
pub const ERR_TRANSFER_LOGIN_EXPECTED: &str = "Expected Login message";
pub const ERR_TRANSFER_USERNAME_INVALID: &str = "Invalid username";
pub const ERR_TRANSFER_PASSWORD_INVALID: &str = "Invalid password";
pub const ERR_TRANSFER_USER_NOT_FOUND: &str = "User not found";
pub const ERR_TRANSFER_INVALID_CREDENTIALS: &str = "Invalid credentials";
pub const ERR_TRANSFER_ACCOUNT_DISABLED: &str = "Account disabled";
pub const ERR_TRANSFER_CONNECTION_CLOSED: &str = "Connection closed";

// Prefix-style
pub const ERR_TRANSFER_READ_HANDSHAKE: &str = "Failed to read handshake: ";
pub const ERR_TRANSFER_READ_LOGIN: &str = "Failed to read login: ";
pub const ERR_TRANSFER_READ_MESSAGE: &str = "Failed to read message: ";
pub const ERR_TRANSFER_DB_ERROR: &str = "Database error: ";
pub const ERR_TRANSFER_PASSWORD_VERIFY_ERROR: &str = "Password verification error: ";
pub const ERR_TRANSFER_VERSION_MINOR_MISMATCH: &str =
    "Minor version mismatch, pre-1.0, server minor: ";
pub const ERR_TRANSFER_VERSION_CLIENT_TOO_NEW: &str = "Client version too new, server minor: ";

// --- Voice DTLS plumbing errors ---
//
// `voice::udp` cert/key load failures and listener-bind failure.
// Composed via `format!("{}{}", PREFIX, err)`. The DTLS-listener prefix
// includes a trailing space so callers can tack on the bind address
// before the underlying error: `format!("{}{}: {}", PREFIX, addr, e)`.
pub const ERR_VOICE_DTLS_LISTENER_PREFIX: &str = "Failed to create voice DTLS listener on ";
pub const ERR_VOICE_READ_CERT_FILE: &str = "Failed to read certificate file: ";
pub const ERR_VOICE_READ_KEY_FILE: &str = "Failed to read private key file: ";
pub const ERR_VOICE_PARSE_CERT: &str = "Failed to parse certificate: ";

// =============================================================================
// Log Messages
// =============================================================================

// --- Connection / Main ---
pub const LOG_ACCEPT_ERROR: &str = "Accept error";
pub const LOG_CONNECTION_ERROR: &str = "Connection error";
pub const LOG_CONNECTION_ERROR_TLS: &str = "Connection error (TLS handshake)";
pub const LOG_CONNECTION_ERROR_WS: &str = "Connection error (WebSocket handshake)";
pub const LOG_CONNECTION_LIMIT: &str = "Connection limit reached";
pub const LOG_DISCONNECTED: &str = "Disconnected";
pub const LOG_ERROR_HANDLING_MESSAGE: &str = "Error handling message";
pub const LOG_LOGGING_INIT_FAILED: &str = "Logging init failed";
pub const LOG_PARSE_MESSAGE_ERROR: &str = "Parse message error";
pub const LOG_REJECTED_BANNED_IP: &str = "Rejected banned IP";
pub const LOG_REJECTED_BANNED_IP_TRANSFER: &str = "Rejected banned IP on transfer port";
pub const LOG_REJECTED_BANNED_IP_WS: &str = "Rejected banned IP on WebSocket port";
pub const LOG_REJECTED_BANNED_IP_WS_TRANSFER: &str =
    "Rejected banned IP on WebSocket transfer port";
pub const LOG_TRANSFER_ON_MAIN_PORT: &str = "Transfer message received on main port";

// --- Startup / Shutdown ---
pub const LOG_CLEANUP_EXPIRED: &str = "Cleaned up expired entries";
pub const LOG_CLEANUP_EXPIRED_BANS_FAILED: &str = "Failed to cleanup expired bans";
pub const LOG_CLEANUP_EXPIRED_TRUSTS_FAILED: &str = "Failed to cleanup expired trusts";
pub const LOG_LOADED_CACHE: &str = "Loaded entries into cache";
pub const LOG_LOAD_BANS_FAILED: &str = "Failed to load bans";
pub const LOG_LOAD_TRUSTS_FAILED: &str = "Failed to load trusts";
pub const LOG_CHANNEL_SETTINGS_CREATE_FAILED: &str = "Failed to create channel settings";
pub const LOG_CHANNEL_SETTINGS_LOAD_FAILED: &str = "Failed to load channel settings";
pub const LOG_CHANNEL_SETTINGS_DELETE_FAILED: &str = "Failed to delete stale channel settings";
pub const LOG_CHANNEL_SETTINGS_PRUNED: &str = "Pruned stale channel settings";
pub const LOG_CHANNELS_INITIALIZED: &str = "Initialized persistent channels";
pub const LOG_FILE_INDEX_DIRTY: &str = "File index is dirty, triggering reindex";
pub const LOG_FILE_INDEX_STALE: &str = "File index exceeded max age, triggering reindex";
pub const LOG_UPLOAD_BYPASS_FOLDER_RESTRICTION: &str =
    "Upload bypassed folder restriction via file_upload_anywhere";
pub const LOG_VOICE_DTLS_FAILED: &str = "Voice DTLS listener failed";
pub const LOG_VOICE_UNAVAILABLE: &str = "Voice chat will be unavailable";

// --- File Index ---
pub const LOG_FILE_INDEX_REBUILT: &str = "File index rebuilt";
pub const LOG_FILE_INDEX_BUILD_FAILED: &str = "Failed to build file index";
pub const LOG_FILE_INDEX_SEARCH_FAILED: &str = "Search failed, index may be corrupted";
pub const LOG_FILE_INDEX_DELETE_FAILED: &str = "Failed to delete corrupted index";

// --- i18n ---
pub const LOG_TRANSLATION_ERRORS: &str = "Translation errors";
pub const LOG_MISSING_TRANSLATION_KEY: &str = "Missing translation key";

// --- Transfers ---
pub const LOG_TRANSFER_CONNECTION: &str = "Transfer: connection";
pub const LOG_TRANSFER_HANDSHAKE_FAILED: &str = "Transfer: handshake failed";
pub const LOG_TRANSFER_LOGIN_FAILED: &str = "Transfer: login failed";
pub const LOG_TRANSFER_AUTHENTICATED: &str = "Transfer: authenticated";
pub const LOG_TRANSFER_REQUEST_FAILED: &str = "Transfer: request failed";
pub const LOG_TRANSFER_COMPLETE: &str = "Transfer: complete";

pub const LOG_DOWNLOAD_SCAN_FAILED: &str = "Download: failed to scan files";
pub const LOG_DOWNLOAD_STARTING: &str = "Download: starting";
pub const LOG_DOWNLOAD_SEND_FAILED: &str = "Download: failed to send response";
pub const LOG_DOWNLOAD_BANNED: &str = "Download: terminated by ban";
pub const LOG_DOWNLOAD_HASH_MISMATCH: &str = "Download: resume hash mismatch";
pub const LOG_DOWNLOAD_STREAM_ERROR: &str = "Download: streaming error";
pub const LOG_DOWNLOAD_COMPLETE: &str = "Download: complete";
pub const LOG_DOWNLOAD_FAILED: &str = "Download: failed";
pub const LOG_DOWNLOAD_RESUMING: &str = "Download: resuming";
pub const LOG_DOWNLOAD_SENDING: &str = "Download: sending";
pub const LOG_DOWNLOAD_ALREADY_COMPLETE: &str = "Download: already complete";

pub const LOG_UPLOAD_STARTING: &str = "Upload: starting";
pub const LOG_UPLOAD_SEND_FAILED: &str = "Upload: failed to send response";
pub const LOG_UPLOAD_BANNED: &str = "Upload: terminated by ban";
pub const LOG_UPLOAD_ERROR: &str = "Upload: error receiving file";
pub const LOG_UPLOAD_COMPLETE: &str = "Upload: complete";
pub const LOG_UPLOAD_FAILED: &str = "Upload: failed";
pub const LOG_UPLOAD_RECEIVING: &str = "Upload: receiving file";
pub const LOG_UPLOAD_EMPTY_FILE: &str = "Upload: created empty file";
pub const LOG_UPLOAD_ALREADY_COMPLETE: &str = "Upload: file already complete";
pub const LOG_UPLOAD_RESUMING: &str = "Upload: resuming";
pub const LOG_UPLOAD_RECEIVED: &str = "Upload: received data";
pub const LOG_UPLOAD_HASH_VERIFIED: &str = "Upload: hash verified";

// --- File Scanning ---
pub const LOG_SCAN_DIRECTORY: &str = "Scanning directory";
pub const LOG_SCAN_ENTRY: &str = "Processing entry";
pub const LOG_SCAN_METADATA_FAILED: &str = "Skipping entry, metadata failed";
pub const LOG_SCAN_NON_UTF8: &str = "Skipping non-UTF-8 filename";
pub const LOG_SCAN_DROPBOX_DENIED: &str = "Skipping, dropbox access denied";
pub const LOG_SCAN_ADDING_FILE: &str = "Adding file";
pub const LOG_SCAN_RECURSING: &str = "Recursing into directory";
pub const LOG_SCAN_SPECIAL_FILE: &str = "Skipping special file";
pub const LOG_SCAN_DONE: &str = "Done scanning directory";

// --- Voice DTLS ---
pub const LOG_VOICE_REJECTED_BANNED: &str = "Voice DTLS: rejected banned IP";
pub const LOG_VOICE_REJECTED_NO_SESSION: &str = "Voice DTLS: rejected, no voice session";
pub const LOG_VOICE_NEW_CONNECTION: &str = "Voice DTLS: new connection";
pub const LOG_VOICE_ACCEPT_ERROR: &str = "Voice DTLS: accept error";
pub const LOG_VOICE_CONNECTION_CLOSED: &str = "Voice DTLS: connection closed";
pub const LOG_VOICE_READ_ERROR: &str = "Voice DTLS: read error";
pub const LOG_VOICE_CONNECTION_TIMEOUT: &str = "Voice DTLS: connection timeout";
pub const LOG_VOICE_INVALID_PACKET: &str = "Voice DTLS: invalid packet";
pub const LOG_VOICE_SESSION_NOT_FOUND: &str = "Voice DTLS: session not found, closing connection";
pub const LOG_VOICE_KEEPALIVE: &str = "Voice DTLS: keepalive";
pub const LOG_VOICE_NO_PERMISSION: &str =
    "Voice DTLS: lacks voice_talk permission, dropping packet";
pub const LOG_VOICE_RELAY_FAILED: &str = "Voice DTLS: failed to relay";
pub const LOG_VOICE_CLEANUP_TIMEOUT: &str = "Voice DTLS: cleanup timed out client";
pub const LOG_VOICE_STALE_SESSION: &str =
    "Voice DTLS: removed stale voice session, no UDP connection";

// --- Flood Protection ---
/// Log: flood violation
pub const LOG_FLOOD_LIMITED: &str = "Chat rate limited";
/// Log: flood disconnect
pub const LOG_FLOOD_DISCONNECT: &str = "Disconnected for repeated flood violations";

// --- Handler: Ban ---
pub const LOG_BAN_CREATE_NOT_LOGGED_IN: &str = "BanCreate: not logged in";
pub const LOG_BAN_CREATE_PERMISSION_DENIED: &str = "BanCreate: permission denied";
pub const LOG_BAN_CREATE_ADMIN_NICKNAME: &str = "BanCreate: attempted to ban admin by nickname";
pub const LOG_BAN_CREATE_ADMIN_CIDR: &str = "BanCreate: attempted to ban CIDR with admin connected";
pub const LOG_BAN_CREATE_ADMIN_IP: &str = "BanCreate: attempted to ban IP with admin connected";
pub const LOG_BAN_CREATE_DB_ERROR: &str = "BanCreate: database error";
pub const LOG_BAN_CREATE_SUCCESS: &str = "BanCreate: success";
pub const LOG_BAN_DELETE_NOT_LOGGED_IN: &str = "BanDelete: not logged in";
pub const LOG_BAN_DELETE_PERMISSION_DENIED: &str = "BanDelete: permission denied";
pub const LOG_BAN_DELETE_DB_ERROR_NICKNAME: &str = "BanDelete: database error for nickname";
pub const LOG_BAN_DELETE_DB_ERROR_CIDR: &str = "BanDelete: database error for CIDR";
pub const LOG_BAN_DELETE_DB_ERROR_IP: &str = "BanDelete: database error for IP";
pub const LOG_BAN_DELETE_SUCCESS: &str = "BanDelete: success";
pub const LOG_BAN_LIST_NOT_LOGGED_IN: &str = "BanList: not logged in";
pub const LOG_BAN_LIST_PERMISSION_DENIED: &str = "BanList: permission denied";
pub const LOG_BAN_LIST_DB_ERROR: &str = "BanList: database error";

// --- Handler: Trust ---
pub const LOG_TRUST_CREATE_NOT_LOGGED_IN: &str = "TrustCreate: not logged in";
pub const LOG_TRUST_CREATE_PERMISSION_DENIED: &str = "TrustCreate: permission denied";
pub const LOG_TRUST_CREATE_DB_ERROR: &str = "TrustCreate: database error";
pub const LOG_TRUST_CREATE_SUCCESS: &str = "TrustCreate: success";
pub const LOG_TRUST_DELETE_NOT_LOGGED_IN: &str = "TrustDelete: not logged in";
pub const LOG_TRUST_DELETE_PERMISSION_DENIED: &str = "TrustDelete: permission denied";
pub const LOG_TRUST_DELETE_DB_ERROR_NICKNAME: &str = "TrustDelete: database error for nickname";
pub const LOG_TRUST_DELETE_DB_ERROR_CIDR: &str = "TrustDelete: database error for CIDR";
pub const LOG_TRUST_DELETE_DB_ERROR_IP: &str = "TrustDelete: database error for IP";
pub const LOG_TRUST_DELETE_SUCCESS: &str = "TrustDelete: success";
pub const LOG_TRUST_LIST_NOT_LOGGED_IN: &str = "TrustList: not logged in";
pub const LOG_TRUST_LIST_PERMISSION_DENIED: &str = "TrustList: permission denied";
pub const LOG_TRUST_LIST_DB_ERROR: &str = "TrustList: database error";

// --- Handler: User ---
pub const LOG_USER_CREATE_NOT_LOGGED_IN: &str = "UserCreate: not logged in";
pub const LOG_USER_CREATE_PERMISSION_DENIED: &str = "UserCreate: permission denied";
pub const LOG_USER_CREATE_UNOWNED_PERMISSION: &str =
    "UserCreate: tried to grant unowned permission";
pub const LOG_USER_CREATE_UNOWNED_REVOKE: &str = "UserCreate: tried to revoke unowned permission";
pub const LOG_USER_CREATE_UNOWNED_GROUP: &str =
    "UserCreate: tried to assign group with unowned permission";
pub const LOG_USER_CREATE_DB_ERROR: &str = "UserCreate: database error";
pub const LOG_USER_CREATE_HASH_ERROR: &str = "UserCreate: password hashing error";
pub const LOG_USER_CREATE_SUCCESS: &str = "UserCreate: success";
pub const LOG_USER_DELETE_NOT_LOGGED_IN: &str = "UserDelete: not logged in";
pub const LOG_USER_DELETE_PERMISSION_DENIED: &str = "UserDelete: permission denied";
pub const LOG_USER_DELETE_ADMIN: &str = "UserDelete: attempted to delete admin user";
pub const LOG_USER_DELETE_DB_ERROR: &str = "UserDelete: database error";
pub const LOG_USER_DELETE_SUCCESS: &str = "UserDelete: success";
pub const LOG_USER_UPDATE_NOT_LOGGED_IN: &str = "UserUpdate: not logged in";
pub const LOG_USER_UPDATE_PERMISSION_DENIED: &str = "UserUpdate: permission denied";
pub const LOG_USER_UPDATE_ADMIN: &str = "UserUpdate: attempted to edit admin user";
pub const LOG_USER_UPDATE_UNOWNED_PERMISSION: &str =
    "UserUpdate: tried to grant unowned permission";
pub const LOG_USER_UPDATE_UNOWNED_REVOKE: &str = "UserUpdate: tried to revoke unowned permission";
pub const LOG_USER_UPDATE_DB_ERROR: &str = "UserUpdate: database error";
pub const LOG_USER_UPDATE_DB_ERROR_LOOKUP: &str = "UserUpdate: database error looking up user";
pub const LOG_USER_UPDATE_DB_ERROR_TARGET: &str = "UserUpdate: database error getting target user";
pub const LOG_USER_UPDATE_DB_ERROR_USER: &str = "UserUpdate: database error getting user";
pub const LOG_USER_UPDATE_DB_ERROR_PERMISSIONS: &str =
    "UserUpdate: database error fetching permissions for merge";
pub const LOG_USER_UPDATE_DB_ERROR_GROUP: &str = "UserUpdate: database error fetching group";
pub const LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS: &str =
    "UserUpdate: database error fetching group permissions";
pub const LOG_USER_UPDATE_PASSWORD_VERIFY: &str = "UserUpdate: password verification error";
pub const LOG_USER_UPDATE_HASH_ERROR: &str = "UserUpdate: password hashing error";
pub const LOG_USER_UPDATE_SUCCESS: &str = "UserUpdate: success";
pub const LOG_USER_KICK_NOT_LOGGED_IN: &str = "UserKick: not logged in";
pub const LOG_USER_KICK_PERMISSION_DENIED: &str = "UserKick: permission denied";
pub const LOG_USER_KICK_DB_ERROR: &str = "UserKick: database error";
pub const LOG_USER_KICK_SUCCESS: &str = "UserKick: success";
pub const LOG_USER_EDIT_NOT_LOGGED_IN: &str = "UserEdit: not logged in";
pub const LOG_USER_EDIT_PERMISSION_DENIED: &str = "UserEdit: permission denied";
pub const LOG_USER_EDIT_ADMIN: &str = "UserEdit: attempted to edit admin user";
pub const LOG_USER_EDIT_DB_ERROR: &str = "UserEdit: database error";
pub const LOG_USER_INFO_NOT_LOGGED_IN: &str = "UserInfo: not logged in";
pub const LOG_USER_INFO_PERMISSION_DENIED: &str = "UserInfo: permission denied";
pub const LOG_USER_INFO_DB_ERROR: &str = "UserInfo: database error";
pub const LOG_USER_LIST_NOT_LOGGED_IN: &str = "UserList: not logged in";
pub const LOG_USER_LIST_PERMISSION_DENIED: &str = "UserList: permission denied";
pub const LOG_USER_LIST_DB_ERROR: &str = "UserList: database error";
pub const LOG_USER_AWAY_NOT_LOGGED_IN: &str = "UserAway: not logged in";
pub const LOG_USER_BACK_NOT_LOGGED_IN: &str = "UserBack: not logged in";
pub const LOG_USER_STATUS_NOT_LOGGED_IN: &str = "UserStatus: not logged in";
pub const LOG_USER_MESSAGE_NOT_LOGGED_IN: &str = "UserMessage: not logged in";
pub const LOG_USER_MESSAGE_PERMISSION_DENIED: &str = "UserMessage: permission denied";
pub const LOG_USER_BROADCAST_NOT_LOGGED_IN: &str = "UserBroadcast: not logged in";
pub const LOG_USER_BROADCAST_PERMISSION_DENIED: &str = "UserBroadcast: permission denied";

// --- Handler: Group ---
pub const LOG_GROUP_CREATE_NOT_LOGGED_IN: &str = "GroupCreate: not logged in";
pub const LOG_GROUP_CREATE_PERMISSION_DENIED: &str = "GroupCreate: permission denied";
pub const LOG_GROUP_CREATE_UNOWNED_PERMISSION: &str =
    "GroupCreate: tried to grant unowned permission";
pub const LOG_GROUP_CREATE_DB_ERROR: &str = "GroupCreate: database error";
pub const LOG_GROUP_CREATE_SUCCESS: &str = "GroupCreate: success";
pub const LOG_GROUP_DELETE_NOT_LOGGED_IN: &str = "GroupDelete: not logged in";
pub const LOG_GROUP_DELETE_PERMISSION_DENIED: &str = "GroupDelete: permission denied";
pub const LOG_GROUP_DELETE_DB_ERROR: &str = "GroupDelete: database error";
pub const LOG_GROUP_DELETE_SUCCESS: &str = "GroupDelete: success";
pub const LOG_GROUP_UPDATE_NOT_LOGGED_IN: &str = "GroupUpdate: not logged in";
pub const LOG_GROUP_UPDATE_PERMISSION_DENIED: &str = "GroupUpdate: permission denied";
pub const LOG_GROUP_UPDATE_UNOWNED_PERMISSION: &str =
    "GroupUpdate: tried to grant unowned permission";
pub const LOG_GROUP_UPDATE_DB_ERROR: &str = "GroupUpdate: database error";
pub const LOG_GROUP_UPDATE_DB_ERROR_PERMISSIONS: &str =
    "GroupUpdate: database error resolving permissions";
pub const LOG_GROUP_UPDATE_SUCCESS: &str = "GroupUpdate: success";
pub const LOG_GROUP_EDIT_NOT_LOGGED_IN: &str = "GroupEdit: not logged in";
pub const LOG_GROUP_EDIT_PERMISSION_DENIED: &str = "GroupEdit: permission denied";
pub const LOG_GROUP_EDIT_DB_ERROR: &str = "GroupEdit: database error";
pub const LOG_GROUP_LIST_NOT_LOGGED_IN: &str = "GroupList: not logged in";
pub const LOG_GROUP_LIST_PERMISSION_DENIED: &str = "GroupList: permission denied";
pub const LOG_GROUP_LIST_DB_ERROR: &str = "GroupList: database error";

// --- Handler: Tracker ---
pub const LOG_TRACKER_LIST_NOT_LOGGED_IN: &str = "TrackerList: not logged in";
pub const LOG_TRACKER_LIST_PERMISSION_DENIED: &str = "TrackerList: permission denied";
pub const LOG_TRACKER_LIST_DB_ERROR: &str = "TrackerList: database error";
pub const LOG_TRACKER_EDIT_NOT_LOGGED_IN: &str = "TrackerEdit: not logged in";
pub const LOG_TRACKER_EDIT_PERMISSION_DENIED: &str = "TrackerEdit: permission denied";
pub const LOG_TRACKER_EDIT_DB_ERROR: &str = "TrackerEdit: database error";
pub const LOG_TRACKER_ACCEPT_FINGERPRINT_NOT_LOGGED_IN: &str =
    "TrackerAcceptFingerprint: not logged in";
pub const LOG_TRACKER_ACCEPT_FINGERPRINT_PERMISSION_DENIED: &str =
    "TrackerAcceptFingerprint: permission denied";
pub const LOG_TRACKER_ACCEPT_FINGERPRINT_NO_PENDING: &str =
    "TrackerAcceptFingerprint: no pending fingerprint";
pub const LOG_TRACKER_ACCEPT_FINGERPRINT_DB_ERROR: &str =
    "TrackerAcceptFingerprint: database error";
pub const LOG_TRACKER_ACCEPT_FINGERPRINT_SUCCESS: &str = "TrackerAcceptFingerprint: success";
pub const LOG_TRACKER_ADD_NOT_LOGGED_IN: &str = "TrackerAdd: not logged in";
pub const LOG_TRACKER_ADD_PERMISSION_DENIED: &str = "TrackerAdd: permission denied";
pub const LOG_TRACKER_ADD_DB_ERROR: &str = "TrackerAdd: database error";
pub const LOG_TRACKER_ADD_LIMIT_REACHED: &str = "TrackerAdd: tracker limit reached";
pub const LOG_TRACKER_ADD_SUCCESS: &str = "TrackerAdd: success";
pub const LOG_TRACKER_UPDATE_NOT_LOGGED_IN: &str = "TrackerUpdate: not logged in";
pub const LOG_TRACKER_UPDATE_PERMISSION_DENIED: &str = "TrackerUpdate: permission denied";
pub const LOG_TRACKER_UPDATE_DB_ERROR: &str = "TrackerUpdate: database error";
pub const LOG_TRACKER_UPDATE_SUCCESS: &str = "TrackerUpdate: success";
pub const LOG_TRACKER_REMOVE_NOT_LOGGED_IN: &str = "TrackerRemove: not logged in";
pub const LOG_TRACKER_REMOVE_PERMISSION_DENIED: &str = "TrackerRemove: permission denied";
pub const LOG_TRACKER_REMOVE_DB_ERROR: &str = "TrackerRemove: database error";
pub const LOG_TRACKER_REMOVE_SUCCESS: &str = "TrackerRemove: success";

// --- Tracker Registration ---
// Manager
pub const LOG_TRACKER_REGISTRATION_BOOTSTRAP_DONE: &str = "Tracker bootstrap complete";
pub const LOG_TRACKER_REGISTRATION_SPAWN_SKIPPED: &str = "Tracker disabled, skipping registration";
pub const LOG_TRACKER_REGISTRATION_TASK_ABORTED: &str = "Tracker registration stopped";
pub const LOG_TRACKER_REGISTRATION_HANDLE_REPLACED: &str =
    "Tracker registration replaced without explicit stop";
// Task lifecycle
pub const LOG_TRACKER_REGISTRATION_EXITING: &str =
    "Tracker registration stopped: unrecoverable error";
pub const LOG_TRACKER_REGISTRATION_BACKOFF: &str = "Tracker registration backoff before retry";
// Connection setup
pub const LOG_TRACKER_REGISTRATION_INVALID_HOST: &str =
    "Tracker address could not be resolved as a hostname or IP literal";
pub const LOG_TRACKER_REGISTRATION_TCP_FAILED: &str = "Tracker TCP connect failed";
pub const LOG_TRACKER_REGISTRATION_TLS_FAILED: &str = "Tracker TLS handshake failed";
pub const LOG_TRACKER_REGISTRATION_NO_PEER_CERTS: &str = "Tracker peer presented no certificates";
// Fingerprint stages
pub const LOG_TRACKER_REGISTRATION_STAGE1_MISMATCH: &str = "Tracker fingerprint mismatch";
pub const LOG_TRACKER_REGISTRATION_STAGE2_MISMATCH: &str =
    "Tracker self-reported fingerprint disagrees with TLS certificate";
pub const LOG_TRACKER_REGISTRATION_STAGE2_MALFORMED: &str =
    "Tracker self-reported fingerprint is not in canonical form";

/// Sentinel substituted for `server_reported` in operator logs when the
/// tracker's self-reported fingerprint fails canonical-form validation.
/// Logging the raw bytes would let a hostile tracker stuff terminal-control
/// sequences into the warn line; the sentinel keeps the log scannable.
pub const TRACKER_FINGERPRINT_MALFORMED_SENTINEL: &str = "<malformed>";
pub const LOG_TRACKER_REGISTRATION_TOFU_WRITE_FAILED: &str = "Tracker fingerprint write failed";
pub const LOG_TRACKER_REGISTRATION_TOFU_PINNED: &str = "Tracker fingerprint pinned";
// BBS handshake
pub const LOG_TRACKER_REGISTRATION_SEND_HANDSHAKE_FAILED: &str = "Tracker send failed: Handshake";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR: &str =
    "Tracker read error: HandshakeResponse";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_CLOSED: &str =
    "Tracker closed connection during Handshake";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_REJECTED: &str = "Tracker rejected Handshake";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_UNEXPECTED: &str =
    "Tracker sent unexpected response to Handshake";
// Idle / mid-loop
pub const LOG_TRACKER_REGISTRATION_UNEXPECTED_FRAME: &str =
    "Tracker sent unexpected mid-idle frame, reconnecting";
pub const LOG_TRACKER_REGISTRATION_CLOSED_MID_IDLE: &str = "Tracker closed connection mid-idle";
pub const LOG_TRACKER_REGISTRATION_READ_ERROR_MID_IDLE: &str = "Tracker read error mid-idle";
// Register / refresh
pub const LOG_TRACKER_REGISTRATION_BUILD_PAYLOAD_FAILED: &str =
    "Tracker payload build failed: TrackerServerRegister";
pub const LOG_TRACKER_REGISTRATION_SEND_REGISTER_FAILED: &str =
    "Tracker send failed: TrackerServerRegister";
pub const LOG_TRACKER_REGISTRATION_CLOSED_AWAITING_RESPONSE: &str =
    "Tracker closed connection awaiting register response";
pub const LOG_TRACKER_REGISTRATION_RESPONSE_READ_ERROR: &str =
    "Tracker read error: TrackerServerRegisterResponse";
pub const LOG_TRACKER_REGISTRATION_RESPONSE_TIMEOUT: &str =
    "Tracker timeout: TrackerServerRegisterResponse";
pub const LOG_TRACKER_REGISTRATION_REFRESHED: &str = "Tracker refreshed";
pub const LOG_TRACKER_REGISTRATION_CONNECTED: &str = "Tracker connected";
pub const LOG_TRACKER_REGISTRATION_REGISTER_REJECTED: &str = "Tracker rejected register";
pub const LOG_TRACKER_REGISTRATION_INVALID_ERROR_KIND: &str =
    "Tracker sent malformed error_kind, treating as protocol error";
pub const LOG_TRACKER_REGISTRATION_TRACKER_REPORTED_ERROR: &str =
    "Tracker reported a protocol-level error, exiting";
pub const LOG_TRACKER_REGISTRATION_WRONG_FLOW_RESPONSE: &str =
    "Tracker sent a client-flow response on a server connection, exiting";

// Panic messages for the tracker manager's sync-lock acquisitions.
// These only ever surface if a lock is actually poisoned (a panic
// inside a critical section), which we never expect in normal
// operation — but per the project rule "no raw error strings" the
// strings live as named constants.
pub const EXPECT_TRACKER_STATUS_LOCK_POISONED: &str = "tracker status lock poisoned";
pub const EXPECT_TRACKER_MANAGER_LOCK_POISONED: &str = "tracker manager lock poisoned";

// --- Handler: Chat ---
pub const LOG_CHAT_SEND_NOT_LOGGED_IN: &str = "ChatSend: not logged in";
pub const LOG_CHAT_SEND_PERMISSION_DENIED: &str = "ChatSend: permission denied";
pub const LOG_CHAT_JOIN_NOT_LOGGED_IN: &str = "ChatJoin: not logged in";
pub const LOG_CHAT_JOIN_PERMISSION_DENIED: &str = "ChatJoin: permission denied";
pub const LOG_CHAT_JOIN_CREATE_DENIED: &str = "ChatJoin: ChatCreate permission denied";
pub const LOG_CHAT_LEAVE_NOT_LOGGED_IN: &str = "ChatLeave: not logged in";
pub const LOG_CHAT_LIST_NOT_LOGGED_IN: &str = "ChatList: not logged in";
pub const LOG_CHAT_LIST_PERMISSION_DENIED: &str = "ChatList: permission denied";
pub const LOG_CHAT_SECRET_NOT_LOGGED_IN: &str = "ChatSecret: not logged in";
pub const LOG_CHAT_SECRET_PERMISSION_DENIED: &str = "ChatSecret: permission denied";
pub const LOG_CHAT_SECRET_DB_ERROR: &str = "ChatSecret: database error";
pub const LOG_CHAT_TOPIC_NOT_LOGGED_IN: &str = "ChatTopicUpdate: not logged in";
pub const LOG_CHAT_TOPIC_PERMISSION_DENIED: &str = "ChatTopicUpdate: permission denied";
pub const LOG_CHAT_TOPIC_DB_ERROR: &str = "ChatTopicUpdate: database error";

// --- Handler: News ---
pub const LOG_NEWS_CREATE_NOT_LOGGED_IN: &str = "NewsCreate: not logged in";
pub const LOG_NEWS_CREATE_PERMISSION_DENIED: &str = "NewsCreate: permission denied";
pub const LOG_NEWS_CREATE_DB_ERROR: &str = "NewsCreate: database error";
pub const LOG_NEWS_CREATE_SUCCESS: &str = "NewsCreate: success";
pub const LOG_NEWS_DELETE_NOT_LOGGED_IN: &str = "NewsDelete: not logged in";
pub const LOG_NEWS_DELETE_PERMISSION_DENIED: &str = "NewsDelete: permission denied";
pub const LOG_NEWS_DELETE_ADMIN: &str = "NewsDelete: attempted to delete admin news";
pub const LOG_NEWS_DELETE_DB_ERROR_GET: &str = "NewsDelete: database error getting news";
pub const LOG_NEWS_DELETE_DB_ERROR_DELETE: &str = "NewsDelete: database error deleting news";
pub const LOG_NEWS_DELETE_SUCCESS: &str = "NewsDelete: success";
pub const LOG_NEWS_EDIT_NOT_LOGGED_IN: &str = "NewsEdit: not logged in";
pub const LOG_NEWS_EDIT_PERMISSION_DENIED: &str = "NewsEdit: permission denied";
pub const LOG_NEWS_EDIT_ADMIN: &str = "NewsEdit: attempted to edit admin news";
pub const LOG_NEWS_EDIT_DB_ERROR: &str = "NewsEdit: database error";
pub const LOG_NEWS_LIST_NOT_LOGGED_IN: &str = "NewsList: not logged in";
pub const LOG_NEWS_LIST_PERMISSION_DENIED: &str = "NewsList: permission denied";
pub const LOG_NEWS_LIST_DB_ERROR: &str = "NewsList: database error";
pub const LOG_NEWS_SHOW_NOT_LOGGED_IN: &str = "NewsShow: not logged in";
pub const LOG_NEWS_SHOW_PERMISSION_DENIED: &str = "NewsShow: permission denied";
pub const LOG_NEWS_SHOW_DB_ERROR: &str = "NewsShow: database error";
pub const LOG_NEWS_UPDATE_NOT_LOGGED_IN: &str = "NewsUpdate: not logged in";
pub const LOG_NEWS_UPDATE_PERMISSION_DENIED: &str = "NewsUpdate: permission denied";
pub const LOG_NEWS_UPDATE_ADMIN: &str = "NewsUpdate: attempted to edit admin news";
pub const LOG_NEWS_UPDATE_DB_ERROR_GET: &str = "NewsUpdate: database error getting news";
pub const LOG_NEWS_UPDATE_DB_ERROR: &str = "NewsUpdate: database error";
pub const LOG_NEWS_UPDATE_SUCCESS: &str = "NewsUpdate: success";

// --- Handler: File ---
pub const LOG_FILE_COPY_NOT_LOGGED_IN: &str = "FileCopy: not logged in";
pub const LOG_FILE_COPY_PERMISSION_DENIED: &str = "FileCopy: permission denied";
pub const LOG_FILE_COPY_ROOT_DENIED: &str = "FileCopy: file_root permission denied";
pub const LOG_FILE_COPY_DELETE_DENIED: &str = "FileCopy: file_delete permission denied";
pub const LOG_FILE_COPY_REMOVE_FAILED: &str = "FileCopy: failed to remove existing target";
pub const LOG_FILE_COPY_FAILED: &str = "FileCopy: failed";
pub const LOG_FILE_COPY_SUCCESS: &str = "FileCopy: success";
pub const LOG_FILE_CREATE_DIR_NOT_LOGGED_IN: &str = "FileCreateDir: not logged in";
pub const LOG_FILE_CREATE_DIR_ROOT_DENIED: &str = "FileCreateDir: file_root permission denied";
pub const LOG_FILE_CREATE_DIR_PERMISSION_DENIED: &str = "FileCreateDir: permission denied";
pub const LOG_FILE_CREATE_DIR_FAILED: &str = "FileCreateDir: failed";
pub const LOG_FILE_CREATE_DIR_SUCCESS: &str = "FileCreateDir: success";
pub const LOG_FILE_DELETE_NOT_LOGGED_IN: &str = "FileDelete: not logged in";
pub const LOG_FILE_DELETE_PERMISSION_DENIED: &str = "FileDelete: permission denied";
pub const LOG_FILE_DELETE_ROOT_DENIED: &str = "FileDelete: file_root permission denied";
pub const LOG_FILE_DELETE_SUCCESS: &str = "FileDelete: success";
pub const LOG_FILE_DELETE_FAILED: &str = "FileDelete: failed";
pub const LOG_FILE_INFO_NOT_LOGGED_IN: &str = "FileInfo: not logged in";
pub const LOG_FILE_INFO_PERMISSION_DENIED: &str = "FileInfo: permission denied";
pub const LOG_FILE_INFO_ROOT_DENIED: &str = "FileInfo: file_root permission denied";
pub const LOG_FILE_LIST_NOT_LOGGED_IN: &str = "FileList: not logged in";
pub const LOG_FILE_LIST_PERMISSION_DENIED: &str = "FileList: permission denied";
pub const LOG_FILE_LIST_ROOT_DENIED: &str = "FileList: file_root permission denied";
pub const LOG_FILE_MOVE_NOT_LOGGED_IN: &str = "FileMove: not logged in";
pub const LOG_FILE_MOVE_PERMISSION_DENIED: &str = "FileMove: permission denied";
pub const LOG_FILE_MOVE_ROOT_DENIED: &str = "FileMove: file_root permission denied";
pub const LOG_FILE_MOVE_DELETE_DENIED: &str = "FileMove: file_delete permission denied";
pub const LOG_FILE_MOVE_REMOVE_FAILED: &str = "FileMove: failed to remove existing target";
pub const LOG_FILE_MOVE_FAILED: &str = "FileMove: failed";
pub const LOG_FILE_MOVE_SUCCESS: &str = "FileMove: success";
pub const LOG_FILE_RENAME_NOT_LOGGED_IN: &str = "FileRename: not logged in";
pub const LOG_FILE_RENAME_PERMISSION_DENIED: &str = "FileRename: permission denied";
pub const LOG_FILE_RENAME_ROOT_DENIED: &str = "FileRename: file_root permission denied";
pub const LOG_FILE_RENAME_FAILED: &str = "FileRename: failed";
pub const LOG_FILE_RENAME_SUCCESS: &str = "FileRename: success";
pub const LOG_FILE_REINDEX_NOT_LOGGED_IN: &str = "FileReindex: not logged in";
pub const LOG_FILE_REINDEX_PERMISSION_DENIED: &str = "FileReindex: permission denied";
pub const LOG_FILE_REINDEX_TRIGGERED: &str = "FileReindex: triggered";
pub const LOG_FILE_REINDEX_IN_PROGRESS: &str = "FileReindex: already in progress";
pub const LOG_FILE_SEARCH_NOT_LOGGED_IN: &str = "FileSearch: not logged in";
pub const LOG_FILE_SEARCH_PERMISSION_DENIED: &str = "FileSearch: permission denied";
pub const LOG_FILE_SEARCH_ROOT_DENIED: &str = "FileSearch: file_root permission denied";
pub const LOG_FILE_SEARCH_ERROR: &str = "FileSearch: search error";
pub const LOG_FILE_SEARCH_PANIC: &str = "FileSearch: task panicked";

// --- Handler: Server Info ---
pub const LOG_SERVER_INFO_NOT_LOGGED_IN: &str = "ServerInfoUpdate: not logged in";
pub const LOG_SERVER_INFO_ADMIN_REQUIRED: &str = "ServerInfoUpdate: admin required";
pub const LOG_SERVER_INFO_DB_NAME: &str = "ServerInfoUpdate: database error setting server name";
pub const LOG_SERVER_INFO_DB_DESC: &str =
    "ServerInfoUpdate: database error setting server description";
pub const LOG_SERVER_INFO_DB_CONNECTIONS: &str =
    "ServerInfoUpdate: database error setting max_connections_per_ip";
pub const LOG_SERVER_INFO_DB_TRANSFERS: &str =
    "ServerInfoUpdate: database error setting max_transfers_per_ip";
pub const LOG_SERVER_INFO_DB_IMAGE: &str = "ServerInfoUpdate: database error setting server image";
pub const LOG_SERVER_INFO_DB_PUBLIC_ADDRESS: &str =
    "ServerInfoUpdate: database error setting public_address";
pub const LOG_SERVER_INFO_DB_REINDEX: &str =
    "ServerInfoUpdate: database error setting file_reindex_interval";
pub const LOG_SERVER_INFO_DB_PERSISTENT: &str =
    "ServerInfoUpdate: database error setting persistent_channels";
pub const LOG_SERVER_INFO_DB_AUTO_JOIN: &str =
    "ServerInfoUpdate: database error setting auto_join_channels";
pub const LOG_SERVER_INFO_DB_CHAT_BURST: &str =
    "ServerInfoUpdate: database error setting chat_burst_limit";
pub const LOG_SERVER_INFO_DB_CHAT_RATE: &str =
    "ServerInfoUpdate: database error setting chat_rate_limit";
pub const LOG_SERVER_INFO_DB_PASSWORD: &str =
    "ServerInfoUpdate: database error setting min_password_strength";
pub const LOG_SERVER_INFO_CHANNEL_CREATE_FAILED: &str =
    "ServerInfoUpdate: failed to create channel settings";
pub const LOG_SERVER_INFO_CHANNEL_DELETE_FAILED: &str =
    "ServerInfoUpdate: failed to delete channel settings";
pub const LOG_SERVER_INFO_SUCCESS: &str = "ServerInfoUpdate: success";

// --- Handler: Connection Monitor ---
pub const LOG_CONN_MONITOR_NOT_LOGGED_IN: &str = "ConnectionMonitor: not logged in";
pub const LOG_CONN_MONITOR_PERMISSION_DENIED: &str = "ConnectionMonitor: permission denied";

// --- Handler: Handshake ---
pub const LOG_HANDSHAKE_DUPLICATE: &str = "Handshake: duplicate attempt";
pub const LOG_HANDSHAKE_MAJOR_MISMATCH: &str = "Handshake: major version mismatch";
pub const LOG_HANDSHAKE_MINOR_MISMATCH: &str = "Handshake: minor version mismatch";
pub const LOG_HANDSHAKE_CLIENT_TOO_NEW: &str = "Handshake: client too new";

// --- Handler: Login ---
pub const LOG_LOGIN_HANDSHAKE_REQUIRED: &str = "Login: handshake required";
pub const LOG_LOGIN_ALREADY_LOGGED_IN: &str = "Login: already logged in";
pub const LOG_LOGIN_INVALID_CREDENTIALS: &str = "Login: invalid credentials";
pub const LOG_LOGIN_ACCOUNT_DISABLED: &str = "Login: account disabled";
pub const LOG_LOGIN_SUCCESS: &str = "Login: success";
pub const LOG_LOGIN_FIRST_ADMIN: &str = "Login: created first admin user";
pub const LOG_LOGIN_DB_ERROR: &str = "Login: database error";
pub const LOG_LOGIN_DB_NICKNAME: &str = "Login: database error checking nickname uniqueness";
pub const LOG_LOGIN_PERMISSIONS_ERROR: &str = "Login: error fetching permissions";
pub const LOG_LOGIN_GROUP_ERROR: &str = "Login: error fetching group";
pub const LOG_LOGIN_HASH_ERROR: &str = "Login: failed to hash password";
pub const LOG_LOGIN_CREATE_USER_ERROR: &str = "Login: failed to create first user";
pub const LOG_LOGIN_PASSWORD_VERIFY_ERROR: &str = "Login: password verification error";

// --- Handler: Voice ---
pub const LOG_VOICE_JOIN_NOT_LOGGED_IN: &str = "VoiceJoin: not logged in";
pub const LOG_VOICE_JOIN_PERMISSION_DENIED: &str = "VoiceJoin: permission denied";
pub const LOG_VOICE_LEAVE_NOT_LOGGED_IN: &str = "VoiceLeave: not logged in";

// =============================================================================
// Handler Names — wire-level message-type labels passed as the `command` field
// to `Error` responses via `send_error_and_disconnect(..., Some(...))`.
//
// Defined as constants (not literal strings at each callsite) so the compiler
// catches typos: a misspelled label would otherwise silently produce a
// malformed `Error.command` on the wire and a misleading log line. Each value
// must exactly match the corresponding `ClientMessage` enum variant name in
// `nexus-common/src/protocol.rs`.
// =============================================================================

pub const HANDLER_BAN_CREATE: &str = "BanCreate";
pub const HANDLER_BAN_DELETE: &str = "BanDelete";
pub const HANDLER_BAN_LIST: &str = "BanList";
pub const HANDLER_CHAT_JOIN: &str = "ChatJoin";
pub const HANDLER_CHAT_LEAVE: &str = "ChatLeave";
pub const HANDLER_CHAT_LIST: &str = "ChatList";
pub const HANDLER_CHAT_SECRET: &str = "ChatSecret";
pub const HANDLER_CHAT_SEND: &str = "ChatSend";
pub const HANDLER_CHAT_TOPIC_UPDATE: &str = "ChatTopicUpdate";
pub const HANDLER_CONNECTION_MONITOR: &str = "ConnectionMonitor";
pub const HANDLER_FILE_COPY: &str = "FileCopy";
pub const HANDLER_FILE_CREATE_DIR: &str = "FileCreateDir";
pub const HANDLER_FILE_DELETE: &str = "FileDelete";
pub const HANDLER_FILE_INFO: &str = "FileInfo";
pub const HANDLER_FILE_LIST: &str = "FileList";
pub const HANDLER_FILE_MOVE: &str = "FileMove";
pub const HANDLER_FILE_REINDEX: &str = "FileReindex";
pub const HANDLER_FILE_RENAME: &str = "FileRename";
pub const HANDLER_FILE_SEARCH: &str = "FileSearch";
pub const HANDLER_GROUP_CREATE: &str = "GroupCreate";
pub const HANDLER_GROUP_DELETE: &str = "GroupDelete";
pub const HANDLER_GROUP_EDIT: &str = "GroupEdit";
pub const HANDLER_GROUP_LIST: &str = "GroupList";
pub const HANDLER_GROUP_UPDATE: &str = "GroupUpdate";
pub const HANDLER_LOGIN: &str = "Login";
pub const HANDLER_NEWS_CREATE: &str = "NewsCreate";
pub const HANDLER_NEWS_DELETE: &str = "NewsDelete";
pub const HANDLER_NEWS_EDIT: &str = "NewsEdit";
pub const HANDLER_NEWS_LIST: &str = "NewsList";
pub const HANDLER_NEWS_SHOW: &str = "NewsShow";
pub const HANDLER_NEWS_UPDATE: &str = "NewsUpdate";
pub const HANDLER_SERVER_INFO_UPDATE: &str = "ServerInfoUpdate";
pub const HANDLER_TRACKER_ACCEPT_FINGERPRINT: &str = "TrackerAcceptFingerprint";
pub const HANDLER_TRACKER_ADD: &str = "TrackerAdd";
pub const HANDLER_TRACKER_EDIT: &str = "TrackerEdit";
pub const HANDLER_TRACKER_LIST: &str = "TrackerList";
pub const HANDLER_TRACKER_REMOVE: &str = "TrackerRemove";
pub const HANDLER_TRACKER_UPDATE: &str = "TrackerUpdate";
pub const HANDLER_TRUST_CREATE: &str = "TrustCreate";
pub const HANDLER_TRUST_DELETE: &str = "TrustDelete";
pub const HANDLER_TRUST_LIST: &str = "TrustList";
pub const HANDLER_USER_AWAY: &str = "UserAway";
pub const HANDLER_USER_BACK: &str = "UserBack";
pub const HANDLER_USER_BROADCAST: &str = "UserBroadcast";
pub const HANDLER_USER_CREATE: &str = "UserCreate";
pub const HANDLER_USER_DELETE: &str = "UserDelete";
pub const HANDLER_USER_EDIT: &str = "UserEdit";
pub const HANDLER_USER_INFO: &str = "UserInfo";
pub const HANDLER_USER_KICK: &str = "UserKick";
pub const HANDLER_USER_LIST: &str = "UserList";
pub const HANDLER_USER_MESSAGE: &str = "UserMessage";
pub const HANDLER_USER_STATUS: &str = "UserStatus";
pub const HANDLER_USER_UPDATE: &str = "UserUpdate";
pub const HANDLER_VOICE_JOIN: &str = "VoiceJoin";
pub const HANDLER_VOICE_LEAVE: &str = "VoiceLeave";
