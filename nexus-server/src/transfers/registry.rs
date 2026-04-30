//! Transfer registry for tracking active file transfers
//!
//! Provides a way to track active transfers by IP address and signal them
//! when their IP is banned. Uses oneshot channels to communicate ban events
//! without holding locks during I/O operations.
//!
//! The registry stores `ActiveTransfer` structs with queryable metadata,
//! enabling connection monitor integration.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use nexus_common::protocol::TransferInfo;
use tokio::sync::oneshot;

use crate::constants::{ERR_BAN_TX_LOCK_POISONED, ERR_TRANSFER_REGISTRY_LOCK_POISONED};

/// Unique identifier for a transfer session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId(u64);

impl TransferId {
    /// Get the inner ID value
    #[allow(dead_code)] // Public API for future connection monitor integration
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TransferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Direction of a file transfer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// Server sending file(s) to client
    Download,
    /// Client sending file(s) to server
    Upload,
}

impl std::fmt::Display for TransferDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Download => write!(f, "download"),
            Self::Upload => write!(f, "upload"),
        }
    }
}

/// Parameters for [`TransferRegistry::register`] / [`ActiveTransfer::new`].
///
/// Bundles the transfer's identity (peer + authenticated user) with
/// the description (direction, path, total size) so the constructors
/// stay narrow.
pub struct TransferRegistration {
    /// Client's socket address (IP + port).
    pub peer_addr: SocketAddr,
    /// Display name (equals username for regular accounts).
    pub nickname: String,
    /// Username of the authenticated user.
    pub username: String,
    /// Whether the user is an admin.
    pub is_admin: bool,
    /// Whether this is a shared account.
    pub is_shared: bool,
    /// Upload or download.
    pub direction: TransferDirection,
    /// Path being transferred (requested path for downloads, destination for uploads).
    pub path: String,
    /// Total size in bytes (0 if unknown).
    pub total_size: u64,
}

/// Runtime state for an active transfer
///
/// This struct is shared between the registry and the Transfer via Arc.
/// Progress is updated atomically during streaming without holding locks.
///
/// Note: This is distinct from `nexus_common::protocol::TransferInfo` which
/// is the serializable wire format for connection monitor responses.
pub struct ActiveTransfer {
    /// Unique transfer identifier
    pub id: TransferId,
    /// Client's socket address (IP + port, port distinguishes WebSocket vs TCP)
    pub peer_addr: SocketAddr,
    /// Display name (equals username for regular accounts)
    pub nickname: String,
    /// Username of the authenticated user
    pub username: String,
    /// Whether the user is an admin
    pub is_admin: bool,
    /// Whether this is a shared account
    pub is_shared: bool,
    /// Whether this is an upload or download
    pub direction: TransferDirection,
    /// Path being transferred (requested path for downloads, destination for uploads)
    pub path: String,
    /// Total size in bytes (0 if unknown, e.g., downloads before resolution)
    pub total_size: AtomicU64,
    /// Bytes transferred so far (updated atomically during streaming)
    pub bytes_transferred: AtomicU64,
    /// When the transfer started
    pub started_at: Instant,
    /// Channel to signal ban - wrapped in Mutex<Option<>> since we take it once
    ban_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl ActiveTransfer {
    /// Create a new ActiveTransfer from a registration bundle.
    fn new(id: TransferId, params: TransferRegistration, ban_tx: oneshot::Sender<()>) -> Self {
        Self {
            id,
            peer_addr: params.peer_addr,
            nickname: params.nickname,
            username: params.username,
            is_admin: params.is_admin,
            is_shared: params.is_shared,
            direction: params.direction,
            path: params.path,
            total_size: AtomicU64::new(params.total_size),
            bytes_transferred: AtomicU64::new(0),
            started_at: Instant::now(),
            ban_tx: Mutex::new(Some(ban_tx)),
        }
    }

    /// Set the total size (used for downloads after path resolution)
    #[allow(dead_code)] // Public API for future use
    pub fn set_total_size(&self, size: u64) {
        self.total_size.store(size, Ordering::Relaxed);
    }

    /// Get the current bytes transferred
    pub fn get_bytes_transferred(&self) -> u64 {
        self.bytes_transferred.load(Ordering::Relaxed)
    }

    /// Add to bytes transferred, returns the new total
    pub fn add_bytes_transferred(&self, bytes: u64) -> u64 {
        self.bytes_transferred.fetch_add(bytes, Ordering::Relaxed) + bytes
    }

    /// Get the total size (0 if unknown)
    pub fn get_total_size(&self) -> u64 {
        self.total_size.load(Ordering::Relaxed)
    }

    /// Get elapsed time since transfer started
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Send ban signal to this transfer (takes ownership of sender)
    ///
    /// Returns true if signal was sent, false if already sent or receiver dropped.
    fn send_ban_signal(&self) -> bool {
        let mut guard = self.ban_tx.lock().expect(ERR_BAN_TX_LOCK_POISONED);
        if let Some(tx) = guard.take() {
            tx.send(()).is_ok()
        } else {
            false
        }
    }

    /// Convert to wire format for connection monitor response
    pub fn to_transfer_info(&self) -> TransferInfo {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        TransferInfo {
            nickname: self.nickname.clone(),
            username: self.username.clone(),
            ip: self.peer_addr.ip().to_string(),
            port: self.peer_addr.port(),
            is_admin: self.is_admin,
            is_shared: self.is_shared,
            direction: self.direction.to_string(),
            path: self.path.clone(),
            total_size: self.get_total_size(),
            bytes_transferred: self.get_bytes_transferred(),
            started_at: now - self.elapsed().as_secs() as i64,
        }
    }
}

impl std::fmt::Debug for ActiveTransfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferInfo")
            .field("id", &self.id)
            .field("peer_addr", &self.peer_addr)
            .field("nickname", &self.nickname)
            .field("username", &self.username)
            .field("is_admin", &self.is_admin)
            .field("is_shared", &self.is_shared)
            .field("direction", &self.direction)
            .field("path", &self.path)
            .field("total_size", &self.get_total_size())
            .field("bytes_transferred", &self.get_bytes_transferred())
            .field("started_at", &self.started_at)
            .finish()
    }
}

