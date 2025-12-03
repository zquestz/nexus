//! Version validation
//!
//! Validates protocol version strings.

/// Maximum length for version strings in characters
pub const MAX_VERSION_LENGTH: usize = 32;

/// Validation error for version strings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// Version string is empty
    Empty,
    /// Version string exceeds maximum length
    TooLong,
    /// Version string contains invalid characters
    InvalidCharacters,
}

/// Validate a handshake version string
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (32 characters)
/// - No control characters
///
/// # Errors
///
/// Returns a `HandshakeError` variant describing the validation failure.
pub fn validate_version(version: &str) -> Result<(), VersionError> {
    if version.is_empty() {
        return Err(VersionError::Empty);
    }
    if version.len() > MAX_VERSION_LENGTH {
        return Err(VersionError::TooLong);
    }
    for ch in version.chars() {
        if ch.is_control() {
            return Err(VersionError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_versions() {
        assert!(validate_version("0.3.0").is_ok());
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("0.1.0-alpha").is_ok());
        assert!(validate_version("2.0.0-beta.1").is_ok());
        assert!(validate_version(&"a".repeat(MAX_VERSION_LENGTH)).is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_version(""), Err(VersionError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_version(&"a".repeat(MAX_VERSION_LENGTH + 1)),
            Err(VersionError::TooLong)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_version("0.3.0\0"),
            Err(VersionError::InvalidCharacters)
        );
        // Newline
        assert_eq!(
            validate_version("0.3.0\n"),
            Err(VersionError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_version("0.3.0\t"),
            Err(VersionError::InvalidCharacters)
        );
        // Other control characters
        assert_eq!(
            validate_version("0.3.0\x01"),
            Err(VersionError::InvalidCharacters)
        );
    }
}
