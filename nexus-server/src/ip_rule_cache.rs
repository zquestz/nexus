//! In-memory IP rule cache for fast pre-TLS checking, using radix tries
//! (`iprange`) for O(log n) containment over single IPs and CIDR ranges.
//!
//! IPv4-mapped IPv6 addresses (`::ffff:192.168.1.100`) normalize to IPv4 so
//! rules match regardless of how the OS presents incoming connections.
//!
//! Access control: trusted IPs are allowed (bypassing bans), otherwise allow
//! iff not banned.

use std::net::IpAddr;

use nexus_common::address::normalize_ip;
use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::IpRange;

use crate::constants::{
    ERR_IP_RULE_EXPIRY_MISSING, ERR_IPV4_PREFIX_FROM_MAPPED,
    ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK, ERR_TARGET_NOT_CANONICAL,
};
use crate::db::bans::BanRecord;
use crate::db::trusts::TrustRecord;

/// A cached rule entry (used for both bans and trusts)
#[derive(Debug, Clone)]
struct RuleEntry {
    /// IP or CIDR range as stored in DB
    ip_address: String,
    /// Parsed network (single IP becomes /32 or /128)
    net: IpNet,
    /// Unix expiry timestamp (None = permanent)
    expires_at: Option<i64>,
}

/// In-memory cache for IP access rules (trusts and bans). O(log n) lookups via
/// radix tries; expiry handled by lazy rebuild when `next_expiry` is reached.
#[derive(Debug)]
pub struct IpRuleCache {
    trust_ipv4: IpRange<Ipv4Net>,
    trust_ipv6: IpRange<Ipv6Net>,
    ban_ipv4: IpRange<Ipv4Net>,
    ban_ipv6: IpRange<Ipv6Net>,
    /// Source entries for trust rebuilds and removal
    trust_entries: Vec<RuleEntry>,
    /// Source entries for ban rebuilds and removal
    ban_entries: Vec<RuleEntry>,
    /// Earliest expiry across both trusts and bans (None if all permanent)
    next_expiry: Option<i64>,
}

impl IpRuleCache {
    pub fn new() -> Self {
        Self {
            trust_ipv4: IpRange::new(),
            trust_ipv6: IpRange::new(),
            ban_ipv4: IpRange::new(),
            ban_ipv6: IpRange::new(),
            trust_entries: Vec::new(),
            ban_entries: Vec::new(),
            next_expiry: None,
        }
    }

    /// Load cache from database records
    pub fn from_records(ban_records: Vec<BanRecord>, trust_records: Vec<TrustRecord>) -> Self {
        let mut cache = Self::new();

        for record in ban_records {
            if let Some(net) = parse_ip_or_cidr(&record.ip_address) {
                cache.ban_entries.push(RuleEntry {
                    ip_address: record.ip_address,
                    net,
                    expires_at: record.expires_at,
                });
            }
        }

        for record in trust_records {
            if let Some(net) = parse_ip_or_cidr(&record.ip_address) {
                cache.trust_entries.push(RuleEntry {
                    ip_address: record.ip_address,
                    net,
                    expires_at: record.expires_at,
                });
            }
        }

        cache.rebuild_tries();
        cache
    }

    /// Allow iff trusted (bypasses ban check) or not banned. Lazily rebuilds
    /// on expiry first.
    pub fn should_allow(&mut self, ip: IpAddr) -> bool {
        self.maybe_rebuild_on_expiry();
        self.should_allow_read_only(ip)
    }

    /// Allow iff trusted (bypasses ban check) or not banned. Does not rebuild
    /// on expiry — caller must check `needs_rebuild()`.
    pub fn should_allow_read_only(&self, ip: IpAddr) -> bool {
        if self.is_trusted_read_only(ip) {
            return true;
        }
        !self.is_banned_read_only(ip)
    }

    /// True if `ip` matches a non-expired trust. Lazily rebuilds on expiry.
    #[cfg(test)]
    pub fn is_trusted(&mut self, ip: IpAddr) -> bool {
        self.maybe_rebuild_on_expiry();
        self.is_trusted_read_only(ip)
    }

