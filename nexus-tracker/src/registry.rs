//! In-memory registry of currently-registered Nexus servers.
//!
//! One entry per open tracker connection, keyed by [`ConnectionId`].
//! Enforces a global cap (`--max-entries`) and per-source cap
//! (`--max-entries-per-ip`, keyed by IPv4 address or IPv6 /64); `0`
//! means unlimited for either.
//!
//! Holds no lock; the use site wraps it in `Arc<Mutex<Registry>>`.
//! Staleness is not tracked here: each connection reads frames with a
//! `2 × refresh_interval` idle timeout and unregisters via drop guard on
//! expiry, so every entry is backed by a live connection.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

use nexus_common::address::ipv6_slash_64_bucket_key;
use nexus_common::names::fold_name;
use nexus_common::tracker_protocol::ServerEntry;

/// Identifier for a registered entry, assigned at register time.
pub type ConnectionId = u64;

/// A registered entry plus lifecycle metadata: `peer_ip` for cap
/// accounting, `last_refresh` for the per-entry refresh-floor check.
#[derive(Clone, Debug)]
pub struct RegisteredEntry {
    pub entry: ServerEntry,
    pub peer_ip: IpAddr,
    pub last_refresh: Instant,
}

/// Capacity-rejection variants from [`Registry::register`]; the handler
/// maps each to a protocol `error_kind` plus translated message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterError {
    /// Total tracker capacity (`--max-entries`) reached.
    Capacity,
    /// Per-source cap (`--max-entries-per-ip`) reached.
    PerIpCapacity,
}

#[derive(Debug)]
pub struct Registry {
    entries: HashMap<ConnectionId, RegisteredEntry>,
    next_id: ConnectionId,
    max_entries: u32,
    max_per_ip: u32,
    counts_by_ip: HashMap<IpAddr, u32>,
}

impl Registry {
    /// Construct a registry with the given caps (`0` disables a cap).
    #[must_use]
    pub fn new(max_entries: u32, max_per_ip: u32) -> Self {
        Self {
            entries: HashMap::new(),
            next_id: 1,
            max_entries,
            max_per_ip,
            counts_by_ip: HashMap::new(),
        }
    }

    /// Register a fresh entry, returning the new `ConnectionId`. Errors
    /// when the global or per-source cap is reached.
    pub fn register(
        &mut self,
        entry: ServerEntry,
        peer_ip: IpAddr,
        now: Instant,
    ) -> Result<ConnectionId, RegisterError> {
        if self.is_full() {
            return Err(RegisterError::Capacity);
        }
        let ip_bucket = ipv6_slash_64_bucket_key(peer_ip);
        if self.max_per_ip != 0 {
            let count = self.counts_by_ip.get(&ip_bucket).copied().unwrap_or(0);
            if count >= self.max_per_ip {
                return Err(RegisterError::PerIpCapacity);
            }
        }

        let id = self.next_id;
        // 2^64 ids ~= 584 billion years at 1/sec; wrap is theoretical.
        self.next_id = self.next_id.wrapping_add(1);

        self.entries.insert(
            id,
            RegisteredEntry {
                entry,
                peer_ip,
                last_refresh: now,
            },
        );
        *self.counts_by_ip.entry(ip_bucket).or_insert(0) += 1;

        Ok(id)
    }

    /// Replace an existing entry's data and reset its `last_refresh`.
    /// Returns `false` (rather than panicking) if `id` is gone — it may
    /// have been unregistered out of band.
    pub fn refresh(&mut self, id: ConnectionId, entry: ServerEntry, now: Instant) -> bool {
        if let Some(registered) = self.entries.get_mut(&id) {
            registered.entry = entry;
            registered.last_refresh = now;
            true
        } else {
            false
        }
    }

    /// An entry's `last_refresh`, or `None` if unregistered. The refresh
    /// handler checks this floor before paying for password verification.
    #[must_use]
    pub fn last_refresh(&self, id: ConnectionId) -> Option<Instant> {
        self.entries.get(&id).map(|r| r.last_refresh)
    }

    /// Remove an entry. Idempotent — calling on an unknown id is a no-op.
    pub fn unregister(&mut self, id: ConnectionId) {
        if let Some(removed) = self.entries.remove(&id) {
            self.decrement_ip_count(&removed.peer_ip);
        }
    }

    /// Snapshot of all entries, sorted by `name` (case-insensitive
    /// ascending). Order among equal names is unspecified.
    #[must_use]
    pub fn list(&self) -> Vec<ServerEntry> {
        let mut out: Vec<ServerEntry> = self.entries.values().map(|r| r.entry.clone()).collect();
        out.sort_by_cached_key(|e| fold_name(&e.name));
        out
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `true` when the global cap is reached; always `false` when
    /// unlimited (`max_entries == 0`).
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.max_entries != 0 && self.entries.len() as u64 >= u64::from(self.max_entries)
    }

