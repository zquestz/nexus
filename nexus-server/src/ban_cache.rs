//! In-memory ban cache for fast pre-TLS IP checking
//!
//! Uses radix tries via `iprange` for O(log n) containment checks.
//! Supports both single IPs and CIDR ranges.
//!
//! IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
//! normalized to IPv4 for ban checking, ensuring bans work correctly regardless
//! of how the OS presents incoming connections.

use std::net::{IpAddr, Ipv6Addr};
use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::IpRange;

use crate::db::bans::BanRecord;

/// A cached ban entry
#[derive(Debug, Clone)]
struct BanEntry {
    /// The IP or CIDR range (as stored in DB)
    ip_address: String,
    /// Parsed network (single IP becomes /32 or /128)
    net: IpNet,
    /// Unix timestamp when ban expires (None = permanent)
    expires_at: Option<i64>,
}

/// In-memory cache for IP bans
///
/// Provides fast O(log n) lookups using radix tries.
/// Handles expiry via lazy rebuild when `next_expiry` is reached.
#[derive(Debug)]
pub struct BanCache {
    /// IPv4 radix trie for fast containment checks
    ipv4: IpRange<Ipv4Net>,
    /// IPv6 radix trie for fast containment checks
    ipv6: IpRange<Ipv6Net>,
    /// Source entries for rebuilds and removal
    entries: Vec<BanEntry>,
    /// Earliest expiry timestamp (None if all bans are permanent)
    next_expiry: Option<i64>,
}

impl BanCache {
    /// Create an empty ban cache
    pub fn new() -> Self {
        Self {
            ipv4: IpRange::new(),
            ipv6: IpRange::new(),
            entries: Vec::new(),
            next_expiry: None,
        }
    }

    /// Load cache from database records
    pub fn from_records(records: Vec<BanRecord>) -> Self {
        let mut cache = Self::new();

        for record in records {
            if let Some(net) = parse_ip_or_cidr(&record.ip_address) {
                cache.entries.push(BanEntry {
                    ip_address: record.ip_address,
                    net,
                    expires_at: record.expires_at,
                });
            }
        }

        cache.rebuild_tries();
        cache
    }

    /// Check if an IP address is banned
    ///
    /// Returns true if the IP matches any non-expired ban entry.
    ///
    /// IPv4-mapped IPv6 addresses (e.g., `::ffff:192.168.1.100`) are automatically
    /// normalized to IPv4 before checking.
    pub fn is_banned(&mut self, ip: IpAddr) -> bool {
        // Check if we need to rebuild due to expiry
        if let Some(expiry) = self.next_expiry
            && current_timestamp() >= expiry
        {
            self.rebuild_tries();
        }

        // Normalize IPv4-mapped IPv6 addresses to IPv4
        let ip = normalize_ip(ip);

        match ip {
            IpAddr::V4(v4) => self.ipv4.contains(&v4),
            IpAddr::V6(v6) => self.ipv6.contains(&v6),
        }
    }