    /// True if `ip` matches a non-expired trust. Does not rebuild on expiry.
    ///
    /// Re-folds IPv4-mapped IPv6 to IPv4 as defense-in-depth; the accept path
    /// (`normalize_socket_addr` in `main.rs`) is the primary funnel.
    pub fn is_trusted_read_only(&self, ip: IpAddr) -> bool {
        let ip = normalize_ip(ip);

        match ip {
            IpAddr::V4(v4) => self.trust_ipv4.contains(&v4),
            IpAddr::V6(v6) => self.trust_ipv6.contains(&v6),
        }
    }

    /// True if `ip` matches a non-expired ban. Lazily rebuilds on expiry.
    #[cfg(test)]
    pub fn is_banned(&mut self, ip: IpAddr) -> bool {
        self.maybe_rebuild_on_expiry();
        self.is_banned_read_only(ip)
    }

    /// True if `ip` matches a non-expired ban. Does not rebuild on expiry.
    ///
    /// Re-folds IPv4-mapped IPv6 to IPv4 as defense-in-depth; the accept path
    /// (`normalize_socket_addr` in `main.rs`) is the primary funnel.
    pub fn is_banned_read_only(&self, ip: IpAddr) -> bool {
        let ip = normalize_ip(ip);

        match ip {
            IpAddr::V4(v4) => self.ban_ipv4.contains(&v4),
            IpAddr::V6(v6) => self.ban_ipv6.contains(&v6),
        }
    }

    /// Read-only check for expired entries; lets callers acquire a write lock
    /// before `rebuild_if_needed()`.
    pub fn needs_rebuild(&self) -> bool {
        if let Some(expiry) = self.next_expiry {
            current_timestamp() >= expiry
        } else {
            false
        }
    }

    fn maybe_rebuild_on_expiry(&mut self) {
        if self.needs_rebuild() {
            self.rebuild_tries();
        }
    }

    /// Upsert a trust entry (replacing any exact-match). False if unparseable.
    pub fn add_trust(&mut self, ip_or_cidr: &str, expires_at: Option<i64>) -> bool {
        assert_canonical_target(ip_or_cidr);
        let Some(net) = parse_ip_or_cidr(ip_or_cidr) else {
            return false;
        };

        self.trust_entries.retain(|e| e.ip_address != ip_or_cidr);

        self.trust_entries.push(RuleEntry {
            ip_address: ip_or_cidr.to_string(),
            net,
            expires_at,
        });

        self.rebuild_tries();
        true
    }

    /// Remove a trust by exact IP/CIDR match. True if an entry was removed.
    pub fn remove_trust(&mut self, ip_or_cidr: &str) -> bool {
        let before = self.trust_entries.len();
        self.trust_entries.retain(|e| e.ip_address != ip_or_cidr);
        let removed = self.trust_entries.len() < before;

        if removed {
            self.rebuild_tries();
        }

        removed
    }

    /// Remove all trusts contained by `cidr` (e.g. single IPs inside an
    /// untrusted range). Returns the removed IP/CIDR strings.
    pub fn remove_trusts_contained_by(&mut self, cidr: &str) -> Vec<String> {
        let Some(range_net) = parse_ip_or_cidr(cidr) else {
            return Vec::new();
        };

        let mut removed = Vec::new();

        self.trust_entries.retain(|entry| {
            let is_contained = is_contained_by(&entry.net, &range_net);

            if is_contained {
                removed.push(entry.ip_address.clone());
                false
            } else {
                true
            }
        });

        if !removed.is_empty() {
            self.rebuild_tries();
        }

        removed
    }

    /// Upsert a ban (replacing any exact-match). False if unparseable.
    pub fn add_ban(&mut self, ip_or_cidr: &str, expires_at: Option<i64>) -> bool {
        assert_canonical_target(ip_or_cidr);
        let Some(net) = parse_ip_or_cidr(ip_or_cidr) else {
            return false;
        };

        self.ban_entries.retain(|e| e.ip_address != ip_or_cidr);

        self.ban_entries.push(RuleEntry {
            ip_address: ip_or_cidr.to_string(),
            net,
            expires_at,
        });

        self.rebuild_tries();
        true
    }

