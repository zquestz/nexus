//! Time constants for duration calculations
//!
//! Shared constants used by both client and server for parsing and formatting durations.

/// Seconds per minute
pub const SECONDS_PER_MINUTE: u64 = 60;

/// Seconds per hour
pub const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;

/// Seconds per day
pub const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;
