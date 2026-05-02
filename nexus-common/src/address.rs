//! IP address classification.
//!
//! Workspace-shared predicates for "is this IP a private/loopback
//! address?", "is this IP from a range that should bypass SOCKS5?",
//! and "is this IP one that's never a valid public unicast endpoint?".
//!
//! The project-specific ranges (RFC 1918, IPv6 ULA, Yggdrasil mesh,
//! IPv6 documentation) are exposed as `ipnet`-typed constants for
//! readability. Categories that `std` already covers
//! (`Ipv4Addr::is_loopback`, `is_link_local`, `is_documentation`,
//! `is_multicast`, and the IPv6 equivalents) are used directly via
//! `std`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::{Ipv4Net, Ipv6Net};

// =============================================================================
// Range constants
// =============================================================================

/// IPv4 RFC 1918 private range `10.0.0.0/8`.
pub const RFC1918_10: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(10, 0, 0, 0), 8);

/// IPv4 RFC 1918 private range `172.16.0.0/12`.
pub const RFC1918_172: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(172, 16, 0, 0), 12);

/// IPv4 RFC 1918 private range `192.168.0.0/16`.
pub const RFC1918_192: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(192, 168, 0, 0), 16);

/// IPv6 Unique Local Addresses (`fc00::/7`).
pub const ULA: Ipv6Net = Ipv6Net::new_assert(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7);

/// Yggdrasil mesh network (`0200::/7`).
pub const YGGDRASIL: Ipv6Net = Ipv6Net::new_assert(Ipv6Addr::new(0x0200, 0, 0, 0, 0, 0, 0, 0), 7);

/// IPv6 documentation range (`2001:db8::/32`, RFC 3849). `std` exposes
/// `Ipv4Addr::is_documentation` for the v4 ranges but has no stable
/// equivalent for v6, so we keep the range here.
pub const DOCUMENTATION_V6: Ipv6Net =
    Ipv6Net::new_assert(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 0), 32);

/// IPv4 "this network" range (`0.0.0.0/8`, RFC 1122 §3.2.1.3). The
/// `0.0.0.0` host inside the range is the unspecified address (covered
/// by `Ipv4Addr::is_unspecified`); the broader `/8` catches the rest
/// of the range, which is reserved for "this network, this host" /
/// "this network, specific host" use and is never a valid public
/// unicast endpoint.
pub const THIS_NETWORK: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(0, 0, 0, 0), 8);

// =============================================================================
// Primitive predicates (project-specific ranges that `std` doesn't cover)
// =============================================================================

/// Whether `ip` is in any of the IPv4 RFC 1918 private ranges.
#[must_use]
pub fn is_ipv4_rfc1918(ip: Ipv4Addr) -> bool {
    RFC1918_10.contains(&ip) || RFC1918_172.contains(&ip) || RFC1918_192.contains(&ip)
}

/// Whether `ip` is in the IPv6 ULA range (`fc00::/7`).
#[must_use]
pub fn is_ipv6_ula(ip: Ipv6Addr) -> bool {
    ULA.contains(&ip)
}

/// Whether `ip` is in the Yggdrasil mesh range (`0200::/7`).
#[must_use]
pub fn is_ipv6_yggdrasil(ip: Ipv6Addr) -> bool {
    YGGDRASIL.contains(&ip)
}

/// Whether `ip` is in the IPv6 documentation range (`2001:db8::/32`).
#[must_use]
pub fn is_ipv6_documentation(ip: Ipv6Addr) -> bool {
    DOCUMENTATION_V6.contains(&ip)
}

// =============================================================================
// Composed predicates
// =============================================================================

