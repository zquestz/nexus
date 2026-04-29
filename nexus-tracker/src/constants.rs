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
