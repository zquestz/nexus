//! String constants for server output and error messages

/// Certificate file names
pub const CERT_FILENAME: &str = "server.crt";
pub const KEY_FILENAME: &str = "server.key";

/// Output message constants
pub const MSG_BANNER: &str = "Nexus BBS Server v";
pub const MSG_DATABASE: &str = "Database: ";
pub const MSG_CERTIFICATES: &str = "Certificates: ";
pub const MSG_LISTENING: &str = "Listening on ";
pub const MSG_TLS_ENABLED: &str = " (TLS enabled)";
pub const MSG_GENERATING_CERT: &str = "Generating self-signed TLS certificate...";
pub const MSG_CERT_GENERATED: &str = "Certificate generated: ";
pub const MSG_KEY_GENERATED: &str = "Private key generated: ";
pub const MSG_CERT_FINGERPRINT: &str = "Certificate fingerprint (SHA-256): ";

/// General error messages
pub const ERR_SET_PERMISSIONS: &str = "Failed to set file permissions: ";
pub const ERR_DATABASE_INIT: &str = "Failed to initialize database: ";
pub const ERR_TLS_INIT: &str = "Failed to initialize TLS: ";
pub const ERR_BIND_FAILED: &str = "Failed to bind to ";
pub const ERR_CONNECTION: &str = "Error handling connection from ";
pub const ERR_ACCEPT: &str = "Failed to accept connection: ";

/// TLS certificate generation error messages
pub const ERR_GENERATE_KEYPAIR: &str = "Failed to generate key pair: ";
pub const ERR_CREATE_CERT_PARAMS: &str = "Failed to create certificate parameters: ";
pub const ERR_GENERATE_CERT: &str = "Failed to generate certificate: ";
pub const ERR_WRITE_CERT_FILE: &str = "Failed to write certificate file: ";
pub const ERR_SET_CERT_PERMISSIONS: &str = "Failed to set certificate permissions: ";
pub const ERR_WRITE_KEY_FILE: &str = "Failed to write private key file: ";
pub const ERR_SET_KEY_PERMISSIONS: &str = "Failed to set key permissions: ";
pub const ERR_NO_CERTS_FOUND: &str = "No certificates found in certificate file";

/// TLS certificate loading error messages
pub const ERR_OPEN_CERT_FILE: &str = "Failed to open certificate file: ";
pub const ERR_PARSE_CERT: &str = "Failed to parse certificate: ";
pub const ERR_OPEN_KEY_FILE: &str = "Failed to open private key file: ";
pub const ERR_PARSE_KEY: &str = "Failed to parse private key: ";
pub const ERR_NO_KEY_FOUND: &str = "No private key found in key file";
pub const ERR_CREATE_TLS_CONFIG: &str = "Failed to create TLS configuration: ";

/// File permissions error messages
pub const ERR_READ_METADATA: &str = "Failed to read file metadata: ";
pub const ERR_SET_PERMS: &str = "Failed to set permissions: ";

/// UPnP output messages
pub const MSG_REQUESTING_PORT_FORWARD: &str = "Requesting port forwarding: ";
pub const MSG_UPNP_CONFIGURED: &str = "UPnP configured: ";

/// UPnP error messages
pub const ERR_IPV6_NOT_SUPPORTED: &str = "UPnP is not supported for IPv6 addresses. Use IPv4 binding (e.g., --bind 0.0.0.0) for UPnP support.";
pub const ERR_UPNP_SEARCH_TASK_FAILED: &str = "UPnP search task failed: ";
pub const ERR_UPNP_GATEWAY_NOT_FOUND: &str = "UPnP gateway not found: ";
pub const ERR_UPNP_GET_EXTERNAL_IP_TASK: &str = "Failed to get external IP task: ";
pub const ERR_UPNP_GET_EXTERNAL_IP: &str = "Failed to get external IP: ";
pub const ERR_UPNP_PORT_FORWARD_TASK: &str = "Port forwarding task failed: ";
pub const ERR_UPNP_ADD_PORT_MAPPING: &str = "Failed to add port mapping: ";
pub const ERR_UPNP_REMOVE_PORT_TASK: &str = "Remove port mapping task failed: ";
pub const ERR_UPNP_REMOVE_PORT_MAPPING: &str = "Failed to remove port mapping: ";
pub const ERR_UPNP_RENEW_LEASE_TASK: &str = "Renew lease task failed: ";
pub const ERR_UPNP_RENEW_LEASE: &str = "Failed to renew lease: ";
pub const ERR_UPNP_CREATE_UDP_SOCKET: &str = "Failed to create UDP socket: ";
pub const ERR_UPNP_DETERMINE_ROUTING: &str = "Failed to determine routing: ";
pub const ERR_UPNP_LOOPBACK_ONLY: &str = "Only loopback address available";
pub const ERR_UPNP_IPV6_EXPECTED_IPV4: &str = "Local address is IPv6, expected IPv4";
pub const ERR_UPNP_GET_LOCAL_ADDRESS: &str = "Failed to get local address: ";

/// UPnP warning messages
pub const WARN_UPNP_RENEW_FAILED: &str = "Warning: Failed to renew UPnP lease: ";
pub const WARN_UPNP_PORT_EXPIRE: &str =
    "Port forwarding may expire. You may need to restart the server.";

/// Username validation constants
pub const MAX_USERNAME_LENGTH: usize = 32;

/// Username validation error messages
pub const ERR_USERNAME_INVALID: &str = "Username contains invalid characters (letters, numbers, and symbols allowed - no whitespace or control characters)";
pub const ERR_USERNAME_TOO_LONG: &str = "Username is too long (max 32 characters)";
pub const ERR_USERNAME_EMPTY: &str = "Username cannot be empty";

/// Database path error messages
pub const ERR_NO_DATA_DIR: &str = "Unable to determine data directory for your platform";
pub const ERR_CREATE_DB_DIR: &str = "Failed to create directory: ";

/// User manager error messages
pub const ERR_CHECK_PERMISSION: &str = "Error checking permission for ";
pub const ERR_CHECK_USER_LIST_PERMISSION: &str = "Error checking user_list permission for ";

/// Database directory and file names
pub const DATA_DIR_NAME: &str = "nexusd";
pub const DATABASE_FILENAME: &str = "nexus.db";

/// Database configuration keys
pub const CONFIG_KEY_TOPIC: &str = "topic";

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
