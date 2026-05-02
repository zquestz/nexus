//! Command-line argument parsing

use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use nexus_common::logging::{LogLevel, parse_log_retention};
use nexus_common::{
    DEFAULT_PORT, DEFAULT_TRANSFER_PORT, DEFAULT_TRANSFER_WEBSOCKET_PORT, DEFAULT_WEBSOCKET_PORT,
};

use crate::constants::ERR_DATA_DIR_NOT_ABSOLUTE;

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
fn default_data_dir_help() -> &'static str {
    #[cfg(target_os = "linux")]
    return "Data directory (default: ~/.local/share/nexusd/)";

    #[cfg(target_os = "macos")]
    return "Data directory (default: ~/Library/Application Support/nexusd/)";

    #[cfg(target_os = "windows")]
    return "Data directory (default: %APPDATA%\\nexusd\\)";

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return "Data directory (overrides platform default)";
}

/// Get default file root help text for the current platform.
fn default_file_root_help() -> &'static str {
    #[cfg(target_os = "linux")]
    return "File area root directory (default: ~/.local/share/nexusd/files/)";

    #[cfg(target_os = "macos")]
    return "File area root directory (default: ~/Library/Application Support/nexusd/files/)";

    #[cfg(target_os = "windows")]
    return "File area root directory (default: %APPDATA%\\nexusd\\files\\)";

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return "File area root directory (overrides platform default)";
}

/// Nexus BBS Server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// IP address to bind to (IPv4 or IPv6)
    #[arg(short, long, default_value = "0.0.0.0")]
    pub bind: IpAddr,

    /// Port to listen on
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Data directory (overrides platform default)
    #[arg(short, long, help = default_data_dir_help(), value_parser = absolute_data_dir)]
    pub data_dir: Option<PathBuf>,

    /// File area root directory (overrides platform default)
    #[arg(short = 'f', long = "file-root", help = default_file_root_help())]
    pub file_root: Option<PathBuf>,

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

    /// Port for file transfers
    #[arg(short = 't', long, default_value_t = DEFAULT_TRANSFER_PORT)]
    pub transfer_port: u16,

    /// Enable WebSocket support (ports 7502/7503 by default)
    #[arg(long)]
    pub websocket: bool,

    /// Port for WebSocket BBS connections (requires --websocket)
    #[arg(long, default_value_t = DEFAULT_WEBSOCKET_PORT)]
    pub websocket_port: u16,

    /// Port for WebSocket file transfers (requires --websocket)
    #[arg(long, default_value_t = DEFAULT_TRANSFER_WEBSOCKET_PORT)]
    pub transfer_websocket_port: u16,
}