    fn decrement_ip_count(&mut self, peer_ip: &IpAddr) {
        let ip_bucket = ipv6_slash_64_bucket_key(*peer_ip);
        if let Some(count) = self.counts_by_ip.get_mut(&ip_bucket) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.counts_by_ip.remove(&ip_bucket);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::time::Duration;

    /// Build a `ServerEntry` for tests; vary `name` or `user_count`.
    fn make_entry(name: &str) -> ServerEntry {
        ServerEntry {
            name: name.to_string(),
            description: None,
            address: "bbs.example.com".to_string(),
            port: 7500,
            websocket_port: None,
            version: "0.8.6".to_string(),
            fingerprint: "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
                 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"
                .to_string(),
            user_count: 0,
            allows_guest: true,
        }
    }

    fn ip(octet: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, octet))
    }

    fn ipv6(segment4: u16, host: u16) -> IpAddr {
        IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x0db8, 0x1234, segment4, 0, 0, 0, host,
        ))
    }

    #[test]
    fn test_register_assigns_increasing_ids() {
        let mut r = Registry::new(0, 0);
        let now = Instant::now();
        let id1 = r.register(make_entry("Alpha"), ip(1), now).unwrap();
        let id2 = r.register(make_entry("Beta"), ip(2), now).unwrap();
        let id3 = r.register(make_entry("Gamma"), ip(3), now).unwrap();
        assert!(id2 > id1);
        assert!(id3 > id2);
    }

    #[test]
    fn test_register_over_total_capacity_returns_capacity() {
        let mut r = Registry::new(2, 0);
        let now = Instant::now();
        r.register(make_entry("A"), ip(1), now).unwrap();
        r.register(make_entry("B"), ip(2), now).unwrap();
        let err = r.register(make_entry("C"), ip(3), now).unwrap_err();
        assert_eq!(err, RegisterError::Capacity);
    }

    #[test]
    fn test_register_over_per_ip_capacity_returns_per_ip_capacity() {
        let mut r = Registry::new(0, 1);
        let now = Instant::now();
        r.register(make_entry("First"), ip(1), now).unwrap();
        // Second register from same IP exceeds the per-source cap of 1.
        let err = r.register(make_entry("Second"), ip(1), now).unwrap_err();
        assert_eq!(err, RegisterError::PerIpCapacity);
    }

    #[test]
    fn test_register_with_max_entries_zero_is_unlimited() {
        let mut r = Registry::new(0, 0);
        let now = Instant::now();
        // 100 entries from 100 distinct IPs all succeed.
        for i in 0..100u8 {
            r.register(make_entry("S"), ip(i), now).unwrap();
        }
        assert_eq!(r.len(), 100);
    }

    #[test]
    fn test_register_with_max_per_ip_zero_is_unlimited_per_ip() {
        let mut r = Registry::new(0, 0);
        let now = Instant::now();
        // 50 entries all from the same IP succeed.
        for _ in 0..50 {
            r.register(make_entry("S"), ip(1), now).unwrap();
        }
        assert_eq!(r.len(), 50);
    }

    #[test]
    fn test_per_ip_cap_is_independent_across_ips() {
        let mut r = Registry::new(0, 1);
        let now = Instant::now();
        // Each of 5 distinct IPs can register exactly once.
        for i in 0..5u8 {
            r.register(make_entry("S"), ip(i), now).unwrap();
        }
        // But a sixth register from any of those IPs is rejected.
        let err = r.register(make_entry("Dup"), ip(0), now).unwrap_err();
        assert_eq!(err, RegisterError::PerIpCapacity);
    }

    #[test]
    fn test_per_ip_cap_shares_ipv6_slash_64() {
        let mut r = Registry::new(0, 1);
        let now = Instant::now();

        r.register(make_entry("First"), ipv6(0x5678, 1), now)
            .unwrap();
        let err = r
            .register(make_entry("Second"), ipv6(0x5678, 2), now)
            .unwrap_err();
        assert_eq!(err, RegisterError::PerIpCapacity);

        r.register(make_entry("Other prefix"), ipv6(0x5679, 1), now)
            .expect("different /64 should have an independent cap bucket");
    }

    #[test]
    fn test_unregister_frees_ipv6_slash_64_slot() {
        let mut r = Registry::new(0, 1);
        let now = Instant::now();

        let id = r
            .register(make_entry("First"), ipv6(0x5678, 1), now)
            .unwrap();
        assert_eq!(
            r.register(make_entry("Second"), ipv6(0x5678, 2), now)
                .unwrap_err(),
            RegisterError::PerIpCapacity
        );

        r.unregister(id);
        r.register(make_entry("Replacement"), ipv6(0x5678, 3), now)
            .expect("unregister should free the shared /64 bucket");
    }

    #[test]
    fn test_refresh_updates_entry_and_resets_last_refresh() {
        let mut r = Registry::new(0, 0);
        let t0 = Instant::now();
        let id = r
            .register(
                ServerEntry {
                    user_count: 5,
                    ..make_entry("S")
                },
                ip(1),
                t0,
            )
            .unwrap();

        let t1 = t0 + Duration::from_secs(60);
        r.refresh(
            id,
            ServerEntry {
                user_count: 12,
                ..make_entry("S")
            },
            t1,
        );

        assert_eq!(r.last_refresh(id), Some(t1));

        // user_count update is reflected in the listing.
        let list = r.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].user_count, 12);
    }

    #[test]
    fn test_refresh_unknown_id_returns_false() {
        let mut r = Registry::new(0, 0);
        let updated = r.refresh(999, make_entry("S"), Instant::now());
        assert!(!updated, "refresh on unknown id should return false");
    }

    #[test]
    fn test_unregister_is_idempotent() {
        let mut r = Registry::new(0, 0);
        let id = r.register(make_entry("S"), ip(1), Instant::now()).unwrap();
        r.unregister(id);
        // Calling again is a no-op, not a panic.
        r.unregister(id);
        // And on an id that never existed.
        r.unregister(424242);
        assert!(r.is_empty());
    }

    #[test]
    fn test_failed_register_does_not_increment_per_ip_count() {
        // Regression guard: a register rejected for PerIpCapacity must
        // not bump the IP count, else the slot wouldn't free after
        // unregister(id1).
        let mut r = Registry::new(0, 1);
        let now = Instant::now();
        let id1 = r.register(make_entry("First"), ip(1), now).unwrap();
        let err = r.register(make_entry("Second"), ip(1), now).unwrap_err();
        assert_eq!(err, RegisterError::PerIpCapacity);
        r.unregister(id1);
        r.register(make_entry("Replacement"), ip(1), now)
            .expect("rejected register must not have bumped per-IP count");
    }

    #[test]
    fn test_unregister_frees_per_ip_slot() {
        let mut r = Registry::new(0, 1);
        let now = Instant::now();
        let id = r.register(make_entry("First"), ip(1), now).unwrap();
        // Same IP at cap.
        assert_eq!(
            r.register(make_entry("Second"), ip(1), now).unwrap_err(),
            RegisterError::PerIpCapacity
        );
        r.unregister(id);
        // After unregister the slot is free again.
        r.register(make_entry("Replacement"), ip(1), now)
            .expect("slot should be free after unregister");
    }

    #[test]
    fn test_list_returns_alphabetically_sorted_case_insensitive() {
        let mut r = Registry::new(0, 0);
        let now = Instant::now();
        r.register(make_entry("zeta BBS"), ip(1), now).unwrap();
        r.register(make_entry("Alpha BBS"), ip(2), now).unwrap();
        r.register(make_entry("middle BBS"), ip(3), now).unwrap();

        let list = r.list();
        let names: Vec<_> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha BBS", "middle BBS", "zeta BBS"]);
    }

    #[test]
    fn test_list_handles_unicode_names() {
        let mut r = Registry::new(0, 0);
        let now = Instant::now();
        r.register(make_entry("Æther"), ip(1), now).unwrap();
        r.register(make_entry("álpha"), ip(2), now).unwrap();
        r.register(make_entry("zeta"), ip(3), now).unwrap();

        let list = r.list();
        // Confirm all three without panicking; cross-script collation
        // order isn't worth pinning.
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_list_empty_returns_empty_vec() {
        let r = Registry::new(0, 0);
        assert!(r.list().is_empty());
    }

    #[test]
    fn test_is_full_and_len_reflect_state() {
        let mut r = Registry::new(2, 0);
        let now = Instant::now();
        assert_eq!(r.len(), 0);
        assert!(!r.is_full());

        r.register(make_entry("A"), ip(1), now).unwrap();
        assert_eq!(r.len(), 1);
        assert!(!r.is_full());

        r.register(make_entry("B"), ip(2), now).unwrap();
        assert_eq!(r.len(), 2);
        assert!(r.is_full(), "at capacity");

        // is_full stays true on attempted-but-rejected register.
        let _ = r.register(make_entry("C"), ip(3), now);
        assert!(r.is_full());
    }

    #[test]
    fn test_unlimited_global_cap_is_never_full() {
        let r = Registry::new(0, 0);
        assert!(!r.is_full());
        // Even after many adds.
        let mut r = r;
        let now = Instant::now();
        for i in 0..50u8 {
            r.register(make_entry("S"), ip(i), now).unwrap();
        }
        assert!(!r.is_full());
    }

    #[test]
    fn test_last_refresh_returns_registered_instant() {
        let mut r = Registry::new(0, 0);
        let t0 = Instant::now();
        let id = r.register(make_entry("X"), ip(1), t0).unwrap();
        assert_eq!(r.last_refresh(id), Some(t0));

        // refresh() updates last_refresh.
        let t1 = t0 + Duration::from_secs(10);
        r.refresh(id, make_entry("X"), t1);
        assert_eq!(r.last_refresh(id), Some(t1));
    }

    #[test]
    fn test_last_refresh_returns_none_for_unknown_id() {
        let r = Registry::new(0, 0);
        assert_eq!(r.last_refresh(999), None);
    }
}
