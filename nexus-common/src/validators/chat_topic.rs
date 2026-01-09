//! Topic validation
//!
//! Validates chat topic strings.

/// Maximum length for topics in bytes
pub const MAX_CHAT_TOPIC_LENGTH: usize = 256;

/// Validation error for topics
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatTopicError {
    /// Chat topic exceeds maximum length
    TooLong,
    /// Chat topic contains newline characters
    ContainsNewlines,
    /// Chat topic contains invalid characters
    InvalidCharacters,
}

/// Validate a chat topic
///
/// Checks:
/// - Does not exceed maximum length (256 characters)
/// - No control characters (newlines reported separately)
///
/// Note: Empty topics are allowed (to clear the topic).
///
/// # Errors
///
/// Returns a `ChatTopicError` variant describing the validation failure.
pub fn validate_chat_topic(topic: &str) -> Result<(), ChatTopicError> {
    if topic.len() > MAX_CHAT_TOPIC_LENGTH {
        return Err(ChatTopicError::TooLong);
    }
    for ch in topic.chars() {
        if ch.is_control() {
            if ch == '\n' || ch == '\r' {
                return Err(ChatTopicError::ContainsNewlines);
            }
            return Err(ChatTopicError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_topics() {
        assert!(validate_chat_topic("Welcome to the server!").is_ok());
        assert!(validate_chat_topic(&"a".repeat(MAX_CHAT_TOPIC_LENGTH)).is_ok());
        // Unicode
        assert!(validate_chat_topic("日本語のトピック").is_ok());
        assert!(validate_chat_topic("Тема чата").is_ok());
        // Emoji
        assert!(validate_chat_topic("Welcome! 🎉").is_ok());
    }

    #[test]
    fn test_empty_allowed() {
        assert!(validate_chat_topic("").is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_chat_topic(&"a".repeat(MAX_CHAT_TOPIC_LENGTH + 1)),
            Err(ChatTopicError::TooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_chat_topic("Line1\nLine2"),
            Err(ChatTopicError::ContainsNewlines)
        );
        assert_eq!(
            validate_chat_topic("Line1\rLine2"),
            Err(ChatTopicError::ContainsNewlines)
        );
        assert_eq!(
            validate_chat_topic("Line1\r\nLine2"),
            Err(ChatTopicError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_chat_topic("Hello\0World"),
            Err(ChatTopicError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_chat_topic("Hello\tWorld"),
            Err(ChatTopicError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_chat_topic("Test\x01Control"),
            Err(ChatTopicError::InvalidCharacters)
        );
        assert_eq!(
            validate_chat_topic("Test\x7FDelete"),
            Err(ChatTopicError::InvalidCharacters)
        );
    }
}