/// Registry for tracking active file transfers
///
/// Thread-safe registry that allows:
/// - Registering new transfers with metadata
/// - Unregistering transfers when they complete
/// - Disconnecting all transfers matching a predicate (e.g., banned IPs)
/// - Querying all active transfers for connection monitor
///
/// The registry uses oneshot channels to signal bans, so transfer tasks can
/// use `tokio::select!` to check for bans during I/O without polling.
pub struct TransferRegistry {
    transfers: Mutex<HashMap<TransferId, Arc<ActiveTransfer>>>,
    next_id: AtomicU64,
}

impl TransferRegistry {
    /// Create a new empty transfer registry
    pub fn new() -> Self {
        Self {
            transfers: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a new transfer and get its shared info plus ban signal receiver.
    ///
    /// The returned receiver will receive `()` if this transfer's IP
    /// is banned while the transfer is active. The transfer's id is
    /// available as `info.id`.
    pub fn register(
        &self,
        params: TransferRegistration,
    ) -> (Arc<ActiveTransfer>, oneshot::Receiver<()>) {
        let id = TransferId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (ban_tx, ban_rx) = oneshot::channel();

        let info = Arc::new(ActiveTransfer::new(id, params, ban_tx));

        self.transfers
            .lock()
            .expect(ERR_TRANSFER_REGISTRY_LOCK_POISONED)
            .insert(id, Arc::clone(&info));

        (info, ban_rx)
    }

    /// Unregister a transfer (called when transfer completes or fails)
    pub fn unregister(&self, id: TransferId) {
        self.transfers
            .lock()
            .expect(ERR_TRANSFER_REGISTRY_LOCK_POISONED)
            .remove(&id);
    }

    /// Disconnect all transfers where the predicate returns true for their IP
    ///
    /// Sends ban signal to all matching transfers. The transfers are responsible
    /// for closing their connections when they receive the signal.
    ///
    /// # Arguments
    /// * `predicate` - Function that returns true for IPs that should be disconnected
    ///
    /// # Returns
    /// The number of transfers that were signaled
    pub fn disconnect_matching<F>(&self, predicate: F) -> usize
    where
        F: Fn(std::net::IpAddr) -> bool,
    {
        let transfers = self
            .transfers
            .lock()
            .expect(ERR_TRANSFER_REGISTRY_LOCK_POISONED);

        let mut count = 0;
        for info in transfers.values() {
            if predicate(info.peer_addr.ip()) && info.send_ban_signal() {
                count += 1;
            }
        }

        count
    }

    /// Get a snapshot of all active transfers
    ///
    /// Returns cloned Arc references to all active transfer structs.
    /// Safe to call while transfers are in progress.
    #[allow(dead_code)] // Public API for future connection monitor integration
    pub fn snapshot(&self) -> Vec<Arc<ActiveTransfer>> {
        self.transfers
            .lock()
            .expect(ERR_TRANSFER_REGISTRY_LOCK_POISONED)
            .values()
            .cloned()
            .collect()
    }

    /// Get the number of active transfers
    #[allow(dead_code)] // Used in tests and future connection monitor
    pub fn active_count(&self) -> usize {
        self.transfers
            .lock()
            .expect(ERR_TRANSFER_REGISTRY_LOCK_POISONED)
            .len()
    }
}

impl Default for TransferRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that unregisters an active transfer when dropped
///
/// This ensures transfers are always unregistered even if the handler
/// returns early due to errors or panics.
pub struct TransferRegistryGuard<'a> {
    registry: &'a TransferRegistry,
    id: TransferId,
}

impl<'a> TransferRegistryGuard<'a> {
    /// Create a new guard that will unregister the transfer on drop
    pub fn new(registry: &'a TransferRegistry, id: TransferId) -> Self {
        Self { registry, id }
    }

