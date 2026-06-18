//! Duration parsing and formatting for ban/trust handlers and transfer termination.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::constants::ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK;
use crate::i18n::t_args;
use nexus_common::time::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};

/// Parse a `<number><unit>` duration (unit: `m`/`h`/`d`) into an expiry timestamp.
/// `Ok(None)` is permanent (None / empty / `"0"` / zero-of-any-unit);
/// `Ok(Some(ts))` is the Unix expiry; `Err(())` is an invalid format.
pub fn parse_duration(duration: &Option<String>) -> Result<Option<i64>, ()> {
    let Some(duration_str) = duration else {
        return Ok(None);
    };

    let duration_str = duration_str.trim();
    if duration_str.is_empty() || duration_str == "0" {
        return Ok(None);
    }

    // Split off the trailing unit character. Iterating chars (not byte
    // slicing) avoids a panic when the input ends in a multi-byte char.
    let mut chars = duration_str.chars();
    let Some(unit) = chars.next_back() else {
        return Err(());
    };
    let number_str = chars.as_str();

    let number: u64 = number_str.parse().map_err(|_| ())?;
    if number == 0 {
        return Ok(None);
    }

    let seconds = match unit {
        'm' => number.checked_mul(SECONDS_PER_MINUTE),
        'h' => number.checked_mul(SECONDS_PER_HOUR),
        'd' => number.checked_mul(SECONDS_PER_DAY),
        _ => return Err(()),
    }
    .ok_or(())?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
        .as_secs();

    let expiry = now.checked_add(seconds).ok_or(())?;
    Ok(Some(i64::try_from(expiry).map_err(|_| ())?))
}

