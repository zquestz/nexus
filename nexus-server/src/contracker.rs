//! Connection tracking for DoS protection
//!
//! This module provides connection limiting per IP address to prevent
//! resource exhaustion attacks.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use crate::constants::MAX_CONNECTIONS_PER_IP;

/// Tracks active connections per IP address
///
/// This is used to enforce connection limits and prevent a single IP
/// from exhausting server resources.
#[derive(Debug, Clone)]
pub struct ConnectionTracker {
    /// Map of IP addresses to their current connection count
    connections: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Maximum connections allowed per IP
    max_per_ip: usize,
}

impl ConnectionTracker {
    /// Create a new connection tracker with the specified limit
    #[must_use]
    pub fn new(max_per_ip: usize) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            max_per_ip,
        }
    }

    /// Try to acquire a connection slot for the given IP
    ///
    /// Returns `Some(ConnectionGuard)` if the connection is allowed,
    /// or `None` if the IP has reached its connection limit.
    ///
    /// The returned guard will automatically release the slot when dropped.
    pub fn try_acquire(&self, ip: IpAddr) -> Option<ConnectionGuard> {
        let mut connections = self.connections.lock().expect("connection tracker lock");
        let count = connections.entry(ip).or_insert(0);

        if *count >= self.max_per_ip {
            return None;
        }

        *count += 1;
        Some(ConnectionGuard {
            ip,
            connections: self.connections.clone(),
        })
    }
}

impl Default for ConnectionTracker {
    fn default() -> Self {
        Self::new(MAX_CONNECTIONS_PER_IP)
    }
}

/// RAII guard that releases a connection slot when dropped
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    impl ConnectionTracker {
        /// Get the current connection count for an IP (test only)
        fn connection_count(&self, ip: IpAddr) -> usize {
            let connections = self.connections.lock().expect("connection tracker lock");
            connections.get(&ip).copied().unwrap_or(0)
        }

        /// Get the total number of active connections (test only)
        fn total_connections(&self) -> usize {
            let connections = self.connections.lock().expect("connection tracker lock");
            connections.values().sum()
        }
    }

    #[test]
    fn test_acquire_and_release() {
        let tracker = ConnectionTracker::new(2);
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
        let tracker = ConnectionTracker::new(1);
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
        let tracker = ConnectionTracker::new(5);
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
        let tracker = ConnectionTracker::new(2);
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
    fn test_default_limit() {
        let tracker = ConnectionTracker::default();
        assert_eq!(tracker.max_per_ip, MAX_CONNECTIONS_PER_IP);
    }
}
