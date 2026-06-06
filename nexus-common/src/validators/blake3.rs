//! BLAKE3 hash validation
//!
//! Validates lowercase hex-encoded BLAKE3-256 hash strings used for file
//! transfer and file-info integrity fields.

/// Expected length for BLAKE3-256 hash strings (64 hex characters)
pub const BLAKE3_HEX_LENGTH: usize = 64;

/// Validation error for BLAKE3 hash strings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blake3Error {
    /// Hash string has wrong length (must be exactly 64 characters)
    InvalidLength,
    /// Hash string contains non-hexadecimal or uppercase characters
    InvalidCharacters,
}

/// Validate a BLAKE3-256 hash string
///
/// Checks:
/// - Exactly 64 characters long
/// - Only lowercase hexadecimal characters (0-9, a-f)
///
/// # Errors
///
/// Returns a `Blake3Error` variant describing the validation failure.
pub fn validate_blake3(hash: &str) -> Result<(), Blake3Error> {
    if hash.len() != BLAKE3_HEX_LENGTH {
        return Err(Blake3Error::InvalidLength);
    }

    for ch in hash.chars() {
        if !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase() {
            return Err(Blake3Error::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hash() {
        assert!(
            validate_blake3(
                "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9ad\
                c112b7cc9a93cae41f3262"
            )
            .is_ok()
        );
        assert!(
            validate_blake3(
                "6437b3ac38465133ffb63b75273a8db548c558465d79\
                db03fd359c6cd5bd9d85"
            )
            .is_ok()
        );
        assert!(
            validate_blake3("0000000000000000000000000000000000000000000000000000000000000000")
                .is_ok()
        );
        assert!(
            validate_blake3("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
                .is_ok()
        );
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_blake3(""), Err(Blake3Error::InvalidLength));
    }

    #[test]
    fn test_too_short() {
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc949"),
            Err(Blake3Error::InvalidLength)
        );
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f326"),
            Err(Blake3Error::InvalidLength)
        );
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f32620"),
            Err(Blake3Error::InvalidLength)
        );
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262aa"),
            Err(Blake3Error::InvalidLength)
        );
    }

    #[test]
    fn test_uppercase_rejected() {
        assert_eq!(
            validate_blake3("AF1349B9F5F9A1A6A0404DEA36DCC9499BCB25C9ADC112B7CC9A93CAE41F3262"),
            Err(Blake3Error::InvalidCharacters)
        );
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f326A"),
            Err(Blake3Error::InvalidCharacters)
        );
    }

    #[test]
    fn test_non_hex_characters() {
        assert_eq!(
            validate_blake3("gf1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"),
            Err(Blake3Error::InvalidCharacters)
        );
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f326 "),
            Err(Blake3Error::InvalidCharacters)
        );
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f326\n"),
            Err(Blake3Error::InvalidCharacters)
        );
        assert_eq!(
            validate_blake3("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f326-"),
            Err(Blake3Error::InvalidCharacters)
        );
    }

    #[test]
    fn test_constant_value() {
        assert_eq!(BLAKE3_HEX_LENGTH, 64);
    }
}
