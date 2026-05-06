//! Kick reason validation
//!
//! Validates the optional `reason` string on `UserKick`. Standalone
//! validator (not a re-export of `validate_ip_rule_reason`) because
//! the two domains are unrelated — IP rules describe persistent
//! access-control entries, kicks are transient session terminations
//! — and the rules can drift independently if either side's UX needs
//! evolve.

/// Maximum length for a kick reason in characters.
///
/// Counted as Unicode scalar values rather than bytes so non-ASCII
/// admins (CJK, Cyrillic, etc.) get the same effective capacity as
/// ASCII admins. Aligned with `MAX_IP_RULE_REASON_LENGTH` (256) — see
/// that constant's doc for the moderation-reason consistency
/// rationale. The single-line "kicked by X with reason: …" rendering
/// is the immediate motivation for the cap; the alignment with
/// ban/trust reasons is what locks in the specific value.
pub const MAX_KICK_REASON_LENGTH: usize = 256;

/// Validation error for kick reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KickReasonError {
    /// The reason exceeds the maximum length.
    TooLong,
    /// The reason contains control characters (including newlines and
    /// tabs — the reason is rendered as a single-line message).
    InvalidCharacters,
}

/// Validate a kick reason string.
///
/// # Rules
/// - Maximum length: 256 characters
/// - No control characters (newlines, tabs, null bytes, etc. all
///   rejected — reasons are rendered into a single-line kick message)
///
/// # Arguments
/// * `reason` - The reason to validate
///
/// # Returns
/// * `Ok(())` if the reason is valid
/// * `Err(KickReasonError)` if validation fails
///
/// # Examples
/// ```
/// use nexus_common::validators::{validate_kick_reason, KickReasonError, MAX_KICK_REASON_LENGTH};
///
/// // Valid reasons
/// assert!(validate_kick_reason("Please stop spamming").is_ok());
/// assert!(validate_kick_reason("").is_ok()); // Empty is valid (optional field)
/// assert!(validate_kick_reason("迷惑行為").is_ok()); // Unicode allowed
/// // Char-counted, not byte-counted: a multi-byte string at the
/// // character cap is valid even though its UTF-8 byte length is larger.
/// assert!(validate_kick_reason(&"あ".repeat(MAX_KICK_REASON_LENGTH)).is_ok());
///
/// // Invalid: too long
/// let long = "x".repeat(MAX_KICK_REASON_LENGTH + 1);
/// assert_eq!(validate_kick_reason(&long), Err(KickReasonError::TooLong));
///
/// // Invalid: control characters
/// assert_eq!(validate_kick_reason("a\nb"), Err(KickReasonError::InvalidCharacters));
/// assert_eq!(validate_kick_reason("a\tb"), Err(KickReasonError::InvalidCharacters));
/// assert_eq!(validate_kick_reason("a\0b"), Err(KickReasonError::InvalidCharacters));
/// ```
pub fn validate_kick_reason(reason: &str) -> Result<(), KickReasonError> {
    if reason.chars().count() > MAX_KICK_REASON_LENGTH {
        return Err(KickReasonError::TooLong);
    }
    if reason.chars().any(|c| c.is_control()) {
        return Err(KickReasonError::InvalidCharacters);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_reasons() {
        assert!(validate_kick_reason("").is_ok());
        assert!(validate_kick_reason("Spamming chat").is_ok());
        assert!(validate_kick_reason("迷惑行為").is_ok());
        assert!(validate_kick_reason("Спам").is_ok());
        assert!(validate_kick_reason("🚫 banned for spam").is_ok());
        // At max length (ASCII)
        assert!(validate_kick_reason(&"a".repeat(MAX_KICK_REASON_LENGTH)).is_ok());
        // At max length in chars (multi-byte input)
        assert!(validate_kick_reason(&"あ".repeat(MAX_KICK_REASON_LENGTH)).is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_kick_reason(&"a".repeat(MAX_KICK_REASON_LENGTH + 1)),
            Err(KickReasonError::TooLong)
        );
        // Char-counted, not byte-counted: over the char limit rejects
        // even when the byte length wouldn't be the same threshold.
        assert_eq!(
            validate_kick_reason(&"あ".repeat(MAX_KICK_REASON_LENGTH + 1)),
            Err(KickReasonError::TooLong)
        );
    }

    #[test]
    fn test_control_characters_rejected() {
        // Newline
        assert_eq!(
            validate_kick_reason("line one\nline two"),
            Err(KickReasonError::InvalidCharacters)
        );
        // Carriage return
        assert_eq!(
            validate_kick_reason("a\rb"),
            Err(KickReasonError::InvalidCharacters)
        );
        // Tab
        assert_eq!(
            validate_kick_reason("a\tb"),
            Err(KickReasonError::InvalidCharacters)
        );
        // Null byte
        assert_eq!(
            validate_kick_reason("a\0b"),
            Err(KickReasonError::InvalidCharacters)
        );
        // Bell
        assert_eq!(
            validate_kick_reason("a\x07b"),
            Err(KickReasonError::InvalidCharacters)
        );
        // Escape
        assert_eq!(
            validate_kick_reason("a\x1bb"),
            Err(KickReasonError::InvalidCharacters)
        );
        // DEL
        assert_eq!(
            validate_kick_reason("a\x7fb"),
            Err(KickReasonError::InvalidCharacters)
        );
    }
}
