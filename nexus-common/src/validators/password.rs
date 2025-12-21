//! Password validation
//!
//! Validates password strings for different contexts:
//! - `validate_password_input` - For login flow (empty allowed, auth decides)
//! - `validate_password` - For setting/changing passwords (must not be empty)

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

/// Validate a password for login
///
/// Checks:
/// - Does not exceed maximum length (256 bytes)
///
/// Empty passwords are allowed - the authentication logic determines whether
/// an empty password is valid for a given account (e.g., guest accounts).
///
/// Note: We don't check for control characters in passwords since they
/// may be part of a passphrase or generated password.
///
/// # Errors
///
/// Returns a `PasswordError` variant describing the validation failure.
pub fn validate_password_input(password: &str) -> Result<(), PasswordError> {
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(PasswordError::TooLong);
    }
    Ok(())
}

/// Validate a password for setting or changing
///
/// Checks:
/// - Not empty
/// - Does not exceed maximum length (256 bytes)
///
/// Use this when a user is creating an account or changing their password,
/// where a password must be provided.
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

    // ========================================================================
    // validate_password_input tests (login flow)
    // ========================================================================

    #[test]
    fn test_input_valid_passwords() {
        assert!(validate_password_input("password123").is_ok());
        assert!(validate_password_input("a").is_ok());
        assert!(validate_password_input(&"a".repeat(MAX_PASSWORD_LENGTH)).is_ok());
        // Passwords can contain special characters
        assert!(validate_password_input("p@$$w0rd!#$%").is_ok());
        // Passwords can contain spaces
        assert!(validate_password_input("correct horse battery staple").is_ok());
        // Passwords can contain unicode
        assert!(validate_password_input("密码🔐").is_ok());
        // Passwords can contain control characters (passphrases, generated)
        assert!(validate_password_input("pass\tword").is_ok());
        assert!(validate_password_input("pass\nword").is_ok());
    }

    #[test]
    fn test_input_empty_allowed() {
        // Empty passwords are allowed for login (guest accounts)
        assert!(validate_password_input("").is_ok());
    }

    #[test]
    fn test_input_too_long() {
        assert_eq!(
            validate_password_input(&"a".repeat(MAX_PASSWORD_LENGTH + 1)),
            Err(PasswordError::TooLong)
        );
    }

    // ========================================================================
    // validate_password tests (create/change flow)
    // ========================================================================

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