    /// Remove a ban by exact IP/CIDR match. True if an entry was removed.
    pub fn remove_ban(&mut self, ip_or_cidr: &str) -> bool {
        let before = self.ban_entries.len();
        self.ban_entries.retain(|e| e.ip_address != ip_or_cidr);
        let removed = self.ban_entries.len() < before;

        if removed {
            self.rebuild_tries();
        }

        removed
    }

    /// Remove all bans contained by `cidr` (e.g. single IPs inside an unbanned
    /// range). Returns the removed IP/CIDR strings.
    pub fn remove_bans_contained_by(&mut self, cidr: &str) -> Vec<String> {
        let Some(range_net) = parse_ip_or_cidr(cidr) else {
            return Vec::new();
        };

        let mut removed = Vec::new();

        self.ban_entries.retain(|entry| {
            let is_contained = is_contained_by(&entry.net, &range_net);

            if is_contained {
                removed.push(entry.ip_address.clone());
                false
            } else {
                true
            }
        });

        if !removed.is_empty() {
            self.rebuild_tries();
        }

        removed
    }

    /// Rebuild all radix tries from entries, dropping expired rules and
    /// recomputing `next_expiry`.
    fn rebuild_tries(&mut self) {
        let now = current_timestamp();

        self.trust_entries.retain(|e| {
            e.expires_at.is_none() || e.expires_at.expect(ERR_IP_RULE_EXPIRY_MISSING) > now
        });
        self.ban_entries.retain(|e| {
            e.expires_at.is_none() || e.expires_at.expect(ERR_IP_RULE_EXPIRY_MISSING) > now
        });

        self.trust_ipv4 = IpRange::new();
        self.trust_ipv6 = IpRange::new();

        for entry in &self.trust_entries {
            match entry.net {
                IpNet::V4(net) => {
                    self.trust_ipv4.add(net);
                }
                IpNet::V6(net) => {
                    self.trust_ipv6.add(net);
                }
            }
        }

        self.trust_ipv4.simplify();
        self.trust_ipv6.simplify();

        self.ban_ipv4 = IpRange::new();
        self.ban_ipv6 = IpRange::new();

        for entry in &self.ban_entries {
            match entry.net {
                IpNet::V4(net) => {
                    self.ban_ipv4.add(net);
                }
                IpNet::V6(net) => {
                    self.ban_ipv6.add(net);
                }
            }
        }

        self.ban_ipv4.simplify();
        self.ban_ipv6.simplify();

        let trust_expiry = self.trust_entries.iter().filter_map(|e| e.expires_at).min();
        let ban_expiry = self.ban_entries.iter().filter_map(|e| e.expires_at).min();

        self.next_expiry = match (trust_expiry, ban_expiry) {
            (Some(t), Some(b)) => Some(t.min(b)),
            (Some(t), None) => Some(t),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    }

    #[cfg(test)]
    pub fn trust_count(&self) -> usize {
        self.trust_entries.len()
    }

    #[cfg(test)]
    pub fn ban_count(&self) -> usize {
        self.ban_entries.len()
    }
}

impl Default for IpRuleCache {
    fn default() -> Self {
        Self::new()
    }
}

/// True if `entry_net` is fully contained within `range_net` (same family,
/// network covered, and prefix at least as specific).
fn is_contained_by(entry_net: &IpNet, range_net: &IpNet) -> bool {
    match (entry_net, range_net) {
        (IpNet::V4(entry_net), IpNet::V4(range_net)) => {
            range_net.contains(&entry_net.network())
                && entry_net.prefix_len() >= range_net.prefix_len()
        }
        (IpNet::V6(entry_net), IpNet::V6(range_net)) => {
            range_net.contains(&entry_net.network())
                && entry_net.prefix_len() >= range_net.prefix_len()
        }
        _ => false, // IPv4/IPv6 mismatch
    }
}

/// Whether `net` represents a multi-host CIDR range (prefix below the max
/// for its address family) rather than a single host.
pub fn is_cidr_range(net: IpNet) -> bool {
    match net {
        IpNet::V4(v4) => v4.prefix_len() < 32,
        IpNet::V6(v6) => v6.prefix_len() < 128,
    }
}

/// Parse an IP or CIDR into an IpNet. Single IPs become /32 (IPv4) or /128
/// (IPv6).
pub fn parse_ip_or_cidr(s: &str) -> Option<IpNet> {
    if let Ok(net) = s.parse::<IpNet>() {
        return Some(net);
    }

    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(match ip {
            IpAddr::V4(v4) => IpNet::V4(Ipv4Net::new(v4, 32).ok()?),
            IpAddr::V6(v6) => IpNet::V6(Ipv6Net::new(v6, 128).ok()?),
        });
    }

