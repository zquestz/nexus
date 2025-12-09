//! Server name validation
//!
//! Validates server name strings.

/// Maximum length for server name in characters
pub const MAX_SERVER_NAME_LENGTH: usize = 64;

/// Validation error for server names
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerNameError {
    /// Server name is empty or contains only whitespace
    Empty,
    /// Server name exceeds maximum length
    TooLong,
    /// Server name contains newline characters
    ContainsNewlines,
    /// Server name contains invalid characters
    InvalidCharacters,
}

/// Validate a server name
///
/// Checks:
/// - Not empty or whitespace-only
/// - Does not exceed maximum length (64 characters)
/// - No control characters (newlines reported separately)
///
/// # Errors
///
/// Returns a `ServerNameError` variant describing the validation failure.
pub fn validate_server_name(name: &str) -> Result<(), ServerNameError> {
    if name.trim().is_empty() {
        return Err(ServerNameError::Empty);
    }
    if name.len() > MAX_SERVER_NAME_LENGTH {
        return Err(ServerNameError::TooLong);
    }
    for ch in name.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(ServerNameError::ContainsNewlines);
            }
            return Err(ServerNameError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(validate_server_name("My Server").is_ok());
        assert!(validate_server_name("Nexus BBS").is_ok());
        assert!(validate_server_name(&"a".repeat(MAX_SERVER_NAME_LENGTH)).is_ok());
        // Unicode
        assert!(validate_server_name("サーバー").is_ok());
        assert!(validate_server_name("Сервер").is_ok());
        // Emoji
        assert!(validate_server_name("My Server 🚀").is_ok());
    }

    #[test]
    fn test_empty_names() {
        assert_eq!(validate_server_name(""), Err(ServerNameError::Empty));
        assert_eq!(validate_server_name("   "), Err(ServerNameError::Empty));
        assert_eq!(validate_server_name("\t"), Err(ServerNameError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_server_name(&"a".repeat(MAX_SERVER_NAME_LENGTH + 1)),
            Err(ServerNameError::TooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_server_name("Line1\nLine2"),
            Err(ServerNameError::ContainsNewlines)
        );
        assert_eq!(
            validate_server_name("Line1\rLine2"),
            Err(ServerNameError::ContainsNewlines)
        );
        assert_eq!(
            validate_server_name("Line1\r\nLine2"),
            Err(ServerNameError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_server_name("Hello\0World"),
            Err(ServerNameError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_server_name("Hello\tWorld"),
            Err(ServerNameError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_server_name("Test\x01Control"),
            Err(ServerNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_server_name("Test\x7FDelete"),
            Err(ServerNameError::InvalidCharacters)
        );
    }
}
