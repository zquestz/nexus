//! Server logging infrastructure
//!
//! Provides log level configuration, tracing subscriber initialization,
//! global log level state (set once at startup, readable anywhere),
//! and log file management (directory resolution, retention purge).

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use tracing_subscriber::{
    Layer, fmt as subscriber_fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::constants::{DATA_DIR_NAME, LOG_FILE_PREFIX, LOGS_DIR_NAME};

/// Server log level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Logging disabled
    None,
    /// Only errors
    Error,
    /// Errors and warnings
    Warn,
    /// Errors, warnings, and informational messages (default)
    Info,
    /// All messages including debug output
    Debug,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::None => write!(f, "none"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(LogLevel::None),
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            _ => Err(format!(
                "Invalid log level '{}'. Valid values: none, error, warn, info, debug",
                s
            )),
        }
    }
}

impl LogLevel {
    /// Convert to a tracing level filter, or None if logging is disabled
    pub fn to_tracing_level(self) -> Option<tracing::Level> {
        match self {
            LogLevel::None => None,
            LogLevel::Error => Some(tracing::Level::ERROR),
            LogLevel::Warn => Some(tracing::Level::WARN),
            LogLevel::Info => Some(tracing::Level::INFO),
            LogLevel::Debug => Some(tracing::Level::DEBUG),
        }
    }
}

/// Parse a log retention duration string (e.g. "30d", "7d", "0")
///
/// Returns Duration. "0" means no file logging (stderr only).
/// Non-zero values must be at least 1 day.
pub fn parse_log_retention(s: &str) -> Result<Duration, String> {
    if s == "0" {
        return Ok(Duration::ZERO);
    }

    let duration = humantime::parse_duration(s)
        .map_err(|e| format!("Invalid log retention '{}': {}", s, e))?;

    let one_day = Duration::from_secs(24 * 60 * 60);
    if duration < one_day {
        return Err(format!(
            "Log retention must be 0 (disabled) or at least 1 day, got '{}'",
            s
        ));
    }

    Ok(duration)
}

/// Global server log level, set once at startup
static LOG_LEVEL: OnceLock<String> = OnceLock::new();

/// Initialize logging: sets the global log level and configures the tracing subscriber.
///
/// - `level`: Log level (None disables all logging)
/// - `retention`: Log file retention duration (zero disables file logging)
/// - `no_timestamps`: If true, omit timestamps from stderr output
///
/// Returns an error if the log directory cannot be created.
pub fn init(level: &LogLevel, retention: Duration, no_timestamps: bool) -> Result<(), String> {
    // Set global log level (readable anywhere via server_log_level())
    LOG_LEVEL
        .set(level.to_string())
        .expect("log level already initialized");

    let Some(tracing_level) = level.to_tracing_level() else {
        return Ok(());
    };
    let filter = tracing_subscriber::filter::LevelFilter::from_level(tracing_level);

    // Build stderr layer - human-readable, timestamps optional
    let build_stderr_layer = |f| -> Box<dyn Layer<_> + Send + Sync> {
        if !no_timestamps {
            Box::new(
                subscriber_fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_target(false)
                    .with_filter(f),
            )
        } else {
            Box::new(
                subscriber_fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_target(false)
                    .without_time()
                    .with_filter(f),
            )
        }
    };

    // File layer - JSON format with daily rotation (only if retention > 0)
    // Always includes timestamps regardless of --no-log-timestamps
    if retention > Duration::ZERO {
        let log_dir = log_dir();
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            // Fall back to stderr-only so the error can be logged
            tracing_subscriber::registry()
                .with(build_stderr_layer(filter))
                .init();
            return Err(format!(
                "Failed to create log directory {}: {}",
                log_dir.display(),
                e
            ));
        }

        // Restrict log directory to owner only (0o700) — logs contain IPs and usernames
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&log_dir, std::fs::Permissions::from_mode(0o700));
        }

        let file_appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
        let json_layer = subscriber_fmt::layer()
            .json()
            .with_writer(file_appender)
            .with_target(true)
            .with_filter(filter);

        tracing_subscriber::registry()
            .with(build_stderr_layer(filter))
            .with(json_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(build_stderr_layer(filter))
            .init();
    }

    Ok(())
}

/// Get the server's configured log level string
pub fn server_log_level() -> &'static str {
    LOG_LEVEL.get().map(|s| s.as_str()).unwrap_or("info")
}

/// Get the log directory path (~/.local/share/nexusd/logs/)
pub fn log_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .expect("Failed to determine data directory")
        .join(DATA_DIR_NAME)
        .join(LOGS_DIR_NAME)
}

/// Purge log files older than the retention period
///
/// Checks file modification time against the retention duration.
/// Called on startup and daily by a timer task.
pub fn purge_old_logs(retention: Duration) {
    if retention == Duration::ZERO {
        return;
    }

    let log_path = log_dir();
    let entries = match std::fs::read_dir(&log_path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Only purge files matching the log file prefix (e.g. "nexusd.2025-07-11")
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(LOG_FILE_PREFIX) {
            continue;
        }

        if let Ok(metadata) = path.metadata()
            && let Ok(modified) = metadata.modified()
            && let Ok(age) = modified.elapsed()
            && age > retention
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}