    None
}

/// Debug-assert `ip_or_cidr` is already canonical (per [`canonicalize_target`]).
///
/// Handlers canonicalize before reaching the cache/DB; cache `add_*` and DB
/// `create_or_update_*` rely on stored strings being canonical for dedup and
/// round-trip removal. Fires in debug builds if a caller skips the funnel.
#[track_caller]
pub fn assert_canonical_target(ip_or_cidr: &str) {
    debug_assert_eq!(
        canonicalize_target(ip_or_cidr)
            .map(|(c, _, _)| c)
            .as_deref(),
        Some(ip_or_cidr),
        "{}",
        ERR_TARGET_NOT_CANONICAL,
    );
}

/// Canonicalize an IP/CIDR string, returning `Some((canonical, net, is_range))`
/// (or `None` if unparseable). The canonical form is stored in `ip_bans` /
/// `ip_trusted` and echoed back to admins.
///
/// - Bare IPs and `/32` / `/128` collapse to a bare-IP form.
/// - CIDR ranges keep their prefix with host bits zeroed (`192.168.1.5/24` →
///   `192.168.1.0/24`).
/// - IPv4-mapped IPv6 (`::ffff:0:0/96`, prefix ≥ 96) folds to IPv4
///   (`::ffff:192.168.1.0/120` → `192.168.1.0/24`); prefix < 96 spans
///   non-mapped space and stays IPv6.
///
/// `net` matches `canonical` and `is_range` matches [`is_cidr_range`]`(net)`,
/// both returned to spare callers a re-parse/match.
pub fn canonicalize_target(s: &str) -> Option<(String, IpNet, bool)> {
    // `trunc` zeroes host bits so `net` matches the canonical string (no-op
    // for /32 and /128).
    let net = fold_ipv4_mapped(parse_ip_or_cidr(s)?).trunc();
    let is_range = is_cidr_range(net);
    let canonical = if is_range {
        net.to_string()
    } else {
        net.addr().to_string()
    };
    Some((canonical, net, is_range))
}

/// Fold an IPv4-mapped IPv6 `IpNet` to its IPv4 equivalent when the CIDR sits
/// entirely within the mapped range (IPv6 prefix ≥ 96). Returns the input
/// unchanged for already-IPv4, non-mapped, or prefix < 96 (which spans
/// non-mapped space, so no clean IPv4 equivalent). Bare IPs arrive as /128.
fn fold_ipv4_mapped(net: IpNet) -> IpNet {
    if let IpNet::V6(v6) = net
        && v6.prefix_len() >= 96
        && let Some(v4) = v6.addr().to_ipv4_mapped()
    {
        let v4_prefix = v6.prefix_len() - 96;
        return IpNet::V4(Ipv4Net::new(v4, v4_prefix).expect(ERR_IPV4_PREFIX_FROM_MAPPED));
    }
    net
}

