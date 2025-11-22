//! Yggdrasil network utilities

use ipnet::Ipv6Net;
use once_cell::sync::Lazy;
use std::net::Ipv6Addr;

/// The Yggdrasil IPv6 address range (0200::/7)
pub const YGGDRASIL_RANGE: &str = "0200::/7";

/// Parsed Yggdrasil IPv6 network range
static YGGDRASIL_NET: Lazy<Ipv6Net> = Lazy::new(|| {
    YGGDRASIL_RANGE
        .parse()
        .expect("YGGDRASIL_RANGE constant should be a valid IPv6 network")
});

/// Check if an IPv6 address is in the Yggdrasil range (0200::/7)
pub fn is_yggdrasil_address(addr: &Ipv6Addr) -> bool {
    YGGDRASIL_NET.contains(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yggdrasil_address_validation() {
        // Valid Yggdrasil addresses
        assert!(is_yggdrasil_address(&"0200::1".parse().unwrap()));
        assert!(is_yggdrasil_address(&"0201:1234:5678::abcd".parse().unwrap()));
        assert!(is_yggdrasil_address(&"0300::1".parse().unwrap()));
        assert!(is_yggdrasil_address(&"03ff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()));

        // Invalid addresses (not in Yggdrasil range)
        assert!(!is_yggdrasil_address(&"::1".parse().unwrap()));
        assert!(!is_yggdrasil_address(&"2001:db8::1".parse().unwrap()));
        assert!(!is_yggdrasil_address(&"fe80::1".parse().unwrap()));
        assert!(!is_yggdrasil_address(&"0100::1".parse().unwrap()));
        assert!(!is_yggdrasil_address(&"0400::1".parse().unwrap()));
    }
}
