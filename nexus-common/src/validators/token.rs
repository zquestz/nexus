//! Transfer token validation
//!
//! Validates transfer tokens for file transfer authentication.

/// Expected length for transfer tokens (32 hex characters = 128 bits)
pub const TOKEN_HEX_LENGTH: usize = 32;

/// Validation error for transfer tokens
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// Token string has wrong length (must be exactly 32 characters)
    InvalidLength,
    /// Token string contains non-hexadecimal or uppercase characters
    InvalidCharacters,
}

/// Validate a transfer token string
///
/// Checks:
/// - Exactly 32 characters long (128 bits)
/// - Only lowercase hexadecimal characters (0-9, a-f)
///
/// # Errors
///
/// Returns a `TokenError` variant describing the validation failure.
pub fn validate_token(token: &str) -> Result<(), TokenError> {
    if token.len() != TOKEN_HEX_LENGTH {
        return Err(TokenError::InvalidLength);
    }

    for ch in token.chars() {
        if !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase() {
            return Err(TokenError::InvalidCharacters);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_token() {
        assert!(validate_token("ffeeddccbbaa99887766554433221100").is_ok());
        assert!(validate_token("00000000000000000000000000000000").is_ok());
        assert!(validate_token("ffffffffffffffffffffffffffffffff").is_ok());
        assert!(validate_token("abcdef0123456789abcdef0123456789").is_ok());
    }

    #[test]
    fn test_token_empty() {
        assert_eq!(validate_token(""), Err(TokenError::InvalidLength));
    }

    #[test]
    fn test_token_too_short() {
        assert_eq!(
            validate_token("ffeeddccbbaa998877665544332211"),
            Err(TokenError::InvalidLength)
        );
        assert_eq!(
            validate_token("ffeeddccbbaa9988776655443322110"),
            Err(TokenError::InvalidLength)
        );
    }

    #[test]
    fn test_token_too_long() {
        assert_eq!(
            validate_token("ffeeddccbbaa998877665544332211000"),
            Err(TokenError::InvalidLength)
        );
        assert_eq!(
            validate_token("ffeeddccbbaa99887766554433221100aa"),
            Err(TokenError::InvalidLength)
        );
    }

    #[test]
    fn test_token_uppercase_rejected() {
        assert_eq!(
            validate_token("FFEEDDCCBBAA99887766554433221100"),
            Err(TokenError::InvalidCharacters)
        );
        assert_eq!(
            validate_token("ffeeddccbbaa9988776655443322110A"),
            Err(TokenError::InvalidCharacters)
        );
    }

    #[test]
    fn test_token_non_hex_characters() {
        assert_eq!(
            validate_token("gfeeddccbbaa99887766554433221100"),
            Err(TokenError::InvalidCharacters)
        );
        assert_eq!(
            validate_token("ffeeddccbbaa9988776655443322110 "),
            Err(TokenError::InvalidCharacters)
        );
        assert_eq!(
            validate_token("ffeeddccbbaa9988776655443322110\n"),
            Err(TokenError::InvalidCharacters)
        );
    }

    #[test]
    fn test_constant_value() {
        assert_eq!(TOKEN_HEX_LENGTH, 32);
    }
}
