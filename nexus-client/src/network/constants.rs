//! Network module constants

use std::time::Duration;

/// Connection timeout duration (30 seconds)
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// DNS resolution timeout (15 seconds). Mirrors the BBS-server
/// tracker task's `DNS_LOOKUP_TIMEOUT` so client and server
/// agree on what "wedged resolver" means in time. Bounds both
/// the BBS connect path and the tracker query path.
pub const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(15);

/// BBS connect post-TLS read/send timeout (30 seconds). Bounds each
/// individual handshake / login leg so a peer that completes TLS but
/// then never reads or never replies can't park the connect task
/// indefinitely. Mirrors the tracker-side `TRACKER_RESPONSE_TIMEOUT`
/// so the two flows share a single liveness expectation.
pub const BBS_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

pub use nexus_common::{FEATURE_CHAT, FEATURE_FILES, FEATURE_NEWS};

/// Buffer size for the Iced stream channel
pub const STREAM_CHANNEL_SIZE: usize = 100;

/// Default features to request during login
pub const DEFAULT_FEATURES: &[&str] = &[FEATURE_CHAT, FEATURE_FILES, FEATURE_NEWS];

/// Ping interval for NAT keepalive (5 minutes)
///
/// Most consumer NAT routers drop idle TCP connections after 30-60 minutes.
/// Sending a ping every 5 minutes keeps the NAT mapping alive.
pub const PING_INTERVAL: u64 = 300;
