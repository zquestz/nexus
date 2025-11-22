//! Command-line argument parsing

use clap::Parser;
use std::net::Ipv6Addr;
use std::path::PathBuf;

/// Nexus BBS Server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// IPv6 address to bind to (must be in Yggdrasil range: 0200::/7)
    #[arg(short, long)]
    pub bind: Ipv6Addr,

    /// Port to listen on
    #[arg(short, long, default_value = "7500")]
    pub port: u16,

    /// Database file path (overrides platform default)
    #[arg(short, long)]
    pub database: Option<PathBuf>,
}
