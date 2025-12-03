//! Password validation
//!
//! Validates password strings.

/// Maximum length for passwords in bytes
///
/// This limit prevents DoS attacks via Argon2 hashing of extremely long passwords.
pub const MAX_PASSWORD_LENGTH: usize = 256;

/// Validation error for passwords
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    /// Password is empty
    Empty,
    /// Password exceeds maximum length
    TooLong,
}

/// Validate a password
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (256 bytes)
///
/// Note: We don't check for control characters in passwords since they
/// may be part of a passphrase or generated password.
///
/// # Errors
///
/// Returns a `PasswordError` variant describing the validation failure.
pub fn validate_password(password: &str) -> Result<(), PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(PasswordError::TooLong);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_passwords() {
        assert!(validate_password("password123").is_ok());
        assert!(validate_password("a").is_ok());
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_LENGTH)).is_ok());
        // Passwords can contain special characters
        assert!(validate_password("p@$$w0rd!#$%").is_ok());
        // Passwords can contain spaces
        assert!(validate_password("correct horse battery staple").is_ok());
        // Passwords can contain unicode
        assert!(validate_password("密码🔐").is_ok());
        // Passwords can contain control characters (passphrases, generated)
        assert!(validate_password("pass\tword").is_ok());
        assert!(validate_password("pass\nword").is_ok());
    }

    #[test]
    fn test_empty() {
        assert_eq!(validate_password(""), Err(PasswordError::Empty));
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_password(&"a".repeat(MAX_PASSWORD_LENGTH + 1)),
            Err(PasswordError::TooLong)
        );
    }
}
