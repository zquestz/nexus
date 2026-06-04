//! Constants for server operator messages and configuration.
//! User-facing (client) error messages live in handlers/errors.rs; this file
//! is operator-only (logs, startup, diagnostics).

use nexus_common::validators::PasswordStrength;

pub const FILES_DIR_NAME: &str = "files";
pub const FILES_SHARED_DIR: &str = "shared";
pub const FILES_USERS_DIR: &str = "users";

/// Case-insensitive; includes the leading space.
pub const FOLDER_SUFFIX_UPLOAD: &str = " [NEXUS-UL]";
/// Case-insensitive; includes the leading space.
pub const FOLDER_SUFFIX_DROPBOX: &str = " [NEXUS-DB]";
/// Prefix for user-specific drop boxes; includes the leading space.
pub const FOLDER_SUFFIX_DROPBOX_PREFIX: &str = " [NEXUS-DB-";

/// Fallback when a path has no filename or a non-UTF-8 filename.
pub const DEFAULT_FILENAME: &str = "file";

pub const CONFIG_KEY_MAX_CONNECTIONS_PER_IP: &str = "max_connections_per_ip";
/// Matches migration default.
pub const DEFAULT_MAX_CONNECTIONS_PER_IP: usize = 5;

pub const CONFIG_KEY_MAX_TRANSFERS_PER_IP: &str = "max_transfers_per_ip";
/// Matches migration default.
pub const DEFAULT_MAX_TRANSFERS_PER_IP: usize = 3;

pub const CONFIG_KEY_FILE_REINDEX_INTERVAL: &str = "file_reindex_interval";
/// Minutes; matches migration default. 0 disables automatic reindexing.
pub const DEFAULT_FILE_REINDEX_INTERVAL: u32 = 5;

/// Max age before a forced reindex runs even if nothing was marked dirty,
/// so external filesystem changes (e.g. admin adds files via SSH) are picked up.
pub const FILE_INDEX_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

// Defense-in-depth DB-setter errors (via `io::Error::other`). Operator-log
// only — handlers send the generic translated `err-database` to clients. Do
// NOT add to locale files; intentionally English-only.

pub const ERR_SERVER_NAME_EMPTY: &str = "Server name cannot be empty";
pub const ERR_SERVER_NAME_TOO_LONG: &str = "Server name is too long";
pub const ERR_SERVER_NAME_NEWLINES: &str = "Server name cannot contain newlines";
pub const ERR_SERVER_NAME_INVALID_CHARS: &str = "Server name contains invalid characters";
pub const ERR_SERVER_DESC_TOO_LONG: &str = "Server description is too long";
pub const ERR_SERVER_DESC_NEWLINES: &str = "Server description cannot contain newlines";
pub const ERR_SERVER_DESC_INVALID_CHARS: &str = "Server description contains invalid characters";

/// Prefix; format with the limit.
pub const ERR_SCHEDULER_CHUNK_SIZE_TOO_SMALL: &str = "scheduler chunk size must be ≥";
/// Prefix; format with the limit.
pub const ERR_SCHEDULER_CHUNK_SIZE_TOO_LARGE: &str = "scheduler chunk size must be ≤";

pub const ERR_SERVER_IMAGE_TOO_LARGE: &str = "Server image is too large";
pub const ERR_SERVER_IMAGE_INVALID_FORMAT: &str = "Server image has invalid format";
pub const ERR_SERVER_IMAGE_UNSUPPORTED_TYPE: &str = "Server image has unsupported type";

pub const ERR_PUBLIC_ADDRESS_TOO_LONG: &str = "Public address is too long";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_SCHEME: &str = "Public address must not include a URL scheme";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_BRACKETS: &str = "Public address must not include brackets";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_PATH: &str = "Public address must not include a path";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_USERINFO: &str = "Public address must not include a username";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_WHITESPACE: &str =
    "Public address must not contain whitespace";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_PORT: &str = "Public address must not include a port";
pub const ERR_PUBLIC_ADDRESS_CONTAINS_ZONE_ID: &str =
    "Public address must not include an IPv6 zone identifier";
pub const ERR_PUBLIC_ADDRESS_INVALID_FORMAT: &str =
    "Public address is not a valid hostname or IP address";

