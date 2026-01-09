//! Ban reason validation
//!
//! Validates ban reason strings for the ban system.

/// Maximum length of a ban reason in characters
pub const MAX_BAN_REASON_LENGTH: usize = 2048;

/// Errors that can occur when validating a ban reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanReasonError {
    /// The reason exceeds the maximum length
    TooLong,
    /// The reason contains invalid characters (control characters other than newline/tab)
    InvalidCharacters,
}

/// Validate a ban reason string
///
/// # Rules
/// - Maximum length: 2048 characters
/// - No control characters except newline (`\n`) and tab (`\t`)
///
/// # Arguments
/// * `reason` - The ban reason to validate
///
/// # Returns
/// * `Ok(())` if the reason is valid
/// * `Err(BanReasonError)` if validation fails
///
/// # Examples
/// ```
/// use nexus_common::validators::{validate_ban_reason, BanReasonError, MAX_BAN_REASON_LENGTH};
///
/// // Valid reasons
/// assert!(validate_ban_reason("Spamming chat").is_ok());
/// assert!(validate_ban_reason("Multiple violations:\n- Spam\n- Harassment").is_ok());
/// assert!(validate_ban_reason("").is_ok()); // Empty is valid (optional field)
///
/// // Invalid: too long
/// let long_reason = "x".repeat(MAX_BAN_REASON_LENGTH + 1);
/// assert_eq!(validate_ban_reason(&long_reason), Err(BanReasonError::TooLong));
///
/// // Invalid: control characters
/// assert_eq!(validate_ban_reason("reason\x00with null"), Err(BanReasonError::InvalidCharacters));
/// ```
pub fn validate_ban_reason(reason: &str) -> Result<(), BanReasonError> {
    // Check length
    if reason.len() > MAX_BAN_REASON_LENGTH {
        return Err(BanReasonError::TooLong);
    }

    // Check for invalid control characters (allow \n and \t)
    if reason
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(BanReasonError::InvalidCharacters);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_reason() {
        assert!(validate_ban_reason("Spamming").is_ok());
        assert!(validate_ban_reason("Flooding chat with garbage").is_ok());
    }

    #[test]
    fn test_empty_reason() {
        // Empty is valid since reason is optional
        assert!(validate_ban_reason("").is_ok());
    }

    #[test]
    fn test_reason_with_newlines() {
        assert!(validate_ban_reason("Multiple issues:\n- Spam\n- Harassment").is_ok());
    }

    #[test]
    fn test_reason_with_tabs() {
        assert!(validate_ban_reason("Violation:\t spamming").is_ok());
    }

    #[test]
    fn test_max_length_reason() {
        let reason = "x".repeat(MAX_BAN_REASON_LENGTH);
        assert!(validate_ban_reason(&reason).is_ok());
    }

    #[test]
    fn test_too_long_reason() {
        let reason = "x".repeat(MAX_BAN_REASON_LENGTH + 1);
        assert_eq!(validate_ban_reason(&reason), Err(BanReasonError::TooLong));
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_ban_reason("reason\x00here"),
            Err(BanReasonError::InvalidCharacters)
        );

        // Bell
        assert_eq!(
            validate_ban_reason("reason\x07here"),
            Err(BanReasonError::InvalidCharacters)
        );

        // Backspace
        assert_eq!(
            validate_ban_reason("reason\x08here"),
            Err(BanReasonError::InvalidCharacters)
        );

        // Carriage return alone (not part of \r\n) is control
        assert_eq!(
            validate_ban_reason("reason\rhere"),
            Err(BanReasonError::InvalidCharacters)
        );
    }

    #[test]
    fn test_unicode_reason() {
        assert!(validate_ban_reason("スパム行為").is_ok());
        assert!(validate_ban_reason("Спам").is_ok());
        assert!(validate_ban_reason("🚫 Banned for spam").is_ok());
    }
}
