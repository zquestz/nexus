//! In-memory IP rule cache for fast pre-TLS checking
//!
//! Uses radix tries via `iprange` for O(log n) containment checks.
//! Supports both single IPs and CIDR ranges.
//!
//! IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
//! normalized to IPv4 for checking, ensuring rules work correctly regardless
//! of how the OS presents incoming connections.
//!
//! ## Access Control Logic
//!
//! ```text
//! fn should_allow_connection(ip: IpAddr) -> bool {
//!     if cache.is_trusted(ip) {
//!         return true;  // Trusted = in, done
//!     }
//!     !cache.is_banned(ip)
//! }
//! ```

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::IpRange;

use crate::db::bans::BanRecord;
use crate::db::trusts::TrustRecord;

/// A cached rule entry (used for both bans and trusts)
#[derive(Debug, Clone)]
struct RuleEntry {
    /// The IP or CIDR range (as stored in DB)
    ip_address: String,
    /// Parsed network (single IP becomes /32 or /128)
    net: IpNet,
    /// Unix timestamp when rule expires (None = permanent)
    expires_at: Option<i64>,
}

/// In-memory cache for IP access rules (trusts and bans)
///
/// Provides fast O(log n) lookups using radix tries.
/// Handles expiry via lazy rebuild when `next_expiry` is reached.
#[derive(Debug)]
pub struct IpRuleCache {
    /// IPv4 radix trie for trusted IPs
    trust_ipv4: IpRange<Ipv4Net>,
    /// IPv6 radix trie for trusted IPs
    trust_ipv6: IpRange<Ipv6Net>,
    /// IPv4 radix trie for banned IPs
    ban_ipv4: IpRange<Ipv4Net>,
    /// IPv6 radix trie for banned IPs
    ban_ipv6: IpRange<Ipv6Net>,
    /// Source entries for trust rebuilds and removal
    trust_entries: Vec<RuleEntry>,
    /// Source entries for ban rebuilds and removal
    ban_entries: Vec<RuleEntry>,
    /// Earliest expiry timestamp across both trusts and bans (None if all are permanent)
    next_expiry: Option<i64>,
}

impl IpRuleCache {
    /// Create an empty IP rule cache
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