/// Matches migration default.
pub const DEFAULT_SERVER_NAME: &str = "Nexus BBS";
pub const DEFAULT_SERVER_DESCRIPTION: &str = "";
pub const DEFAULT_SERVER_IMAGE: &str = "";
/// Empty = unset; admin must configure to enable URI sharing.
pub const DEFAULT_PUBLIC_ADDRESS: &str = "";

/// Subdirectory within the platform data directory (e.g. `~/.local/share/nexusd/`
/// on Linux). Hosts the database, TLS cert/key, file search index, and logs.
pub const DATA_DIR_NAME: &str = "nexusd";

/// tracing-appender appends the date (e.g. nexusd.2025-07-11). Passed to
/// `nexus_common::logging::init` and `purge_old_logs`.
pub const LOG_FILE_PREFIX: &str = "nexusd";

pub const DATABASE_FILENAME: &str = "nexus.db";

pub const CONFIG_KEY_SERVER_NAME: &str = "server_name";
pub const CONFIG_KEY_SERVER_DESCRIPTION: &str = "server_description";
pub const CONFIG_KEY_SERVER_IMAGE: &str = "server_image";
pub const CONFIG_KEY_PUBLIC_ADDRESS: &str = "public_address";

pub const FEATURE_CHAT: &str = "chat";
pub const FEATURE_NEWS: &str = "news";

/// Space-separated list; these channels survive restart and can't be deleted when empty.
pub const CONFIG_KEY_PERSISTENT_CHANNELS: &str = "persistent_channels";
pub const DEFAULT_PERSISTENT_CHANNELS: &str = nexus_common::validators::DEFAULT_CHANNEL;

/// Space-separated list; auto-joined by users on login.
pub const CONFIG_KEY_AUTO_JOIN_CHANNELS: &str = "auto_join_channels";
/// Defaults to the persistent channels for backward compatibility.
pub const DEFAULT_AUTO_JOIN_CHANNELS: &str = nexus_common::validators::DEFAULT_CHANNEL;

pub const CONFIG_KEY_MIN_PASSWORD_STRENGTH: &str = "min_password_strength";
pub const DEFAULT_MIN_PASSWORD_STRENGTH: PasswordStrength = PasswordStrength::Good;

pub const CONFIG_KEY_CHAT_BURST_LIMIT: &str = "chat_burst_limit";
/// 0 = no burst, capacity is 1.
pub const DEFAULT_CHAT_BURST_LIMIT: u32 = 5;

pub const CONFIG_KEY_CHAT_RATE_LIMIT: &str = "chat_rate_limit";
/// Messages per minute; 0 = disabled.
pub const DEFAULT_CHAT_RATE_LIMIT: u32 = 20;

/// Bytes/sec; 0 = unlimited.
pub const CONFIG_KEY_MAX_OUTBOUND_RATE: &str = "max_outbound_rate";
/// Bytes/sec; 0 = unlimited.
pub const DEFAULT_MAX_OUTBOUND_RATE: u64 = 0;

/// Normal per-connection message queue depth. Large enough to absorb short
/// client hiccups while still bounding memory for slow consumers.
pub const SESSION_MESSAGE_QUEUE_CAPACITY: usize = 1024;
/// Control queue is reserved for the slow-client disconnect only.
pub const SESSION_CONTROL_QUEUE_CAPACITY: usize = 1;

/// Upper bound for one BBS protocol message write. This is intentionally high
/// enough for very slow links while still bounding peers that stop reading.
pub const BBS_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// WF2Q+ scheduler chunk size in bytes.
pub const CONFIG_KEY_SCHEDULER_CHUNK_SIZE: &str = "scheduler_chunk_size";

/// 5 balances concurrency vs. SQLite's single-writer limit for a small-to-medium
/// BBS workload. WAL mode allows many readers + one writer, so this is ample for
/// the read-heavy load.
pub const MAX_DB_CONNECTIONS: u32 = 5;

pub const CERT_FILENAME: &str = "server.crt";
pub const KEY_FILENAME: &str = "server.key";
pub const TLS_CERT_COMMON_NAME: &str = "Nexus BBS";
/// Error-string pattern matched to detect an unclean TLS close.
pub const TLS_CLOSE_NOTIFY_MSG: &str = "peer closed connection without sending TLS close_notify";