    /// Get the transfer ID
    #[allow(dead_code)] // Public API for future use
    pub fn id(&self) -> TransferId {
        self.id
    }
}

impl Drop for TransferRegistryGuard<'_> {
    fn drop(&mut self) {
        self.registry.unregister(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    fn make_test_addr(ip: IpAddr) -> SocketAddr {
        SocketAddr::new(ip, 12345)
    }

    #[test]
    fn test_register_and_unregister() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (info, _rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/test.zip".to_string(),
            total_size: 1024,
        });

        assert_eq!(registry.active_count(), 1);
        assert_eq!(info.username, "testuser");
        assert_eq!(info.direction, TransferDirection::Download);
        assert_eq!(info.path, "/files/test.zip");
        assert_eq!(info.get_total_size(), 1024);

        registry.unregister(info.id);
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn test_unique_ids() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (info1, _rx1) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "user1".to_string(),
            username: "user1".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file1".to_string(),
            total_size: 0,
        });
        let (info2, _rx2) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "user2".to_string(),
            username: "user2".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "/file2".to_string(),
            total_size: 0,
        });
        let (info3, _rx3) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "user3".to_string(),
            username: "user3".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file3".to_string(),
            total_size: 0,
        });

        assert_ne!(info1.id, info2.id);
        assert_ne!(info2.id, info3.id);
        assert_ne!(info1.id, info3.id);
    }

    #[test]
    fn test_disconnect_matching_single_ip() {
        let registry = TransferRegistry::new();
        let banned_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let safe_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 200));

        let (_, mut rx1) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(banned_ip),
            nickname: "banned1".to_string(),
            username: "banned1".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        let (_, mut rx2) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(safe_ip),
            nickname: "safe".to_string(),
            username: "safe".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        let (_, mut rx3) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(banned_ip),
            nickname: "banned2".to_string(),
            username: "banned2".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "/file".to_string(),
            total_size: 0,
        });

        assert_eq!(registry.active_count(), 3);

        let disconnected = registry.disconnect_matching(|ip| ip == banned_ip);

        assert_eq!(disconnected, 2);
        // Transfers remain registered until explicitly unregistered
        assert_eq!(registry.active_count(), 3);

        // Banned transfers should have received the signal
        assert!(rx1.try_recv().is_ok());
        assert!(rx3.try_recv().is_ok());

        // Safe transfer should not have received anything
        assert!(rx2.try_recv().is_err());
    }

    #[test]
    fn test_disconnect_matching_cidr_simulation() {
        let registry = TransferRegistry::new();

        // Simulate a /24 CIDR ban on 10.0.1.0/24
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 1, 254));
        let ip3 = IpAddr::V4(Ipv4Addr::new(10, 0, 2, 1)); // Different subnet

        let (_, _rx1) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(ip1),
            nickname: "user1".to_string(),
            username: "user1".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        let (_, _rx2) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(ip2),
            nickname: "user2".to_string(),
            username: "user2".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        let (_, _rx3) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(ip3),
            nickname: "user3".to_string(),
            username: "user3".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });

        // Predicate checks if IP is in 10.0.1.0/24
        let disconnected = registry.disconnect_matching(|ip| {
            if let IpAddr::V4(v4) = ip {
                let octets = v4.octets();
                octets[0] == 10 && octets[1] == 0 && octets[2] == 1
            } else {
                false
            }
        });

        assert_eq!(disconnected, 2);
    }

    #[test]
    fn test_disconnect_matching_ipv6() {
        let registry = TransferRegistry::new();

        let ipv4 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ipv6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));

        let (_, _rx1) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(ipv4),
            nickname: "user1".to_string(),
            username: "user1".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        let (_, mut rx2) = registry.register(TransferRegistration {
            peer_addr: make_test_addr(ipv6),
            nickname: "user2".to_string(),
            username: "user2".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "/file".to_string(),
            total_size: 0,
        });

        let disconnected = registry.disconnect_matching(|ip| ip == ipv6);

        assert_eq!(disconnected, 1);
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_guard_unregisters_on_drop() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (info, _rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        assert_eq!(registry.active_count(), 1);

        {
            let _guard = TransferRegistryGuard::new(&registry, info.id);
            assert_eq!(registry.active_count(), 1);
        } // guard dropped here

        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn test_disconnect_already_unregistered() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (info, _rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        registry.unregister(info.id);

        // Should not panic or error when trying to disconnect an already-gone transfer
        let disconnected = registry.disconnect_matching(|_| true);
        assert_eq!(disconnected, 0);
    }

    #[test]
    fn test_receiver_dropped_before_ban() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (_, rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });
        drop(rx); // Simulate transfer ending before ban

        // Should not panic when receiver is dropped
        let disconnected = registry.disconnect_matching(|_| true);

        // Returns 0 because send failed (receiver dropped)
        assert_eq!(disconnected, 0);
    }

    #[test]
    fn test_snapshot() {
        let registry = TransferRegistry::new();

        let addr1 = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        let addr2 = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)));

        let (_, _rx1) = registry.register(TransferRegistration {
            peer_addr: addr1,
            nickname: "user1".to_string(),
            username: "user1".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/downloads/file1.zip".to_string(),
            total_size: 1000,
        });
        let (_, _rx2) = registry.register(TransferRegistration {
            peer_addr: addr2,
            nickname: "user2".to_string(),
            username: "user2".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "/uploads".to_string(),
            total_size: 2000,
        });

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 2);

        // Verify we can access all metadata
        let usernames: Vec<&str> = snapshot.iter().map(|i| i.username.as_str()).collect();
        assert!(usernames.contains(&"user1"));
        assert!(usernames.contains(&"user2"));
    }

    #[test]
    fn test_bytes_transferred_atomic_update() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (info, _rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 1000,
        });

        assert_eq!(info.get_bytes_transferred(), 0);

        info.add_bytes_transferred(100);
        assert_eq!(info.get_bytes_transferred(), 100);

        info.add_bytes_transferred(250);
        assert_eq!(info.get_bytes_transferred(), 350);
    }

    #[test]
    fn test_set_total_size() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (info, _rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0, // Unknown at registration
        });

        assert_eq!(info.get_total_size(), 0);

        // Later, after path resolution
        info.set_total_size(5000);
        assert_eq!(info.get_total_size(), 5000);
    }

    #[test]
    fn test_transfer_direction_display() {
        assert_eq!(format!("{}", TransferDirection::Download), "download");
        assert_eq!(format!("{}", TransferDirection::Upload), "upload");
    }

    #[test]
    fn test_transfer_id_display() {
        let id = TransferId(42);
        assert_eq!(format!("{id}"), "42");
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_double_ban_signal() {
        let registry = TransferRegistry::new();
        let addr = make_test_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let (_, _rx) = registry.register(TransferRegistration {
            peer_addr: addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/file".to_string(),
            total_size: 0,
        });

        // First disconnect should succeed
        let disconnected1 = registry.disconnect_matching(|_| true);
        assert_eq!(disconnected1, 1);

        // Second disconnect should return 0 (already signaled)
        let disconnected2 = registry.disconnect_matching(|_| true);
        assert_eq!(disconnected2, 0);
    }
}
