//! Duration parsing utilities for commands
//!
//! Shared utilities for parsing duration strings in ban/trust commands.

/// Check if a string looks like a duration format
///
/// Valid formats:
/// - `"0"` - Special case for permanent
/// - `"<number><unit>"` where unit is `m` (minutes), `h` (hours), or `d` (days)
///
/// # Examples
/// ```ignore
/// assert!(is_duration_format("0"));
/// assert!(is_duration_format("10m"));
/// assert!(is_duration_format("4h"));
/// assert!(is_duration_format("7d"));
/// assert!(!is_duration_format("invalid"));
/// assert!(!is_duration_format("10x"));
/// ```
pub fn is_duration_format(s: &str) -> bool {
    // "0" is special case for permanent
    if s == "0" {
        return true;
    }

    // Must be one or more ASCII digits followed by m, h, or d
    let Some(num_part) = s
        .strip_suffix('m')
        .or_else(|| s.strip_suffix('h'))
        .or_else(|| s.strip_suffix('d'))
    else {
        return false;
    };

    !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_duration_format_permanent() {
        assert!(is_duration_format("0"));
    }

    #[test]
    fn test_is_duration_format_minutes() {
        assert!(is_duration_format("10m"));
        assert!(is_duration_format("1m"));
        assert!(is_duration_format("999m"));
    }

    #[test]
    fn test_is_duration_format_hours() {
        assert!(is_duration_format("1h"));
        assert!(is_duration_format("24h"));
        assert!(is_duration_format("168h"));
    }

    #[test]
    fn test_is_duration_format_days() {
        assert!(is_duration_format("1d"));
        assert!(is_duration_format("7d"));
        assert!(is_duration_format("30d"));
    }

    #[test]
    fn test_is_duration_format_invalid() {
        assert!(!is_duration_format(""));
        assert!(!is_duration_format("m"));
        assert!(!is_duration_format("h"));
        assert!(!is_duration_format("d"));
        assert!(!is_duration_format("10"));
        assert!(!is_duration_format("10x"));
        assert!(!is_duration_format("abc"));
        assert!(!is_duration_format("10min"));
        assert!(!is_duration_format("-10m"));
    }

    #[test]
    fn test_is_duration_format_multibyte_no_panic() {
        // Regression: byte-index split_at panicked on multi-byte final chars
        assert!(!is_duration_format("причина"));
        assert!(!is_duration_format("é"));
        assert!(!is_duration_format("5分"));
        assert!(!is_duration_format("10м")); // Cyrillic м, not ASCII m
    }
}
