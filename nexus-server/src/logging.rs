//! Server logging infrastructure
//!
//! Provides log level configuration, tracing subscriber initialization,
//! global log level state (set once at startup, readable anywhere),
//! and log file management (directory resolution, retention purge).

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use tracing_subscriber::{
    Layer, fmt as subscriber_fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

#[cfg(unix)]
use crate::constants::DATA_DIR_MODE;
use crate::constants::{
    ERR_CREATE_LOG_DIR, ERR_LOG_LEVEL_ALREADY_SET, ERR_LOG_LEVEL_INVALID,
    ERR_LOG_RETENTION_INVALID, ERR_LOG_RETENTION_TOO_SHORT, LOG_FILE_PREFIX, LOGS_DIR_NAME,
};

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
            _ => Err(format!("{}{}", ERR_LOG_LEVEL_INVALID, s)),
        }
    }
}

impl LogLevel {
    /// Convert to a tracing level filter, or `None` if logging is disabled.
    #[must_use]
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

/// Parse a log retention duration string (e.g. `"30d"`, `"7d"`, `"0"`).
///
/// Returns a `Duration`. The string `"0"` means no file logging (stderr
/// only). Non-zero values must be at least 1 day; sub-day retention isn't
/// useful given daily rotation.
pub fn parse_log_retention(s: &str) -> Result<Duration, String> {
    if s == "0" {
        return Ok(Duration::ZERO);
    }

    let duration = humantime::parse_duration(s)
        .map_err(|e| format!("{}{} ({})", ERR_LOG_RETENTION_INVALID, s, e))?;

    let one_day = Duration::from_secs(24 * 60 * 60);
    if duration < one_day {
        return Err(format!("{}{}", ERR_LOG_RETENTION_TOO_SHORT, s));
    }

    Ok(duration)
}

/// Global server log level, set once at startup.
static LOG_LEVEL: OnceLock<String> = OnceLock::new();

/// Initialize logging: sets the global log level and configures the tracing
/// subscriber.
///
/// - `data_dir`: server data directory; logs go to `<data_dir>/logs/`
///   when `retention > 0`.
/// - `level`: log level (`None` disables all logging).
/// - `retention`: log file retention duration (`Duration::ZERO` disables
///   file logging — stderr only).
/// - `no_timestamps`: if true, omit timestamps from stderr output (useful
///   when the supervisor — Docker, systemd — adds its own).
///
/// Returns an error if the log directory cannot be created. In that case
/// the subscriber is still installed in stderr-only mode so the error can
/// be logged.
pub fn init(
    data_dir: &Path,
    level: LogLevel,
    retention: Duration,
    no_timestamps: bool,
) -> Result<(), String> {
    // Set global log level (readable anywhere via `server_log_level()`).
    LOG_LEVEL
        .set(level.to_string())
        .expect(ERR_LOG_LEVEL_ALREADY_SET);

    let Some(tracing_level) = level.to_tracing_level() else {
        return Ok(());
    };
    let filter = tracing_subscriber::filter::LevelFilter::from_level(tracing_level);

    // Build stderr layer — human-readable, timestamps optional.
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

    if retention > Duration::ZERO {
        let log_path = log_dir(data_dir);
        // Create both `<data_dir>` (if missing) and `<data_dir>/logs/`
        // with owner-only mode atomically on Unix, so there is no window
        // where the data directory is world-readable.
        let create_result = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = std::fs::DirBuilder::new();
                builder.recursive(true);
                builder.mode(DATA_DIR_MODE);
                builder.create(&log_path)
            }
            #[cfg(not(unix))]
            {
                std::fs::create_dir_all(&log_path)
            }
        };
        if let Err(e) = create_result {
            // Fall back to stderr-only so the error can be logged.
            tracing_subscriber::registry()
                .with(build_stderr_layer(filter))
                .init();
            return Err(format!(
                "{}{}: {}",
                ERR_CREATE_LOG_DIR,
                log_path.display(),
                e
            ));
        }

        // Force-set the mode on pre-existing directories so a wrongly-
        // permissioned `logs/` from a previous run gets corrected.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(DATA_DIR_MODE));
        }

        let file_appender = tracing_appender::rolling::daily(&log_path, LOG_FILE_PREFIX);
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

/// Get the server's configured log level string.
#[must_use]
pub fn server_log_level() -> &'static str {
    LOG_LEVEL.get().map(String::as_str).unwrap_or("info")
}

/// Get the log directory path under the given server data directory.
#[must_use]
pub fn log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(LOGS_DIR_NAME)
}

/// Purge log files older than the retention period.
///
/// Checks file modification time against the retention duration. Called
/// on startup and daily by a timer task.
pub fn purge_old_logs(log_dir: &Path, retention: Duration) {
    if retention == Duration::ZERO {
        return;
    }

    let entries = match std::fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Only purge files matching the log file prefix
        // (e.g. `nexusd.2025-07-11`).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_str() {
        assert_eq!("none".parse::<LogLevel>().unwrap(), LogLevel::None);
        assert_eq!("ERROR".parse::<LogLevel>().unwrap(), LogLevel::Error);
        assert_eq!("Warn".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert_eq!("info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("debug".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert!("trace".parse::<LogLevel>().is_err());
        assert!("".parse::<LogLevel>().is_err());
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::None.to_string(), "none");
        assert_eq!(LogLevel::Error.to_string(), "error");
        assert_eq!(LogLevel::Warn.to_string(), "warn");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Debug.to_string(), "debug");
    }

    #[test]
    fn test_log_level_to_tracing_level() {
        assert_eq!(LogLevel::None.to_tracing_level(), None);
        assert_eq!(
            LogLevel::Error.to_tracing_level(),
            Some(tracing::Level::ERROR)
        );
        assert_eq!(
            LogLevel::Warn.to_tracing_level(),
            Some(tracing::Level::WARN)
        );
        assert_eq!(
            LogLevel::Info.to_tracing_level(),
            Some(tracing::Level::INFO)
        );
        assert_eq!(
            LogLevel::Debug.to_tracing_level(),
            Some(tracing::Level::DEBUG)
        );
    }

    #[test]
    fn test_parse_log_retention_zero() {
        assert_eq!(parse_log_retention("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn test_parse_log_retention_valid() {
        assert_eq!(
            parse_log_retention("1d").unwrap(),
            Duration::from_secs(24 * 60 * 60)
        );
        assert_eq!(
            parse_log_retention("30d").unwrap(),
            Duration::from_secs(30 * 24 * 60 * 60)
        );
    }

    #[test]
    fn test_parse_log_retention_below_one_day() {
        assert!(parse_log_retention("12h").is_err());
        assert!(parse_log_retention("60m").is_err());
        assert!(parse_log_retention("1s").is_err());
    }

    #[test]
    fn test_parse_log_retention_invalid_format() {
        assert!(parse_log_retention("not a duration").is_err());
        assert!(parse_log_retention("").is_err());
    }

    #[test]
    fn test_log_dir_under_data_dir() {
        let data = Path::new("/var/lib/nexusd");
        assert_eq!(log_dir(data), Path::new("/var/lib/nexusd/logs"));
    }
}
