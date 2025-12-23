//! Username validation
//!
//! Validates username strings.

/// Maximum length for usernames in characters
pub const MAX_USERNAME_LENGTH: usize = 32;

/// Characters that are not allowed in usernames (path-sensitive)
const FORBIDDEN_CHARS: &[char] = &['/', '\\', ':', '.', '<', '>', '"', '|', '?', '*'];

/// Validation error for usernames
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsernameError {
    /// Username is empty
    Empty,
    /// Username exceeds maximum length
    TooLong,
    /// Username contains invalid characters
    InvalidCharacters,
}

/// Validate a username
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
/// Returns a `UsernameError` variant describing the validation failure.
pub fn validate_username(username: &str) -> Result<(), UsernameError> {
    if username.is_empty() {
        return Err(UsernameError::Empty);
    }
    if username.chars().count() > MAX_USERNAME_LENGTH {
        return Err(UsernameError::TooLong);
    }
    for ch in username.chars() {
        if FORBIDDEN_CHARS.contains(&ch) {
            return Err(UsernameError::InvalidCharacters);
        }
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(UsernameError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_usernames() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("Alice123").is_ok());
        assert!(validate_username("user_name").is_ok());
        assert!(validate_username("user-name").is_ok());
        assert!(validate_username(&"a".repeat(MAX_USERNAME_LENGTH)).is_ok());
        // Unicode letters
        assert!(validate_username("用户").is_ok());
        assert!(validate_username("Пользователь").is_ok());
        assert!(validate_username("ユーザー").is_ok());
        // Mixed
        assert!(validate_username("Alice用户").is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_username(""), Err(UsernameError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_username(&"a".repeat(MAX_USERNAME_LENGTH + 1)),
            Err(UsernameError::TooLong)
        );
    }

    #[test]
    fn test_invalid_characters() {
        // Spaces not allowed
        assert_eq!(
            validate_username("user name"),
            Err(UsernameError::InvalidCharacters)
        );
        // Control characters not allowed
        assert_eq!(
            validate_username("user\0name"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("user\tname"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("user\nname"),
            Err(UsernameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_path_sensitive_characters() {
        // Forward slash (Unix path separator)
        assert_eq!(
            validate_username("user/name"),
            Err(UsernameError::InvalidCharacters)
        );
        // Backslash (Windows path separator)
        assert_eq!(
            validate_username("user\\name"),
            Err(UsernameError::InvalidCharacters)
        );
        // Colon (Windows drive, macOS resource fork)
        assert_eq!(
            validate_username("user:name"),
            Err(UsernameError::InvalidCharacters)
        );
        // Dot (directory traversal, hidden files)
        assert_eq!(
            validate_username("user.name"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username(".."),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username(".hidden"),
            Err(UsernameError::InvalidCharacters)
        );
        // Windows reserved characters
        assert_eq!(
            validate_username("user<name"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("user>name"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("user\"name"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("user|name"),
            Err(UsernameError::InvalidCharacters)
        );
        // Wildcards
        assert_eq!(
            validate_username("user?name"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("user*name"),
            Err(UsernameError::InvalidCharacters)
        );
    }
}
