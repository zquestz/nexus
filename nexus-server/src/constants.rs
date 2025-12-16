//! Constants for server operator messages and configuration
//!
//! NOTE: User-facing error messages (sent to clients) are in handlers/errors.rs
//! This file contains only server operator messages (logs, startup, diagnostics)

// =============================================================================
// Connection Limits
// =============================================================================

/// Configuration key for max connections per IP in the database
pub const CONFIG_KEY_MAX_CONNECTIONS_PER_IP: &str = "max_connections_per_ip";

/// Default maximum connections per IP address (matches migration default)
pub const DEFAULT_MAX_CONNECTIONS_PER_IP: usize = 5;

// =============================================================================
// Database Validation Errors (defense-in-depth, operator-facing)
// =============================================================================

/// Error when max_connections_per_ip is set to zero
pub const ERR_MAX_CONNECTIONS_ZERO: &str = "max_connections_per_ip must be greater than 0";

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

// =============================================================================
// Chat State Keys
// =============================================================================

/// Chat state key for topic
pub const CHAT_STATE_KEY_TOPIC: &str = "topic";

/// Chat state key for username who set the topic
pub const CHAT_STATE_KEY_TOPIC_SET_BY: &str = "topic_set_by";

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

/// Listening address display
pub const MSG_LISTENING: &str = "Listening on ";

/// TLS enabled indicator
pub const MSG_TLS_ENABLED: &str = " (TLS enabled)";

/// Certificate fingerprint display
pub const MSG_CERT_FINGERPRINT: &str = "Certificate fingerprint (SHA-256): ";

/// Certificate generation start message
pub const MSG_GENERATING_CERT: &str = "Generating self-signed TLS certificate...";

/// Certificate file generated message
pub const MSG_CERT_GENERATED: &str = "Certificate generated: ";

/// Private key file generated message
pub const MSG_KEY_GENERATED: &str = "Private key generated: ";

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

/// Database directory creation error
pub const ERR_CREATE_DB_DIR: &str = "Failed to create directory: ";

/// Data directory error
pub const ERR_NO_DATA_DIR: &str = "Unable to determine data directory for your platform";

/// TLS initialization error
pub const ERR_TLS_INIT: &str = "Failed to initialize TLS: ";

/// Server bind error
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";

/// Connection handling error
pub const ERR_CONNECTION: &str = "Error handling connection from ";

/// Connection accept error
pub const ERR_ACCEPT: &str = "Failed to accept connection: ";

/// Message handling error
pub const ERR_HANDLING_MESSAGE: &str = "Error handling message: ";

/// Message parsing error
pub const ERR_PARSE_MESSAGE: &str = "Failed to parse message from ";

/// Connection limit exceeded error (debug only)
pub const ERR_CONNECTION_LIMIT: &str = "Connection limit exceeded for IP: ";

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

/// UPnP port forwarding request message
pub const MSG_REQUESTING_PORT_FORWARD: &str = "Requesting port forwarding: ";

/// UPnP configuration success message
pub const MSG_UPNP_CONFIGURED: &str = "UPnP configured: ";

/// UPnP setup failure warning
pub const MSG_UPNP_WARNING: &str = "Warning: UPnP setup failed: ";

/// UPnP disabled continuation message
pub const MSG_UPNP_CONTINUE: &str = "Server will continue without UPnP port forwarding.";

/// UPnP manual configuration suggestion
pub const MSG_UPNP_MANUAL: &str =
    "You may need to manually configure port forwarding on your router.";

/// UPnP lease renewal failure warning
pub const WARN_UPNP_RENEW_FAILED: &str = "Warning: Failed to renew UPnP lease: ";

/// UPnP port expiration warning
pub const WARN_UPNP_PORT_EXPIRE: &str =
    "Port forwarding may expire. You may need to restart the server.";

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

/// Lease renewal task error
pub const ERR_UPNP_RENEW_LEASE_TASK: &str = "Renew lease task failed: ";

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

/// Error when translation key is missing (format: key, locale)
pub const ERR_I18N_MISSING_KEY: &str = "Missing translation key";

/// Error when translation key is missing in English (format: key)
pub const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

/// Error when translation has formatting errors (format: key, errors)
pub const ERR_I18N_TRANSLATION_ERRORS: &str = "Translation errors for key";

/// "for locale: " - used in i18n error messages
pub const MSG_I18N_FOR_LOCALE: &str = "for locale";

/// Error when FTL file parsing fails
pub const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";

/// Error when adding resource to bundle fails
pub const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";
