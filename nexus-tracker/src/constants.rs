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
// Password storage
// =============================================================================

/// Filename for the registration password hash within the data directory.
/// Presence of this file gates `TrackerRegister`; absence means open registration.
pub const REGISTRATION_PASSWORD_FILENAME: &str = "registration.password";

/// Filename for the listing password hash within the data directory.
/// Presence of this file gates `TrackerList`; absence means open listing.
pub const LISTING_PASSWORD_FILENAME: &str = "listing.password";

/// Permissions mode for password hash files on Unix (owner read/write only).
#[cfg(unix)]
pub const PASSWORD_FILE_MODE: u32 = 0o600;

/// Argon2 hashing failed (caller appends underlying error).
pub const ERR_HASH_PASSWORD: &str = "Failed to hash password: ";

/// Password hash file write failed (caller appends path and underlying error).
pub const ERR_WRITE_PASSWORD_FILE: &str = "Failed to write password file: ";

/// Password hash file permission set failed (caller appends path and underlying error).
#[cfg(unix)]
pub const ERR_SET_PASSWORD_PERMS: &str = "Failed to set password file permissions: ";

/// Password hash file read failed (caller appends path and underlying error).
#[allow(dead_code)] // used by TrackerRegister/TrackerList handlers (next step)
pub const ERR_READ_PASSWORD_FILE: &str = "Failed to read password file: ";

/// Password hash file deletion failed (caller appends path and underlying error).
pub const ERR_DELETE_PASSWORD_FILE: &str = "Failed to delete password file: ";

/// Password prompt failed (caller appends underlying error).
pub const ERR_PROMPT_PASSWORD: &str = "Failed to prompt for password: ";

/// Empty password rejected at set time (use `clear-password` to disable gating).
pub const ERR_PASSWORD_EMPTY: &str =
    "Password cannot be empty (use `clear-password` to disable gating)";

/// Password confirmation did not match.
pub const ERR_PASSWORD_MISMATCH: &str = "Passwords do not match";

/// Stored password hash failed to parse as PHC (caller appends underlying error).
#[allow(dead_code)] // used by TrackerRegister/TrackerList handlers (next step)
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

/// Prompt shown when setting a password (kind already announced via the
/// preceding `Setting password` log line).
pub const PROMPT_NEW_PASSWORD: &str = "New password: ";

/// Prompt shown to confirm the password just entered.
pub const PROMPT_CONFIRM_PASSWORD: &str = "Confirm password: ";
