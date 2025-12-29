//! SHA-256 hash validation
//!
//! Validates SHA-256 hash strings for file transfer integrity verification.

/// Expected length for SHA-256 hash strings (64 hex characters)
pub const SHA256_HEX_LENGTH: usize = 64;

/// Validation error for SHA-256 hash strings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sha256Error {
    /// Hash string has wrong length (must be exactly 64 characters)
    InvalidLength,
    /// Hash string contains non-hexadecimal or uppercase characters
    InvalidCharacters,
}

/// Validate a SHA-256 hash string
///
/// Checks:
/// - Exactly 64 characters long
/// - Only lowercase hexadecimal characters (0-9, a-f)
///
/// # Errors
///
/// Returns a `Sha256Error` variant describing the validation failure.
pub fn validate_sha256(hash: &str) -> Result<(), Sha256Error> {
    if hash.len() != SHA256_HEX_LENGTH {
        return Err(Sha256Error::InvalidLength);
    }

    for ch in hash.chars() {
        if !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase() {
            return Err(Sha256Error::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hash() {
        // Valid 64-character lowercase hex string
        assert!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
                .is_ok()
        );
        assert!(
            validate_sha256("0000000000000000000000000000000000000000000000000000000000000000")
                .is_ok()
        );
        assert!(
            validate_sha256("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
                .is_ok()
        );
        assert!(
            validate_sha256("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                .is_ok()
        );
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_sha256(""), Err(Sha256Error::InvalidLength));
    }

    #[test]
    fn test_too_short() {
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb924"),
            Err(Sha256Error::InvalidLength)
        );
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"),
            Err(Sha256Error::InvalidLength)
        );
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8550"),
            Err(Sha256Error::InvalidLength)
        );
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855aa"),
            Err(Sha256Error::InvalidLength)
        );
    }

    #[test]
    fn test_uppercase_rejected() {
        // Uppercase hex characters should be rejected
        assert_eq!(
            validate_sha256("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"),
            Err(Sha256Error::InvalidCharacters)
        );
        // Mixed case should also be rejected
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85A"),
            Err(Sha256Error::InvalidCharacters)
        );
    }

    #[test]
    fn test_non_hex_characters() {
        // 'g' is not a hex character
        assert_eq!(
            validate_sha256("g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            Err(Sha256Error::InvalidCharacters)
        );
        // Space
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85 "),
            Err(Sha256Error::InvalidCharacters)
        );
        // Newline
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85\n"),
            Err(Sha256Error::InvalidCharacters)
        );
        // Special characters
        assert_eq!(
            validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8-5"),
            Err(Sha256Error::InvalidCharacters)
        );
    }

    #[test]
    fn test_constant_value() {
        assert_eq!(SHA256_HEX_LENGTH, 64);
    }
}