pub const MSG_BANNER: &str = "Nexus BBS Server v";
pub const MSG_DATABASE: &str = "Database: ";
pub const MSG_CERTIFICATES: &str = "Certificates: ";
pub const MSG_LISTENING: &str = "BBS port: ";
pub const MSG_TRANSFER_LISTENING: &str = "Transfer port: ";
pub const MSG_WS_LISTENING: &str = "WebSocket port: ";
pub const MSG_WS_TRANSFER_LISTENING: &str = "WebSocket transfer port: ";
pub const MSG_VOICE_LISTENING: &str = "Voice UDP port: ";
pub const MSG_LOG_LEVEL: &str = "Log level: ";
pub const MSG_LOG_DIR: &str = "Log directory: ";
pub const MSG_SHUTDOWN_RECEIVED: &str = "Shutdown signal received";

/// expect() message — panics if the crypto provider fails to install (required
/// for any TLS/DTLS operation).
pub const ERR_RUSTLS_PROVIDER: &str = "failed to install rustls crypto provider";

/// expect() message — a poisoned lock means another thread panicked while
/// holding it; unrecoverable.
pub const ERR_IP_CACHE_POISONED: &str = "ip rule cache lock poisoned";

/// Extremely rare — Windows without `%APPDATA%`, Linux without `HOME`, etc.
pub const ERR_NO_DATA_DIR: &str = "Platform does not provide a data directory";

pub const ERR_DATA_DIR_NOT_ABSOLUTE: &str = "--data-dir must be an absolute path: ";
pub const ERR_CREATE_DATA_DIR: &str = "Failed to create data directory: ";

#[cfg(unix)]
pub const ERR_SET_DATA_DIR_PERMS: &str = "Failed to set data directory permissions: ";

pub const ERR_DATABASE_INIT: &str = "Failed to initialize database: ";
pub const ERR_TRACKER_BOOTSTRAP_FAILED: &str = "Failed to bootstrap tracker manager: ";
pub const ERR_TLS_INIT: &str = "Failed to initialize TLS: ";
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";
pub const ERR_BBS_WRITE_TIMEOUT: &str = "BBS message write timed out";
pub const ERR_EGRESS_STAGE_FAILED: &str = "egress message staging failed";

#[cfg(unix)]
pub const ERR_SET_PERMISSIONS: &str = "Failed to set file permissions: ";

#[cfg(unix)]
pub const ERR_SIGNAL_SIGTERM: &str = "Failed to setup SIGTERM handler";

#[cfg(unix)]
pub const ERR_SIGNAL_SIGINT: &str = "Failed to setup SIGINT handler";

#[cfg(not(unix))]
pub const ERR_SIGNAL_CTRLC: &str = "Failed to setup Ctrl+C handler";

pub const LOG_UPNP_SETUP_FAILED: &str = "UPnP setup failed";
pub const MSG_UPNP_CONTINUE: &str = "Server will continue without UPnP port forwarding.";
pub const MSG_UPNP_MANUAL: &str =
    "You may need to manually configure port forwarding on your router.";
pub const LOG_UPNP_REMOVE_FAILED: &str = "Failed to remove UPnP port mapping";

/// Re-exported from `nexus-common` so the value is defined once for the workspace.
pub use nexus_common::DEFAULT_LOCALE;

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

pub const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";
pub const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";
pub const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";

pub const MSG_FILE_ROOT: &str = "File area: ";
/// Caller-side prefix wrapping any `init_file_area` failure.
pub const ERR_INIT_FILE_AREA: &str = "Failed to initialize file area: ";
pub const ERR_FILE_INVALID_PATH: &str = "Invalid file path";
/// Raised on a path-traversal attempt.
pub const ERR_FILE_ACCESS_DENIED: &str = "Access denied: path outside file area";
pub const ERR_FILE_NOT_FOUND: &str = "File or directory not found";
pub const ERR_FILE_CANONICALIZE: &str = "Failed to resolve path";
pub const ERR_FILE_ROOT_CANONICALIZE: &str = "Failed to canonicalize file root: ";
pub const ERR_FILE_INVALID_AREA_ROOT: &str = "Area root must be an absolute path";
pub const ERR_PERSONAL_FILE_AREA_TARGET_EXISTS: &str = "personal file area target already exists";
pub const ERR_PERSONAL_FILE_AREA_MIGRATION_FAILED: &str = "personal file area migration failed";
pub const ERR_PERSONAL_FILE_AREA_PATH_BUSY: &str = "personal file area path is busy";

