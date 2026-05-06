//! Tracker name validation.
//!
//! Validates the admin-supplied name for a configured tracker. Mirrors
//! [`super::server_name`]'s rules: trim-not-empty, length cap, no
//! control characters (newlines reported separately). Unicode and
//! emoji allowed.

/// Maximum length for a tracker name in characters.
///
/// Tracker names are admin-only metadata for distinguishing trackers
/// in the management UI — short labels like "Public NA Tracker" or
/// "Backup Tracker" are the typical case. Aligned with
/// `MAX_SERVER_NAME_LENGTH` (64) for consistency: both are short
/// display labels rendered in lists / dropdowns where extra width
/// pays off less than display compactness. Counted as Unicode scalar
/// values rather than bytes so non-ASCII names aren't penalized for
/// their UTF-8 byte length.
pub const MAX_TRACKER_NAME_LENGTH: usize = 64;

// Compile-time guard: the doc claim that `MAX_TRACKER_NAME_LENGTH`
// is aligned with `MAX_SERVER_NAME_LENGTH` is load-bearing for the
// "short display label" property. A future tweak to either value
// should either match the other or rewrite the alignment claim —
// this assertion prevents the values from drifting silently.
const _: () = assert!(
    MAX_TRACKER_NAME_LENGTH == super::server_name::MAX_SERVER_NAME_LENGTH,
    "MAX_TRACKER_NAME_LENGTH and MAX_SERVER_NAME_LENGTH must stay aligned"
);

/// Validation error for tracker names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerNameError {
    /// Tracker name is empty or contains only whitespace.
    Empty,
    /// Tracker name exceeds maximum length.
    TooLong,
    /// Tracker name contains newline characters.
    ContainsNewlines,
    /// Tracker name contains other control characters.
    InvalidCharacters,
}

/// Validate a tracker name.
///
/// Checks:
/// - Not empty or whitespace-only
/// - Does not exceed `MAX_TRACKER_NAME_LENGTH` characters
/// - No control characters (newlines reported separately)
///
/// # Errors
///
/// Returns a `TrackerNameError` variant describing the validation failure.
pub fn validate_tracker_name(name: &str) -> Result<(), TrackerNameError> {
    if name.trim().is_empty() {
        return Err(TrackerNameError::Empty);
    }
    if name.chars().count() > MAX_TRACKER_NAME_LENGTH {
        return Err(TrackerNameError::TooLong);
    }
    for ch in name.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(TrackerNameError::ContainsNewlines);
            }
            return Err(TrackerNameError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(validate_tracker_name("Public NA Tracker").is_ok());
        assert!(validate_tracker_name("My Tracker").is_ok());
        assert!(validate_tracker_name("a").is_ok());
        assert!(validate_tracker_name(&"a".repeat(MAX_TRACKER_NAME_LENGTH)).is_ok());
        // Unicode
        assert!(validate_tracker_name("トラッカー").is_ok());
        assert!(validate_tracker_name("Трекер").is_ok());
        // Emoji
        assert!(validate_tracker_name("Public Tracker 🌍").is_ok());
        // Internal whitespace allowed
        assert!(validate_tracker_name("Multi  Word  Name").is_ok());
        // Leading/trailing whitespace allowed (client trims; we don't)
        assert!(validate_tracker_name("  Edge Spaces  ").is_ok());
    }

    #[test]
    fn test_empty_names() {
        assert_eq!(validate_tracker_name(""), Err(TrackerNameError::Empty));
        assert_eq!(validate_tracker_name("   "), Err(TrackerNameError::Empty));
        assert_eq!(validate_tracker_name("\t"), Err(TrackerNameError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_tracker_name(&"a".repeat(MAX_TRACKER_NAME_LENGTH + 1)),
            Err(TrackerNameError::TooLong)
        );
        // Char-counted, not byte-counted: at limit accepted, over rejected.
        assert!(validate_tracker_name(&"あ".repeat(MAX_TRACKER_NAME_LENGTH)).is_ok());
        assert_eq!(
            validate_tracker_name(&"あ".repeat(MAX_TRACKER_NAME_LENGTH + 1)),
            Err(TrackerNameError::TooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_tracker_name("Line1\nLine2"),
            Err(TrackerNameError::ContainsNewlines)
        );
        assert_eq!(
            validate_tracker_name("Line1\rLine2"),
            Err(TrackerNameError::ContainsNewlines)
        );
        assert_eq!(
            validate_tracker_name("Line1\r\nLine2"),
            Err(TrackerNameError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_tracker_name("Hello\0World"),
            Err(TrackerNameError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_tracker_name("Hello\tWorld"),
            Err(TrackerNameError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_tracker_name("Test\x01Control"),
            Err(TrackerNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_tracker_name("Test\x7FDelete"),
            Err(TrackerNameError::InvalidCharacters)
        );
    }
}
