//! Network module constants

use std::time::Duration;

/// Connection timeout duration (30 seconds)
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Buffer size for the Iced stream channel
pub const STREAM_CHANNEL_SIZE: usize = 100;

/// Default features to request during login
pub const DEFAULT_FEATURES: &[&str] = &["chat", "files", "news"];

/// Ping interval for NAT keepalive (5 minutes)
///
/// Most consumer NAT routers drop idle TCP connections after 30-60 minutes.
/// Sending a ping every 5 minutes keeps the NAT mapping alive.
pub const PING_INTERVAL: u64 = 300;
