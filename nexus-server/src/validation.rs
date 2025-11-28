//! Input validation utilities

use crate::constants::*;

/// Validate a username
///
/// Usernames must:
/// - Be between 1 and 32 characters
/// - Contain only Unicode letters (any language) or ASCII graphic characters
/// - Not contain whitespace or control characters
pub fn validate_username(username: &str) -> Result<(), &'static str> {
    if username.is_empty() {
        return Err(ERR_USERNAME_EMPTY);
    }

    if username.chars().count() > MAX_USERNAME_LENGTH {
        return Err(ERR_USERNAME_TOO_LONG);
    }

    for ch in username.chars() {
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(ERR_USERNAME_INVALID);
        }
    }

    Ok(())
}