/// Current Unix timestamp in seconds.
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ip_or_cidr_single_ipv4() {
        let net = parse_ip_or_cidr("192.168.1.100").unwrap();
        assert_eq!(net.to_string(), "192.168.1.100/32");
    }

    #[test]
    fn test_parse_ip_or_cidr_single_ipv6() {
        let net = parse_ip_or_cidr("2001:db8::1").unwrap();
        assert_eq!(net.to_string(), "2001:db8::1/128");
    }

    #[test]
    fn test_parse_ip_or_cidr_cidr_v4() {
        let net = parse_ip_or_cidr("192.168.1.0/24").unwrap();
        assert_eq!(net.to_string(), "192.168.1.0/24");
    }

    #[test]
    fn test_parse_ip_or_cidr_cidr_v6() {
        let net = parse_ip_or_cidr("2001:db8::/32").unwrap();
        assert_eq!(net.to_string(), "2001:db8::/32");
    }

    #[test]
    fn test_parse_ip_or_cidr_invalid() {
        assert!(parse_ip_or_cidr("not-an-ip").is_none());
        assert!(parse_ip_or_cidr("").is_none());
        assert!(parse_ip_or_cidr("192.168.1.0/33").is_none()); // invalid prefix
    }

    #[test]
    fn test_canonicalize_target_normalizes_shorthand() {
        // (input, expected_canonical, expected_is_range)
        let cases: &[(&str, &str, bool)] = &[
            // IPv6 case fold
            ("2001:DB8::1", "2001:db8::1", false),
            // IPv6 leading-zero octet
            ("2001:0db8::1", "2001:db8::1", false),
            // IPv6 fully expanded with leading zeros
            (
                "2001:0db8:0000:0000:0000:0000:0000:0001",
                "2001:db8::1",
                false,
            ),
            // IPv4 CIDR with host bits set zeroes to the network address
            ("192.168.1.5/24", "192.168.1.0/24", true),
            // Non-octet-aligned IPv4 CIDR: host bits cleared bitwise
            ("192.168.1.5/26", "192.168.1.0/26", true),
            ("192.168.1.250/28", "192.168.1.240/28", true),
            // /19 cuts into the 3rd octet
            ("10.20.30.45/19", "10.20.0.0/19", true),
            // IPv6 CIDR with host bits set zeroes likewise (and folds case)
            ("2001:DB8::5/32", "2001:db8::/32", true),
            // Non-hextet-aligned IPv6 CIDR
            ("2001:db8::5/127", "2001:db8::4/127", true),
            // /32 and /128 collapse to bare IP
            ("192.168.1.100/32", "192.168.1.100", false),
            ("2001:db8::1/128", "2001:db8::1", false),
            // IPv4-mapped IPv6 (single host) folds to bare IPv4
            ("::ffff:192.168.1.1", "192.168.1.1", false),
            ("::ffff:c0a8:101", "192.168.1.1", false),
            // IPv4-mapped IPv6 CIDR (prefix >= 96) folds to IPv4 CIDR
            ("::ffff:192.168.1.0/120", "192.168.1.0/24", true),
            // IPv4-mapped IPv6 CIDR with host bits set zeroes via network()
            ("::ffff:192.168.1.5/120", "192.168.1.0/24", true),
            // IPv4-mapped IPv6 CIDR at the /96 boundary covers all of IPv4
            ("::ffff:0.0.0.0/96", "0.0.0.0/0", true),
            // IPv4-mapped IPv6 CIDR with prefix < 96 spans non-mapped space —
            // stays as IPv6 CIDR (no clean IPv4 equivalent)
            ("::ffff:0:0/95", "::fffe:0:0/95", true),
            // Plain IPv4 round-trips unchanged
            ("10.0.0.1", "10.0.0.1", false),
        ];
        for &(input, expected, expected_is_range) in cases {
            let Some((canonical, net, is_range)) = canonicalize_target(input) else {
                panic!("canonicalize_target({input:?}) returned None");
            };
            assert_eq!(canonical, expected, "canonical for {input:?}");
            assert_eq!(is_range, expected_is_range, "is_range for {input:?}");
            // net round-trips with canonical; is_range agrees with is_cidr_range.
            assert_eq!(parse_ip_or_cidr(&canonical), Some(net), "net for {input:?}");
            assert_eq!(is_cidr_range(net), is_range, "is_cidr_range for {input:?}");
        }

        for input in ["not-an-ip", "", "192.168.1.0/33", "192.168.1"] {
            assert!(
                canonicalize_target(input).is_none(),
                "canonicalize_target({input:?}) should be None"
            );
        }
    }

    #[test]
    fn test_assert_canonical_target_passes_for_canonical_inputs() {
        // Smoke: assertion is silent for already-canonical inputs.
        assert_canonical_target("192.168.1.1");
        assert_canonical_target("2001:db8::1");
        assert_canonical_target("192.168.1.0/24");
        assert_canonical_target("2001:db8::/32");
    }

    #[test]
    #[should_panic(expected = "ip_or_cidr must be canonical")]
    fn test_add_ban_panics_on_non_canonical_input() {
        // Uppercase IPv6 isn't canonical; the debug_assert catches a caller
        // that skips `canonicalize_target`.
        let mut cache = IpRuleCache::new();
        cache.add_ban("2001:DB8::1", None);
    }

    #[test]
    #[should_panic(expected = "ip_or_cidr must be canonical")]
    fn test_add_trust_panics_on_non_canonical_input() {
        // CIDR with host bits set isn't canonical; same contract as add_ban.
        let mut cache = IpRuleCache::new();
        cache.add_trust("192.168.1.5/24", None);
    }

    #[test]
    fn test_trust_cache_empty() {
        let mut cache = IpRuleCache::new();
        assert!(!cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_trusted("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_trust_cache_add_single_ip() {
        let mut cache = IpRuleCache::new();
        assert!(cache.add_trust("192.168.1.100", None));

        assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_trusted("192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_trust_cache_add_cidr() {
        let mut cache = IpRuleCache::new();
        assert!(cache.add_trust("192.168.1.0/24", None));

        assert!(cache.is_trusted("192.168.1.0".parse().unwrap()));
        assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(cache.is_trusted("192.168.1.255".parse().unwrap()));
        assert!(!cache.is_trusted("192.168.2.1".parse().unwrap()));
    }

    #[test]
    fn test_trust_cache_remove() {
        let mut cache = IpRuleCache::new();
        cache.add_trust("192.168.1.100", None);
        cache.add_trust("192.168.1.101", None);

        assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(cache.is_trusted("192.168.1.101".parse().unwrap()));

        assert!(cache.remove_trust("192.168.1.100"));
        assert!(!cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(cache.is_trusted("192.168.1.101".parse().unwrap()));

        assert!(!cache.remove_trust("192.168.1.100"));
    }

    #[test]
    fn test_trust_cache_remove_contained_by() {
        let mut cache = IpRuleCache::new();
        cache.add_trust("192.168.1.100", None);
        cache.add_trust("192.168.1.101", None);
        cache.add_trust("192.168.1.0/25", None); // .0 - .127
        cache.add_trust("192.168.2.50", None);

        let removed = cache.remove_trusts_contained_by("192.168.1.0/24");

        assert_eq!(removed.len(), 3);
        assert!(removed.contains(&"192.168.1.100".to_string()));
        assert!(removed.contains(&"192.168.1.101".to_string()));
        assert!(removed.contains(&"192.168.1.0/25".to_string()));

        // .2.50 is outside the removed range
        assert!(cache.is_trusted("192.168.2.50".parse().unwrap()));
        assert!(!cache.is_trusted("192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_empty() {
        let mut cache = IpRuleCache::new();
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_add_single_ip() {
        let mut cache = IpRuleCache::new();
        assert!(cache.add_ban("192.168.1.100", None));

        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_add_cidr() {
        let mut cache = IpRuleCache::new();
        assert!(cache.add_ban("192.168.1.0/24", None));

        assert!(cache.is_banned("192.168.1.0".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.255".parse().unwrap()));
        assert!(!cache.is_banned("192.168.2.1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_add_ipv6_cidr() {
        let mut cache = IpRuleCache::new();
        assert!(cache.add_ban("2001:db8::/32", None));

        assert!(cache.is_banned("2001:db8::1".parse().unwrap()));
        assert!(cache.is_banned("2001:db8:1234::5678".parse().unwrap()));
        assert!(!cache.is_banned("2001:db9::1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_remove() {
        let mut cache = IpRuleCache::new();
        cache.add_ban("192.168.1.100", None);
        cache.add_ban("192.168.1.101", None);

        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        assert!(cache.remove_ban("192.168.1.100"));
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        assert!(!cache.remove_ban("192.168.1.100"));
    }

    #[test]
    fn test_ban_cache_remove_contained_by() {
        let mut cache = IpRuleCache::new();
        cache.add_ban("192.168.1.100", None);
        cache.add_ban("192.168.1.101", None);
        cache.add_ban("192.168.1.0/25", None); // .0 - .127
        cache.add_ban("192.168.2.50", None);

        let removed = cache.remove_bans_contained_by("192.168.1.0/24");

        assert_eq!(removed.len(), 3);
        assert!(removed.contains(&"192.168.1.100".to_string()));
        assert!(removed.contains(&"192.168.1.101".to_string()));
        assert!(removed.contains(&"192.168.1.0/25".to_string()));

        // .2.50 is outside the removed range
        assert!(cache.is_banned("192.168.2.50".parse().unwrap()));
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_expiry() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        cache.add_ban("192.168.1.100", None); // permanent
        cache.add_ban("192.168.1.101", Some(now + 3600)); // future
        cache.add_ban("192.168.1.102", Some(now - 1)); // already expired

        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        // Expired ban inactive; is_banned triggers the lazy rebuild that
        // prunes it.
        assert!(!cache.is_banned("192.168.1.102".parse().unwrap()));

        assert_eq!(cache.ban_count(), 2);
    }

    #[test]
    fn test_trust_cache_expiry() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        cache.add_trust("192.168.1.100", None); // permanent
        cache.add_trust("192.168.1.101", Some(now + 3600)); // future
        cache.add_trust("192.168.1.102", Some(now - 1)); // already expired

        assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(cache.is_trusted("192.168.1.101".parse().unwrap()));

        assert!(!cache.is_trusted("192.168.1.102".parse().unwrap()));

        assert_eq!(cache.trust_count(), 2);
    }

    #[test]
    fn test_next_expiry_across_both() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        // All permanent → no next_expiry.
        cache.add_ban("192.168.1.100", None);
        cache.add_trust("10.0.0.1", None);
        assert!(cache.next_expiry.is_none());

        cache.add_ban("192.168.1.101", Some(now + 3600));
        assert_eq!(cache.next_expiry, Some(now + 3600));

        // Earlier trust expiry wins.
        cache.add_trust("10.0.0.2", Some(now + 1800));
        assert_eq!(cache.next_expiry, Some(now + 1800));

        // Removing it falls back to the ban's expiry.
        cache.remove_trust("10.0.0.2");
        assert_eq!(cache.next_expiry, Some(now + 3600));
    }

    #[test]
    fn test_should_allow_unbanned_untrusted() {
        let mut cache = IpRuleCache::new();
        assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_should_allow_trusted() {
        let mut cache = IpRuleCache::new();
        cache.add_trust("192.168.1.100", None);
        assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_should_deny_banned() {
        let mut cache = IpRuleCache::new();
        cache.add_ban("192.168.1.100", None);
        assert!(!cache.should_allow("192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_trust_bypasses_ban() {
        let mut cache = IpRuleCache::new();
        cache.add_ban("0.0.0.0/0", None);
        cache.add_trust("192.168.1.100", None);

        // Trust bypasses the catch-all ban; untrusted IPs stay denied.
        assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
        assert!(!cache.should_allow("192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_whitelist_only_mode() {
        let mut cache = IpRuleCache::new();

        // Ban all v4+v6, trust one range → effective whitelist.
        cache.add_ban("0.0.0.0/0", None);
        cache.add_ban("::/0", None);
        cache.add_trust("192.168.1.0/24", None);

        assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
        assert!(cache.should_allow("192.168.1.1".parse().unwrap()));

        assert!(!cache.should_allow("192.168.2.1".parse().unwrap()));
        assert!(!cache.should_allow("10.0.0.1".parse().unwrap()));
        assert!(!cache.should_allow("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_ipv4_mapped_ipv6_normalization() {
        let mut cache = IpRuleCache::new();

        cache.add_ban("192.168.1.0/24", None);

        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));

        // Mapped IPv6 normalizes and matches the IPv4 ban.
        assert!(cache.is_banned("::ffff:192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("::ffff:192.168.2.100".parse().unwrap()));
    }

    #[test]
    fn test_trust_ipv4_mapped_ipv6_normalization() {
        let mut cache = IpRuleCache::new();

        cache.add_trust("192.168.1.100", None);

        // Mapped IPv6 normalizes and matches the IPv4 trust.
        assert!(cache.is_trusted("::ffff:192.168.1.100".parse().unwrap()));
        assert!(!cache.is_trusted("::ffff:192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_normalize_ip() {
        let v4: IpAddr = "192.168.1.100".parse().unwrap();
        assert_eq!(normalize_ip(v4), v4); // IPv4 unchanged

        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        assert_eq!(normalize_ip(v6), v6); // non-mapped IPv6 unchanged

        let mapped: IpAddr = "::ffff:192.168.1.100".parse().unwrap();
        let expected: IpAddr = "192.168.1.100".parse().unwrap();
        assert_eq!(normalize_ip(mapped), expected); // mapped → IPv4

        let mapped_zero: IpAddr = "::ffff:0.0.0.0".parse().unwrap();
        let expected_zero: IpAddr = "0.0.0.0".parse().unwrap();
        assert_eq!(normalize_ip(mapped_zero), expected_zero);
    }

    #[test]
    fn test_from_records() {
        let now = current_timestamp();

        let ban_records = vec![
            BanRecord {
                ip_address: "192.168.1.100".to_string(),
                nickname: None,
                reason: None,
                created_by: "admin".to_string(),
                created_at: now,
                expires_at: None,
            },
            BanRecord {
                ip_address: "10.0.0.0/8".to_string(),
                nickname: Some("spammer".to_string()),
                reason: Some("flooding".to_string()),
                created_by: "admin".to_string(),
                created_at: now,
                expires_at: Some(now + 3600),
            },
        ];

        let trust_records = vec![TrustRecord {
            ip_address: "172.16.0.0/12".to_string(),
            nickname: None,
            reason: Some("office network".to_string()),
            created_by: "admin".to_string(),
            created_at: now,
            expires_at: None,
        }];

        let mut cache = IpRuleCache::from_records(ban_records, trust_records);

        assert_eq!(cache.ban_count(), 2);
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("10.0.0.1".parse().unwrap()));
        assert!(!cache.is_banned("11.0.0.1".parse().unwrap()));

        assert_eq!(cache.trust_count(), 1);
        assert!(cache.is_trusted("172.16.0.1".parse().unwrap()));
        assert!(!cache.is_trusted("172.32.0.1".parse().unwrap()));
    }

    #[test]
    fn test_upsert_behavior() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        cache.add_ban("192.168.1.100", None);
        assert_eq!(cache.ban_count(), 1);
        assert!(cache.next_expiry.is_none());

        // Re-adding the same IP upserts (no duplicate) and applies the expiry.
        cache.add_ban("192.168.1.100", Some(now + 3600));
        assert_eq!(cache.ban_count(), 1);
        assert_eq!(cache.next_expiry, Some(now + 3600));

        cache.add_trust("10.0.0.1", None);
        assert_eq!(cache.trust_count(), 1);

        cache.add_trust("10.0.0.1", Some(now + 1800));
        assert_eq!(cache.trust_count(), 1);
        assert_eq!(cache.next_expiry, Some(now + 1800));
    }
}