pub const ERR_CHANNEL_CLOSED: &str = "channel closed";

// expect() messages for poisoned mutexes — a previous holder panicked mid-update,
// leaving the in-memory state unknown-shape. Should never fire in normal operation.
pub const ERR_TRANSFER_REGISTRY_LOCK_POISONED: &str = "transfer registry lock poisoned";
pub const ERR_TRANSFER_IDENTITY_LOCK_POISONED: &str = "transfer identity lock poisoned";
pub const ERR_BAN_TX_LOCK_POISONED: &str = "ban_tx lock poisoned";
pub const ERR_CONNECTION_TRACKER_LOCK: &str = "connection tracker lock";
pub const ERR_TRANSFER_TRACKER_LOCK: &str = "transfer tracker lock";
pub const ERR_VOICE_TRACKER_LOCK: &str = "voice tracker lock";
pub const ERR_VOICE_DEMUX_ACCEPT_TX_LOCK: &str = "voice UDP demux accept tx lock";
pub const ERR_VOICE_DEMUX_CHILDREN_LOCK: &str = "voice UDP demux children lock";
pub const ERR_VOICE_DEMUX_READ_TASK_LOCK: &str = "voice UDP demux read task lock";
pub const ERR_VOICE_UDP_CHILD_TX_LOCK: &str = "voice UDP child tx lock";

/// expect() message — `duration_since(UNIX_EPOCH)` failed (clock set before 1970).
/// Used wherever the daemon derives a Unix-epoch timestamp; the daemon can't
/// sensibly continue with a clock skewed that far back.
pub const ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK: &str =
    "system time is before Unix epoch — check system clock configuration";

/// expect() message — building an `IpNet` from a parsed `IpAddr` can't fail.
/// Used by `db::bans::row_to_ban` / `db::trusts::row_to_trust`.
pub const ERR_VALID_IP_PREFIX: &str = "valid prefix";

/// debug_assert message — a string reached the cache/DB write boundary without
/// `canonicalize_target`. The handler layer is the single IP-canonicalization funnel.
pub const ERR_TARGET_NOT_CANONICAL: &str =
    "ip_or_cidr must be canonical (use canonicalize_target at the handler boundary)";

/// expect() message — `Ipv4Net::new` can't fail in `ip_rule_cache::fold_ipv4_mapped`,
/// which only passes a prefix in [0,32] (derived from an IPv6 prefix in [96,128]).
pub const ERR_IPV4_PREFIX_FROM_MAPPED: &str = "ipv6 prefix - 96 yields IPv4 prefix in [0, 32]";

/// expect() message — `expires_at` is `Some` after the `None` guard in
/// `ip_rule_cache::rebuild_tries`.
pub const ERR_IP_RULE_EXPIRY_MISSING: &str = "expires_at is present after None guard";

/// expect() message — `sessions` confirmed non-empty just upstream (`users::manager::helpers`).
pub const ERR_SESSIONS_NOT_EMPTY: &str = "sessions is not empty";

/// expect() message — `target_sessions` confirmed non-empty just upstream (handlers/user_info).
pub const ERR_TARGET_SESSIONS_NON_EMPTY: &str = "target_sessions is non-empty";

/// expect() message — `DEFAULT_LOCALE` is hand-edited to always parse as a valid locale.
pub const ERR_DEFAULT_LOCALE_INVALID: &str = "DEFAULT_LOCALE is a valid locale";

/// `PasswordError::TaskJoin` Display — the Argon2 blocking task panicked/was
/// cancelled. Surfaces so callers fail closed rather than treating it as verify-success.
pub const ERR_PASSWORD_TASK_JOIN: &str = "password task did not complete";

