//! Directory name validation
//!
//! Validates directory names for file area operations.

/// Maximum length for directory names in bytes
pub const MAX_DIR_NAME_LENGTH: usize = 255;

/// Validation error for directory names
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirNameError {
    /// Name is empty
    Empty,
    /// Name exceeds maximum length
    TooLong,
    /// Name contains path separators (/ or \)
    ContainsPathSeparator,
    /// Name contains parent directory reference (..)
    ContainsParentRef,
    /// Name contains null bytes
    ContainsNull,
    /// Name contains invalid characters (control characters)
    InvalidCharacters,
}

/// Validate a directory name for creation
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (255 bytes)
/// - No path separators (/ or \)
/// - Not ".." (parent directory reference)
/// - No null bytes
/// - No control characters
///
/// # Errors
///
/// Returns a `DirNameError` variant describing the validation failure.
pub fn validate_dir_name(name: &str) -> Result<(), DirNameError> {
    // Check for empty name
    if name.is_empty() {
        return Err(DirNameError::Empty);
    }

    // Check length
    if name.len() > MAX_DIR_NAME_LENGTH {
        return Err(DirNameError::TooLong);
    }

    // Check for parent directory reference
    if name == ".." {
        return Err(DirNameError::ContainsParentRef);
    }

    // Check each character
    for ch in name.chars() {
        // Path separators
        if ch == '/' || ch == '\\' {
            return Err(DirNameError::ContainsPathSeparator);
        }

        // Null byte
        if ch == '\0' {
            return Err(DirNameError::ContainsNull);
        }

        // Control characters (except we already handled null)
        if ch.is_control() {
            return Err(DirNameError::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(validate_dir_name("Documents").is_ok());
        assert!(validate_dir_name("My Files").is_ok());
        assert!(validate_dir_name("folder-with-dashes").is_ok());
        assert!(validate_dir_name("folder_with_underscores").is_ok());
        assert!(validate_dir_name("folder.with").is_ok());
        assert!(validate_dir_name(".hidden").is_ok());
        assert!(validate_dir_name("...").is_ok());
        assert!(validate_dir_name(".").is_ok()); // Current dir reference is OK as a name
    }

    #[test]
    fn test_unicode_names() {
        assert!(validate_dir_name("日本語フォルダ").is_ok());
        assert!(validate_dir_name("Документы").is_ok());
        assert!(validate_dir_name("文件夹").is_ok());
        assert!(validate_dir_name("Émojis 👋").is_ok());
        assert!(validate_dir_name("Ümlauts äöü").is_ok());
    }

    #[test]
    fn test_empty_name() {
        assert_eq!(validate_dir_name(""), Err(DirNameError::Empty));
    }

    #[test]
    fn test_too_long() {
        let long_name = "a".repeat(MAX_DIR_NAME_LENGTH + 1);
        assert_eq!(validate_dir_name(&long_name), Err(DirNameError::TooLong));

        // Exactly at limit should be ok
        let max_name = "a".repeat(MAX_DIR_NAME_LENGTH);
        assert!(validate_dir_name(&max_name).is_ok());
    }

    #[test]
    fn test_path_separators() {
        assert_eq!(
            validate_dir_name("path/to/dir"),
            Err(DirNameError::ContainsPathSeparator)
        );
        assert_eq!(
            validate_dir_name("path\\to\\dir"),
            Err(DirNameError::ContainsPathSeparator)
        );
        assert_eq!(
            validate_dir_name("/leading"),
            Err(DirNameError::ContainsPathSeparator)
        );
        assert_eq!(
            validate_dir_name("trailing/"),
            Err(DirNameError::ContainsPathSeparator)
        );
    }

    #[test]
    fn test_parent_reference() {
        assert_eq!(
            validate_dir_name(".."),
            Err(DirNameError::ContainsParentRef)
        );
    }

    #[test]
    fn test_null_bytes() {
        assert_eq!(
            validate_dir_name("name\0with\0null"),
            Err(DirNameError::ContainsNull)
        );
        assert_eq!(validate_dir_name("\0"), Err(DirNameError::ContainsNull));
    }

    #[test]
    fn test_control_characters() {
        // Tab
        assert_eq!(
            validate_dir_name("name\twith\ttab"),
            Err(DirNameError::InvalidCharacters)
        );
        // Newline
        assert_eq!(
            validate_dir_name("name\nwith\nnewline"),
            Err(DirNameError::InvalidCharacters)
        );
        // Carriage return
        assert_eq!(
            validate_dir_name("name\rwith\rreturn"),
            Err(DirNameError::InvalidCharacters)
        );
        // Bell
        assert_eq!(
            validate_dir_name("name\x07bell"),
            Err(DirNameError::InvalidCharacters)
        );
        // Escape
        assert_eq!(
            validate_dir_name("name\x1Bescape"),
            Err(DirNameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_special_but_valid_names() {
        // These might look suspicious but are valid directory names
        assert!(validate_dir_name("CON").is_ok()); // Windows reserved, but we validate at FS level
        assert!(validate_dir_name("PRN").is_ok());
        assert!(validate_dir_name("NUL").is_ok());
        assert!(validate_dir_name(" leading space").is_ok());
        assert!(validate_dir_name("trailing space ").is_ok());
        assert!(validate_dir_name("  ").is_ok()); // Just spaces
    }
}