    /// Check if a connection should be allowed (mutable version)
    ///
    /// Returns true if:
    /// - The IP is trusted (bypasses ban check), OR
    /// - The IP is not banned
    ///
    /// This version handles lazy expiry rebuild internally.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    pub fn should_allow(&mut self, ip: IpAddr) -> bool {
        self.maybe_rebuild_on_expiry();
        self.should_allow_read_only(ip)
    }

    /// Check if a connection should be allowed (read-only version)
    ///
    /// Returns true if:
    /// - The IP is trusted (bypasses ban check), OR
    /// - The IP is not banned
    ///
    /// This version does NOT check for expiry rebuild. Call `needs_rebuild()`
    /// separately and use `rebuild_if_needed()` if true.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    pub fn should_allow_read_only(&self, ip: IpAddr) -> bool {
        if self.is_trusted_read_only(ip) {
            return true;
        }
        !self.is_banned_read_only(ip)
    }

    /// Check if an IP address is trusted (mutable version)
    ///
    /// Returns true if the IP matches any non-expired trust entry.
    /// This version handles lazy expiry rebuild internally.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    #[cfg(test)]
    pub fn is_trusted(&mut self, ip: IpAddr) -> bool {
        self.maybe_rebuild_on_expiry();
        self.is_trusted_read_only(ip)
    }

    /// Check if an IP address is trusted (read-only version)
    ///
    /// Returns true if the IP matches any non-expired trust entry.
    /// This version does NOT check for expiry rebuild.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    pub fn is_trusted_read_only(&self, ip: IpAddr) -> bool {
        // Normalize IPv4-mapped IPv6 addresses to IPv4
        let ip = normalize_ip(ip);

        match ip {
            IpAddr::V4(v4) => self.trust_ipv4.contains(&v4),
            IpAddr::V6(v6) => self.trust_ipv6.contains(&v6),
        }
    }

    /// Check if an IP address is banned (mutable version)
    ///
    /// Returns true if the IP matches any non-expired ban entry.
    /// This version handles lazy expiry rebuild internally.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    #[cfg(test)]
    pub fn is_banned(&mut self, ip: IpAddr) -> bool {
        self.maybe_rebuild_on_expiry();
        self.is_banned_read_only(ip)
    }

    /// Check if an IP address is banned (read-only version)
    ///
    /// Returns true if the IP matches any non-expired ban entry.
    /// This version does NOT check for expiry rebuild.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    pub fn is_banned_read_only(&self, ip: IpAddr) -> bool {
        // Normalize IPv4-mapped IPv6 addresses to IPv4
        let ip = normalize_ip(ip);

        match ip {
            IpAddr::V4(v4) => self.ban_ipv4.contains(&v4),
            IpAddr::V6(v6) => self.ban_ipv6.contains(&v6),
        }
    }

    /// Check if the cache needs to be rebuilt due to expired entries
    ///
    /// This is a read-only check that can be used to determine if a write
    /// lock is needed before calling `rebuild_if_needed()`.
    pub fn needs_rebuild(&self) -> bool {
        if let Some(expiry) = self.next_expiry {
            current_timestamp() >= expiry
        } else {
            false
        }
    }

    /// Check if we need to rebuild due to expiry (internal)
    fn maybe_rebuild_on_expiry(&mut self) {
        if self.needs_rebuild() {
            self.rebuild_tries();
        }
    }

    // =========================================================================
    // Trust operations
    // =========================================================================

    /// Add a trust entry to the cache
    ///
    /// The `ip_or_cidr` should be a valid IP address or CIDR notation.
    /// Returns true if successfully added, false if parsing failed.
    pub fn add_trust(&mut self, ip_or_cidr: &str, expires_at: Option<i64>) -> bool {
        let Some(net) = parse_ip_or_cidr(ip_or_cidr) else {
            return false;
        };

        // Remove any existing entry for this exact IP/CIDR
        self.trust_entries.retain(|e| e.ip_address != ip_or_cidr);

        // Add new entry
        self.trust_entries.push(RuleEntry {
            ip_address: ip_or_cidr.to_string(),
            net,
            expires_at,
        });

        // Rebuild tries and recalculate next_expiry
        self.rebuild_tries();
        true
    }

    /// Remove a trust entry from the cache by exact IP/CIDR match
    ///
    /// Returns true if an entry was removed.
    pub fn remove_trust(&mut self, ip_or_cidr: &str) -> bool {
        let before = self.trust_entries.len();
        self.trust_entries.retain(|e| e.ip_address != ip_or_cidr);
        let removed = self.trust_entries.len() < before;

        if removed {
            self.rebuild_tries();
        }

        removed
    }

    /// Remove all trusts that fall within a CIDR range
    ///
    /// Used when untrusting a CIDR range to also remove any single IPs within it.
    /// Returns the list of IP/CIDR strings that were removed.
    pub fn remove_trusts_contained_by(&mut self, cidr: &str) -> Vec<String> {
        let Some(range_net) = parse_ip_or_cidr(cidr) else {
            return Vec::new();
        };

        let mut removed = Vec::new();

        self.trust_entries.retain(|entry| {
            let is_contained = is_contained_by(&entry.net, &range_net);

            if is_contained {
                removed.push(entry.ip_address.clone());
                false // Remove from entries
            } else {
                true // Keep in entries
            }
        });

        if !removed.is_empty() {
            self.rebuild_tries();
        }

        removed
    }

    // =========================================================================
    // Ban operations
    // =========================================================================

    /// Add a ban to the cache
    ///
    /// The `ip_or_cidr` should be a valid IP address or CIDR notation.
    /// Returns true if successfully added, false if parsing failed.
    pub fn add_ban(&mut self, ip_or_cidr: &str, expires_at: Option<i64>) -> bool {
        let Some(net) = parse_ip_or_cidr(ip_or_cidr) else {
            return false;
        };

        // Remove any existing entry for this exact IP/CIDR
        self.ban_entries.retain(|e| e.ip_address != ip_or_cidr);

        // Add new entry
        self.ban_entries.push(RuleEntry {
            ip_address: ip_or_cidr.to_string(),
            net,
            expires_at,
        });

        // Rebuild tries and recalculate next_expiry
        self.rebuild_tries();
        true
    }

    /// Remove a ban from the cache by exact IP/CIDR match
    ///
    /// Returns true if an entry was removed.
    pub fn remove_ban(&mut self, ip_or_cidr: &str) -> bool {
        let before = self.ban_entries.len();
        self.ban_entries.retain(|e| e.ip_address != ip_or_cidr);
        let removed = self.ban_entries.len() < before;

        if removed {
            self.rebuild_tries();
        }

        removed
    }

    /// Remove all bans that fall within a CIDR range
    ///
    /// Used when unbanning a CIDR range to also remove any single IPs within it.
    /// Returns the list of IP/CIDR strings that were removed.
    pub fn remove_bans_contained_by(&mut self, cidr: &str) -> Vec<String> {
        let Some(range_net) = parse_ip_or_cidr(cidr) else {
            return Vec::new();
        };

        let mut removed = Vec::new();

        self.ban_entries.retain(|entry| {
            let is_contained = is_contained_by(&entry.net, &range_net);

            if is_contained {
                removed.push(entry.ip_address.clone());
                false // Remove from entries
            } else {
                true // Keep in entries
            }
        });

        if !removed.is_empty() {
            self.rebuild_tries();
        }

        removed
    }

    // =========================================================================
    // Internal methods
    // =========================================================================

    /// Rebuild all radix tries from entries, filtering out expired rules
    fn rebuild_tries(&mut self) {
        let now = current_timestamp();

        // Filter out expired entries
        self.trust_entries
            .retain(|e| e.expires_at.is_none() || e.expires_at.unwrap() > now);
        self.ban_entries
            .retain(|e| e.expires_at.is_none() || e.expires_at.unwrap() > now);

        // Rebuild trust tries
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

        // Rebuild ban tries
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

        // Calculate next expiry across both types
        let trust_expiry = self.trust_entries.iter().filter_map(|e| e.expires_at).min();
        let ban_expiry = self.ban_entries.iter().filter_map(|e| e.expires_at).min();

        self.next_expiry = match (trust_expiry, ban_expiry) {
            (Some(t), Some(b)) => Some(t.min(b)),
            (Some(t), None) => Some(t),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    }

    /// Get the number of active trust entries
    #[cfg(test)]
    pub fn trust_count(&self) -> usize {
        self.trust_entries.len()
    }

    /// Get the number of active ban entries
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

/// Check if one network is contained within another
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
        _ => false, // IPv4/IPv6 mismatch, can't be contained
    }
}

