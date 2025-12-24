//! File path validation
//!
//! Validates file paths for file area operations.

/// Maximum length for file paths in characters
pub const MAX_FILE_PATH_LENGTH: usize = 4096;

/// Validation error for file paths
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePathError {
    /// Path exceeds maximum length
    TooLong,
    /// Path contains null bytes
    ContainsNull,
    /// Path contains invalid characters (control characters)
    InvalidCharacters,
}

/// Validate a file path from the client
///
/// Checks:
/// - Does not exceed maximum length (4096 characters)
/// - No null bytes
/// - No control characters (except path separators are allowed)
///
/// Note: This validator does NOT check for path traversal (../) as that
/// is handled by the server's `resolve_path()` function which canonicalizes
/// and verifies the path is within the allowed area.
///
/// # Errors
///
/// Returns a `FilePathError` variant describing the validation failure.
pub fn validate_file_path(path: &str) -> Result<(), FilePathError> {
    if path.len() > MAX_FILE_PATH_LENGTH {
        return Err(FilePathError::TooLong);
    }

    for ch in path.chars() {
        if ch == '\0' {
            return Err(FilePathError::ContainsNull);
        }
        // Allow path separators and normal printable characters
        // Reject other control characters
        if ch.is_control() && ch != '/' && ch != '\\' {
            return Err(FilePathError::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paths() {
        assert!(validate_file_path("").is_ok());
        assert!(validate_file_path("/").is_ok());
        assert!(validate_file_path("/Documents").is_ok());
        assert!(validate_file_path("/Documents/file.txt").is_ok());
        assert!(validate_file_path("Documents/file.txt").is_ok());
        assert!(validate_file_path("/path/to/deeply/nested/file.txt").is_ok());
    }

    #[test]
    fn test_unicode_paths() {
        assert!(validate_file_path("/日本語/ファイル.txt").is_ok());
        assert!(validate_file_path("/Документы/файл.txt").is_ok());
        assert!(validate_file_path("/文件夹/文件.txt").is_ok());
        assert!(validate_file_path("/Émojis 👋/file.txt").is_ok());
    }

    #[test]
    fn test_windows_style_paths() {
        // Backslashes are allowed (server handles translation)
        assert!(validate_file_path("\\Documents\\file.txt").is_ok());
        assert!(validate_file_path("/Mixed\\Separators/file.txt").is_ok());
    }

    #[test]
    fn test_path_too_long() {
        let long_path = "/".to_string() + &"a".repeat(MAX_FILE_PATH_LENGTH);
        assert_eq!(validate_file_path(&long_path), Err(FilePathError::TooLong));

        // Exactly at limit should be ok
        let max_path = "a".repeat(MAX_FILE_PATH_LENGTH);
        assert!(validate_file_path(&max_path).is_ok());
    }

    #[test]
    fn test_null_bytes() {
        assert_eq!(
            validate_file_path("/path/with\0null"),
            Err(FilePathError::ContainsNull)
        );
        assert_eq!(validate_file_path("\0"), Err(FilePathError::ContainsNull));
    }

    #[test]
    fn test_control_characters() {
        // Tab
        assert_eq!(
            validate_file_path("/path/with\ttab"),
            Err(FilePathError::InvalidCharacters)
        );
        // Newline
        assert_eq!(
            validate_file_path("/path/with\nnewline"),
            Err(FilePathError::InvalidCharacters)
        );
        // Carriage return
        assert_eq!(
            validate_file_path("/path/with\rreturn"),
            Err(FilePathError::InvalidCharacters)
        );
        // Bell
        assert_eq!(
            validate_file_path("/path/with\x07bell"),
            Err(FilePathError::InvalidCharacters)
        );
        // Escape
        assert_eq!(
            validate_file_path("/path/with\x1Bescape"),
            Err(FilePathError::InvalidCharacters)
        );
    }

    #[test]
    fn test_traversal_patterns_allowed() {
        // These are allowed by the validator - security is handled by resolve_path()
        assert!(validate_file_path("..").is_ok());
        assert!(validate_file_path("../etc/passwd").is_ok());
        assert!(validate_file_path("/path/../../../etc/passwd").is_ok());
    }

    #[test]
    fn test_special_filenames() {
        // These are valid from a character perspective
        assert!(validate_file_path("/path/file with spaces.txt").is_ok());
        assert!(validate_file_path("/path/file-with-dashes.txt").is_ok());
        assert!(validate_file_path("/path/file_with_underscores.txt").is_ok());
        assert!(validate_file_path("/path/file.multiple.dots.txt").is_ok());
        assert!(validate_file_path("/path/.hidden").is_ok());
        assert!(validate_file_path("/path/...").is_ok());
    }
}
