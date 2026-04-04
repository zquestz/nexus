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

// =============================================================================
// Database Validation Errors (defense-in-depth, operator-facing)
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

/// Default server name (matches migration default)
pub const DEFAULT_SERVER_NAME: &str = "Nexus BBS";

/// Default server description (matches migration default)
pub const DEFAULT_SERVER_DESCRIPTION: &str = "";

/// Default server image (matches migration default)
pub const DEFAULT_SERVER_IMAGE: &str = "";

// =============================================================================
// Database Configuration
// =============================================================================

/// Database directory name
pub const DATA_DIR_NAME: &str = "nexusd";

/// Logs directory name (inside data dir)
pub const LOGS_DIR_NAME: &str = "logs";

/// Log file prefix (tracing-appender appends date, e.g. nexusd.2025-07-11)
pub const LOG_FILE_PREFIX: &str = "nexusd";

/// Database file name
pub const DATABASE_FILENAME: &str = "nexus.db";

/// Database configuration key for server name
pub const CONFIG_KEY_SERVER_NAME: &str = "server_name";

/// Database configuration key for server description
pub const CONFIG_KEY_SERVER_DESCRIPTION: &str = "server_description";

/// Database configuration key for server image
pub const CONFIG_KEY_SERVER_IMAGE: &str = "server_image";

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
pub const TLS_CERT_COMMON_NAME: &str = "Nexus BBS Server";

/// TLS close notify error pattern
pub const TLS_CLOSE_NOTIFY_MSG: &str = "peer closed connection without sending TLS close_notify";

/// TLS handshake failure error prefix (used for debug-only logging)
pub const TLS_HANDSHAKE_FAILED_PREFIX: &str = "TLS handshake failed:";

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

/// Certificate fingerprint display
pub const MSG_CERT_FINGERPRINT: &str = "Certificate fingerprint (SHA-256): ";

/// Certificate generation start message
pub const MSG_GENERATING_CERT: &str = "Generating self-signed TLS certificate...";

/// Certificate file generated message
pub const MSG_CERT_GENERATED: &str = "Certificate generated: ";

/// Private key file generated message
pub const MSG_KEY_GENERATED: &str = "Private key generated: ";

/// Log level display
pub const MSG_LOG_LEVEL: &str = "Log level: ";

/// Log directory display
pub const MSG_LOG_DIR: &str = "Log directory: ";

/// Shutdown signal received message
pub const MSG_SHUTDOWN_RECEIVED: &str = "\nShutdown signal received";

// =============================================================================
// Server Error Messages (operator-facing)
// =============================================================================

/// Generic error prefix
pub const ERR_GENERIC: &str = "Error: ";

/// Database initialization error
pub const ERR_DATABASE_INIT: &str = "Failed to initialize database: ";

/// Database path error
pub const ERR_DB_PATH_NO_PARENT: &str = "Database path should have a parent directory";

/// Data directory error
pub const ERR_NO_DATA_DIR: &str = "Unable to determine data directory for your platform";

/// TLS initialization error
pub const ERR_TLS_INIT: &str = "Failed to initialize TLS: ";

/// Server bind error
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";

/// File permissions error
#[cfg(unix)]
pub const ERR_SET_PERMISSIONS: &str = "Failed to set file permissions: ";

/// File metadata read error
#[cfg(unix)]
pub const ERR_READ_METADATA: &str = "Failed to read file metadata: ";

/// Permission set error
#[cfg(unix)]
pub const ERR_SET_PERMS: &str = "Failed to set permissions: ";

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
// TLS Certificate Generation Errors (operator-facing)
// =============================================================================

/// Key pair generation error
pub const ERR_GENERATE_KEYPAIR: &str = "Failed to generate key pair: ";

/// Certificate parameters creation error
pub const ERR_CREATE_CERT_PARAMS: &str = "Failed to create certificate parameters: ";

/// Certificate generation error
pub const ERR_GENERATE_CERT: &str = "Failed to generate certificate: ";

/// Certificate file write error
pub const ERR_WRITE_CERT_FILE: &str = "Failed to write certificate file: ";

/// Certificate permissions error
#[cfg(unix)]
pub const ERR_SET_CERT_PERMISSIONS: &str = "Failed to set certificate permissions: ";

/// Key file write error
pub const ERR_WRITE_KEY_FILE: &str = "Failed to write private key file: ";

/// Key permissions error
#[cfg(unix)]
pub const ERR_SET_KEY_PERMISSIONS: &str = "Failed to set key permissions: ";

// =============================================================================
// TLS Certificate Loading Errors (operator-facing)
// =============================================================================

/// Certificate file open error
pub const ERR_OPEN_CERT_FILE: &str = "Failed to open certificate file: ";

/// Certificate parsing error
pub const ERR_PARSE_CERT: &str = "Failed to parse certificate: ";

/// No certificates found error
pub const ERR_NO_CERTS_FOUND: &str = "No certificates found in certificate file";

/// Key file open error
pub const ERR_OPEN_KEY_FILE: &str = "Failed to open private key file: ";

/// Key parsing error
pub const ERR_PARSE_KEY: &str = "Failed to parse private key: ";

/// No key found error
pub const ERR_NO_KEY_FOUND: &str = "No private key found in key file";

/// TLS configuration creation error
pub const ERR_CREATE_TLS_CONFIG: &str = "Failed to create TLS configuration: ";

