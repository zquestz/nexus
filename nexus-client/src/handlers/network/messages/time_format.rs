//! Time formatting utilities for IP rule lists
//!
//! Shared utilities for formatting remaining time in ban/trust list displays.

use nexus_common::time::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};

use crate::i18n::t_args;

/// Key prefix for i18n time formatting messages
#[derive(Debug, Clone, Copy)]
pub enum TimeFormatContext {
    /// Ban-related time formatting (uses msg-ban-* keys)
    Ban,
    /// Trust-related time formatting (uses msg-trust-* keys)
    Trust,
}

impl TimeFormatContext {
    fn expired_key(self) -> &'static str {
        match self {
            Self::Ban => "msg-ban-expired",
            Self::Trust => "msg-trust-expired",
        }
    }

    fn remaining_days_key(self) -> &'static str {
        match self {
            Self::Ban => "msg-ban-remaining-days",
            Self::Trust => "msg-trust-remaining-days",
        }
    }

    fn remaining_hours_key(self) -> &'static str {
        match self {
            Self::Ban => "msg-ban-remaining-hours",
            Self::Trust => "msg-trust-remaining-hours",
        }
    }

    fn remaining_minutes_key(self) -> &'static str {
        match self {
            Self::Ban => "msg-ban-remaining-minutes",
            Self::Trust => "msg-trust-remaining-minutes",
        }
    }
}

/// Format remaining time in terse format (e.g., "2h 30m", "7d 0h")
///
/// Uses the appropriate i18n keys based on the context (ban or trust).
///
/// # Arguments
/// * `expires_at` - Unix timestamp when the rule expires
/// * `context` - Whether this is for a ban or trust entry
///
/// # Returns
/// A localized string representing the remaining time, or "expired" if past
pub fn format_remaining_time(expires_at: i64, context: TimeFormatContext) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let remaining_secs = expires_at.saturating_sub(now);
    if remaining_secs <= 0 {
        return crate::i18n::t(context.expired_key());
    }

    let remaining_secs = remaining_secs as u64;
    let days = remaining_secs / SECONDS_PER_DAY;
    let hours = (remaining_secs % SECONDS_PER_DAY) / SECONDS_PER_HOUR;
    let minutes = (remaining_secs % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;

    if days > 0 {
        t_args(
            context.remaining_days_key(),
            &[("days", &days.to_string()), ("hours", &hours.to_string())],
        )
    } else if hours > 0 {
        t_args(
            context.remaining_hours_key(),
            &[
                ("hours", &hours.to_string()),
                ("minutes", &minutes.to_string()),
            ],
        )
    } else {
        t_args(
            context.remaining_minutes_key(),
            &[("minutes", &minutes.to_string())],
        )
    }
}
