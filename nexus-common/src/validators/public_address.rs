//! Public address validation
//!
//! Validates the public_address field used to advertise a server's
//! shareable hostname or IP. Bare form only: no scheme, no brackets,
//! no port, no path, no userinfo. Accepts internationalized domain
//! names (IDN) in Unicode or Punycode form; storage preserves the
//! admin's input as-typed.

use std::net::{Ipv4Addr, Ipv6Addr};

/// Maximum length for a public address (RFC 1035 DNS name limit)
pub const MAX_PUBLIC_ADDRESS_LENGTH: usize = 253;

/// Validation error for public addresses
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicAddressError {
    /// Exceeds maximum length
    TooLong,
    /// Includes a URL scheme like `nexus://` or `http://`
    ContainsScheme,
    /// Includes square brackets (IPv6 bracketing is added at URI build time)
    ContainsBrackets,
    /// Includes a path component (slash)
    ContainsPath,
    /// Includes an `@` userinfo separator
    ContainsUserinfo,
    /// Includes whitespace
    ContainsWhitespace,
    /// Has a port embedded (e.g. `example.com:7500`)
    ContainsPort,
    /// Has an IPv6 zone identifier (e.g. `fe80::1%eth0`)
    ContainsZoneId,
    /// Not a valid hostname, IPv4, or IPv6 address
    InvalidFormat,
}

/// Validate a public address string.
///
/// Accepts internationalized domain names (IDN) in Unicode or Punycode form
/// (validated per IDNA 2008), IPv4 literals, and bare IPv6 literals (no
/// brackets). Empty input is treated as "unset" and returns `Ok(())`.
///
/// Use this when you only need a yes/no validation answer — input is
/// preserved as-typed at the call site. If you also need the DNS-wire
/// (Punycode) / canonical-IP form for downstream use (e.g. handing to
/// a system resolver), call [`validate_and_normalize_public_address`]
/// instead so the IDN encoding happens once.
///
/// # Errors
///
/// Returns a `PublicAddressError` variant describing the validation failure.
pub fn validate_public_address(addr: &str) -> Result<(), PublicAddressError> {
    validate_and_normalize_public_address(addr).map(|_| ())
}

