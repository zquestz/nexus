//! Database validation error constants

use crate::constants::MAX_USERNAME_LENGTH;
use crate::i18n::{t, t_args};

// Username Validation Errors
/// Get translated "username empty" error
pub fn err_username_empty(locale: &str) -> String {
    t(locale, "err-username-empty")
}

/// Get translated "username invalid" error
pub fn err_username_invalid(locale: &str) -> String {
    t(locale, "err-username-invalid")
}

/// Get translated "username too long" error
pub fn err_username_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-username-too-long",
        &[("max_length", &max_length.to_string())],
    )
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
pub fn validate_username(username: &str, locale: &str) -> Result<(), String> {
    if username.is_empty() {
        return Err(err_username_empty(locale));
    }

    if username.chars().count() > MAX_USERNAME_LENGTH {
        return Err(err_username_too_long(locale, MAX_USERNAME_LENGTH));
    }

    for ch in username.chars() {
        if !ch.is_alphabetic() && !ch.is_ascii_graphic() {
            return Err(err_username_invalid(locale));
        }
    }

    Ok(())
}
