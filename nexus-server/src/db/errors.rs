//! Database validation error constants

use crate::constants::MAX_USERNAME_LENGTH;

// Username Validation Errors
/// Error message when username is empty
pub const ERR_USERNAME_EMPTY: &str = "Username cannot be empty";

/// Error message when username contains invalid characters
pub const ERR_USERNAME_INVALID: &str = "Username contains invalid characters (letters, numbers, and symbols allowed - no whitespace or control characters)";

/// Format username too long error
pub fn err_username_too_long(max_length: usize) -> String {
    format!("Username is too long (max {} characters)", max_length)
}

/// Validate a username
///
/// Usernames must:
/// - Be between 1 and 32 characters
/// - Contain only Unicode letters (any language) or ASCII graphic characters
/// - Not contain whitespace or control characters
///
/// # Errors
///
/// Returns a String error message if validation fails.
pub fn validate_username(username: &str) -> Result<(), String> {
    if username.is_empty() {
        return Err(ERR_USERNAME_EMPTY.to_string());
    }

    if username.chars().count() > MAX_USERNAME_LENGTH {
        return Err(err_username_too_long(MAX_USERNAME_LENGTH));
    }

    for ch in username.chars() {
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(ERR_USERNAME_INVALID.to_string());
        }
    }

    Ok(())
}