//! Connection tracking for DoS protection
//!
//! This module provides connection limiting per IP address to prevent
//! resource exhaustion attacks. It tracks both main BBS connections
//! and file transfer connections with separate limits.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Tracks active connections per IP address for both main and transfer connections
///
/// This is used to enforce connection limits and prevent a single IP
/// from exhausting server resources.
///
/// A limit of 0 means unlimited connections are allowed.
#[derive(Debug)]
pub struct ConnectionTracker {
    /// Map of IP addresses to their current main connection count
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Maximum main connections allowed per IP (0 = unlimited)
    max_connections_per_ip: AtomicUsize,
    /// Map of IP addresses to their current transfer connection count
    transfer_connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Maximum transfer connections allowed per IP (0 = unlimited)
    max_transfers_per_ip: AtomicUsize,
}

impl ConnectionTracker {
    /// Create a new connection tracker with the specified limits
    ///
    /// A limit of 0 means unlimited connections are allowed for that type.
    #[must_use]
    pub fn new(max_connections_per_ip: usize, max_transfers_per_ip: usize) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            max_connections_per_ip: AtomicUsize::new(max_connections_per_ip),
            transfer_connections: Arc::new(Mutex::new(HashMap::new())),
            max_transfers_per_ip: AtomicUsize::new(max_transfers_per_ip),
        }
    }

    /// Update the maximum main connections allowed per IP
    ///
    /// This affects new connections only; existing connections are not disconnected.
    /// A limit of 0 means unlimited connections are allowed.
    pub fn set_max_connections_per_ip(&self, limit: usize) {
        self.max_connections_per_ip.store(limit, Ordering::Relaxed);
    }

    /// Update the maximum transfer connections allowed per IP
    ///
    /// This affects new connections only; existing connections are not disconnected.
    /// A limit of 0 means unlimited connections are allowed.
    pub fn set_max_transfers_per_ip(&self, limit: usize) {
        self.max_transfers_per_ip.store(limit, Ordering::Relaxed);
    }

    /// Try to acquire a main connection slot for the given IP
    ///
    /// Returns `Some(ConnectionGuard)` if the connection is allowed,
    /// or `None` if the IP has reached its connection limit.
    ///
    /// The returned guard will automatically release the slot when dropped.
    pub fn try_acquire(&self, ip: IpAddr) -> Option<ConnectionGuard> {
        let max = self.max_connections_per_ip.load(Ordering::Relaxed);
        let mut connections = self.connections.lock().expect("connection tracker lock");
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

    /// Try to acquire a transfer connection slot for the given IP
    ///
    /// Returns `Some(TransferGuard)` if the connection is allowed,
    /// or `None` if the IP has reached its transfer limit.
    ///
    /// The returned guard will automatically release the slot when dropped.
    pub fn try_acquire_transfer(&self, ip: IpAddr) -> Option<TransferGuard> {
        let max = self.max_transfers_per_ip.load(Ordering::Relaxed);
        let mut connections = self
            .transfer_connections
            .lock()
            .expect("transfer tracker lock");
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
}

/// RAII guard that releases a main connection slot when dropped
///
/// This ensures connection slots are always released, even if the
/// connection handler panics or returns early.
#[derive(Debug)]
pub struct ConnectionGuard {
    ip: IpAddr,
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let mut connections = self.connections.lock().expect("connection tracker lock");
        if let Some(count) = connections.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connections.remove(&self.ip);
            }
        }
    }
}

/// RAII guard that releases a transfer connection slot when dropped
///
/// This ensures connection slots are always released, even if the
/// connection handler panics or returns early.
#[derive(Debug)]
pub struct TransferGuard {
    ip: IpAddr,
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        let mut connections = self.connections.lock().expect("transfer tracker lock");
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
        /// Get the current main connection limit
        fn max_connections_per_ip(&self) -> usize {
            self.max_connections_per_ip.load(Ordering::Relaxed)
        }

        /// Get the current transfer connection limit
        fn max_transfers_per_ip(&self) -> usize {
            self.max_transfers_per_ip.load(Ordering::Relaxed)
        }

        /// Get the current main connection count for an IP
        fn connection_count(&self, ip: IpAddr) -> usize {
            let connections = self.connections.lock().expect("connection tracker lock");
            connections.get(&ip).copied().unwrap_or(0)
        }

        /// Get the current transfer connection count for an IP
        fn transfer_count(&self, ip: IpAddr) -> usize {
            let connections = self
                .transfer_connections
                .lock()
                .expect("transfer tracker lock");
            connections.get(&ip).copied().unwrap_or(0)
        }

        /// Get the total number of active main connections across all IPs
        fn total_connections(&self) -> usize {
            let connections = self.connections.lock().expect("connection tracker lock");
            connections.values().sum()
        }

        /// Get the total number of active transfer connections across all IPs
        fn total_transfers(&self) -> usize {
            let connections = self
                .transfer_connections
                .lock()
                .expect("transfer tracker lock");
            connections.values().sum()
        }
    }

    // =========================================================================
    // Main connection tests
    // =========================================================================

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
        let connections = tracker.connections.lock().expect("connection tracker lock");
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

    // =========================================================================
    // Transfer connection tests
    // =========================================================================

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
            .expect("transfer tracker lock");
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

    // =========================================================================
    // Independent limits tests
    // =========================================================================

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
}