// `files::index::FileIndex` prefixes, composed `format!("{}{}", PREFIX, err)`.
pub const ERR_FILE_INDEX_CREATE_TEMP: &str = "Failed to create temp index: ";
pub const ERR_FILE_INDEX_SET_PERMISSIONS: &str = "Failed to set index permissions: ";
pub const ERR_FILE_INDEX_WRITE_ENTRY: &str = "Failed to write index entry: ";
pub const ERR_FILE_INDEX_FLUSH: &str = "Failed to flush index: ";
pub const ERR_FILE_INDEX_SWAP: &str = "Failed to swap index file: ";
pub const ERR_FILE_INDEX_SEARCH_PATTERN: &str = "Invalid search pattern: ";

// `files::operations` spawn_blocking failures; prefix identifies the failed call.
pub const ERR_FILE_OP_REMOVE_TASK: &str = "remove task failed: ";
pub const ERR_FILE_OP_COPY_TASK: &str = "copy task failed: ";
pub const ERR_FILE_OP_RENAME_TASK: &str = "rename task failed: ";

// `files::activity`: activity keys require a path with parent and filename.
pub const ERR_FILE_ACTIVITY_INVALID_PATH: &str =
    "activity key requires a path with parent and filename";

// Transfer-port auth flow (port 7501 / WS 7503). Not localized — the transfer
// port has no locale negotiation before auth completes. Pure-static below;
// prefix-style further down.
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

pub const ERR_TRANSFER_READ_HANDSHAKE: &str = "Failed to read handshake: ";
pub const ERR_TRANSFER_READ_LOGIN: &str = "Failed to read login: ";
pub const ERR_TRANSFER_READ_MESSAGE: &str = "Failed to read message: ";
pub const ERR_TRANSFER_DB_ERROR: &str = "Database error: ";
pub const ERR_TRANSFER_PASSWORD_VERIFY_ERROR: &str = "Password verification error: ";
pub const ERR_TRANSFER_VERSION_MINOR_MISMATCH: &str =
    "Minor version mismatch, pre-1.0, server minor: ";
pub const ERR_TRANSFER_VERSION_CLIENT_TOO_NEW: &str = "Client version too new, server minor: ";

// `voice::udp` cert/key load + listener-bind prefixes. The listener prefix
// has a trailing space so callers append `addr: err`.
pub const ERR_VOICE_DTLS_LISTENER_PREFIX: &str = "Failed to create voice DTLS listener on ";
pub const ERR_VOICE_READ_CERT_FILE: &str = "Failed to read certificate file: ";
pub const ERR_VOICE_READ_KEY_FILE: &str = "Failed to read private key file: ";
pub const ERR_VOICE_PARSE_CERT: &str = "Failed to parse certificate: ";

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
pub const LOG_EGRESS_CHUNK_SIZE_INVALID: &str =
    "Configured egress scheduler chunk size is invalid; using default";
pub const LOG_EGRESS_REGISTER_FAILED: &str = "Egress registration failed";
pub const LOG_EGRESS_REGISTER_REJECTED: &str = "Egress registration rejected";
pub const LOG_EGRESS_UNREGISTER_FAILED: &str = "Egress unregister failed";
pub const LOG_EGRESS_TRANSITION_FAILED: &str = "Egress user transition failed";
pub const LOG_EGRESS_RATE_UPDATE_FAILED: &str = "Egress rate update failed";
pub const LOG_EGRESS_CHUNK_SIZE_UPDATE_FAILED: &str = "Egress chunk-size update failed";
pub const LOG_EGRESS_USER_WEIGHT_UPDATE_FAILED: &str = "Egress user weight update failed";
pub const LOG_EGRESS_STAGE_FAILED: &str = "Egress message staging failed";
pub const LOG_EGRESS_DISPATCH_CLOSED: &str = "Egress dispatch channel closed";
pub const LOG_EGRESS_DISPATCH_WRONG_CONNECTION: &str =
    "Egress dispatch targeted the wrong connection";
pub const LOG_EGRESS_ACK_FAILED: &str = "Egress chunk ack failed";
pub const LOG_EGRESS_WRITE_FAILED_NOTIFY_FAILED: &str = "Egress write-failure notify failed";

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

