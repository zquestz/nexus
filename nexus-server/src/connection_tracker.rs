//! Per-IP connection limiting for DoS protection.
//!
//! Main BBS and file-transfer connections each have their own configurable
//! limit; voice connections are counted separately but share the main BBS
//! limit value (one voice stream per connected user from an IP).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::constants::{
    ERR_CONNECTION_TRACKER_LOCK, ERR_TRANSFER_TRACKER_LOCK, ERR_VOICE_TRACKER_LOCK,
};

/// Per-IP connection limiting. A limit of 0 means unlimited.
#[derive(Debug)]
pub struct ConnectionTracker {
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// 0 = unlimited.
    max_connections_per_ip: AtomicUsize,
    transfer_connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// 0 = unlimited.
    max_transfers_per_ip: AtomicUsize,
    /// Capped against `max_connections_per_ip` (shared value, separate count).
    voice_connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl ConnectionTracker {
    /// Create a tracker with the given per-IP limits (0 = unlimited).
    #[must_use]
    pub fn new(max_connections_per_ip: usize, max_transfers_per_ip: usize) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            max_connections_per_ip: AtomicUsize::new(max_connections_per_ip),
            transfer_connections: Arc::new(Mutex::new(HashMap::new())),
            max_transfers_per_ip: AtomicUsize::new(max_transfers_per_ip),
            voice_connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Set the per-IP main limit (0 = unlimited). Affects new connections
    /// only; existing connections are not disconnected.
    pub fn set_max_connections_per_ip(&self, limit: usize) {
        self.max_connections_per_ip.store(limit, Ordering::Relaxed);
    }

    /// Set the per-IP transfer limit (0 = unlimited). Affects new connections
    /// only; existing connections are not disconnected.
    pub fn set_max_transfers_per_ip(&self, limit: usize) {
        self.max_transfers_per_ip.store(limit, Ordering::Relaxed);
    }

    /// Acquire a main connection slot, or `None` if the IP is at its limit.
    /// The returned guard releases the slot on drop.
    pub fn try_acquire(&self, ip: IpAddr) -> Option<ConnectionGuard> {
        let max = self.max_connections_per_ip.load(Ordering::Relaxed);
        let mut connections = self.connections.lock().expect(ERR_CONNECTION_TRACKER_LOCK);
        let count = connections.entry(ip).or_insert(0);

        // 0 means unlimited
        if max > 0 && *count >= max {
            return None;
        }

        *count += 1;
        Some(ConnectionGuard {
            ip,
            connections: self.connections.clone(),
        })
    }

    /// Acquire a transfer slot, or `None` if the IP is at its transfer limit.
    /// The returned guard releases the slot on drop.
    pub fn try_acquire_transfer(&self, ip: IpAddr) -> Option<TransferGuard> {
        let max = self.max_transfers_per_ip.load(Ordering::Relaxed);
        let mut connections = self
            .transfer_connections
            .lock()
            .expect(ERR_TRANSFER_TRACKER_LOCK);
        let count = connections.entry(ip).or_insert(0);

        // 0 means unlimited
        if max > 0 && *count >= max {
            return None;
        }

        *count += 1;
        Some(TransferGuard {
            ip,
            connections: self.transfer_connections.clone(),
        })
    }

    /// Acquire a voice slot, or `None` if the IP is at its limit. Capped
    /// against `max_connections_per_ip` but counted separately, so a per-IP
    /// BBS limit of N also allows N voice streams. Guard releases on drop.
    pub fn try_acquire_voice(&self, ip: IpAddr) -> Option<VoiceGuard> {
        let max = self.max_connections_per_ip.load(Ordering::Relaxed);
        let mut connections = self.voice_connections.lock().expect(ERR_VOICE_TRACKER_LOCK);
        let count = connections.entry(ip).or_insert(0);

        // 0 means unlimited
        if max > 0 && *count >= max {
            return None;
        }

        *count += 1;
        Some(VoiceGuard {
            ip,
            connections: self.voice_connections.clone(),
        })
    }
}

/// RAII guard releasing a main connection slot on drop (even on panic or
/// early return).
#[derive(Debug)]
pub struct ConnectionGuard {
    ip: IpAddr,
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let mut connections = self.connections.lock().expect(ERR_CONNECTION_TRACKER_LOCK);
        if let Some(count) = connections.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(&self.ip);
            }
        }
    }
}

