//! Command-line argument parsing

use clap::{Parser, Subcommand, ValueEnum};
use nexus_common::{DEFAULT_TRACKER_PORT, DEFAULT_TRACKER_WEBSOCKET_PORT};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::constants::ERR_DATA_DIR_NOT_ABSOLUTE;
use crate::logging::{LogLevel, parse_log_retention};

/// Reject relative `--data-dir` paths at parse time. Daemons should run
/// with absolute paths so behavior doesn't depend on launch CWD.
fn absolute_data_dir(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if !path.is_absolute() {
        return Err(format!("{}{}", ERR_DATA_DIR_NOT_ABSOLUTE, s));
    }
    Ok(path)
}

/// Get default data directory help text for the current platform.
fn default_data_dir_help() -> String {
    #[cfg(target_os = "linux")]
    return "Data directory (default: ~/.local/share/nexus-trackerd/)".to_string();

    #[cfg(target_os = "macos")]
    return "Data directory (default: ~/Library/Application Support/nexus-trackerd/)".to_string();

    #[cfg(target_os = "windows")]
    return "Data directory (default: %APPDATA%\\nexus-trackerd\\)".to_string();

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return "Data directory (overrides platform default)".to_string();
}

/// Which password to operate on with `set-password` / `clear-password`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lowercase")]
pub enum PasswordKind {
    /// Registration password — required by registering servers when the
    /// tracker gates registration.
    Registration,
    /// Listing password — required by clients fetching the server list
    /// when the tracker gates listings.
    Listing,
}

impl std::fmt::Display for PasswordKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasswordKind::Registration => write!(f, "registration"),
            PasswordKind::Listing => write!(f, "listing"),
        }
    }
}

/// Nexus tracker daemon
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// IP address to bind to (IPv4 or IPv6)
    #[arg(short, long, default_value = "0.0.0.0")]
    pub bind: IpAddr,

    /// Port to listen on
    #[arg(short, long, default_value_t = DEFAULT_TRACKER_PORT)]
    pub port: u16,

    /// Data directory (overrides platform default)
    #[arg(short, long, help = default_data_dir_help(), global = true, value_parser = absolute_data_dir)]
    pub data_dir: Option<PathBuf>,

    /// Log level (none, error, warn, info, debug)
    #[arg(long, default_value = "info")]
    pub log_level: LogLevel,

    /// Log file retention duration (e.g. "30d", "7d", "0" for stderr only)
    #[arg(long, default_value = "30d", value_parser = parse_log_retention)]
    pub log_retention: Duration,

    /// Disable timestamps in stderr log output (for Docker/systemd)
    #[arg(long)]
    pub no_log_timestamps: bool,

    /// Enable UPnP port forwarding (automatic NAT traversal)
    #[arg(long)]
    pub upnp: bool,

    /// Enable WebSocket support (port 7511 by default)
    #[arg(long)]
    pub websocket: bool,

    /// Port for WebSocket tracker connections (requires --websocket)
    #[arg(long, default_value_t = DEFAULT_TRACKER_WEBSOCKET_PORT)]
    pub websocket_port: u16,

    /// Maximum number of registered servers (0 = unlimited)
    #[arg(long, default_value_t = 10_000, value_parser = clap::value_parser!(u32).range(0..=1_000_000))]
    pub max_entries: u32,

    /// Maximum entries from a single source IP (0 = unlimited).
    /// Default 1 keeps the listing one-per-operator; raise for shared
    /// NAT'd networks where multiple operators register from the same IP.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(0..=1000))]
    pub max_entries_per_ip: u32,

    /// Refresh interval to instruct servers (seconds)
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u32).range(120..=600))]
    pub refresh_interval: u32,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Subcommands for tracker administration tasks.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Set a password from stdin (TTY → interactive prompt; pipe → first line).
    SetPassword {
        /// Which password to set
        #[arg(value_name = "registration|listing")]
        kind: PasswordKind,
    },
    /// Clear a password (the corresponding flow becomes open).
    ClearPassword {
        /// Which password to clear
        #[arg(value_name = "registration|listing")]
        kind: PasswordKind,
    },
}
