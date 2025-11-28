//! Command-line argument parsing

use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;

/// Get default database path help text for current platform
fn default_database_help() -> String {
    #[cfg(target_os = "linux")]
    return "Database file path (default: ~/.local/share/nexusd/nexus.db)".to_string();

    #[cfg(target_os = "macos")]
    return "Database file path (default: ~/Library/Application Support/nexusd/nexus.db)"
        .to_string();

    #[cfg(target_os = "windows")]
    return "Database file path (default: %APPDATA%\\nexusd\\nexus.db)".to_string();

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return "Database file path (overrides platform default)".to_string();
}

/// Nexus BBS Server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// IP address to bind to (IPv4 or IPv6)
    #[arg(short, long, default_value = "0.0.0.0")]
    pub bind: IpAddr,

    /// Port to listen on
    #[arg(short, long, default_value = "7500")]
    pub port: u16,

    /// Database file path (overrides platform default)
    #[arg(short, long, help = default_database_help())]
    pub database: Option<PathBuf>,

    /// Enable debug logging (shows user connect/disconnect messages)
    #[arg(long, default_value = "false")]
    pub debug: bool,

    /// Enable UPnP port forwarding (automatic NAT traversal)
    #[arg(long, default_value = "false")]
    pub upnp: bool,
}