/// RAII guard releasing a transfer connection slot on drop (even on panic or
/// early return).
#[derive(Debug)]
pub struct TransferGuard {
    ip: IpAddr,
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        let mut connections = self.connections.lock().expect(ERR_TRANSFER_TRACKER_LOCK);
        if let Some(count) = connections.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(&self.ip);
            }
        }
    }
}

/// RAII guard that releases a voice connection slot when dropped.
#[derive(Debug)]
pub struct VoiceGuard {
    ip: IpAddr,
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl Drop for VoiceGuard {
    fn drop(&mut self) {
        let mut connections = self.connections.lock().expect(ERR_VOICE_TRACKER_LOCK);
        if let Some(count) = connections.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(&self.ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    impl ConnectionTracker {
        fn max_connections_per_ip(&self) -> usize {
            self.max_connections_per_ip.load(Ordering::Relaxed)
        }

        fn max_transfers_per_ip(&self) -> usize {
            self.max_transfers_per_ip.load(Ordering::Relaxed)
        }

        fn connection_count(&self, ip: IpAddr) -> usize {
            let connections = self.connections.lock().expect(ERR_CONNECTION_TRACKER_LOCK);
            connections.get(&ip).copied().unwrap_or(0)
        }

        fn transfer_count(&self, ip: IpAddr) -> usize {
            let connections = self
                .transfer_connections
                .lock()
                .expect(ERR_TRANSFER_TRACKER_LOCK);
            connections.get(&ip).copied().unwrap_or(0)
        }

        fn voice_count(&self, ip: IpAddr) -> usize {
            let connections = self.voice_connections.lock().expect(ERR_VOICE_TRACKER_LOCK);
            connections.get(&ip).copied().unwrap_or(0)
        }

        fn total_connections(&self) -> usize {
            let connections = self.connections.lock().expect(ERR_CONNECTION_TRACKER_LOCK);
            connections.values().sum()
        }

        fn total_transfers(&self) -> usize {
            let connections = self
                .transfer_connections
                .lock()
                .expect(ERR_TRANSFER_TRACKER_LOCK);
            connections.values().sum()
        }
    }

    #[test]
    fn test_acquire_and_release() {
        let tracker = ConnectionTracker::new(2, 3);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Should be able to acquire up to the limit
        let guard1 = tracker.try_acquire(ip);
        assert!(guard1.is_some());
        assert_eq!(tracker.connection_count(ip), 1);

        let guard2 = tracker.try_acquire(ip);
        assert!(guard2.is_some());
        assert_eq!(tracker.connection_count(ip), 2);

        // Should be rejected at the limit
        let guard3 = tracker.try_acquire(ip);
        assert!(guard3.is_none());
        assert_eq!(tracker.connection_count(ip), 2);

        // Drop one guard and try again
        drop(guard1);
        assert_eq!(tracker.connection_count(ip), 1);

        let guard3 = tracker.try_acquire(ip);
        assert!(guard3.is_some());
        assert_eq!(tracker.connection_count(ip), 2);
    }

    #[test]
    fn test_different_ips_independent() {
        let tracker = ConnectionTracker::new(1, 1);
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        // Each IP should have its own limit
        let guard1 = tracker.try_acquire(ip1);
        assert!(guard1.is_some());

        let guard2 = tracker.try_acquire(ip2);
        assert!(guard2.is_some());

        // ip1 is at limit
        let guard3 = tracker.try_acquire(ip1);
        assert!(guard3.is_none());

        // ip2 is also at limit
        let guard4 = tracker.try_acquire(ip2);
        assert!(guard4.is_none());

        assert_eq!(tracker.total_connections(), 2);
    }

    #[test]
    fn test_total_connections() {
        let tracker = ConnectionTracker::new(5, 5);
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        assert_eq!(tracker.total_connections(), 0);

        let _g1 = tracker.try_acquire(ip1).unwrap();
        let _g2 = tracker.try_acquire(ip1).unwrap();
        let _g3 = tracker.try_acquire(ip2).unwrap();

        assert_eq!(tracker.total_connections(), 3);
        assert_eq!(tracker.connection_count(ip1), 2);
        assert_eq!(tracker.connection_count(ip2), 1);
    }

    #[test]
    fn test_cleanup_on_zero() {
        let tracker = ConnectionTracker::new(2, 2);
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));

        let guard = tracker.try_acquire(ip).unwrap();
        assert_eq!(tracker.connection_count(ip), 1);

        drop(guard);

        // IP should be removed from the map when count reaches 0
        assert_eq!(tracker.connection_count(ip), 0);
        let connections = tracker
            .connections
            .lock()
            .expect(ERR_CONNECTION_TRACKER_LOCK);
        assert!(!connections.contains_key(&ip));
    }

    #[test]
    fn test_unlimited_when_zero() {
        let tracker = ConnectionTracker::new(0, 0);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Should be able to acquire many connections when limit is 0 (unlimited)
        let mut guards = Vec::new();
        for _ in 0..100 {
            let guard = tracker.try_acquire(ip);
            assert!(
                guard.is_some(),
                "unlimited should allow any number of connections"
            );
            guards.push(guard);
        }

        assert_eq!(tracker.connection_count(ip), 100);
    }

    #[test]
    fn test_set_max_connections_per_ip() {
        let tracker = ConnectionTracker::new(2, 2);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        assert_eq!(tracker.max_connections_per_ip(), 2);

        // Acquire up to limit
        let _g1 = tracker.try_acquire(ip).unwrap();
        let _g2 = tracker.try_acquire(ip).unwrap();
        assert!(tracker.try_acquire(ip).is_none());

        // Increase limit
        tracker.set_max_connections_per_ip(3);
        assert_eq!(tracker.max_connections_per_ip(), 3);

        // Now we can acquire one more
        let _g3 = tracker.try_acquire(ip).unwrap();
        assert!(tracker.try_acquire(ip).is_none());
    }

    #[test]
    fn test_set_limit_to_unlimited() {
        let tracker = ConnectionTracker::new(1, 1);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // At limit
        let _g1 = tracker.try_acquire(ip).unwrap();
        assert!(tracker.try_acquire(ip).is_none());

        // Set to unlimited
        tracker.set_max_connections_per_ip(0);

        // Now unlimited
        let _g2 = tracker.try_acquire(ip).unwrap();
        let _g3 = tracker.try_acquire(ip).unwrap();
        assert_eq!(tracker.connection_count(ip), 3);
    }

    #[test]
    fn test_set_limit_lower_does_not_disconnect() {
        let tracker = ConnectionTracker::new(5, 5);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Acquire 3 connections
        let _g1 = tracker.try_acquire(ip).unwrap();
        let _g2 = tracker.try_acquire(ip).unwrap();
        let _g3 = tracker.try_acquire(ip).unwrap();
        assert_eq!(tracker.connection_count(ip), 3);

        // Lower limit to 1
        tracker.set_max_connections_per_ip(1);

        // Existing connections are not affected
        assert_eq!(tracker.connection_count(ip), 3);

        // But new connections are rejected
        assert!(tracker.try_acquire(ip).is_none());
    }

    #[test]
    fn test_transfer_acquire_and_release() {
        let tracker = ConnectionTracker::new(5, 2);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Should be able to acquire up to the transfer limit
        let guard1 = tracker.try_acquire_transfer(ip);
        assert!(guard1.is_some());
        assert_eq!(tracker.transfer_count(ip), 1);

        let guard2 = tracker.try_acquire_transfer(ip);
        assert!(guard2.is_some());
        assert_eq!(tracker.transfer_count(ip), 2);

        // Should be rejected at the limit
        let guard3 = tracker.try_acquire_transfer(ip);
        assert!(guard3.is_none());
        assert_eq!(tracker.transfer_count(ip), 2);

        // Drop one guard and try again
        drop(guard1);
        assert_eq!(tracker.transfer_count(ip), 1);

        let guard3 = tracker.try_acquire_transfer(ip);
        assert!(guard3.is_some());
        assert_eq!(tracker.transfer_count(ip), 2);
    }

    #[test]
    fn test_transfer_different_ips_independent() {
        let tracker = ConnectionTracker::new(5, 1);
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        let guard1 = tracker.try_acquire_transfer(ip1);
        assert!(guard1.is_some());

        let guard2 = tracker.try_acquire_transfer(ip2);
        assert!(guard2.is_some());

        // Both at limit
        assert!(tracker.try_acquire_transfer(ip1).is_none());
        assert!(tracker.try_acquire_transfer(ip2).is_none());

        assert_eq!(tracker.total_transfers(), 2);
    }

    #[test]
    fn test_transfer_cleanup_on_zero() {
        let tracker = ConnectionTracker::new(5, 2);
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));

