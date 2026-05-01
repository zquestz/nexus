//! In-memory registry of currently-registered Nexus servers.
//!
//! Each open tracker connection corresponds to exactly one entry, keyed
//! by a monotonically-increasing [`ConnectionId`]. The registry enforces
//! both a global capacity limit (`--max-entries`) and a per-source-IP
//! cap (`--max-entries-per-ip`); for each, `0` is reserved for
//! "unlimited".
//!
//! The registry is a dumb store. It accepts what the handler hands it
//! and sorts listings alphabetically by `name` (case-insensitive,
//! Unicode-aware). Address resolution (substituting the peer's IP when
//! `entry.address` is empty) and password validation live in the
//! handler — not here.
//!
//! ## Caller responsibilities
//!
//! - **Cross-task sharing:** [`Registry`] mutates through `&mut self`
//!   and holds no internal lock. The use site wraps it in
//!   `Arc<Mutex<Registry>>` so connection tasks can register / refresh /
//!   unregister and the listing handler can take a snapshot.
//! - **Stale eviction:** the registry does not track staleness; that's
//!   the connection task's job. Each connection in `connection.rs`
//!   reads frames with a `2 × refresh_interval` idle timeout, and on
//!   expiry the task drops, the drop guard fires, and the entry is
//!   unregistered. By construction, every entry is backed by a live
//!   connection, so a centralized stale-eviction worker would have
//!   nothing to do.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

use nexus_common::tracker_protocol::ServerEntry;

/// Identifier for a registered entry, assigned at register time.
pub type ConnectionId = u64;

/// A registered server entry plus the metadata the registry needs to
/// manage its lifecycle: `peer_ip` for cap accounting and logging, and
/// `last_refresh` for the per-entry refresh-floor check (rejects
/// refreshes that arrive faster than half the protocol minimum).
#[derive(Clone, Debug)]
pub struct RegisteredEntry {
    pub entry: ServerEntry,
    pub peer_ip: IpAddr,
    pub last_refresh: Instant,
}

/// Capacity-rejection variants from [`Registry::register`]. The handler
/// maps these to the protocol-level `error_kind` and a translated
/// human-readable message. Notably absent: `NotFound`, which `register`
/// can never produce; the unknown-id case for [`Registry::refresh`] is
/// reported via its `bool` return rather than as an error variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterError {
    /// Total tracker capacity (`--max-entries`) reached.
    Capacity,
    /// Per-source-IP cap (`--max-entries-per-ip`) reached.
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
    /// Construct a new registry with the given caps. Pass `0` for either
    /// cap to disable the corresponding limit.
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

    /// Register a fresh entry, returning the new `ConnectionId`.
    ///
    /// # Errors
    ///
    /// - [`RegisterError::Capacity`] if the global cap is reached.
    /// - [`RegisterError::PerIpCapacity`] if `peer_ip` is already at its
    ///   per-IP cap.
    pub fn register(
        &mut self,
        entry: ServerEntry,
        peer_ip: IpAddr,
        now: Instant,
    ) -> Result<ConnectionId, RegisterError> {
        if self.is_full() {
            return Err(RegisterError::Capacity);
        }
        if self.max_per_ip != 0 {
            let count = self.counts_by_ip.get(&peer_ip).copied().unwrap_or(0);
            if count >= self.max_per_ip {
                return Err(RegisterError::PerIpCapacity);
            }
        }

        let id = self.next_id;
        // 2^64 ids is many lifetimes of refreshes; wrap is theoretical.
        self.next_id = self.next_id.wrapping_add(1);

        self.entries.insert(
            id,
            RegisteredEntry {
                entry,
                peer_ip,
                last_refresh: now,
            },
        );
        *self.counts_by_ip.entry(peer_ip).or_insert(0) += 1;

        Ok(id)
    }

    /// Replace an existing entry's data and reset its `last_refresh`.
    ///
    /// Returns `true` if the entry was updated, `false` if `id` is not
    /// present (the entry was unregistered or stale-evicted out of
    /// band). Callers typically hold a drop guard that keeps the id
    /// alive for the connection's lifetime, so the `false` path is
    /// rarely reached — but the registry doesn't enforce that
    /// convention, so we report rather than panic.
    pub fn refresh(&mut self, id: ConnectionId, entry: ServerEntry, now: Instant) -> bool {
        if let Some(registered) = self.entries.get_mut(&id) {
            registered.entry = entry;
            registered.last_refresh = now;
            true
        } else {
            false
        }
    }

    /// Read-only peek at an entry's `last_refresh`. Returns `None` if
    /// `id` is not registered. Used by the refresh handler to enforce
    /// the per-entry floor before paying for password verification.
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

    /// Snapshot of all entries, sorted alphabetically by `name`
    /// (case-insensitive ascending). Order among entries with equal
    /// names is unspecified.
    #[must_use]
    pub fn list(&self) -> Vec<ServerEntry> {
        let mut out: Vec<ServerEntry> = self.entries.values().map(|r| r.entry.clone()).collect();
        out.sort_by_key(|e| e.name.to_lowercase());
        out
    }

    /// Number of registered entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no entries are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `true` when the global cap has been reached. Always `false` when
    /// `max_entries == 0` (unlimited).
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.max_entries != 0 && (self.entries.len() as u32) >= self.max_entries
    }

    fn decrement_ip_count(&mut self, peer_ip: &IpAddr) {
        if let Some(count) = self.counts_by_ip.get_mut(peer_ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.counts_by_ip.remove(peer_ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    /// Build a `ServerEntry` for tests with sensible defaults; the only
    /// field tests typically vary is `name` (for sort tests) or
    /// `user_count` (for refresh tests).
    fn make_entry(name: &str) -> ServerEntry {
        ServerEntry {
            name: name.to_string(),
            description: None,
            address: "bbs.example.com".to_string(),
            port: 7500,
            websocket_port: None,
            version: "0.8.1".to_string(),
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
        // Second register from same IP exceeds the per-IP cap of 1.
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

        // last_refresh was reset to t1 (used by the refresh-floor check).
        assert_eq!(r.last_refresh(id), Some(t1));

        // And the user_count update is reflected in the listing.
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
        // Regression guard: a `register` rejected for `PerIpCapacity`
        // must not have bumped the IP's count. If the increment moves
        // ahead of the cap check in a future refactor, this test fails
        // because after `unregister(id1)` the slot wouldn't be free.
        let mut r = Registry::new(0, 1);
        let now = Instant::now();
        let id1 = r.register(make_entry("First"), ip(1), now).unwrap();
        // Attempt a second register that must be rejected.
        let err = r.register(make_entry("Second"), ip(1), now).unwrap_err();
        assert_eq!(err, RegisterError::PerIpCapacity);
        // After unregistering the first, the slot must be free —
        // i.e., the rejected register did NOT leak a +1 into the count.
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
        // Mix scripts; the Unicode-aware lowercase compare orders them
        // by codepoint after lowercasing.
        r.register(make_entry("Æther"), ip(1), now).unwrap();
        r.register(make_entry("álpha"), ip(2), now).unwrap();
        r.register(make_entry("zeta"), ip(3), now).unwrap();

        let list = r.list();
        // Just confirm we got all three without panicking — the exact
        // collation order for cross-script Unicode isn't worth pinning.
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