    /// Add a ban to the cache
    ///
    /// The `ip_or_cidr` should be a valid IP address or CIDR notation.
    /// Returns true if successfully added, false if parsing failed.
    pub fn add(&mut self, ip_or_cidr: &str, expires_at: Option<i64>) -> bool {
        let Some(net) = parse_ip_or_cidr(ip_or_cidr) else {
            return false;
        };

        // Remove any existing entry for this exact IP/CIDR
        self.entries.retain(|e| e.ip_address != ip_or_cidr);

        // Add new entry
        self.entries.push(BanEntry {
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
    pub fn remove(&mut self, ip_or_cidr: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.ip_address != ip_or_cidr);
        let removed = self.entries.len() < before;

        if removed {
            self.rebuild_tries();
        }

        removed
    }

    /// Remove all bans that fall within a CIDR range
    ///
    /// Used when unbanning a CIDR range to also remove any single IPs within it.
    /// Returns the list of IP/CIDR strings that were removed.
    pub fn remove_contained_by(&mut self, cidr: &str) -> Vec<String> {
        let Some(range_net) = parse_ip_or_cidr(cidr) else {
            return Vec::new();
        };

        let mut removed = Vec::new();

        self.entries.retain(|entry| {
            // Check if entry is contained within the range
            let is_contained = match (&entry.net, &range_net) {
                (IpNet::V4(entry_net), IpNet::V4(range_net)) => {
                    range_net.contains(&entry_net.network())
                        && entry_net.prefix_len() >= range_net.prefix_len()
                }
                (IpNet::V6(entry_net), IpNet::V6(range_net)) => {
                    range_net.contains(&entry_net.network())
                        && entry_net.prefix_len() >= range_net.prefix_len()
                }
                _ => false, // IPv4/IPv6 mismatch, can't be contained
            };

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

    /// Rebuild the radix tries from entries, filtering out expired bans
    fn rebuild_tries(&mut self) {
        let now = current_timestamp();

        // Filter out expired entries
        self.entries
            .retain(|e| e.expires_at.is_none() || e.expires_at.unwrap() > now);

        // Rebuild tries
        self.ipv4 = IpRange::new();
        self.ipv6 = IpRange::new();

        for entry in &self.entries {
            match entry.net {
                IpNet::V4(net) => {
                    self.ipv4.add(net);
                }
                IpNet::V6(net) => {
                    self.ipv6.add(net);
                }
            }
        }

        // Simplify tries (merge adjacent/overlapping ranges)
        self.ipv4.simplify();
        self.ipv6.simplify();

        // Calculate next expiry
        self.next_expiry = self.entries.iter().filter_map(|e| e.expires_at).min();
    }

    /// Get the number of active ban entries
    #[cfg(test)]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for BanCache {
    fn default() -> Self {
        Self::new()
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
/// This ensures that banning `192.168.1.100` also blocks connections that
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
fn to_ipv4_mapped(v6: &Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    let octets = v6.octets();
    // Check for ::ffff:x.x.x.x pattern (bytes 0-9 are 0, bytes 10-11 are 0xff)
    if octets[0..10] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] && octets[10] == 0xff && octets[11] == 0xff {
        Some(std::net::Ipv4Addr::new(
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

    #[test]
    fn test_ban_cache_empty() {
        let mut cache = BanCache::new();
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_add_single_ip() {
        let mut cache = BanCache::new();
        assert!(cache.add("192.168.1.100", None));

        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("192.168.1.101".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_add_cidr() {
        let mut cache = BanCache::new();
        assert!(cache.add("192.168.1.0/24", None));

        assert!(cache.is_banned("192.168.1.0".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.255".parse().unwrap()));
        assert!(!cache.is_banned("192.168.2.1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_add_ipv6_cidr() {
        let mut cache = BanCache::new();
        assert!(cache.add("2001:db8::/32", None));

        assert!(cache.is_banned("2001:db8::1".parse().unwrap()));
        assert!(cache.is_banned("2001:db8:1234::5678".parse().unwrap()));
        assert!(!cache.is_banned("2001:db9::1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_remove() {
        let mut cache = BanCache::new();
        cache.add("192.168.1.100", None);
        cache.add("192.168.1.101", None);

        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        assert!(cache.remove("192.168.1.100"));
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        // Removing non-existent returns false
        assert!(!cache.remove("192.168.1.100"));
    }

    #[test]
    fn test_ban_cache_remove_contained_by() {
        let mut cache = BanCache::new();
        cache.add("192.168.1.100", None);
        cache.add("192.168.1.101", None);
        cache.add("192.168.1.0/25", None); // .0 - .127
        cache.add("192.168.2.50", None);

        // Remove everything in 192.168.1.0/24
        let removed = cache.remove_contained_by("192.168.1.0/24");

        assert_eq!(removed.len(), 3);
        assert!(removed.contains(&"192.168.1.100".to_string()));
        assert!(removed.contains(&"192.168.1.101".to_string()));
        assert!(removed.contains(&"192.168.1.0/25".to_string()));

        // 192.168.2.50 should still be banned
        assert!(cache.is_banned("192.168.2.50".parse().unwrap()));
        assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_expiry() {
        let mut cache = BanCache::new();
        let now = current_timestamp();

        // Add a permanent ban
        cache.add("192.168.1.100", None);

        // Add a ban that expires in the future
        cache.add("192.168.1.101", Some(now + 3600));

        // Add an already-expired ban
        cache.add("192.168.1.102", Some(now - 1));

        // Permanent and future bans should be active
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("192.168.1.101".parse().unwrap()));

        // Expired ban should not be active (rebuild happens on is_banned check)
        assert!(!cache.is_banned("192.168.1.102".parse().unwrap()));

        // Should have 2 entries after expired one is cleaned up
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_ban_cache_next_expiry() {
        let mut cache = BanCache::new();
        let now = current_timestamp();

        // All permanent - no next_expiry
        cache.add("192.168.1.100", None);
        assert!(cache.next_expiry.is_none());

        // Add timed ban
        cache.add("192.168.1.101", Some(now + 3600));
        assert_eq!(cache.next_expiry, Some(now + 3600));

        // Add earlier expiry
        cache.add("192.168.1.102", Some(now + 1800));
        assert_eq!(cache.next_expiry, Some(now + 1800));

        // Remove earlier one, next_expiry should update
        cache.remove("192.168.1.102");
        assert_eq!(cache.next_expiry, Some(now + 3600));

        // Remove last timed ban
        cache.remove("192.168.1.101");
        assert!(cache.next_expiry.is_none());
    }

    #[test]
    fn test_ban_cache_upsert() {
        let mut cache = BanCache::new();
        let now = current_timestamp();

        // Add permanent ban
        cache.add("192.168.1.100", None);
        assert_eq!(cache.len(), 1);
        assert!(cache.next_expiry.is_none());

        // Update to timed ban
        cache.add("192.168.1.100", Some(now + 3600));
        assert_eq!(cache.len(), 1); // Still 1 entry
        assert_eq!(cache.next_expiry, Some(now + 3600));
    }

    #[test]
    fn test_ban_cache_from_records() {
        let now = current_timestamp();
        let records = vec![
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

        let mut cache = BanCache::from_records(records);

        assert_eq!(cache.len(), 2);
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(cache.is_banned("10.0.0.1".parse().unwrap()));
        assert!(cache.is_banned("10.255.255.255".parse().unwrap()));
        assert!(!cache.is_banned("11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_ban_cache_mixed_ipv4_ipv6() {
        let mut cache = BanCache::new();

        cache.add("192.168.1.0/24", None);
        cache.add("2001:db8::/32", None);

        // IPv4 checks
        assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("192.168.2.100".parse().unwrap()));

        // IPv6 checks
        assert!(cache.is_banned("2001:db8::1".parse().unwrap()));
        assert!(!cache.is_banned("2001:db9::1".parse().unwrap()));

        // IPv4-mapped IPv6 should be normalized and match IPv4 ban
        assert!(cache.is_banned("::ffff:192.168.1.100".parse().unwrap()));
        assert!(!cache.is_banned("::ffff:192.168.2.100".parse().unwrap()));
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
}
