//! Network module constants

use std::time::Duration;

/// Connection timeout duration (30 seconds)
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Buffer size for the Iced stream channel
pub const STREAM_CHANNEL_SIZE: usize = 100;

/// Default features to request during login
pub const DEFAULT_FEATURES: &[&str] = &["chat", "news"];
