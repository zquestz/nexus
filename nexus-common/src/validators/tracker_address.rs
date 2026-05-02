//! Tracker address validation.
//!
//! Validates the admin-supplied address for a configured tracker.
//! Distinct from [`super::public_address`] because the BBS server's
//! `ServerInfo.public_address` legitimately accepts an empty string
//! ("unset / not advertised"), whereas a tracker row with no address
//! has no daemon to connect to — empty must be a hard error.
//!
//! Beyond the empty check, this validator delegates to the shared
//! `validate_public_address` rule set so the address-format rules
//! (length, brackets, scheme, IDN, IPv4/IPv6 literals) stay
//! single-source.

use super::public_address::{PublicAddressError, validate_public_address};

/// Validation error for tracker addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerAddressError {
    /// Address is empty or whitespace-only. There's no tracker daemon
    /// at the empty string; admin must supply a real address.
    Empty,
    /// Address fails the shared public-address rule set (length,
    /// brackets, scheme, path, userinfo, whitespace, port suffix,
    /// IPv6 zone identifier, malformed hostname / IP literal).
    Invalid(PublicAddressError),
}

/// Validate a tracker address.
///
/// Rejects empty / whitespace-only input outright; delegates the rest
/// to [`validate_public_address`].
///
/// # Errors
///
/// Returns a `TrackerAddressError` variant describing the failure.
pub fn validate_tracker_address(addr: &str) -> Result<(), TrackerAddressError> {
    if addr.trim().is_empty() {
        return Err(TrackerAddressError::Empty);
    }
    validate_public_address(addr).map_err(TrackerAddressError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(
            validate_tracker_address(""),
            Err(TrackerAddressError::Empty)
        );
        assert_eq!(
            validate_tracker_address("   "),
            Err(TrackerAddressError::Empty)
        );
        assert_eq!(
            validate_tracker_address("\t\n"),
            Err(TrackerAddressError::Empty)
        );
    }

    #[test]
    fn accepts_valid_addresses() {
        assert!(validate_tracker_address("tracker.example.com").is_ok());
        assert!(validate_tracker_address("127.0.0.1").is_ok());
        assert!(validate_tracker_address("::1").is_ok());
        assert!(validate_tracker_address("xn--nxasmq6b.example").is_ok());
    }

    #[test]
    fn rejects_invalid_via_underlying_validator() {
        // Bracketed IPv6 — caught by validate_public_address.
        assert!(matches!(
            validate_tracker_address("[::1]"),
            Err(TrackerAddressError::Invalid(_))
        ));
        // URL scheme — caught by validate_public_address.
        assert!(matches!(
            validate_tracker_address("https://tracker.example.com"),
            Err(TrackerAddressError::Invalid(_))
        ));
        // Embedded port — caught by validate_public_address.
        assert!(matches!(
            validate_tracker_address("tracker.example.com:7510"),
            Err(TrackerAddressError::Invalid(_))
        ));
    }
}
