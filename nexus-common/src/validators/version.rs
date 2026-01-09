//! Version validation
//!
//! Validates protocol version strings using the semver crate.

use semver::Version;

/// Maximum length for version strings in bytes
pub const MAX_VERSION_LENGTH: usize = 32;

/// Validation error for version strings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// Version string is empty
    Empty,
    /// Version string exceeds maximum length
    TooLong,
    /// Version string is not valid semver format
    InvalidSemver,
}

/// Validate a handshake version string and parse it
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (32 characters)
/// - Valid semver format (via semver crate)
///
/// # Returns
///
/// The parsed `Version` on success.
///
/// # Errors
///
/// Returns a `VersionError` variant describing the validation failure.
pub fn validate_version(version: &str) -> Result<Version, VersionError> {
    if version.is_empty() {
        return Err(VersionError::Empty);
    }
    if version.len() > MAX_VERSION_LENGTH {
        return Err(VersionError::TooLong);
    }

    // Use semver crate for format validation and parsing
    Version::parse(version).map_err(|_| VersionError::InvalidSemver)
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
        assert!(validate_version("1.0.0+build.123").is_ok());
        assert!(validate_version("1.0.0-alpha+build").is_ok());
        assert!(validate_version("10.20.30").is_ok());
        assert!(validate_version("0.0.0").is_ok());
    }

    #[test]
    fn test_returns_parsed_version() {
        let result = validate_version("1.2.3").unwrap();
        assert_eq!(result.major, 1);
        assert_eq!(result.minor, 2);
        assert_eq!(result.patch, 3);
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_version(""), Err(VersionError::Empty));
    }

    #[test]
    fn test_too_long() {
        let long_version = format!("1.0.0-{}", "a".repeat(MAX_VERSION_LENGTH));
        assert_eq!(validate_version(&long_version), Err(VersionError::TooLong));
    }

    #[test]
    fn test_invalid_semver_format() {
        // Not enough components
        assert_eq!(validate_version("1"), Err(VersionError::InvalidSemver));
        assert_eq!(validate_version("1.0"), Err(VersionError::InvalidSemver));

        // Non-numeric components
        assert_eq!(validate_version("a.b.c"), Err(VersionError::InvalidSemver));
        assert_eq!(validate_version("1.2.x"), Err(VersionError::InvalidSemver));

        // Just random strings
        assert_eq!(
            validate_version("not-a-version"),
            Err(VersionError::InvalidSemver)
        );

        // Control characters rejected by semver parser
        assert_eq!(
            validate_version("0.3.0\n"),
            Err(VersionError::InvalidSemver)
        );
    }
}