/// Parse an IP address or CIDR notation into an IpNet
///
/// Single IPs are converted to /32 (IPv4) or /128 (IPv6).
pub fn parse_ip_or_cidr(s: &str) -> Option<IpNet> {
    // Try parsing as CIDR first
    if let Ok(net) = s.parse::<IpNet>() {
        return Some(net);
    }

    // Try parsing as single IP
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(match ip {
            IpAddr::V4(v4) => IpNet::V4(Ipv4Net::new(v4, 32).ok()?),
            IpAddr::V6(v6) => IpNet::V6(Ipv6Net::new(v6, 128).ok()?),
        });
    }

    None
}

/// Normalize an IP address, converting IPv4-mapped IPv6 to IPv4
///
/// This ensures that rules for `192.168.1.100` also match connections that
/// appear as `::ffff:192.168.1.100` (which can happen when the server binds
/// to `::` and receives IPv4 connections).
fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => {
            if let Some(v4) = to_ipv4_mapped(&v6) {
                IpAddr::V4(v4)
            } else {
                ip
            }
        }
        _ => ip,
    }
}

/// Extract IPv4 address from an IPv4-mapped IPv6 address
///
/// Returns `Some(Ipv4Addr)` if the address is in the `::ffff:0:0/96` range,
/// `None` otherwise.
fn to_ipv4_mapped(v6: &Ipv6Addr) -> Option<Ipv4Addr> {
    let octets = v6.octets();
    // Check for ::ffff:x.x.x.x pattern (bytes 0-9 are 0, bytes 10-11 are 0xff)
    if octets[0..10] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] && octets[10] == 0xff && octets[11] == 0xff {
        Some(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        ))
    } else {
        None
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Parsing tests
    // =========================================================================

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
        assert!(parse_ip_or_cidr("192.168.1.0/33").is_none()); // Invalid prefix
    }

    // =========================================================================
    // Trust tests
    // =========================================================================

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

        // Removing non-existent returns false
        assert!(!cache.remove_trust("192.168.1.100"));
    }

    #[test]
    fn test_trust_cache_remove_contained_by() {
        let mut cache = IpRuleCache::new();
        cache.add_trust("192.168.1.100", None);
        cache.add_trust("192.168.1.101", None);
        cache.add_trust("192.168.1.0/25", None); // .0 - .127
        cache.add_trust("192.168.2.50", None);

        // Remove everything in 192.168.1.0/24
        let removed = cache.remove_trusts_contained_by("192.168.1.0/24");

        assert_eq!(removed.len(), 3);
        assert!(removed.contains(&"192.168.1.100".to_string()));
        assert!(removed.contains(&"192.168.1.101".to_string()));
        assert!(removed.contains(&"192.168.1.0/25".to_string()));

        // 192.168.2.50 should still be trusted
        assert!(cache.is_trusted("192.168.2.50".parse().unwrap()));
        assert!(!cache.is_trusted("192.168.1.100".parse().unwrap()));
    }

    // =========================================================================
    // Ban tests
    // =========================================================================

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

        // Removing non-existent returns false
        assert!(!cache.remove_ban("192.168.1.100"));
    }

    #[test]
    fn test_ban_cache_remove_contained_by() {
        let mut cache = IpRuleCache::new();
        cache.add_ban("192.168.1.100", None);
        cache.add_ban("192.168.1.101", None);
        cache.add_ban("192.168.1.0/25", None); // .0 - .127
        cache.add_ban("192.168.2.50", None);

        // Remove everything in 192.168.1.0/24
        let removed = cache.remove_bans_contained_by("192.168.1.0/24");

        assert_eq!(removed.len(), 3);
        assert!(removed.contains(&"192.168.1.100".to_string()));
        assert!(removed.contains(&"192.168.1.101".to_string()));
        assert!(removed.contains(&"192.168.1.0/25".to_string()));

        // 192.168.2.50 should still be banned
        assert!(cache.is_banned("192.168.2.50".parse().unwrap()));
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
    }

    // =========================================================================
    // Expiry tests
    // =========================================================================

    #[test]
    fn test_ban_cache_expiry() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        // Add a permanent ban
        cache.add_ban("192.168.1.100", None);

        // Add a ban that expires in the future
        cache.add_ban("192.168.1.101", Some(now + 3600));

        // Add an already-expired ban
        cache.add_ban("192.168.1.102", Some(now - 1));

        // Permanent and future bans should be active
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        // Expired ban should not be active (rebuild happens on is_banned check)
        assert!(!cache.is_banned("192.168.1.102".parse().unwrap()));

        // Should have 2 entries after expired one is cleaned up
        assert_eq!(cache.ban_count(), 2);
    }

    #[test]
    fn test_trust_cache_expiry() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        // Add a permanent trust
        cache.add_trust("192.168.1.100", None);

        // Add a trust that expires in the future
        cache.add_trust("192.168.1.101", Some(now + 3600));

        // Add an already-expired trust
        cache.add_trust("192.168.1.102", Some(now - 1));

        // Permanent and future trusts should be active
        assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
        assert!(cache.is_trusted("192.168.1.101".parse().unwrap()));

        // Expired trust should not be active
        assert!(!cache.is_trusted("192.168.1.102".parse().unwrap()));

        // Should have 2 entries after expired one is cleaned up
        assert_eq!(cache.trust_count(), 2);
    }

    #[test]
    fn test_next_expiry_across_both() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        // All permanent - no next_expiry
        cache.add_ban("192.168.1.100", None);
        cache.add_trust("10.0.0.1", None);
        assert!(cache.next_expiry.is_none());

        // Add timed ban
        cache.add_ban("192.168.1.101", Some(now + 3600));
        assert_eq!(cache.next_expiry, Some(now + 3600));

        // Add earlier trust expiry
        cache.add_trust("10.0.0.2", Some(now + 1800));
        assert_eq!(cache.next_expiry, Some(now + 1800));

        // Remove earlier trust, next_expiry should update to ban's expiry
        cache.remove_trust("10.0.0.2");
        assert_eq!(cache.next_expiry, Some(now + 3600));
    }

    // =========================================================================
    // Access control logic tests
    // =========================================================================

    #[test]
    fn test_should_allow_unbanned_untrusted() {
        let mut cache = IpRuleCache::new();
        // Not banned, not trusted = allowed
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
        // Ban everything
        cache.add_ban("0.0.0.0/0", None);
        // But trust specific IP
        cache.add_trust("192.168.1.100", None);

        // Trusted IP should be allowed despite being in ban range
        assert!(cache.should_allow("192.168.1.100".parse().unwrap()));

        // Other IPs should be denied
        assert!(!cache.should_allow("192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_whitelist_only_mode() {
        let mut cache = IpRuleCache::new();

        // Ban all IPv4 and IPv6
        cache.add_ban("0.0.0.0/0", None);
        cache.add_ban("::/0", None);

        // Trust specific range
        cache.add_trust("192.168.1.0/24", None);

        // Only trusted range should be allowed
        assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
        assert!(cache.should_allow("192.168.1.1".parse().unwrap()));

        // Everything else denied
        assert!(!cache.should_allow("192.168.2.1".parse().unwrap()));
        assert!(!cache.should_allow("10.0.0.1".parse().unwrap()));
        assert!(!cache.should_allow("2001:db8::1".parse().unwrap()));
    }

    // =========================================================================
    // IPv4-mapped IPv6 normalization tests
    // =========================================================================

    #[test]
    fn test_ipv4_mapped_ipv6_normalization() {
        let mut cache = IpRuleCache::new();

        cache.add_ban("192.168.1.0/24", None);

        // IPv4 check
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));

        // IPv4-mapped IPv6 should be normalized and match IPv4 ban
        assert!(cache.is_banned("::ffff:192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("::ffff:192.168.2.100".parse().unwrap()));
    }

    #[test]
    fn test_trust_ipv4_mapped_ipv6_normalization() {
        let mut cache = IpRuleCache::new();

        cache.add_trust("192.168.1.100", None);

        // IPv4-mapped IPv6 should be normalized and match IPv4 trust
        assert!(cache.is_trusted("::ffff:192.168.1.100".parse().unwrap()));
        assert!(!cache.is_trusted("::ffff:192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_normalize_ip() {
        // IPv4 stays IPv4
        let v4: IpAddr = "192.168.1.100".parse().unwrap();
        assert_eq!(normalize_ip(v4), v4);

        // Regular IPv6 stays IPv6
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        assert_eq!(normalize_ip(v6), v6);

        // IPv4-mapped IPv6 becomes IPv4
        let mapped: IpAddr = "::ffff:192.168.1.100".parse().unwrap();
        let expected: IpAddr = "192.168.1.100".parse().unwrap();
        assert_eq!(normalize_ip(mapped), expected);

        // Edge case: ::ffff:0.0.0.0
        let mapped_zero: IpAddr = "::ffff:0.0.0.0".parse().unwrap();
        let expected_zero: IpAddr = "0.0.0.0".parse().unwrap();
        assert_eq!(normalize_ip(mapped_zero), expected_zero);
    }

    // =========================================================================
    // from_records tests
    // =========================================================================

    #[test]
    fn test_from_records() {
        let now = current_timestamp();

        let ban_records = vec![
            BanRecord {
                id: 1,
                ip_address: "192.168.1.100".to_string(),
                nickname: None,
                reason: None,
                created_by: "admin".to_string(),
                created_at: now,
                expires_at: None,
            },
            BanRecord {
                id: 2,
                ip_address: "10.0.0.0/8".to_string(),
                nickname: Some("spammer".to_string()),
                reason: Some("flooding".to_string()),
                created_by: "admin".to_string(),
                created_at: now,
                expires_at: Some(now + 3600),
            },
        ];

        let trust_records = vec![TrustRecord {
            id: 1,
            ip_address: "172.16.0.0/12".to_string(),
            nickname: None,
            reason: Some("office network".to_string()),
            created_by: "admin".to_string(),
            created_at: now,
            expires_at: None,
        }];

        let mut cache = IpRuleCache::from_records(ban_records, trust_records);

        // Check bans
        assert_eq!(cache.ban_count(), 2);
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("10.0.0.1".parse().unwrap()));
        assert!(!cache.is_banned("11.0.0.1".parse().unwrap()));

        // Check trusts
        assert_eq!(cache.trust_count(), 1);
        assert!(cache.is_trusted("172.16.0.1".parse().unwrap()));
        assert!(!cache.is_trusted("172.32.0.1".parse().unwrap()));
    }

    #[test]
    fn test_upsert_behavior() {
        let mut cache = IpRuleCache::new();
        let now = current_timestamp();

        // Add permanent ban
        cache.add_ban("192.168.1.100", None);
        assert_eq!(cache.ban_count(), 1);
        assert!(cache.next_expiry.is_none());

        // Update to timed ban
        cache.add_ban("192.168.1.100", Some(now + 3600));
        assert_eq!(cache.ban_count(), 1); // Still 1 entry
        assert_eq!(cache.next_expiry, Some(now + 3600));

        // Same for trusts
        cache.add_trust("10.0.0.1", None);
        assert_eq!(cache.trust_count(), 1);

        cache.add_trust("10.0.0.1", Some(now + 1800));
        assert_eq!(cache.trust_count(), 1);
        assert_eq!(cache.next_expiry, Some(now + 1800)); // Updated to earlier expiry
    }
}