pub const LOG_FILE_INDEX_REBUILT: &str = "File index rebuilt";
pub const LOG_FILE_INDEX_BUILD_FAILED: &str = "Failed to build file index";
pub const LOG_FILE_INDEX_SEARCH_FAILED: &str = "Search failed, index may be corrupted";
pub const LOG_FILE_INDEX_DELETE_FAILED: &str = "Failed to delete corrupted index";

pub const LOG_TRANSLATION_ERRORS: &str = "Translation errors";
pub const LOG_MISSING_TRANSLATION_KEY: &str = "Missing translation key";

pub const LOG_TRANSFER_CONNECTION: &str = "Transfer: connection";
pub const LOG_TRANSFER_HANDSHAKE_FAILED: &str = "Transfer: handshake failed";
pub const LOG_TRANSFER_LOGIN_FAILED: &str = "Transfer: login failed";
pub const LOG_TRANSFER_AUTHENTICATED: &str = "Transfer: authenticated";
pub const LOG_TRANSFER_REQUEST_FAILED: &str = "Transfer: request failed";
pub const LOG_TRANSFER_REGISTRATION_DB_ERROR: &str = "Transfer: registration database error";
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

pub const LOG_SCAN_DIRECTORY: &str = "Scanning directory";
pub const LOG_SCAN_ENTRY: &str = "Processing entry";
pub const LOG_SCAN_METADATA_FAILED: &str = "Skipping entry, metadata failed";
pub const LOG_SCAN_NON_UTF8: &str = "Skipping non-UTF-8 filename";
pub const LOG_SCAN_DROPBOX_DENIED: &str = "Skipping, dropbox access denied";
pub const LOG_SCAN_ADDING_FILE: &str = "Adding file";
pub const LOG_SCAN_RECURSING: &str = "Recursing into directory";
pub const LOG_SCAN_SPECIAL_FILE: &str = "Skipping special file";
pub const LOG_SCAN_DONE: &str = "Done scanning directory";

pub const LOG_VOICE_REJECTED_BANNED: &str = "Voice DTLS: rejected banned IP";
pub const LOG_VOICE_REJECTED_LIMIT: &str = "Voice DTLS: rejected, per-IP voice limit reached";
pub const LOG_VOICE_NEW_CONNECTION: &str = "Voice DTLS: new connection";
pub const LOG_VOICE_ACCEPT_ERROR: &str = "Voice DTLS: accept error";
pub const LOG_VOICE_HANDSHAKE_FAILED: &str = "Voice DTLS: handshake failed";
pub const LOG_VOICE_HANDSHAKE_TIMEOUT: &str = "Voice DTLS: handshake timeout";
pub const LOG_VOICE_CONNECTION_CLOSED: &str = "Voice DTLS: connection closed";
pub const LOG_VOICE_READ_ERROR: &str = "Voice DTLS: read error";
pub const LOG_VOICE_INVALID_PACKET: &str = "Voice DTLS: invalid packet";
pub const LOG_VOICE_SESSION_NOT_FOUND: &str = "Voice DTLS: session not found, closing connection";
pub const LOG_VOICE_KEEPALIVE: &str = "Voice DTLS: keepalive";
pub const LOG_VOICE_NO_PERMISSION: &str =
    "Voice DTLS: lacks voice_talk permission, dropping packet";
pub const LOG_VOICE_RELAY_FAILED: &str = "Voice DTLS: failed to relay";
pub const LOG_VOICE_CLEANUP_TIMEOUT: &str = "Voice DTLS: cleanup timed out client";
pub const LOG_VOICE_STALE_SESSION: &str =
    "Voice DTLS: removed stale voice session, no UDP connection";

pub const LOG_FLOOD_LIMITED: &str = "Chat rate limited";
pub const LOG_FLOOD_DISCONNECT: &str = "Disconnected for repeated flood violations";

/// DB error resolving a user's effective bandwidth weight; fires on the login
/// snapshot path (admin/group cascades carry the resolved value in-tx).
pub const LOG_BANDWIDTH_WEIGHT_RESOLVE_FAILED: &str =
    "Failed to resolve bandwidth weight from database";

