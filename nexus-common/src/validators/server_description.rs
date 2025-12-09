//! Server description validation
//!
//! Validates server description strings.

/// Maximum length for server description in characters
pub const MAX_SERVER_DESCRIPTION_LENGTH: usize = 256;

/// Validation error for server description
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerDescriptionError {
    /// Server description exceeds maximum length
    TooLong,
    /// Server description contains newline characters
    ContainsNewlines,
    /// Server description contains invalid characters
    InvalidCharacters,
}

/// Validate a server description
///
/// Checks:
/// - Does not exceed maximum length (256 characters)
/// - No control characters (newlines reported separately)
///
/// Note: Empty descriptions are allowed (to clear the description).
///
/// # Errors
///
/// Returns a `ServerDescriptionError` variant describing the validation failure.
pub fn validate_server_description(description: &str) -> Result<(), ServerDescriptionError> {
    if description.len() > MAX_SERVER_DESCRIPTION_LENGTH {
        return Err(ServerDescriptionError::TooLong);
    }
    for ch in description.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(ServerDescriptionError::ContainsNewlines);
            }
            return Err(ServerDescriptionError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_descriptions() {
        assert!(validate_server_description("Welcome to the server!").is_ok());
        assert!(validate_server_description(&"a".repeat(MAX_SERVER_DESCRIPTION_LENGTH)).is_ok());
        // Unicode
        assert!(validate_server_description("日本語の説明").is_ok());
        assert!(validate_server_description("Описание сервера").is_ok());
        // Emoji
        assert!(validate_server_description("Welcome! 🎉").is_ok());
    }

    #[test]
    fn test_empty_allowed() {
        assert!(validate_server_description("").is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_server_description(&"a".repeat(MAX_SERVER_DESCRIPTION_LENGTH + 1)),
            Err(ServerDescriptionError::TooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_server_description("Line1\nLine2"),
            Err(ServerDescriptionError::ContainsNewlines)
        );
        assert_eq!(
            validate_server_description("Line1\rLine2"),
            Err(ServerDescriptionError::ContainsNewlines)
        );
        assert_eq!(
            validate_server_description("Line1\r\nLine2"),
            Err(ServerDescriptionError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_server_description("Hello\0World"),
            Err(ServerDescriptionError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_server_description("Hello\tWorld"),
            Err(ServerDescriptionError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_server_description("Test\x01Control"),
            Err(ServerDescriptionError::InvalidCharacters)
        );
        assert_eq!(
            validate_server_description("Test\x7FDelete"),
            Err(ServerDescriptionError::InvalidCharacters)
        );
    }
}
