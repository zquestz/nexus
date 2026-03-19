//! Group name validation
//!
//! Validates group name strings.

/// Maximum length for group names in characters
pub const MAX_GROUP_NAME_LENGTH: usize = 32;

/// Characters that are not allowed in group names (path-sensitive + channel prefix)
const FORBIDDEN_CHARS: &[char] = &['/', '\\', ':', '.', '<', '>', '"', '|', '?', '*', '#'];

/// Validation error for group names
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupNameError {
    /// Group name is empty
    Empty,
    /// Group name exceeds maximum length
    TooLong,
    /// Group name contains invalid characters
    InvalidCharacters,
}

/// Validate a group name
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
/// Returns a `GroupNameError` variant describing the validation failure.
pub fn validate_group_name(name: &str) -> Result<(), GroupNameError> {
    if name.is_empty() {
        return Err(GroupNameError::Empty);
    }
    if name.chars().count() > MAX_GROUP_NAME_LENGTH {
        return Err(GroupNameError::TooLong);
    }
    for ch in name.chars() {
        if FORBIDDEN_CHARS.contains(&ch) {
            return Err(GroupNameError::InvalidCharacters);
        }
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(GroupNameError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_group_names() {
        assert!(validate_group_name("admins").is_ok());
        assert!(validate_group_name("Moderators123").is_ok());
        assert!(validate_group_name("power_users").is_ok());
        assert!(validate_group_name("tier-one").is_ok());
        assert!(validate_group_name(&"a".repeat(MAX_GROUP_NAME_LENGTH)).is_ok());
        // Unicode letters
        assert!(validate_group_name("管理员").is_ok());
        assert!(validate_group_name("Администраторы").is_ok());
        assert!(validate_group_name("管理者グループ").is_ok());
        // Mixed
        assert!(validate_group_name("Admin管理員").is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_group_name(""), Err(GroupNameError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_group_name(&"a".repeat(MAX_GROUP_NAME_LENGTH + 1)),
            Err(GroupNameError::TooLong)
        );
    }

    #[test]
    fn test_invalid_characters() {
        // Spaces not allowed
        assert_eq!(
            validate_group_name("group name"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Control characters not allowed
        assert_eq!(
            validate_group_name("group\0name"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group\tname"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group\nname"),
            Err(GroupNameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_path_sensitive_characters() {
        // Forward slash (Unix path separator)
        assert_eq!(
            validate_group_name("group/name"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Backslash (Windows path separator)
        assert_eq!(
            validate_group_name("group\\name"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Colon (Windows drive, macOS resource fork)
        assert_eq!(
            validate_group_name("group:name"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Dot (directory traversal, hidden files)
        assert_eq!(
            validate_group_name("group.name"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name(".."),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name(".hidden"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Windows reserved characters
        assert_eq!(
            validate_group_name("group<name"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group>name"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group\"name"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group|name"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Wildcards
        assert_eq!(
            validate_group_name("group?name"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group*name"),
            Err(GroupNameError::InvalidCharacters)
        );
        // Channel prefix (would conflict with channel names)
        assert_eq!(
            validate_group_name("#groupname"),
            Err(GroupNameError::InvalidCharacters)
        );
        assert_eq!(
            validate_group_name("group#name"),
            Err(GroupNameError::InvalidCharacters)
        );
    }
}