/// Whether `ip` is on a "non-public" network — RFC 1918 IPv4, IPv6
/// ULA, or IPv4/IPv6 loopback. Used as the tracker-registration
/// bypass: an operator on a private network can't reasonably claim a
/// public address that matches their peer IP, so the address-vs-peer
/// check is skipped when the peer is in this set.
///
/// Excludes Yggdrasil mesh — those addresses behave like public
/// addresses *within* the mesh (operators can register hostnames
/// resolving to their Yggdrasil address; the lookup-vs-peer check
/// works naturally there).
///
/// Excludes CGNAT (`100.64.0.0/10`, RFC 6598) — those IPs are visible
/// only between an ISP carrier and its subscribers, but the carrier's
/// public egress pool is what the wider internet sees. A CGNAT-fronted
/// operator's working path is hostname registration (dynamic DNS); the
/// tracker treats their connection as a public peer so the
/// hostname-vs-peer match still applies. Granting CGNAT a LAN-style
/// bypass would let any carrier-side subscriber register arbitrary
/// (unverifiable) advertised addresses, defeating the bypass-set
/// guarantee that an unverified address only comes from a peer the
/// operator already implicitly trusts (their own LAN).
#[must_use]
pub fn is_private_network(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_ipv4_rfc1918(v4) || v4.is_loopback(),
        IpAddr::V6(v6) => is_ipv6_ula(v6) || v6.is_loopback(),
    }
}

/// Whether a connection to `ip` should bypass SOCKS5 — the
/// destination is on the local machine, the local LAN, or the
/// Yggdrasil mesh. None of those use the public-internet route a
/// SOCKS5 proxy expects, and forcing them through a proxy would
/// either fail or break Yggdrasil routing.
#[must_use]
pub fn is_proxy_bypassable(ip: IpAddr) -> bool {
    if let IpAddr::V6(v6) = ip
        && is_ipv6_yggdrasil(v6)
    {
        return true;
    }
    is_private_network(ip)
}

// =============================================================================
// Hard-reject classifier
// =============================================================================

/// Reasons an IP is never a valid public unicast endpoint. Each
/// variant corresponds to a category that's structurally invalid as a
/// public address regardless of the deployment context — distinct
/// variants exist so callers can emit specific telemetry on each
/// rejection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidAddressKind {
    /// `127.0.0.0/8` or `::1` — only addressable from the same host.
    Loopback,
    /// `0.0.0.0` / `::` (bind-address sentinel) or anywhere else in
    /// `0.0.0.0/8` ("this network", RFC 1122) — never a unicast endpoint.
    Unspecified,
    /// `169.254.0.0/16` or `fe80::/10` — link-local, not internet-routable.
    LinkLocal,
    /// `224.0.0.0/4` or `ff00::/8` — multicast, never a unicast endpoint.
    Multicast,
    /// `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`, or
    /// `2001:db8::/32` — reserved for documentation; never assigned
    /// to real hosts.
    Documentation,
    /// `255.255.255.255` — IPv4 limited broadcast, never a unicast endpoint.
    Broadcast,
}

