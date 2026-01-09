//! Nickname validation
//!
//! Validates nickname strings for shared account users.
//! Nicknames follow the same rules as usernames but are kept separate
//! for clarity and future flexibility.

/// Maximum length for nicknames in bytes
pub const MAX_NICKNAME_LENGTH: usize = 32;

/// Characters that are not allowed in nicknames (path-sensitive)
const FORBIDDEN_CHARS: &[char] = &['/', '\\', ':', '.', '<', '>', '"', '|', '?', '*'];

/// Validation error for nicknames
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NicknameError {
    /// Nickname is empty
    Empty,
    /// Nickname exceeds maximum length
    TooLong,
    /// Nickname contains invalid characters
    InvalidCharacters,
}

/// Validate a nickname
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (32 characters)
/// - Contains only valid characters:
///   - Unicode letters (any language)
///   - ASCII graphic characters (printable non-space: `!` through `~`)
///   - No whitespace or control characters
///   - No path-sensitive characters: `/`, `\`, `:`, `.`, `<`, `>`, `"`, `|`, `?`, `*`
///
/// # Errors
///
/// Returns a `NicknameError` variant describing the validation failure.
pub fn validate_nickname(nickname: &str) -> Result<(), NicknameError> {
    if nickname.is_empty() {
        return Err(NicknameError::Empty);
    }
    if nickname.chars().count() > MAX_NICKNAME_LENGTH {
        return Err(NicknameError::TooLong);
    }
    for ch in nickname.chars() {
        if FORBIDDEN_CHARS.contains(&ch) {
            return Err(NicknameError::InvalidCharacters);
        }
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(NicknameError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_nicknames() {
        assert!(validate_nickname("alice").is_ok());
        assert!(validate_nickname("Alice123").is_ok());
        assert!(validate_nickname("nick_name").is_ok());
        assert!(validate_nickname("nick-name").is_ok());
        assert!(validate_nickname(&"a".repeat(MAX_NICKNAME_LENGTH)).is_ok());
        // Unicode letters
        assert!(validate_nickname("用户").is_ok());
        assert!(validate_nickname("Пользователь").is_ok());
        assert!(validate_nickname("ユーザー").is_ok());
        // Mixed
        assert!(validate_nickname("Alice用户").is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_nickname(""), Err(NicknameError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_nickname(&"a".repeat(MAX_NICKNAME_LENGTH + 1)),
            Err(NicknameError::TooLong)
        );
    }

    #[test]
    fn test_invalid_characters() {
        // Spaces not allowed
        assert_eq!(
            validate_nickname("nick name"),
            Err(NicknameError::InvalidCharacters)
        );
        // Control characters not allowed
        assert_eq!(
            validate_nickname("nick\0name"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname("nick\tname"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname("nick\nname"),
            Err(NicknameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_path_sensitive_characters() {
        // Forward slash (Unix path separator)
        assert_eq!(
            validate_nickname("nick/name"),
            Err(NicknameError::InvalidCharacters)
        );
        // Backslash (Windows path separator)
        assert_eq!(
            validate_nickname("nick\\name"),
            Err(NicknameError::InvalidCharacters)
        );
        // Colon (Windows drive, macOS resource fork)
        assert_eq!(
            validate_nickname("nick:name"),
            Err(NicknameError::InvalidCharacters)
        );
        // Dot (directory traversal, hidden files)
        assert_eq!(
            validate_nickname("nick.name"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname(".."),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname(".hidden"),
            Err(NicknameError::InvalidCharacters)
        );
        // Windows reserved characters
        assert_eq!(
            validate_nickname("nick<name"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname("nick>name"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname("nick\"name"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname("nick|name"),
            Err(NicknameError::InvalidCharacters)
        );
        // Wildcards
        assert_eq!(
            validate_nickname("nick?name"),
            Err(NicknameError::InvalidCharacters)
        );
        assert_eq!(
            validate_nickname("nick*name"),
            Err(NicknameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_max_length_constant() {
        // Verify the constant matches username for now
        // (can be changed independently in the future)
        assert_eq!(MAX_NICKNAME_LENGTH, 32);
    }
}