        let guard = tracker.try_acquire_transfer(ip).unwrap();
        assert_eq!(tracker.transfer_count(ip), 1);

        drop(guard);

        assert_eq!(tracker.transfer_count(ip), 0);
        let connections = tracker
            .transfer_connections
            .lock()
            .expect(ERR_TRANSFER_TRACKER_LOCK);
        assert!(!connections.contains_key(&ip));
    }

    #[test]
    fn test_transfer_unlimited_when_zero() {
        let tracker = ConnectionTracker::new(5, 0);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        let mut guards = Vec::new();
        for _ in 0..100 {
            let guard = tracker.try_acquire_transfer(ip);
            assert!(
                guard.is_some(),
                "unlimited should allow any number of transfers"
            );
            guards.push(guard);
        }

        assert_eq!(tracker.transfer_count(ip), 100);
    }

    #[test]
    fn test_set_max_transfers_per_ip() {
        let tracker = ConnectionTracker::new(5, 2);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        assert_eq!(tracker.max_transfers_per_ip(), 2);

        let _g1 = tracker.try_acquire_transfer(ip).unwrap();
        let _g2 = tracker.try_acquire_transfer(ip).unwrap();
        assert!(tracker.try_acquire_transfer(ip).is_none());

        tracker.set_max_transfers_per_ip(3);
        assert_eq!(tracker.max_transfers_per_ip(), 3);

        let _g3 = tracker.try_acquire_transfer(ip).unwrap();
        assert!(tracker.try_acquire_transfer(ip).is_none());
    }

    #[test]
    fn test_connection_and_transfer_limits_independent() {
        let tracker = ConnectionTracker::new(2, 3);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Fill up main connections
        let _c1 = tracker.try_acquire(ip).unwrap();
        let _c2 = tracker.try_acquire(ip).unwrap();
        assert!(tracker.try_acquire(ip).is_none());

        // Should still be able to acquire transfers
        let _t1 = tracker.try_acquire_transfer(ip).unwrap();
        let _t2 = tracker.try_acquire_transfer(ip).unwrap();
        let _t3 = tracker.try_acquire_transfer(ip).unwrap();
        assert!(tracker.try_acquire_transfer(ip).is_none());

        assert_eq!(tracker.connection_count(ip), 2);
        assert_eq!(tracker.transfer_count(ip), 3);
        assert_eq!(tracker.total_connections(), 2);
        assert_eq!(tracker.total_transfers(), 3);
    }

    #[test]
    fn test_limits_are_stored_correctly() {
        let tracker = ConnectionTracker::new(5, 3);
        assert_eq!(tracker.max_connections_per_ip(), 5);
        assert_eq!(tracker.max_transfers_per_ip(), 3);
    }

    #[test]
    fn test_voice_acquire_and_release() {
        let tracker = ConnectionTracker::new(2, 5);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        let guard1 = tracker.try_acquire_voice(ip);
        assert!(guard1.is_some());
        assert_eq!(tracker.voice_count(ip), 1);

        let guard2 = tracker.try_acquire_voice(ip);
        assert!(guard2.is_some());
        assert_eq!(tracker.voice_count(ip), 2);

        // At the (BBS) limit of 2
        assert!(tracker.try_acquire_voice(ip).is_none());

        drop(guard1);
        assert_eq!(tracker.voice_count(ip), 1);
        assert!(tracker.try_acquire_voice(ip).is_some());
    }

    #[test]
    fn test_voice_uses_main_limit_but_counts_separately() {
        let tracker = ConnectionTracker::new(1, 5);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Fill the single main slot.
        let _c = tracker.try_acquire(ip).unwrap();
        assert!(tracker.try_acquire(ip).is_none());

        // Voice has its own count against the same limit value, so a
        // voice slot is still available even though BBS is full.
        let _v = tracker.try_acquire_voice(ip).unwrap();
        assert!(tracker.try_acquire_voice(ip).is_none());

        assert_eq!(tracker.connection_count(ip), 1);
        assert_eq!(tracker.voice_count(ip), 1);
    }

    #[test]
    fn test_voice_unlimited_when_zero() {
        let tracker = ConnectionTracker::new(0, 5);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        let mut guards = Vec::new();
        for _ in 0..100 {
            let guard = tracker.try_acquire_voice(ip);
            assert!(guard.is_some(), "unlimited should allow any number");
            guards.push(guard);
        }
        assert_eq!(tracker.voice_count(ip), 100);
    }

    #[test]
    fn test_voice_cleanup_on_zero() {
        let tracker = ConnectionTracker::new(2, 5);
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));

        let guard = tracker.try_acquire_voice(ip).unwrap();
        assert_eq!(tracker.voice_count(ip), 1);

        drop(guard);
        assert_eq!(tracker.voice_count(ip), 0);
        let connections = tracker
            .voice_connections
            .lock()
            .expect(ERR_VOICE_TRACKER_LOCK);
        assert!(!connections.contains_key(&ip));
    }
}