/// Classify whether `ip` is in one of the "never a public unicast
/// endpoint" categories. Returns `None` if `ip` could legitimately be
/// a public endpoint.
///
/// Order: cross-family categories first (loopback, unspecified,
/// multicast), then per-family ranges. The IPv4 arm checks the limited
/// broadcast (`255.255.255.255`), the broader `0.0.0.0/8` "this
/// network" range (which routes to `Unspecified` since it's
/// morphologically the same family as the exact-`0.0.0.0` case),
/// link-local, and documentation. The IPv6 arm checks unicast
/// link-local and documentation.
#[must_use]
pub fn classify_invalid(ip: IpAddr) -> Option<InvalidAddressKind> {
    if ip.is_loopback() {
        return Some(InvalidAddressKind::Loopback);
    }
    if ip.is_unspecified() {
        return Some(InvalidAddressKind::Unspecified);
    }
    if ip.is_multicast() {
        return Some(InvalidAddressKind::Multicast);
    }
    match ip {
        IpAddr::V4(v4) => {
            // 255.255.255.255 limited broadcast.
            if v4.is_broadcast() {
                return Some(InvalidAddressKind::Broadcast);
            }
            // 0.0.0.0/8 ("this network", RFC 1122 §3.2.1.3). The
            // 0.0.0.0 host was already caught by `is_unspecified`
            // above; this catches the broader range (0.x.y.z).
            if THIS_NETWORK.contains(&v4) {
                return Some(InvalidAddressKind::Unspecified);
            }
            if v4.is_link_local() {
                return Some(InvalidAddressKind::LinkLocal);
            }
            if v4.is_documentation() {
                return Some(InvalidAddressKind::Documentation);
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_unicast_link_local() {
                return Some(InvalidAddressKind::LinkLocal);
            }
            if is_ipv6_documentation(v6) {
                return Some(InvalidAddressKind::Documentation);
            }
        }
    }
    None
}

// =============================================================================
// String normalization
// =============================================================================

/// Strip IPv6 decoration so the result can be fed to `IpAddr::parse`.
///
/// Strips a single matching pair of URL-style brackets (`[` … `]`)
/// like `[::1]`, and truncates at any `%` (IPv6 zone identifier like
/// `fe80::1%eth0`). Mismatched or unbracketed input is returned
/// untouched — `[::1` and `::1]` both pass through unchanged so a
/// downstream `IpAddr::parse` rejects them loudly. Calling this on a
/// hostname like `example.com` is a no-op.
///
/// Matches the bracket-stripping behavior of
/// [`resolve_host_for_connection`] — both functions take the same
/// "exactly one matching pair" approach so a caller picking either
/// gets identical normalization.
///
/// Intended use: `address::normalize_ip_literal(s).parse::<IpAddr>()`.
/// If the input was a hostname, the parse will fail and the caller can
/// fall through to a DNS lookup.
#[must_use]
pub fn normalize_ip_literal(address: &str) -> &str {
    let bare = address
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(address);
    bare.split('%').next().unwrap_or(bare)
}

/// Resolve an as-typed user/admin-supplied address into a form
/// suitable for `tokio::net::lookup_host` and rustls's
/// `ServerName::try_from`. Implements the project's "store as-typed,
/// normalize at connection time" rule for IDN handling:
///
/// 1. Strip URL-style IPv6 brackets (`[::1]` → `::1`).
/// 2. If the bare form parses as an IPv4 or IPv6 address, return it
///    unchanged — both resolvers and rustls accept IP literals.
/// 3. If the bare form contains a `%` (IPv6 zone identifier like
///    `fe80::1%eth0`), return it unchanged. The zone ID is meaningful
///    to the resolver but not to IDNA.
/// 4. Otherwise treat as a hostname and convert Unicode labels to
///    Punycode via `idna::domain_to_ascii`.
///
/// # Errors
///
/// Returns `idna::Errors` if step 4 fails (the input was a non-IP,
/// non-zoned string that wasn't a parseable hostname even under IDNA's
/// permissive rules — e.g. it contained a space or other invalid label
/// character).
pub fn resolve_host_for_connection(address: &str) -> Result<String, idna::Errors> {
    let bare = address
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(address);
    if bare.parse::<Ipv4Addr>().is_ok() || bare.parse::<Ipv6Addr>().is_ok() {
        return Ok(bare.to_string());
    }
    if bare.contains('%') {
        return Ok(bare.to_string());
    }
    idna::domain_to_ascii(bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> Ipv4Addr {
        s.parse().expect("valid v4")
    }
    fn v6(s: &str) -> Ipv6Addr {
        s.parse().expect("valid v6")
    }
    fn ip(s: &str) -> IpAddr {
        s.parse().expect("valid ip")
    }

    // ----- is_ipv4_rfc1918 -----

    #[test]
    fn rfc1918_includes_10_block() {
        assert!(is_ipv4_rfc1918(v4("10.0.0.0")));
        assert!(is_ipv4_rfc1918(v4("10.255.255.255")));
        assert!(is_ipv4_rfc1918(v4("10.42.42.42")));
    }

    #[test]
    fn rfc1918_includes_172_16_block_only() {
        // 172.16-172.31 are private; 172.0-172.15 and 172.32+ are not.
        assert!(is_ipv4_rfc1918(v4("172.16.0.0")));
        assert!(is_ipv4_rfc1918(v4("172.31.255.255")));
        assert!(is_ipv4_rfc1918(v4("172.20.0.1")));
        assert!(!is_ipv4_rfc1918(v4("172.15.255.255")));
        assert!(!is_ipv4_rfc1918(v4("172.32.0.0")));
    }

    #[test]
    fn rfc1918_includes_192_168_block_only() {
        assert!(is_ipv4_rfc1918(v4("192.168.0.0")));
        assert!(is_ipv4_rfc1918(v4("192.168.255.255")));
        assert!(is_ipv4_rfc1918(v4("192.168.1.100")));
        assert!(!is_ipv4_rfc1918(v4("192.167.255.255")));
        assert!(!is_ipv4_rfc1918(v4("192.169.0.0")));
    }

    #[test]
    fn rfc1918_excludes_public_ipv4() {
        assert!(!is_ipv4_rfc1918(v4("8.8.8.8")));
        assert!(!is_ipv4_rfc1918(v4("1.1.1.1")));
        assert!(!is_ipv4_rfc1918(v4("100.64.0.0"))); // CGNAT, not RFC 1918
    }

    // ----- is_ipv6_ula -----

    #[test]
    fn ula_covers_fc_and_fd_blocks() {
        // fc00::/7 covers fc00::/8 and fd00::/8 (the entire ULA range).
        assert!(is_ipv6_ula(v6("fc00::")));
        assert!(is_ipv6_ula(v6("fc00::1")));
        assert!(is_ipv6_ula(v6("fd00::1")));
        assert!(is_ipv6_ula(v6("fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff")));
    }

    #[test]
    fn ula_excludes_neighbors() {
        // fb is just below fc, fe is just above fd (link-local territory).
        assert!(!is_ipv6_ula(v6("fbff:ffff:ffff:ffff:ffff:ffff:ffff:ffff")));
        assert!(!is_ipv6_ula(v6("fe00::1")));
        assert!(!is_ipv6_ula(v6("::1")));
    }

    // ----- is_ipv6_yggdrasil -----

    #[test]
    fn yggdrasil_covers_0200_block() {
        // 0200::/7 covers 0200:: through 03ff::.
        assert!(is_ipv6_yggdrasil(v6("0200::")));
        assert!(is_ipv6_yggdrasil(v6("0200::1")));
        assert!(is_ipv6_yggdrasil(v6("0210:ffff::")));
        assert!(is_ipv6_yggdrasil(v6(
            "03ff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"
        )));
    }

    #[test]
    fn yggdrasil_excludes_neighbors() {
        assert!(!is_ipv6_yggdrasil(v6("0100::1")));
        assert!(!is_ipv6_yggdrasil(v6("0400::1")));
        assert!(!is_ipv6_yggdrasil(v6("::1")));
    }

    // ----- is_ipv6_documentation -----

    #[test]
    fn documentation_v6_covers_db8_prefix() {
        assert!(is_ipv6_documentation(v6("2001:db8::")));
        assert!(is_ipv6_documentation(v6("2001:db8::1")));
        assert!(is_ipv6_documentation(v6(
            "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff"
        )));
    }

    #[test]
    fn documentation_v6_excludes_neighbors() {
        assert!(!is_ipv6_documentation(v6("2001:db7:ffff::")));
        assert!(!is_ipv6_documentation(v6("2001:db9::")));
        assert!(!is_ipv6_documentation(v6("2002::")));
    }

    // ----- is_private_network -----

    #[test]
    fn private_network_includes_rfc1918_and_ula_and_loopback() {
        assert!(is_private_network(ip("10.0.0.1")));
        assert!(is_private_network(ip("172.16.5.5")));
        assert!(is_private_network(ip("192.168.1.1")));
        assert!(is_private_network(ip("127.0.0.1")));
        assert!(is_private_network(ip("fc00::1")));
        assert!(is_private_network(ip("fd00::1")));
        assert!(is_private_network(ip("::1")));
    }

    #[test]
    fn private_network_excludes_yggdrasil() {
        // Yggdrasil is "public-within-the-mesh" — not in the bypass set
        // for tracker registration.
        assert!(!is_private_network(ip("0200::1")));
    }

    #[test]
    fn private_network_excludes_cgnat() {
        // CGNAT (RFC 6598) is *not* in the bypass set by design — see
        // the doc comment on `is_private_network` for rationale.
        assert!(!is_private_network(ip("100.64.0.0")));
        assert!(!is_private_network(ip("100.64.0.1")));
        assert!(!is_private_network(ip("100.127.255.254")));
    }

    #[test]
    fn private_network_excludes_public() {
        assert!(!is_private_network(ip("8.8.8.8")));
        assert!(!is_private_network(ip("2606:4700:4700::1111")));
    }

    // ----- is_proxy_bypassable -----

    #[test]
    fn proxy_bypassable_is_private_network_plus_yggdrasil() {
        // Same set as is_private_network, plus Yggdrasil.
        assert!(is_proxy_bypassable(ip("10.0.0.1")));
        assert!(is_proxy_bypassable(ip("192.168.1.1")));
        assert!(is_proxy_bypassable(ip("127.0.0.1")));
        assert!(is_proxy_bypassable(ip("fc00::1")));
        assert!(is_proxy_bypassable(ip("::1")));
        assert!(is_proxy_bypassable(ip("0200::1")));
    }

    #[test]
    fn proxy_bypassable_excludes_public() {
        assert!(!is_proxy_bypassable(ip("8.8.8.8")));
        assert!(!is_proxy_bypassable(ip("2606:4700:4700::1111")));
    }

    // ----- classify_invalid -----

    #[test]
    fn classify_loopback() {
        assert_eq!(
            classify_invalid(ip("127.0.0.1")),
            Some(InvalidAddressKind::Loopback)
        );
        assert_eq!(
            classify_invalid(ip("127.255.255.255")),
            Some(InvalidAddressKind::Loopback)
        );
        assert_eq!(
            classify_invalid(ip("::1")),
            Some(InvalidAddressKind::Loopback)
        );
    }

    #[test]
    fn classify_unspecified() {
        assert_eq!(
            classify_invalid(ip("0.0.0.0")),
            Some(InvalidAddressKind::Unspecified)
        );
        assert_eq!(
            classify_invalid(ip("::")),
            Some(InvalidAddressKind::Unspecified)
        );
        // 0.0.0.0/8 ("this network", RFC 1122) excluding 0.0.0.0 itself.
        assert_eq!(
            classify_invalid(ip("0.1.2.3")),
            Some(InvalidAddressKind::Unspecified)
        );
        assert_eq!(
            classify_invalid(ip("0.255.255.254")),
            Some(InvalidAddressKind::Unspecified)
        );
        // 1.0.0.0 is just outside the /8 — must NOT classify.
        assert_eq!(classify_invalid(ip("1.0.0.0")), None);
    }

    #[test]
    fn classify_broadcast() {
        assert_eq!(
            classify_invalid(ip("255.255.255.255")),
            Some(InvalidAddressKind::Broadcast)
        );
        // Network broadcast (e.g. 192.168.1.255) is *not* limited
        // broadcast; it's a unicast IP from this predicate's view.
        // We don't try to detect network broadcasts because they're
        // network-mask-dependent.
        assert_eq!(classify_invalid(ip("192.168.1.255")), None);
    }

    #[test]
    fn classify_link_local() {
        assert_eq!(
            classify_invalid(ip("169.254.1.1")),
            Some(InvalidAddressKind::LinkLocal)
        );
        assert_eq!(
            classify_invalid(ip("fe80::1")),
            Some(InvalidAddressKind::LinkLocal)
        );
    }

    #[test]
    fn classify_multicast() {
        assert_eq!(
            classify_invalid(ip("224.0.0.1")),
            Some(InvalidAddressKind::Multicast)
        );
        assert_eq!(
            classify_invalid(ip("239.255.255.255")),
            Some(InvalidAddressKind::Multicast)
        );
        assert_eq!(
            classify_invalid(ip("ff00::1")),
            Some(InvalidAddressKind::Multicast)
        );
    }

    #[test]
    fn classify_documentation() {
        assert_eq!(
            classify_invalid(ip("192.0.2.1")),
            Some(InvalidAddressKind::Documentation)
        );
        assert_eq!(
            classify_invalid(ip("198.51.100.42")),
            Some(InvalidAddressKind::Documentation)
        );
        assert_eq!(
            classify_invalid(ip("203.0.113.7")),
            Some(InvalidAddressKind::Documentation)
        );
        assert_eq!(
            classify_invalid(ip("2001:db8::1")),
            Some(InvalidAddressKind::Documentation)
        );
    }

    #[test]
    fn classify_returns_none_for_legit_public() {
        // Real public unicast addresses pass.
        assert_eq!(classify_invalid(ip("8.8.8.8")), None);
        assert_eq!(classify_invalid(ip("1.1.1.1")), None);
        assert_eq!(classify_invalid(ip("2606:4700:4700::1111")), None);
    }

    #[test]
    fn classify_returns_none_for_rfc1918() {
        // RFC 1918 IPs *can* be valid public-from-the-LAN's-perspective
        // endpoints in a tracker-on-LAN deployment. Those are filtered
        // by `is_private_network`, not `classify_invalid`.
        assert_eq!(classify_invalid(ip("10.0.0.1")), None);
        assert_eq!(classify_invalid(ip("192.168.1.1")), None);
    }

    #[test]
    fn classify_returns_none_for_yggdrasil() {
        // Yggdrasil addresses are valid public-within-the-mesh endpoints.
        assert_eq!(classify_invalid(ip("0200::1")), None);
    }

    // ----- normalize_ip_literal -----

    #[test]
    fn normalize_strips_brackets() {
        assert_eq!(normalize_ip_literal("[::1]"), "::1");
        assert_eq!(normalize_ip_literal("[2001:db8::1]"), "2001:db8::1");
    }

    #[test]
    fn normalize_strips_zone_id() {
        assert_eq!(normalize_ip_literal("fe80::1%eth0"), "fe80::1");
        assert_eq!(normalize_ip_literal("[fe80::1%eth0]"), "fe80::1");
    }

    #[test]
    fn normalize_passthrough_for_undecorated() {
        assert_eq!(normalize_ip_literal("192.168.1.1"), "192.168.1.1");
        assert_eq!(normalize_ip_literal("2001:db8::1"), "2001:db8::1");
        // Hostnames pass through unchanged so the caller can fall
        // through to a DNS lookup.
        assert_eq!(normalize_ip_literal("example.com"), "example.com");
    }

    #[test]
    fn normalize_strips_only_one_matching_bracket_pair() {
        // Doubled brackets: only the outer pair is stripped — matching
        // `resolve_host_for_connection`'s behavior. Downstream
        // `IpAddr::parse` rejects the residual `[::1]` loudly.
        assert_eq!(normalize_ip_literal("[[::1]]"), "[::1]");
    }

    #[test]
    fn normalize_leaves_mismatched_brackets_untouched() {
        // Mismatched brackets are not URL-style decoration; pass them
        // through so downstream parse rejects them rather than
        // accepting a half-malformed input.
        assert_eq!(normalize_ip_literal("[::1"), "[::1");
        assert_eq!(normalize_ip_literal("::1]"), "::1]");
    }

    // ----- resolve_host_for_connection -----

    #[test]
    fn resolve_passes_ipv4_through() {
        assert_eq!(
            resolve_host_for_connection("192.168.1.1").expect("v4"),
            "192.168.1.1"
        );
    }

    #[test]
    fn resolve_passes_ipv6_through() {
        assert_eq!(resolve_host_for_connection("::1").expect("v6"), "::1");
        assert_eq!(
            resolve_host_for_connection("2001:db8::1").expect("v6"),
            "2001:db8::1"
        );
    }

    #[test]
    fn resolve_strips_ipv6_brackets() {
        assert_eq!(
            resolve_host_for_connection("[::1]").expect("bracketed v6"),
            "::1"
        );
        assert_eq!(
            resolve_host_for_connection("[2001:db8::1]").expect("bracketed v6"),
            "2001:db8::1"
        );
    }

    #[test]
    fn resolve_preserves_ipv6_zone_id() {
        assert_eq!(
            resolve_host_for_connection("fe80::1%eth0").expect("zoned v6"),
            "fe80::1%eth0"
        );
        assert_eq!(
            resolve_host_for_connection("[fe80::1%eth0]").expect("bracketed zoned v6"),
            "fe80::1%eth0"
        );
    }

    #[test]
    fn resolve_passes_ascii_hostname_through() {
        assert_eq!(
            resolve_host_for_connection("tracker.example.com").expect("hostname"),
            "tracker.example.com"
        );
    }

    #[test]
    fn resolve_punycodes_unicode_hostname() {
        // bücher.example → xn--bcher-kva.example
        let got = resolve_host_for_connection("bücher.example").expect("idn");
        assert_eq!(got, "xn--bcher-kva.example");
    }

    #[test]
    fn normalize_output_is_parseable() {
        // Round-trip: every form should parse to the same IpAddr.
        let cases = [
            ("[::1]", "::1"),
            ("[2001:db8::1]", "2001:db8::1"),
            ("fe80::1%eth0", "fe80::1"),
            ("192.168.1.1", "192.168.1.1"),
        ];
        for (decorated, bare) in cases {
            let normalized: IpAddr = normalize_ip_literal(decorated)
                .parse()
                .expect("normalized form must parse");
            let direct: IpAddr = bare.parse().expect("bare form must parse");
            assert_eq!(normalized, direct, "case: {decorated}");
        }
    }
}