/// Validate a public address string and return its normalized form.
///
/// On success returns:
/// - `""` for empty input (consistent with [`validate_public_address`]'s
///   "unset" semantics).
/// - For IPv4 / IPv6 literals: the canonical string form via
///   `Ipv4Addr::to_string` / `Ipv6Addr::to_string` — e.g.,
///   `"2001:DB8::1"` becomes `"2001:db8::1"` and `"127.000.000.001"`-
///   style malformed inputs are already rejected by the parser.
/// - For Unicode / Punycode / ASCII hostnames: the Punycode ASCII form
///   produced by `idna::domain_to_ascii_strict`, suitable for handing
///   to a system resolver verbatim.
///
/// Callers that store the user-typed form (preserving Unicode IDN for
/// display) should keep their own copy of the input — this function's
/// return value is the *normalized* form, not necessarily what the
/// user typed. Use this when you need both the validation answer
/// *and* the normalized form, to avoid double-encoding via `idna`.
///
/// # Errors
///
/// Returns a `PublicAddressError` variant describing the validation failure.
pub fn validate_and_normalize_public_address(addr: &str) -> Result<String, PublicAddressError> {
    if addr.is_empty() {
        return Ok(String::new());
    }
    if addr.len() > MAX_PUBLIC_ADDRESS_LENGTH {
        return Err(PublicAddressError::TooLong);
    }
    if addr.contains("://") {
        return Err(PublicAddressError::ContainsScheme);
    }
    if addr.contains('[') || addr.contains(']') {
        return Err(PublicAddressError::ContainsBrackets);
    }
    if addr.contains('/') {
        return Err(PublicAddressError::ContainsPath);
    }
    if addr.contains('@') {
        return Err(PublicAddressError::ContainsUserinfo);
    }
    if addr.chars().any(char::is_whitespace) {
        return Err(PublicAddressError::ContainsWhitespace);
    }

    if addr.contains(':') {
        // IPv6 zone identifiers (e.g. `fe80::1%eth0`) are link-local scopes
        // that don't make sense to advertise publicly; reject with a specific
        // error rather than misreporting as `ContainsPort`.
        if addr.contains('%') {
            return Err(PublicAddressError::ContainsZoneId);
        }
        return match addr.parse::<Ipv6Addr>() {
            Ok(ip) => Ok(ip.to_string()),
            Err(_) => Err(PublicAddressError::ContainsPort),
        };
    }

    if let Ok(ip) = addr.parse::<Ipv4Addr>() {
        return Ok(ip.to_string());
    }

    // IDNA: accepts ASCII hostnames, Unicode IDN, and pre-encoded Punycode.
    // A trailing dot (`example.com.`) is rejected for consistency with the
    // "bare form only" rule — leave DNS syntax out of our advertised value.
    if addr.ends_with('.') {
        return Err(PublicAddressError::InvalidFormat);
    }
    let ascii =
        idna::domain_to_ascii_strict(addr).map_err(|_| PublicAddressError::InvalidFormat)?;
    // DNS wire-form caps: 253-octet total, 63-octet per label. `idna` enforces
    // IDNA 2008 syntax but does not apply these length caps on its own.
    if ascii.len() > MAX_PUBLIC_ADDRESS_LENGTH {
        return Err(PublicAddressError::TooLong);
    }
    if ascii
        .split('.')
        .any(|label| label.is_empty() || label.len() > 63)
    {
        return Err(PublicAddressError::InvalidFormat);
    }
    Ok(ascii)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_is_ok() {
        assert!(validate_public_address("").is_ok());
    }

    #[test]
    fn test_valid_hostnames() {
        assert!(validate_public_address("example.com").is_ok());
        assert!(validate_public_address("bbs.example.com").is_ok());
        assert!(validate_public_address("a.b.c.d.e").is_ok());
        assert!(validate_public_address("server.duckdns.org").is_ok());
        assert!(validate_public_address("my-bbs.example.com").is_ok());
        assert!(validate_public_address("localhost").is_ok());
        assert!(validate_public_address("mybbs.local").is_ok());
        assert!(validate_public_address("single").is_ok());
    }

    #[test]
    fn test_valid_ipv4() {
        assert!(validate_public_address("127.0.0.1").is_ok());
        assert!(validate_public_address("192.168.1.1").is_ok());
        assert!(validate_public_address("203.0.113.5").is_ok());
    }

    #[test]
    fn test_valid_ipv6() {
        assert!(validate_public_address("::1").is_ok());
        assert!(validate_public_address("2001:db8::1").is_ok());
        assert!(validate_public_address("0200:1234:5678:9abc::1").is_ok());
        assert!(validate_public_address("fe80::1").is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_public_address(&"a".repeat(MAX_PUBLIC_ADDRESS_LENGTH + 1)),
            Err(PublicAddressError::TooLong)
        );
    }

    #[test]
    fn test_rejects_scheme() {
        assert_eq!(
            validate_public_address("nexus://example.com"),
            Err(PublicAddressError::ContainsScheme)
        );
        assert_eq!(
            validate_public_address("http://example.com"),
            Err(PublicAddressError::ContainsScheme)
        );
        assert_eq!(
            validate_public_address("https://example.com"),
            Err(PublicAddressError::ContainsScheme)
        );
    }

    #[test]
    fn test_rejects_brackets() {
        assert_eq!(
            validate_public_address("[::1]"),
            Err(PublicAddressError::ContainsBrackets)
        );
        assert_eq!(
            validate_public_address("[2001:db8::1]"),
            Err(PublicAddressError::ContainsBrackets)
        );
    }

    #[test]
    fn test_rejects_path() {
        assert_eq!(
            validate_public_address("example.com/chat"),
            Err(PublicAddressError::ContainsPath)
        );
        assert_eq!(
            validate_public_address("example.com/"),
            Err(PublicAddressError::ContainsPath)
        );
    }

    #[test]
    fn test_rejects_userinfo() {
        assert_eq!(
            validate_public_address("user@example.com"),
            Err(PublicAddressError::ContainsUserinfo)
        );
    }

    #[test]
    fn test_rejects_whitespace() {
        assert_eq!(
            validate_public_address("example.com "),
            Err(PublicAddressError::ContainsWhitespace)
        );
        assert_eq!(
            validate_public_address("example .com"),
            Err(PublicAddressError::ContainsWhitespace)
        );
        assert_eq!(
            validate_public_address("example.com\n"),
            Err(PublicAddressError::ContainsWhitespace)
        );
    }

    #[test]
    fn test_rejects_embedded_port() {
        assert_eq!(
            validate_public_address("example.com:7500"),
            Err(PublicAddressError::ContainsPort)
        );
        assert_eq!(
            validate_public_address("127.0.0.1:7500"),
            Err(PublicAddressError::ContainsPort)
        );
    }

    #[test]
    fn test_rejects_ipv6_zone_id() {
        assert_eq!(
            validate_public_address("fe80::1%eth0"),
            Err(PublicAddressError::ContainsZoneId)
        );
        assert_eq!(
            validate_public_address("fe80::1%25eth0"),
            Err(PublicAddressError::ContainsZoneId)
        );
    }

    #[test]
    fn test_rejects_invalid_hostnames() {
        assert_eq!(
            validate_public_address("-leadinghyphen.com"),
            Err(PublicAddressError::InvalidFormat)
        );
        assert_eq!(
            validate_public_address("trailing-.com"),
            Err(PublicAddressError::InvalidFormat)
        );
        assert_eq!(
            validate_public_address("example.com."),
            Err(PublicAddressError::InvalidFormat)
        );
        assert_eq!(
            validate_public_address(".example.com"),
            Err(PublicAddressError::InvalidFormat)
        );
        assert_eq!(
            validate_public_address(&format!("{}.com", "a".repeat(64))),
            Err(PublicAddressError::InvalidFormat)
        );
    }

    #[test]
    fn test_valid_unicode_idn() {
        assert!(validate_public_address("münchen.de").is_ok());
        assert!(validate_public_address("日本.jp").is_ok());
        assert!(validate_public_address("例え.jp").is_ok());
        assert!(validate_public_address("server.münchen.de").is_ok());
        assert!(validate_public_address("пример.рф").is_ok());
    }

    #[test]
    fn test_valid_punycode() {
        assert!(validate_public_address("xn--mnchen-3ya.de").is_ok());
        assert!(validate_public_address("xn--wgv71a.jp").is_ok());
    }

    #[test]
    fn test_rejects_malformed_idn() {
        // Punycode with invalid payload
        assert_eq!(
            validate_public_address("xn--"),
            Err(PublicAddressError::InvalidFormat)
        );
    }

    // ---- validate_and_normalize_public_address ----

    #[test]
    fn test_normalize_empty_returns_empty() {
        assert_eq!(validate_and_normalize_public_address(""), Ok(String::new()));
    }

    #[test]
    fn test_normalize_ipv4_returns_canonical() {
        // Already canonical — round-trips unchanged.
        assert_eq!(
            validate_and_normalize_public_address("127.0.0.1"),
            Ok("127.0.0.1".to_string())
        );
        assert_eq!(
            validate_and_normalize_public_address("8.8.8.8"),
            Ok("8.8.8.8".to_string())
        );
    }

    #[test]
    fn test_normalize_ipv6_lowercases() {
        // IPv6 canonical form is lowercase; a mixed-case input parses
        // and the canonical form returned is lowercase.
        assert_eq!(
            validate_and_normalize_public_address("2001:DB8::1"),
            Ok("2001:db8::1".to_string())
        );
        // Already canonical — round-trips unchanged.
        assert_eq!(
            validate_and_normalize_public_address("::1"),
            Ok("::1".to_string())
        );
    }

    #[test]
    fn test_normalize_unicode_idn_returns_punycode() {
        assert_eq!(
            validate_and_normalize_public_address("münchen.de"),
            Ok("xn--mnchen-3ya.de".to_string())
        );
        assert_eq!(
            validate_and_normalize_public_address("日本.jp"),
            Ok("xn--wgv71a.jp".to_string())
        );
    }

    #[test]
    fn test_normalize_punycode_passes_through() {
        // Pre-encoded Punycode is already ASCII and round-trips.
        assert_eq!(
            validate_and_normalize_public_address("xn--mnchen-3ya.de"),
            Ok("xn--mnchen-3ya.de".to_string())
        );
    }

    #[test]
    fn test_normalize_ascii_hostname_lowercases() {
        // `idna::domain_to_ascii_strict` lowercases ASCII hostnames as
        // a side effect of the IDNA 2008 mapping.
        assert_eq!(
            validate_and_normalize_public_address("Example.COM"),
            Ok("example.com".to_string())
        );
    }

    #[test]
    fn test_normalize_propagates_validation_errors() {
        // Same rejection set as `validate_public_address` — the
        // wrapping function calls into this one, but verify directly.
        assert_eq!(
            validate_and_normalize_public_address("https://example.com"),
            Err(PublicAddressError::ContainsScheme)
        );
        assert_eq!(
            validate_and_normalize_public_address("[::1]"),
            Err(PublicAddressError::ContainsBrackets)
        );
        assert_eq!(
            validate_and_normalize_public_address("example.com:7500"),
            Err(PublicAddressError::ContainsPort)
        );
    }
}