/// Fires only if a DB row value was outside [MIN_BANDWIDTH_WEIGHT, u16::MAX] —
/// implies out-of-band corruption (hand-edit, unvalidated restore); investigate.
pub const LOG_BANDWIDTH_WEIGHT_CLAMPED: &str =
    "Bandwidth weight read from DB was outside valid range and was clamped";

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
pub const LOG_USER_UPDATE_DB_ERROR_GROUP: &str = "UserUpdate: database error fetching group";
pub const LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS: &str =
    "UserUpdate: database error fetching group permissions";
pub const LOG_USER_UPDATE_DB_ERROR_PERMISSIONS: &str =
    "UserUpdate: database error reading user permissions";
pub const LOG_USER_UPDATE_DB_ERROR_DUPLICATE_CHECK: &str =
    "UserUpdate: database error checking duplicate username";
pub const LOG_USER_UPDATE_FILE_AREA_TARGET_EXISTS: &str =
    "UserUpdate: personal file area target already exists";
pub const LOG_USER_UPDATE_FILE_AREA_BUSY: &str = "UserUpdate: personal file area is busy";
pub const LOG_USER_UPDATE_FILE_AREA_MIGRATE_FAILED: &str =
    "UserUpdate: personal file area migration failed";
pub const LOG_USER_UPDATE_FILE_AREA_MIGRATED: &str = "UserUpdate: personal file area migrated";
pub const LOG_USER_UPDATE_FILE_AREA_ROLLBACK_FAILED: &str =
    "UserUpdate: personal file area rollback failed";
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
pub const LOG_TRACKER_REGISTRATION_BOOTSTRAP_DONE: &str = "Tracker bootstrap complete";
pub const LOG_TRACKER_REGISTRATION_SPAWN_SKIPPED: &str = "Tracker disabled, skipping registration";
pub const LOG_TRACKER_REGISTRATION_TASK_ABORTED: &str = "Tracker registration stopped";
pub const LOG_TRACKER_REGISTRATION_HANDLE_REPLACED: &str =
    "Tracker registration replaced without explicit stop";
pub const LOG_TRACKER_REGISTRATION_EXITING: &str =
    "Tracker registration stopped: unrecoverable error";
pub const LOG_TRACKER_REGISTRATION_BACKOFF: &str = "Tracker registration backoff before retry";
pub const LOG_TRACKER_REGISTRATION_INVALID_HOST: &str =
    "Tracker address could not be resolved as a hostname or IP literal";
pub const LOG_TRACKER_REGISTRATION_DNS_FAILED: &str = "Tracker hostname DNS lookup failed";
pub const LOG_TRACKER_REGISTRATION_DNS_TIMEOUT: &str =
    "Tracker hostname DNS lookup exceeded timeout";
pub const LOG_TRACKER_REGISTRATION_DNS_NO_RECORDS: &str =
    "Tracker hostname DNS lookup returned no records";
pub const LOG_TRACKER_REGISTRATION_TCP_FAILED: &str = "Tracker TCP connect failed";
pub const LOG_TRACKER_REGISTRATION_TLS_FAILED: &str = "Tracker TLS handshake failed";
pub const LOG_TRACKER_REGISTRATION_NO_PEER_CERTS: &str = "Tracker peer presented no certificates";
pub const LOG_TRACKER_REGISTRATION_STAGE1_MISMATCH: &str = "Tracker fingerprint mismatch";
pub const LOG_TRACKER_REGISTRATION_STAGE2_MISMATCH: &str =
    "Tracker self-reported fingerprint disagrees with TLS certificate";
pub const LOG_TRACKER_REGISTRATION_STAGE2_MALFORMED: &str =
    "Tracker self-reported fingerprint is not in canonical form";

/// Substituted for `server_reported` in logs when the tracker's fingerprint
/// fails canonical-form validation — logging raw bytes would let a hostile
/// tracker inject terminal-control sequences into the warn line.
pub const TRACKER_FINGERPRINT_MALFORMED_SENTINEL: &str = "<malformed>";
pub const LOG_TRACKER_REGISTRATION_TOFU_WRITE_FAILED: &str = "Tracker fingerprint write failed";
pub const LOG_TRACKER_REGISTRATION_TOFU_PINNED: &str = "Tracker fingerprint pinned";
pub const LOG_TRACKER_REGISTRATION_SEND_HANDSHAKE_FAILED: &str = "Tracker send failed: Handshake";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_RESPONSE_ERROR: &str =
    "Tracker read error: HandshakeResponse";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_CLOSED: &str =
    "Tracker closed connection during Handshake";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_REJECTED: &str = "Tracker rejected Handshake";
