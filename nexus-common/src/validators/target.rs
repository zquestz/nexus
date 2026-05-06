//! Target validation
//!
//! Validates the target field for ban and trust operations.
//! This performs length-only validation; semantic validation (nickname lookup,
//! IP address parsing, CIDR range parsing) is handled by the server handlers.

/// Maximum length for a target (ban/trust) in characters.
///
/// Target can be:
/// - A nickname (Unicode-friendly; up to 32 chars per `validate_nickname`)
/// - An IPv4 address (max 15 chars, e.g., "255.255.255.255")
/// - An IPv6 address (max 45 chars, e.g., "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff")
/// - A CIDR range (max ~49 chars for IPv6 with prefix, e.g., "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff/128")
///
/// Using 64 as a reasonable upper bound that covers all cases with headroom.
/// Measured in characters so non-ASCII nicknames don't get penalized by their
/// UTF-8 byte length.
pub const MAX_TARGET_LENGTH: usize = 64;

/// Validation error for target (ban/trust)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// Target string is empty
    Empty,
    /// Target exceeds maximum length
    TooLong,
}

/// Validate a target string (length-only)
///
/// This only validates the length. Semantic validation (whether it's a valid
/// nickname, IP address, or CIDR range) is performed by server handlers.
///
/// # Arguments
/// * `target` - The target string to validate
///
/// # Returns
/// * `Ok(())` if the target length is valid
/// * `Err(TargetError)` if validation fails
pub fn validate_target(target: &str) -> Result<(), TargetError> {
    if target.is_empty() {
        return Err(TargetError::Empty);
    }
    if target.chars().count() > MAX_TARGET_LENGTH {
        return Err(TargetError::TooLong);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_targets() {
        // Nickname
        assert!(validate_target("alice").is_ok());
        // IPv4
        assert!(validate_target("192.168.1.1").is_ok());
        // IPv6
        assert!(validate_target("2001:db8::1").is_ok());
        // CIDR
        assert!(validate_target("192.168.0.0/24").is_ok());
        // At max length (ASCII)
        assert!(validate_target(&"a".repeat(MAX_TARGET_LENGTH)).is_ok());
        // Unicode nickname (each char is multiple bytes but counts as 1)
        assert!(validate_target("アリス").is_ok());
        // At max length in chars (would be 3× bytes, still valid)
        assert!(validate_target(&"あ".repeat(MAX_TARGET_LENGTH)).is_ok());
    }

    #[test]
    fn test_empty_target() {
        assert_eq!(validate_target(""), Err(TargetError::Empty));
    }

    #[test]
    fn test_target_too_long() {
        assert_eq!(
            validate_target(&"a".repeat(MAX_TARGET_LENGTH + 1)),
            Err(TargetError::TooLong)
        );
        // Unicode beyond max
        assert_eq!(
            validate_target(&"あ".repeat(MAX_TARGET_LENGTH + 1)),
            Err(TargetError::TooLong)
        );
    }
}
