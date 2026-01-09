//! Message validation
//!
//! Validates chat messages, broadcasts, and private messages.

/// Maximum length for messages (chat, broadcast, private messages) in bytes
pub const MAX_MESSAGE_LENGTH: usize = 1024;

/// Validation error for messages
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    /// Message is empty or contains only whitespace
    Empty,
    /// Message exceeds maximum length
    TooLong,
    /// Message contains newline characters
    ContainsNewlines,
    /// Message contains invalid characters
    InvalidCharacters,
}

/// Validate a message (chat, broadcast, private message)
///
/// Checks:
/// - Not empty or whitespace-only
/// - Does not exceed maximum length (1024 characters)
/// - No control characters (newlines reported separately)
///
/// # Errors
///
/// Returns a `MessageError` variant describing the validation failure.
pub fn validate_message(message: &str) -> Result<(), MessageError> {
    if message.trim().is_empty() {
        return Err(MessageError::Empty);
    }
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(MessageError::TooLong);
    }
    for ch in message.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(MessageError::ContainsNewlines);
            }
            return Err(MessageError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_messages() {
        assert!(validate_message("Hello, world!").is_ok());
        assert!(validate_message("a").is_ok());
        assert!(validate_message(&"a".repeat(MAX_MESSAGE_LENGTH)).is_ok());
    }

    #[test]
    fn test_empty_messages() {
        assert_eq!(validate_message(""), Err(MessageError::Empty));
        assert_eq!(validate_message("   "), Err(MessageError::Empty));
        assert_eq!(validate_message("\t"), Err(MessageError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_message(&"a".repeat(MAX_MESSAGE_LENGTH + 1)),
            Err(MessageError::TooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_message("Hello\nWorld"),
            Err(MessageError::ContainsNewlines)
        );
        assert_eq!(
            validate_message("Hello\rWorld"),
            Err(MessageError::ContainsNewlines)
        );
        assert_eq!(
            validate_message("Hello\r\nWorld"),
            Err(MessageError::ContainsNewlines)
        );
    }

    #[test]
    fn test_valid_characters() {
        // ASCII graphic characters
        assert!(validate_message("Hello, world!").is_ok());
        assert!(validate_message("Test @#$%^&*()").is_ok());
        // Unicode letters
        assert!(validate_message("日本語").is_ok());
        assert!(validate_message("Привет").is_ok());
        assert!(validate_message("مرحبا").is_ok());
        // Spaces
        assert!(validate_message("Multiple   spaces").is_ok());
        // Emoji and other unicode
        assert!(validate_message("Hello 👋 World").is_ok());
        assert!(validate_message("Math: ∑∏∫").is_ok());
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_message("Hello\0World"),
            Err(MessageError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_message("Hello\tWorld"),
            Err(MessageError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_message("Hello\x01World"),
            Err(MessageError::InvalidCharacters)
        );
        assert_eq!(
            validate_message("Test\x7FDelete"),
            Err(MessageError::InvalidCharacters)
        );
        // Escape character
        assert_eq!(
            validate_message("Test\x1BEscape"),
            Err(MessageError::InvalidCharacters)
        );
    }
}
