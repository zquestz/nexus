//! Connection-target address validation.
//!
//! Validates an address the user types into the connect / bookmark
//! form. Distinct from [`super::public_address`] because connection
//! targets legitimately include forms that "publicly advertised"
//! addresses shouldn't:
//!
//! - **IPv6 literal brackets** (`[::1]`) — common admin notation,
//!   stripped here and the inner re-parsed as IPv6.
//! - **IPv6 zone identifiers** (`fe80::1%eth0`) — required for
//!   link-local mesh peers; the host portion before `%` is verified as
//!   IPv6 but the zone is passed through to the resolver as-is.
//!
//! Still rejects the same typo cases (URL scheme, path, userinfo,
//! whitespace, embedded port in non-IPv6 context, malformed
//! hostname / IP literal).

use std::net::{Ipv4Addr, Ipv6Addr};

use super::public_address::MAX_PUBLIC_ADDRESS_LENGTH;

/// Validation error for connection-target addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionAddressError {
    /// Empty / whitespace-only input.
    Empty,
    /// Exceeds [`MAX_PUBLIC_ADDRESS_LENGTH`].
    TooLong,
    /// Contains `://` — admin pasted a URL.
    ContainsScheme,
    /// Contains `/` — path component in a host field.
    ContainsPath,
    /// Contains `@` — userinfo in a host field.
    ContainsUserinfo,
    /// Contains whitespace.
    ContainsWhitespace,
    /// Has a colon-separated port suffix in a non-IPv6 context.
    /// Suggests the user typed `host:port`; ports go in the separate
    /// port input.
    ContainsPort,
    /// Doesn't parse as IPv4, IPv6, or a valid hostname (post-IDNA).
    InvalidFormat,
}