pub const LOG_TRACKER_REGISTRATION_HANDSHAKE_UNEXPECTED: &str =
    "Tracker sent unexpected response to Handshake";
pub const LOG_TRACKER_REGISTRATION_UNEXPECTED_FRAME: &str =
    "Tracker sent unexpected mid-idle frame, reconnecting";
pub const LOG_TRACKER_REGISTRATION_CLOSED_MID_IDLE: &str = "Tracker closed connection mid-idle";
pub const LOG_TRACKER_REGISTRATION_READ_ERROR_MID_IDLE: &str = "Tracker read error mid-idle";
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

// expect() messages for the tracker manager's sync-locks; only surface on a
// poisoned lock (panic in a critical section), never expected in normal operation.
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
pub const LOG_FILE_COPY_SOURCE_BUSY: &str = "FileCopy: source path is busy";
pub const LOG_FILE_COPY_DESTINATION_BUSY: &str = "FileCopy: destination path is busy";
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
pub const LOG_FILE_MOVE_SOURCE_BUSY: &str = "FileMove: source path is busy";
pub const LOG_FILE_MOVE_DESTINATION_BUSY: &str = "FileMove: destination path is busy";
pub const LOG_FILE_MOVE_SUCCESS: &str = "FileMove: success";
pub const LOG_FILE_RENAME_NOT_LOGGED_IN: &str = "FileRename: not logged in";
pub const LOG_FILE_RENAME_PERMISSION_DENIED: &str = "FileRename: permission denied";
pub const LOG_FILE_RENAME_ROOT_DENIED: &str = "FileRename: file_root permission denied";
pub const LOG_FILE_RENAME_FAILED: &str = "FileRename: failed";
pub const LOG_FILE_RENAME_SOURCE_BUSY: &str = "FileRename: source path is busy";
pub const LOG_FILE_RENAME_DESTINATION_BUSY: &str = "FileRename: destination path is busy";
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
pub const LOG_SERVER_INFO_DB_MAX_OUTBOUND_RATE: &str =
    "ServerInfoUpdate: database error setting max_outbound_rate";
pub const LOG_SERVER_INFO_DB_SCHEDULER_CHUNK_SIZE: &str =
    "ServerInfoUpdate: database error setting scheduler_chunk_size";
pub const LOG_SERVER_INFO_CHANNEL_CREATE_FAILED: &str =
    "ServerInfoUpdate: failed to create channel settings";
pub const LOG_SERVER_INFO_CHANNEL_DELETE_FAILED: &str =
    "ServerInfoUpdate: failed to delete channel settings";
pub const LOG_SERVER_INFO_CHANNEL_READ_FAILED: &str =
    "ServerInfoUpdate: failed to read existing channel settings";
pub const LOG_SERVER_INFO_DB_BEGIN: &str = "ServerInfoUpdate: failed to begin transaction";
pub const LOG_SERVER_INFO_DB_COMMIT: &str = "ServerInfoUpdate: failed to commit transaction";
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
pub const LOG_LOGIN_AVATAR_VALIDATE_ERROR: &str = "Login: avatar validation task failed";
pub const LOG_LOGIN_RENAMED_MID_LOGIN: &str = "Login: account renamed";
pub const LOG_LOGIN_PASSWORD_CHANGED: &str = "Login: password changed";

// --- Handler: Voice ---
pub const LOG_VOICE_JOIN_NOT_LOGGED_IN: &str = "VoiceJoin: not logged in";
pub const LOG_VOICE_JOIN_PERMISSION_DENIED: &str = "VoiceJoin: permission denied";
pub const LOG_VOICE_LEAVE_NOT_LOGGED_IN: &str = "VoiceLeave: not logged in";

// Wire-level `command` labels for `Error` responses via
// `send_error_and_disconnect`. Each value must exactly match the corresponding
// `ClientMessage` variant in `nexus-common/src/protocol.rs`; constants (not
// callsite literals) so the compiler catches typos.

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
