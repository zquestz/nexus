//! Status message validation
//!
//! Validates user status messages (used for both away messages and general status).

/// Maximum length for status messages in bytes
pub const MAX_STATUS_LENGTH: usize = 128;

/// Validation error for status messages
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusError {
    /// Status message exceeds maximum length
    TooLong,
    /// Status message contains newline characters
    ContainsNewlines,
    /// Status message contains invalid characters
    InvalidCharacters,
}

/// Validate a status message
///
/// Checks:
/// - Does not exceed maximum length (128 bytes)
/// - No control characters (newlines reported separately)
///
/// Note: Empty messages are allowed (to clear status).
///
/// # Errors
///
/// Returns a `StatusError` variant describing the validation failure.
pub fn validate_status(message: &str) -> Result<(), StatusError> {
    if message.len() > MAX_STATUS_LENGTH {
        return Err(StatusError::TooLong);
    }
    for ch in message.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(StatusError::ContainsNewlines);
            }
            return Err(StatusError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_messages() {
        assert!(validate_status("grabbing lunch").is_ok());
        assert!(validate_status("brb").is_ok());
        assert!(validate_status(&"a".repeat(MAX_STATUS_LENGTH)).is_ok());
        // Unicode
        assert!(validate_status("お昼ご飯").is_ok());
        assert!(validate_status("Отошёл").is_ok());
        // Emoji
        assert!(validate_status("🍕 lunch break").is_ok());
        // General status (not away)
        assert!(validate_status("working on project X").is_ok());
        assert!(validate_status("in a meeting").is_ok());
    }

    #[test]
    fn test_empty_allowed() {
        assert!(validate_status("").is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_status(&"a".repeat(MAX_STATUS_LENGTH + 1)),
            Err(StatusError::TooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_status("Line1\nLine2"),
            Err(StatusError::ContainsNewlines)
        );
        assert_eq!(
            validate_status("Line1\rLine2"),
            Err(StatusError::ContainsNewlines)
        );
        assert_eq!(
            validate_status("Line1\r\nLine2"),
            Err(StatusError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_status("Hello\0World"),
            Err(StatusError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_status("Hello\tWorld"),
            Err(StatusError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_status("Test\x01Control"),
            Err(StatusError::InvalidCharacters)
        );
        assert_eq!(
            validate_status("Test\x7FDelete"),
            Err(StatusError::InvalidCharacters)
        );
    }
}