/// Validate a connection-target address.
///
/// Accepts IP literals (`127.0.0.1`, `::1`, `[::1]`), zoned IPv6
/// (`fe80::1%eth0`), Unicode / ASCII / Punycode hostnames, and
/// `localhost`. Rejects URL fragments, embedded ports, whitespace, and
/// malformed input.
///
/// # Errors
///
/// Returns a [`ConnectionAddressError`] variant describing the
/// validation failure.
pub fn validate_connection_address(addr: &str) -> Result<(), ConnectionAddressError> {
    if addr.trim().is_empty() {
        return Err(ConnectionAddressError::Empty);
    }
    if addr.len() > MAX_PUBLIC_ADDRESS_LENGTH {
        return Err(ConnectionAddressError::TooLong);
    }
    if addr.contains("://") {
        return Err(ConnectionAddressError::ContainsScheme);
    }
    if addr.contains('@') {
        return Err(ConnectionAddressError::ContainsUserinfo);
    }
    if addr.chars().any(char::is_whitespace) {
        return Err(ConnectionAddressError::ContainsWhitespace);
    }

    // Strip outer brackets to support `[::1]` IPv6 literal form.
    // Brackets are only valid as a single matched outer pair.
    let bare = addr
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(addr);

    if bare.is_empty() || bare.contains('[') || bare.contains(']') {
        return Err(ConnectionAddressError::InvalidFormat);
    }

    if bare.contains('/') {
        return Err(ConnectionAddressError::ContainsPath);
    }

    // IPv6 with optional zone identifier. `fe80::1%eth0` →
    // host = `fe80::1`, zone = `eth0`. The zone passes through to
    // the resolver as-is.
    if bare.contains(':') {
        let (host, zone) = match bare.split_once('%') {
            Some((h, z)) => (h, Some(z)),
            None => (bare, None),
        };
        if let Some(z) = zone
            && z.is_empty()
        {
            return Err(ConnectionAddressError::InvalidFormat);
        }
        return match host.parse::<Ipv6Addr>() {
            Ok(_) => Ok(()),
            // `:` present, not IPv6 → must be `host:port`.
            Err(_) => Err(ConnectionAddressError::ContainsPort),
        };
    }

    // No `:` — `%` (zone identifier) is only meaningful inside IPv6.
    if bare.contains('%') {
        return Err(ConnectionAddressError::InvalidFormat);
    }

    if bare.parse::<Ipv4Addr>().is_ok() {
        return Ok(());
    }

    // Hostname path. Reject trailing dots for the same reason
    // public_address does — DNS-syntax FQDN trailing dots aren't part
    // of the user-typed value anyone needs to round-trip.
    if bare.ends_with('.') {
        return Err(ConnectionAddressError::InvalidFormat);
    }
    let ascii =
        idna::domain_to_ascii_strict(bare).map_err(|_| ConnectionAddressError::InvalidFormat)?;
    if ascii.len() > MAX_PUBLIC_ADDRESS_LENGTH {
        return Err(ConnectionAddressError::TooLong);
    }
    if ascii
        .split('.')
        .any(|label| label.is_empty() || label.len() > 63)
    {
        return Err(ConnectionAddressError::InvalidFormat);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(
            validate_connection_address(""),
            Err(ConnectionAddressError::Empty)
        );
        assert_eq!(
            validate_connection_address("   "),
            Err(ConnectionAddressError::Empty)
        );
    }

    #[test]
    fn accepts_ipv4() {
        assert!(validate_connection_address("127.0.0.1").is_ok());
        assert!(validate_connection_address("192.168.1.1").is_ok());
        assert!(validate_connection_address("8.8.8.8").is_ok());
    }

    #[test]
    fn accepts_bare_ipv6() {
        assert!(validate_connection_address("::1").is_ok());
        assert!(validate_connection_address("2001:db8::1").is_ok());
    }

    #[test]
    fn accepts_bracketed_ipv6() {
        // The whole point of path 1 — `[::1]` is a legitimate IPv6
        // literal form that `validate_public_address` rejects.
        assert!(validate_connection_address("[::1]").is_ok());
        assert!(validate_connection_address("[2001:db8::1]").is_ok());
        assert!(validate_connection_address("[fe80::1]").is_ok());
    }

    #[test]
    fn accepts_zoned_ipv6() {
        // Link-local mesh peers need zone IDs.
        assert!(validate_connection_address("fe80::1%eth0").is_ok());
        assert!(validate_connection_address("fe80::1%en0").is_ok());
    }

    #[test]
    fn rejects_zone_id_without_ipv6() {
        // `%` without IPv6 host doesn't make sense.
        assert_eq!(
            validate_connection_address("example.com%eth0"),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }

    #[test]
    fn rejects_empty_zone_id() {
        assert_eq!(
            validate_connection_address("fe80::1%"),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }

    #[test]
    fn accepts_hostnames() {
        assert!(validate_connection_address("example.com").is_ok());
        assert!(validate_connection_address("nas.local").is_ok());
        // Single-label hostnames (LAN names).
        assert!(validate_connection_address("localhost").is_ok());
        assert!(validate_connection_address("router").is_ok());
        // IDN.
        assert!(validate_connection_address("例え.jp").is_ok());
        assert!(validate_connection_address("xn--r8jz45g.jp").is_ok());
    }

    #[test]
    fn rejects_url_scheme() {
        assert_eq!(
            validate_connection_address("https://example.com"),
            Err(ConnectionAddressError::ContainsScheme)
        );
        assert_eq!(
            validate_connection_address("nexus://example.com"),
            Err(ConnectionAddressError::ContainsScheme)
        );
    }

    #[test]
    fn rejects_path() {
        assert_eq!(
            validate_connection_address("example.com/path"),
            Err(ConnectionAddressError::ContainsPath)
        );
    }

    #[test]
    fn rejects_userinfo() {
        assert_eq!(
            validate_connection_address("user@example.com"),
            Err(ConnectionAddressError::ContainsUserinfo)
        );
    }

    #[test]
    fn rejects_whitespace() {
        assert_eq!(
            validate_connection_address("host name"),
            Err(ConnectionAddressError::ContainsWhitespace)
        );
        assert_eq!(
            validate_connection_address("host\tname"),
            Err(ConnectionAddressError::ContainsWhitespace)
        );
    }

    #[test]
    fn rejects_embedded_port() {
        assert_eq!(
            validate_connection_address("example.com:7500"),
            Err(ConnectionAddressError::ContainsPort)
        );
        // Bracketed IPv6 with port suffix.
        assert_eq!(
            validate_connection_address("[::1]:7500"),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }

    #[test]
    fn rejects_too_long() {
        let long_host = "a".repeat(MAX_PUBLIC_ADDRESS_LENGTH + 1);
        assert_eq!(
            validate_connection_address(&long_host),
            Err(ConnectionAddressError::TooLong)
        );
    }

    #[test]
    fn rejects_trailing_dot() {
        assert_eq!(
            validate_connection_address("example.com."),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }

    #[test]
    fn rejects_inner_brackets() {
        // Only outer matched bracket pair allowed.
        assert_eq!(
            validate_connection_address("[host[inner]]"),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }

    #[test]
    fn rejects_empty_brackets() {
        assert_eq!(
            validate_connection_address("[]"),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }

    #[test]
    fn rejects_invalid_hostname() {
        // `_` is invalid in a strict-IDNA hostname label.
        assert_eq!(
            validate_connection_address("under_score"),
            Err(ConnectionAddressError::InvalidFormat)
        );
    }
}
