//! Duration parsing utilities for handlers
//!
//! Shared utilities for parsing duration strings in ban/trust handlers.

use std::time::{SystemTime, UNIX_EPOCH};

use nexus_common::time::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};

/// Parse duration string into expiry timestamp
///
/// Format: `<number><unit>` where unit is `m` (minutes), `h` (hours), `d` (days)
/// Returns None for permanent (no expiry), or Some(timestamp) for timed rules.
///
/// # Arguments
/// * `duration` - Optional duration string (e.g., "10m", "4h", "7d", "0")
///
/// # Returns
/// * `Ok(None)` - Permanent (no expiry)
/// * `Ok(Some(timestamp))` - Expires at the given Unix timestamp
/// * `Err(())` - Invalid duration format
///
/// # Examples
/// ```ignore
/// // Permanent (no duration)
/// assert_eq!(parse_duration(&None), Ok(None));
/// assert_eq!(parse_duration(&Some("".to_string())), Ok(None));
/// assert_eq!(parse_duration(&Some("0".to_string())), Ok(None));
///
/// // Valid durations return Some(timestamp)
/// assert!(parse_duration(&Some("10m".to_string())).unwrap().is_some());
/// assert!(parse_duration(&Some("1h".to_string())).unwrap().is_some());
/// assert!(parse_duration(&Some("7d".to_string())).unwrap().is_some());
///
/// // Invalid durations
/// assert!(parse_duration(&Some("invalid".to_string())).is_err());
/// assert!(parse_duration(&Some("10x".to_string())).is_err());
/// ```
pub fn parse_duration(duration: &Option<String>) -> Result<Option<i64>, ()> {
    let Some(duration_str) = duration else {
        return Ok(None); // No duration = permanent
    };

    let duration_str = duration_str.trim();
    if duration_str.is_empty() || duration_str == "0" {
        return Ok(None); // Empty or "0" = permanent
    }

    // Parse number and unit
    let len = duration_str.len();
    if len < 2 {
        return Err(());
    }

    let unit = &duration_str[len - 1..];
    let number_str = &duration_str[..len - 1];

    let number: u64 = number_str.parse().map_err(|_| ())?;
    if number == 0 {
        return Ok(None); // 0 of anything = permanent
    }

    let seconds = match unit {
        "m" => number * SECONDS_PER_MINUTE,
        "h" => number * SECONDS_PER_HOUR,
        "d" => number * SECONDS_PER_DAY,
        _ => return Err(()),
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs();

    Ok(Some((now + seconds) as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_duration_whitespace() {
        assert_eq!(parse_duration(&Some("  ".to_string())), Ok(None));
        assert_eq!(parse_duration(&Some(" 0 ".to_string())), Ok(None));
    }
}
