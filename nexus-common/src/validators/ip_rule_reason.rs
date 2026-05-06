//! IP rule reason validation
//!
//! Validates reason strings for IP rules (bans and trusts).

/// Maximum length of an IP rule reason in characters.
///
/// Counted as Unicode scalar values rather than bytes so non-ASCII users
/// (CJK, Cyrillic, etc.) get the same effective capacity as ASCII users.
///
/// Aligned with `MAX_KICK_REASON_LENGTH` (256) for consistency across
/// moderation-reason fields. Detailed audit trails belong in operator
/// logs or external trackers, not in the IP rule entry itself.
pub const MAX_IP_RULE_REASON_LENGTH: usize = 256;

// Compile-time guard: the doc claim that `MAX_IP_RULE_REASON_LENGTH`
// is aligned with `MAX_KICK_REASON_LENGTH` is load-bearing for the
// "moderation-reason cap consistency" property documented in
// `docs/protocol/09-admin.md` and the validator docstrings on both
// constants. A future tweak to either value should either match the
// other or rewrite the alignment claim — this assertion prevents the
// values from drifting silently.
const _: () = assert!(
    MAX_IP_RULE_REASON_LENGTH == super::kick_reason::MAX_KICK_REASON_LENGTH,
    "MAX_IP_RULE_REASON_LENGTH and MAX_KICK_REASON_LENGTH must stay aligned"
);

/// Errors that can occur when validating an IP rule reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpRuleReasonError {
    /// The reason exceeds the maximum length
    TooLong,
    /// The reason contains any control character (newlines, tabs, null
    /// bytes, etc.) — reasons are rendered into single-line displays
    /// in admin tools, so all control characters are rejected.
    InvalidCharacters,
}

/// Validate an IP rule reason string
///
/// # Rules
/// - Maximum length: 256 characters
/// - No control characters (newlines, tabs, null bytes, etc. all rejected —
///   reasons are rendered into single-line displays in admin tools)
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
/// assert!(validate_ip_rule_reason("").is_ok()); // Empty is valid (optional field)
///
/// // Invalid: too long
/// let long_reason = "x".repeat(MAX_IP_RULE_REASON_LENGTH + 1);
/// assert_eq!(validate_ip_rule_reason(&long_reason), Err(IpRuleReasonError::TooLong));
///
/// // Invalid: any control character (newlines, tabs, null bytes, etc.)
/// assert_eq!(validate_ip_rule_reason("reason\x00with null"), Err(IpRuleReasonError::InvalidCharacters));
/// assert_eq!(validate_ip_rule_reason("Multiple\nlines"), Err(IpRuleReasonError::InvalidCharacters));
/// assert_eq!(validate_ip_rule_reason("Has\ttab"), Err(IpRuleReasonError::InvalidCharacters));
/// ```
pub fn validate_ip_rule_reason(reason: &str) -> Result<(), IpRuleReasonError> {
    // Check length (in characters, not bytes)
    if reason.chars().count() > MAX_IP_RULE_REASON_LENGTH {
        return Err(IpRuleReasonError::TooLong);
    }

    // Reject any control character. Newlines and tabs were previously
    // carved out, but reasons are rendered into single-line admin
    // displays where embedded newlines / tabs render badly and create
    // a low-stakes log-injection vector.
    if reason.chars().any(|c| c.is_control()) {
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
    fn test_reason_with_newlines_rejected() {
        // Newlines no longer allowed (rendered into single-line displays).
        assert_eq!(
            validate_ip_rule_reason("Multiple issues:\n- Spam\n- Harassment"),
            Err(IpRuleReasonError::InvalidCharacters)
        );
    }

    #[test]
    fn test_reason_with_tabs_rejected() {
        // Tabs no longer allowed.
        assert_eq!(
            validate_ip_rule_reason("Violation:\t spamming"),
            Err(IpRuleReasonError::InvalidCharacters)
        );
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
        // Char-counted, not byte-counted: at limit accepted, over rejected.
        assert!(validate_ip_rule_reason(&"あ".repeat(MAX_IP_RULE_REASON_LENGTH)).is_ok());
        assert_eq!(
            validate_ip_rule_reason(&"あ".repeat(MAX_IP_RULE_REASON_LENGTH + 1)),
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

        // Newline rejected (was previously allowed)
        assert_eq!(
            validate_ip_rule_reason("reason\nhere"),
            Err(IpRuleReasonError::InvalidCharacters)
        );

        // Tab rejected (was previously allowed)
        assert_eq!(
            validate_ip_rule_reason("reason\there"),
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