/// Format the time remaining until `expires_at` (Unix timestamp) into a
/// localized string like "2d 5h", "3h 45m", or "15m" (clamped to a minimum of
/// one minute). The unit labels are translated per `locale` via the
/// `duration-remaining-*` keys, mirroring the client's ban/trust list display.
pub fn format_duration_remaining(locale: &str, expires_at: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
        .as_secs() as i64;

    let remaining_secs = (expires_at - now).max(0);

    let days = remaining_secs / SECONDS_PER_DAY as i64;
    let hours = (remaining_secs % SECONDS_PER_DAY as i64) / SECONDS_PER_HOUR as i64;
    let minutes = (remaining_secs % SECONDS_PER_HOUR as i64) / SECONDS_PER_MINUTE as i64;

    if days > 0 {
        t_args(
            locale,
            "duration-remaining-days",
            &[("days", &days.to_string()), ("hours", &hours.to_string())],
        )
    } else if hours > 0 {
        t_args(
            locale,
            "duration-remaining-hours",
            &[
                ("hours", &hours.to_string()),
                ("minutes", &minutes.to_string()),
            ],
        )
    } else {
        t_args(
            locale,
            "duration-remaining-minutes",
            &[("minutes", &minutes.max(1).to_string())],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fluent wraps interpolated values in directional isolation marks
    /// (U+2066..=U+2069); strip them so the terse duration format can be
    /// asserted exactly.
    fn visible(s: &str) -> String {
        s.chars()
            .filter(|c| !matches!(c, '\u{2066}'..='\u{2069}'))
            .collect()
    }

    #[test]
    fn test_parse_duration_none() {
        assert_eq!(parse_duration(&None), Ok(None));
    }

    #[test]
    fn test_parse_duration_empty() {
        assert_eq!(parse_duration(&Some("".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_zero() {
        assert_eq!(parse_duration(&Some("0".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_zero_minutes() {
        assert_eq!(parse_duration(&Some("0m".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_zero_hours() {
        assert_eq!(parse_duration(&Some("0h".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_zero_days() {
        assert_eq!(parse_duration(&Some("0d".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_minutes() {
        let result = parse_duration(&Some("10m".to_string()));
        assert!(result.is_ok());
        let expires_at = result.unwrap();
        assert!(expires_at.is_some());

        // Should be approximately 10 minutes from now
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let diff = expires_at.unwrap() - now;
        assert!((599..=601).contains(&diff)); // Allow 1 second tolerance
    }

    #[test]
    fn test_parse_duration_hours() {
        let result = parse_duration(&Some("2h".to_string()));
        assert!(result.is_ok());
        let expires_at = result.unwrap();
        assert!(expires_at.is_some());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let diff = expires_at.unwrap() - now;
        assert!((7199..=7201).contains(&diff)); // 2 hours
    }

    #[test]
    fn test_parse_duration_days() {
        let result = parse_duration(&Some("7d".to_string()));
        assert!(result.is_ok());
        let expires_at = result.unwrap();
        assert!(expires_at.is_some());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let diff = expires_at.unwrap() - now;
        let expected = 7 * SECONDS_PER_DAY;
        assert!((expected as i64 - 1..=expected as i64 + 1).contains(&diff));
    }

    #[test]
    fn test_parse_duration_invalid_unit() {
        assert!(parse_duration(&Some("10x".to_string())).is_err());
        assert!(parse_duration(&Some("10s".to_string())).is_err());
        assert!(parse_duration(&Some("10w".to_string())).is_err());
    }

    #[test]
    fn test_parse_duration_invalid_number() {
        assert!(parse_duration(&Some("abch".to_string())).is_err());
        assert!(parse_duration(&Some("-10m".to_string())).is_err());
    }

    #[test]
    fn test_parse_duration_too_short() {
        assert!(parse_duration(&Some("m".to_string())).is_err());
        assert!(parse_duration(&Some("h".to_string())).is_err());
        assert!(parse_duration(&Some("d".to_string())).is_err());
    }

    #[test]
    fn test_parse_duration_multibyte_unit_does_not_panic() {
        // Trailing multi-byte chars must not panic the unit-splitting path.
        assert!(parse_duration(&Some("10€".to_string())).is_err());
        assert!(parse_duration(&Some("10§".to_string())).is_err());
        assert!(parse_duration(&Some("€".to_string())).is_err());
        assert!(parse_duration(&Some("5m€".to_string())).is_err());
    }

    #[test]
    fn test_parse_duration_overflow_rejected() {
        // Overflowing the multiply, or producing a value too large for i64, is
        // rejected as invalid rather than wrapping/panicking. (parse_duration
        // itself isn't length-capped — callers validate length first.)
        assert!(parse_duration(&Some(format!("{}d", u64::MAX))).is_err());
        // Multiplies cleanly in u64 but exceeds i64::MAX → rejected at the cast.
        assert!(parse_duration(&Some("200000000000000000m".to_string())).is_err());
    }

    #[test]
    fn test_parse_duration_whitespace() {
        assert_eq!(parse_duration(&Some("  ".to_string())), Ok(None));
        assert_eq!(parse_duration(&Some(" 0 ".to_string())), Ok(None));
    }

    #[test]
    fn test_format_duration_remaining_days() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // 2 days and 5 hours from now
        let expires_at = now + (2 * SECONDS_PER_DAY as i64) + (5 * SECONDS_PER_HOUR as i64);
        assert_eq!(
            visible(&format_duration_remaining("en", expires_at)),
            "2d 5h"
        );
    }

    #[test]
    fn test_format_duration_remaining_hours() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // 3 hours and 45 minutes from now
        let expires_at = now + (3 * SECONDS_PER_HOUR as i64) + (45 * SECONDS_PER_MINUTE as i64);
        assert_eq!(
            visible(&format_duration_remaining("en", expires_at)),
            "3h 45m"
        );
    }

    #[test]
    fn test_format_duration_remaining_minutes() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // 15 minutes from now
        let expires_at = now + (15 * SECONDS_PER_MINUTE as i64);
        assert_eq!(visible(&format_duration_remaining("en", expires_at)), "15m");
    }

    #[test]
    fn test_format_duration_remaining_minimum_one_minute() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // 30 seconds from now (less than a minute)
        let expires_at = now + 30;
        assert_eq!(visible(&format_duration_remaining("en", expires_at)), "1m");
    }

    #[test]
    fn test_format_duration_remaining_expired() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Already expired (in the past)
        let expires_at = now - 100;
        assert_eq!(visible(&format_duration_remaining("en", expires_at)), "1m");
    }

    #[test]
    fn test_format_duration_remaining_localizes_units() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Unit labels are translated per-locale, not hardcoded English.
        let expires_at = now + (2 * SECONDS_PER_DAY as i64) + (5 * SECONDS_PER_HOUR as i64);
        assert_eq!(
            visible(&format_duration_remaining("ja", expires_at)),
            "2日 5時間"
        );
    }
}
