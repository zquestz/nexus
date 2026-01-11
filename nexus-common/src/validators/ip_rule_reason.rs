//! IP rule reason validation
//!
//! Validates reason strings for IP rules (bans and trusts).

/// Maximum length of an IP rule reason in characters
pub const MAX_IP_RULE_REASON_LENGTH: usize = 2048;

/// Errors that can occur when validating an IP rule reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpRuleReasonError {
    /// The reason exceeds the maximum length
    TooLong,
    /// The reason contains invalid characters (control characters other than newline/tab)
    InvalidCharacters,
}

/// Validate an IP rule reason string
///
/// # Rules
/// - Maximum length: 2048 characters
/// - No control characters except newline (`\n`) and tab (`\t`)
///
/// # Arguments
/// * `reason` - The reason to validate
///
/// # Returns
/// * `Ok(())` if the reason is valid
/// * `Err(IpRuleReasonError)` if validation fails
///
/// # Examples
/// ```
/// use nexus_common::validators::{validate_ip_rule_reason, IpRuleReasonError, MAX_IP_RULE_REASON_LENGTH};
///
/// // Valid reasons
/// assert!(validate_ip_rule_reason("Spamming chat").is_ok());
/// assert!(validate_ip_rule_reason("Multiple violations:\n- Spam\n- Harassment").is_ok());
/// assert!(validate_ip_rule_reason("").is_ok()); // Empty is valid (optional field)
///
/// // Invalid: too long
/// let long_reason = "x".repeat(MAX_IP_RULE_REASON_LENGTH + 1);
/// assert_eq!(validate_ip_rule_reason(&long_reason), Err(IpRuleReasonError::TooLong));
///
/// // Invalid: control characters
/// assert_eq!(validate_ip_rule_reason("reason\x00with null"), Err(IpRuleReasonError::InvalidCharacters));
/// ```
pub fn validate_ip_rule_reason(reason: &str) -> Result<(), IpRuleReasonError> {
    // Check length
    if reason.len() > MAX_IP_RULE_REASON_LENGTH {
        return Err(IpRuleReasonError::TooLong);
    }

    // Check for invalid control characters (allow \n and \t)
    if reason
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(IpRuleReasonError::InvalidCharacters);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_reason() {
        assert!(validate_ip_rule_reason("Spamming").is_ok());
        assert!(validate_ip_rule_reason("Flooding chat with garbage").is_ok());
    }

    #[test]
    fn test_empty_reason() {
        // Empty is valid since reason is optional
        assert!(validate_ip_rule_reason("").is_ok());
    }

    #[test]
    fn test_reason_with_newlines() {
        assert!(validate_ip_rule_reason("Multiple issues:\n- Spam\n- Harassment").is_ok());
    }

    #[test]
    fn test_reason_with_tabs() {
        assert!(validate_ip_rule_reason("Violation:\t spamming").is_ok());
    }

    #[test]
    fn test_max_length_reason() {
        let reason = "x".repeat(MAX_IP_RULE_REASON_LENGTH);
        assert!(validate_ip_rule_reason(&reason).is_ok());
    }

    #[test]
    fn test_too_long_reason() {
        let reason = "x".repeat(MAX_IP_RULE_REASON_LENGTH + 1);
        assert_eq!(
            validate_ip_rule_reason(&reason),
            Err(IpRuleReasonError::TooLong)
        );
    }

    #[test]
    fn test_control_characters() {
        // Null byte
        assert_eq!(
            validate_ip_rule_reason("reason\x00here"),
            Err(IpRuleReasonError::InvalidCharacters)
        );

        // Bell
        assert_eq!(
            validate_ip_rule_reason("reason\x07here"),
            Err(IpRuleReasonError::InvalidCharacters)
        );

        // Backspace
        assert_eq!(
            validate_ip_rule_reason("reason\x08here"),
            Err(IpRuleReasonError::InvalidCharacters)
        );

        // Carriage return alone (not part of \r\n) is control
        assert_eq!(
            validate_ip_rule_reason("reason\rhere"),
            Err(IpRuleReasonError::InvalidCharacters)
        );
    }

    #[test]
    fn test_unicode_reason() {
        assert!(validate_ip_rule_reason("スパム行為").is_ok());
        assert!(validate_ip_rule_reason("Спам").is_ok());
        assert!(validate_ip_rule_reason("🚫 Banned for spam").is_ok());
    }
}