// =============================================================================
// UPnP Messages (operator-facing)
// =============================================================================

/// UPnP setup failure warning
pub const MSG_UPNP_WARNING: &str = "Warning: UPnP setup failed: ";

/// UPnP disabled continuation message
pub const MSG_UPNP_CONTINUE: &str = "Server will continue without UPnP port forwarding.";

/// UPnP manual configuration suggestion
pub const MSG_UPNP_MANUAL: &str =
    "You may need to manually configure port forwarding on your router.";

/// UPnP mapping removal failure warning
pub const WARN_UPNP_REMOVE_MAPPING_FAILED: &str = "Warning: Failed to remove UPnP port mapping: ";

// =============================================================================
// UPnP Error Messages (operator-facing)
// =============================================================================

/// UPnP IPv6 not supported error
pub const ERR_IPV6_NOT_SUPPORTED: &str = "UPnP is not supported for IPv6 addresses. Use IPv4 binding (e.g., --bind 0.0.0.0) for UPnP support.";

/// UPnP search task failure
pub const ERR_UPNP_SEARCH_TASK_FAILED: &str = "UPnP search task failed: ";

/// UPnP gateway not found error
pub const ERR_UPNP_GATEWAY_NOT_FOUND: &str = "UPnP gateway not found: ";

/// External IP task error
pub const ERR_UPNP_GET_EXTERNAL_IP_TASK: &str = "Failed to get external IP task: ";

/// External IP retrieval error
pub const ERR_UPNP_GET_EXTERNAL_IP: &str = "Failed to get external IP: ";

/// Port forwarding task error
pub const ERR_UPNP_PORT_FORWARD_TASK: &str = "Port forwarding task failed: ";

/// Port mapping addition error
pub const ERR_UPNP_ADD_PORT_MAPPING: &str = "Failed to add port mapping: ";

/// Port mapping removal task error
pub const ERR_UPNP_REMOVE_PORT_TASK: &str = "Remove port mapping task failed: ";

/// Port mapping removal error
pub const ERR_UPNP_REMOVE_PORT_MAPPING: &str = "Failed to remove port mapping: ";

/// Lease renewal error
pub const ERR_UPNP_RENEW_LEASE: &str = "Failed to renew lease: ";

/// UDP socket creation error
pub const ERR_UPNP_CREATE_UDP_SOCKET: &str = "Failed to create UDP socket: ";

/// Routing determination error
pub const ERR_UPNP_DETERMINE_ROUTING: &str = "Failed to determine routing: ";

/// Loopback only error
pub const ERR_UPNP_LOOPBACK_ONLY: &str = "Only loopback address available";

/// IPv6 address error when IPv4 expected
pub const ERR_UPNP_IPV6_EXPECTED_IPV4: &str = "Local address is IPv6, expected IPv4";

/// Local address retrieval error
pub const ERR_UPNP_GET_LOCAL_ADDRESS: &str = "Failed to get local address: ";

// =============================================================================
// Internationalization Configuration and Error Messages (operator-facing)
// =============================================================================

/// Default locale (English)
pub const DEFAULT_LOCALE: &str = "en";

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

/// Error when file root directory cannot be determined
pub const ERR_NO_FILE_ROOT: &str = "Unable to determine file root directory for your platform";

/// Error when creating file area directories fails
pub const ERR_CREATE_FILE_DIR: &str = "Failed to create file area directory: ";

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
// Log Messages
// =============================================================================

// --- Connection / Main ---
pub const LOG_ACCEPT_ERROR: &str = "Accept error";
pub const LOG_CONNECTION_ERROR: &str = "Connection error";
pub const LOG_CONNECTION_ERROR_TLS: &str = "Connection error (TLS handshake)";
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
pub const LOG_VOICE_DTLS_FAILED: &str = "Voice DTLS listener failed";
pub const LOG_VOICE_UNAVAILABLE: &str = "Voice chat will be unavailable";

// --- Database ---
pub const LOG_DB_DIR_CREATE_FAILED: &str = "Failed to create database directory";

// --- File Index ---
pub const LOG_FILE_INDEX_REBUILT: &str = "File index rebuilt";
pub const LOG_FILE_INDEX_BUILD_FAILED: &str = "Failed to build file index";
pub const LOG_FILE_INDEX_SEARCH_FAILED: &str = "Search failed, index may be corrupted";
pub const LOG_FILE_INDEX_DELETE_FAILED: &str = "Failed to delete corrupted index";

// --- i18n ---
pub const LOG_TRANSLATION_ERRORS: &str = "Translation errors";
pub const LOG_MISSING_TRANSLATION_KEY: &str = "Missing translation key";

// --- UPnP ---
pub const LOG_UPNP_CONFIGURED: &str = "UPnP configured";
pub const LOG_UPNP_RENEWAL_FAILED: &str = "UPnP lease renewal failed";
pub const LOG_UPNP_REDISCOVERING: &str = "UPnP rediscovering gateway";
pub const LOG_UPNP_REDISCOVERED: &str = "UPnP gateway rediscovered";
pub const LOG_UPNP_REDISCOVERY_FAILED: &str = "UPnP rediscovery failed";
pub const LOG_UPNP_PORT_EXPIRE: &str = "UPnP port mappings may expire";

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
