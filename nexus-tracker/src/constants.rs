//! Tracker constants

/// Subdirectory name within the platform data directory
/// (e.g. `~/.local/share/nexus-trackerd/` on Linux).
pub const DATA_DIR_NAME: &str = "nexus-trackerd";

/// Log file prefix (used by daily rotation, e.g. `nexus-trackerd.2025-04-28`).
pub const LOG_FILE_PREFIX: &str = "nexus-trackerd";

/// Subdirectory name for log files within the data directory.
pub const LOGS_DIR_NAME: &str = "logs";
