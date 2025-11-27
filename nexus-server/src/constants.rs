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
