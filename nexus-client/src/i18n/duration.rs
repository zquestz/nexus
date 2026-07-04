//! Localized human-readable duration formatting.
//!
//! Single source for user-facing durations (transfer ETA and elapsed time,
//! session duration). Every rendering decision is locale-owned: the
//! `time-days` / `time-hours` / `time-minutes` / `time-seconds` keys place
//! `$count` and spell the compact unit per locale ("2m", "2 мин", "2分"),
//! and `time-two-units` joins the two unit groups (CJK locales join with
//! no space: "2分30秒"). No ordering or spacing is decided in code.
//!
//! Compact abbreviations don't inflect in any shipped locale, so the keys
//! need no plural selectors.

use nexus_common::time::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};

use super::t_args;

/// Format a duration as its two most significant units in the locale's
/// compact form. The zero-valued minor unit is dropped and sub-minor
/// remainders are truncated: 61 → "1m 1s", 3600 → "1h", 3659 → "1h",
/// 3661 → "1h 1m", 90000 → "1d 1h".
pub fn format_duration(seconds: u64) -> String {
    let unit = |key: &str, count: u64| t_args(key, &[("count", &count.to_string())]);

    if seconds < SECONDS_PER_MINUTE {
        return unit("time-seconds", seconds);
    }

    let (major_key, major, minor_key, minor) = if seconds < SECONDS_PER_HOUR {
        (
            "time-minutes",
            seconds / SECONDS_PER_MINUTE,
            "time-seconds",
            seconds % SECONDS_PER_MINUTE,
        )
    } else if seconds < SECONDS_PER_DAY {
        (
            "time-hours",
            seconds / SECONDS_PER_HOUR,
            "time-minutes",
            (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
        )
    } else {
        (
            "time-days",
            seconds / SECONDS_PER_DAY,
            "time-hours",
            (seconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR,
        )
    };

    if minor == 0 {
        unit(major_key, major)
    } else {
        t_args(
            "time-two-units",
            &[
                ("major", &unit(major_key, major)),
                ("minor", &unit(minor_key, minor)),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fluent wraps interpolated arguments in Unicode bidi isolates;
    /// strip them so assertions read as plain text.
    fn plain(s: String) -> String {
        s.chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect()
    }

    #[test]
    fn seconds_only_below_one_minute() {
        assert_eq!(plain(format_duration(0)), "0s");
        assert_eq!(plain(format_duration(1)), "1s");
        assert_eq!(plain(format_duration(59)), "59s");
    }

    #[test]
    fn minutes_with_optional_seconds() {
        assert_eq!(plain(format_duration(60)), "1m");
        assert_eq!(plain(format_duration(61)), "1m 1s");
        assert_eq!(plain(format_duration(90)), "1m 30s");
        assert_eq!(plain(format_duration(120)), "2m");
        assert_eq!(plain(format_duration(3599)), "59m 59s");
    }

    #[test]
    fn hours_with_optional_minutes_truncate_seconds() {
        assert_eq!(plain(format_duration(3600)), "1h");
        assert_eq!(plain(format_duration(3659)), "1h"); // 59s truncated
        assert_eq!(plain(format_duration(3661)), "1h 1m");
        assert_eq!(plain(format_duration(7200)), "2h");
        assert_eq!(plain(format_duration(36000)), "10h");
    }

    #[test]
    fn days_with_optional_hours() {
        assert_eq!(plain(format_duration(86400)), "1d");
        assert_eq!(plain(format_duration(90000)), "1d 1h");
        assert_eq!(plain(format_duration(180000)), "2d 2h");
    }
}
