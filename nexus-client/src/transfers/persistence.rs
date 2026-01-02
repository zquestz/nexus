//! Transfer persistence and management
//!
//! Handles saving/loading transfers to disk and managing the transfer queue.
//! Transfers are stored in `transfers.json` in the same directory as `config.json`.

use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

use uuid::Uuid;

use super::types::{Transfer, TransferError, TransferStatus};

/// File permissions for transfers file on Unix (owner read/write only)
#[cfg(unix)]
const TRANSFERS_FILE_MODE: u32 = 0o600;

/// Persistent transfers file structure
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct TransfersFile {
    /// All transfers (active, paused, completed, failed)
    transfers: Vec<Transfer>,
}

/// Manages file transfers and their persistence
///
/// Transfers are global (not per-connection) and persist across application restarts.
/// The manager handles:
/// - Loading/saving transfers to disk
/// - Adding/removing transfers
/// - Updating transfer progress
/// - Querying transfers by status
#[derive(Debug)]
pub struct TransferManager {
    /// All transfers indexed by ID
    transfers: HashMap<Uuid, Transfer>,

    /// Whether there are unsaved changes
    dirty: bool,
}

impl TransferManager {
    /// Create a new empty transfer manager
    pub fn new() -> Self {
        Self {
            transfers: HashMap::new(),
            dirty: false,
        }
    }

    /// Get the platform-specific transfers file path
    ///
    /// Returns None if the config directory cannot be determined.
    pub fn transfers_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("nexus").join("transfers.json"))
    }

    /// Load transfers from disk, or return empty manager if not found
    ///
    /// Returns an empty manager if:
    /// - Config directory cannot be determined
    /// - Transfers file doesn't exist
    /// - Transfers file cannot be read
    /// - Transfers file contains invalid JSON
    pub fn load() -> Self {
        if let Some(path) = Self::transfers_path()
            && path.exists()
            && let Ok(contents) = fs::read_to_string(&path)
            && let Ok(file) = serde_json::from_str::<TransfersFile>(&contents)
        {
            // Reset any active transfers to Queued - they were interrupted by app restart
            let transfers: HashMap<Uuid, Transfer> = file
                .transfers
                .into_iter()
                .map(|mut t| {
                    if t.status == TransferStatus::Connecting
                        || t.status == TransferStatus::Transferring
                    {
                        t.status = TransferStatus::Queued;
                    }
                    (t.id, t)
                })
                .collect();

            let dirty = transfers
                .values()
                .any(|t| t.status == TransferStatus::Queued);

            return Self { transfers, dirty };
        }

        Self::new()
    }

    /// Save transfers to disk with restrictive permissions
    ///
    /// Creates the config directory if it doesn't exist.
    /// On Unix systems, sets file permissions to 0o600 (owner read/write only)
    /// to protect saved credentials.
    ///
    /// Only saves if there are unsaved changes (dirty flag is set).
    pub fn save(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }

        let path = Self::transfers_path()
            .ok_or_else(|| "Could not determine config directory".to_string())?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
        }

        // Build the file structure
        let file = TransfersFile {
            transfers: self.transfers.values().cloned().collect(),
        };

        // Serialize to pretty JSON
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| format!("Failed to serialize transfers: {e}"))?;

        // Write to disk
        fs::write(&path, json).map_err(|e| format!("Failed to write transfers file: {e}"))?;

        // Set restrictive permissions on Unix (owner read/write only)
        #[cfg(unix)]
        Self::set_transfers_permissions(&path)?;

        self.dirty = false;
        Ok(())
    }

    /// Set transfers file permissions to owner read/write only on Unix systems
    #[cfg(unix)]
    fn set_transfers_permissions(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata =
            fs::metadata(path).map_err(|e| format!("Failed to read file metadata: {e}"))?;
        let mut perms = metadata.permissions();
        perms.set_mode(TRANSFERS_FILE_MODE);

        fs::set_permissions(path, perms)
            .map_err(|e| format!("Failed to set file permissions: {e}"))?;

        Ok(())
    }

    /// Add a new transfer
    ///
    /// Returns the transfer ID.
    pub fn add(&mut self, transfer: Transfer) -> Uuid {
        let id = transfer.id;
        self.transfers.insert(id, transfer);
        self.dirty = true;
        id
    }

    /// Remove a transfer by ID
    ///
    /// Returns the removed transfer if it existed.
    pub fn remove(&mut self, id: Uuid) -> Option<Transfer> {
        let transfer = self.transfers.remove(&id);
        if transfer.is_some() {
            self.dirty = true;
        }
        transfer
    }

    /// Get a transfer by ID
    pub fn get(&self, id: Uuid) -> Option<&Transfer> {
        self.transfers.get(&id)
    }

    /// Get a mutable reference to a transfer by ID
    #[allow(dead_code)] // Useful for future direct mutation needs
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Transfer> {
        let transfer = self.transfers.get_mut(&id);
        if transfer.is_some() {
            self.dirty = true;
        }
        transfer
    }

    /// Get all transfers
    pub fn all(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers.values()
    }

    /// Get all active transfers (connecting or transferring)
    #[allow(dead_code)] // Used by active_count(), kept for direct access
    pub fn active(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers.values().filter(|t| t.status.is_active())
    }

    /// Get all queued or active transfers (for subscription creation)
    ///
    /// Returns transfers that need a subscription running - either waiting to start
    /// (Queued) or currently executing (Connecting/Transferring).
    pub fn queued_or_active(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Queued || t.status.is_active())
    }

    /// Get all queued transfers
    #[allow(dead_code)] // Kept for future queue management UI
    pub fn queued(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Queued)
    }

    /// Get the next queued transfer (first one in queue order)
    ///
    /// Returns None if there are no queued transfers or if there's already
    /// an active transfer (Connecting or Transferring status).
    #[allow(dead_code)] // Alternative queue selection strategy
    pub fn next_queued(&self) -> Option<&Transfer> {
        // Don't start a new transfer if one is already active
        if self.active_count() > 0 {
            return None;
        }

        // Get the oldest queued transfer (by created_at timestamp)
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Queued)
            .min_by_key(|t| t.created_at)
    }

    /// Get all paused transfers
    #[allow(dead_code)] // Kept for future pause management UI
    pub fn paused(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Paused)
    }

    /// Get all completed transfers
    pub fn completed(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Completed)
    }

    /// Get all failed transfers
    pub fn failed(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Failed)
    }

    /// Count of all transfers
    #[allow(dead_code)] // Useful for UI badge/counter
    pub fn count(&self) -> usize {
        self.transfers.len()
    }

    /// Count of active transfers
    #[allow(dead_code)] // Used by next_queued(), useful for UI
    pub fn active_count(&self) -> usize {
        self.active().count()
    }

    /// Update transfer progress
    ///
    /// Returns false if the transfer doesn't exist.
    pub fn update_progress(
        &mut self,
        id: Uuid,
        transferred_bytes: u64,
        files_completed: u64,
        current_file: Option<String>,
    ) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.transferred_bytes = transferred_bytes;
            transfer.files_completed = files_completed;
            transfer.current_file = current_file;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Update transfer status to connecting
    pub fn set_connecting(&mut self, id: Uuid) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.status = TransferStatus::Connecting;
            transfer.start();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Update transfer status to transferring and set metadata from server
    pub fn set_transferring(
        &mut self,
        id: Uuid,
        total_bytes: u64,
        file_count: u64,
        server_transfer_id: Option<String>,
    ) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.status = TransferStatus::Transferring;
            transfer.total_bytes = total_bytes;
            transfer.file_count = file_count;
            transfer.server_transfer_id = server_transfer_id;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Mark a file as completed within a transfer
    #[allow(dead_code)] // Progress tracking via files_completed field
    pub fn complete_file(&mut self, id: Uuid) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.files_completed += 1;
            transfer.current_file = None;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Mark a transfer as completed
    pub fn complete(&mut self, id: Uuid) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.complete();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Mark a transfer as failed
    pub fn fail(&mut self, id: Uuid, error: String, error_kind: Option<TransferError>) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.fail(error, error_kind);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Pause a transfer
    pub fn pause(&mut self, id: Uuid) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.pause();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Queue a transfer for resume
    pub fn queue(&mut self, id: Uuid) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.queue();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Remove all completed transfers
    pub fn clear_completed(&mut self) {
        let completed_ids: Vec<Uuid> = self
            .transfers
            .iter()
            .filter(|(_, t)| t.status == TransferStatus::Completed)
            .map(|(id, _)| *id)
            .collect();

        if !completed_ids.is_empty() {
            for id in completed_ids {
                self.transfers.remove(&id);
            }
            self.dirty = true;
        }
    }

    /// Remove all failed transfers
    pub fn clear_failed(&mut self) {
        let failed_ids: Vec<Uuid> = self
            .transfers
            .iter()
            .filter(|(_, t)| t.status == TransferStatus::Failed)
            .map(|(id, _)| *id)
            .collect();

        if !failed_ids.is_empty() {
            for id in failed_ids {
                self.transfers.remove(&id);
            }
            self.dirty = true;
        }
    }

    /// Check if there are unsaved changes
    #[allow(dead_code)] // Useful for optimized save strategies
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the manager as having unsaved changes
    #[allow(dead_code)] // Useful for external state tracking
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl Default for TransferManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::transfers::TransferConnectionInfo;

    fn test_connection_info() -> TransferConnectionInfo {
        TransferConnectionInfo {
            server_name: "Test Server".to_string(),
            server_address: "192.168.1.100".to_string(),
            transfer_port: 7501,
            certificate_fingerprint: "AA:BB:CC:DD".to_string(),
            username: "alice".to_string(),
            password: "secret".to_string(),
            nickname: None,
        }
    }

    fn test_transfer() -> Transfer {
        Transfer::new_download(
            test_connection_info(),
            "/Games/app.zip".to_string(),
            false,
            false,
            PathBuf::from("/home/user/Downloads/app.zip"),
            None,
        )
    }

    #[test]
    fn test_transfer_manager_new() {
        let manager = TransferManager::new();
        assert_eq!(manager.count(), 0);
        assert!(!manager.is_dirty());
    }

    #[test]
    fn test_transfer_manager_add() {
        let mut manager = TransferManager::new();
        let transfer = test_transfer();
        let id = transfer.id;

        let returned_id = manager.add(transfer);

        assert_eq!(returned_id, id);
        assert_eq!(manager.count(), 1);
        assert!(manager.is_dirty());
        assert!(manager.get(id).is_some());
    }

    #[test]
    fn test_transfer_manager_remove() {
        let mut manager = TransferManager::new();
        let transfer = test_transfer();
        let id = transfer.id;
        manager.add(transfer);

        let removed = manager.remove(id);

        assert!(removed.is_some());
        assert_eq!(manager.count(), 0);
        assert!(manager.get(id).is_none());
    }

    #[test]
    fn test_transfer_manager_remove_nonexistent() {
        let mut manager = TransferManager::new();
        let removed = manager.remove(Uuid::new_v4());
        assert!(removed.is_none());
    }

    #[test]
    fn test_transfer_manager_update_progress() {
        let mut manager = TransferManager::new();
        let transfer = test_transfer();
        let id = transfer.id;
        manager.add(transfer);

        let updated = manager.update_progress(id, 500, 1, Some("file.txt".to_string()));

        assert!(updated);
        let t = manager.get(id).unwrap();
        assert_eq!(t.transferred_bytes, 500);
        assert_eq!(t.current_file, Some("file.txt".to_string()));
    }

    #[test]
    fn test_transfer_manager_status_transitions() {
        let mut manager = TransferManager::new();
        let transfer = test_transfer();
        let id = transfer.id;
        manager.add(transfer);

        // Queued -> Connecting
        assert!(manager.set_connecting(id));
        assert_eq!(manager.get(id).unwrap().status, TransferStatus::Connecting);

        // Connecting -> Transferring
        assert!(manager.set_transferring(id, 1000, 3, Some("abc123".to_string())));
        let t = manager.get(id).unwrap();
        assert_eq!(t.status, TransferStatus::Transferring);
        assert_eq!(t.total_bytes, 1000);
        assert_eq!(t.file_count, 3);
        assert_eq!(t.server_transfer_id, Some("abc123".to_string()));

        // Transferring -> Paused
        assert!(manager.pause(id));
        assert_eq!(manager.get(id).unwrap().status, TransferStatus::Paused);

        // Paused -> Queued
        assert!(manager.queue(id));
        assert_eq!(manager.get(id).unwrap().status, TransferStatus::Queued);
    }

    #[test]
    fn test_transfer_manager_complete() {
        let mut manager = TransferManager::new();
        let mut transfer = test_transfer();
        transfer.status = TransferStatus::Transferring;
        let id = transfer.id;
        manager.add(transfer);

        assert!(manager.complete(id));
        assert_eq!(manager.get(id).unwrap().status, TransferStatus::Completed);
    }

    #[test]
    fn test_transfer_manager_fail() {
        let mut manager = TransferManager::new();
        let mut transfer = test_transfer();
        transfer.status = TransferStatus::Transferring;
        let id = transfer.id;
        manager.add(transfer);

        assert!(manager.fail(
            id,
            "Connection lost".to_string(),
            Some(TransferError::ConnectionError)
        ));

        let t = manager.get(id).unwrap();
        assert_eq!(t.status, TransferStatus::Failed);
        assert_eq!(t.error, Some("Connection lost".to_string()));
        assert_eq!(t.error_kind, Some(TransferError::ConnectionError));
    }

    #[test]
    fn test_transfer_manager_complete_file() {
        let mut manager = TransferManager::new();
        let mut transfer = test_transfer();
        transfer.status = TransferStatus::Transferring;
        transfer.current_file = Some("file1.txt".to_string());
        let id = transfer.id;
        manager.add(transfer);

        assert!(manager.complete_file(id));

        let t = manager.get(id).unwrap();
        assert_eq!(t.files_completed, 1);
        assert!(t.current_file.is_none());
    }

    #[test]
    fn test_transfer_manager_filters() {
        let mut manager = TransferManager::new();

        // Add transfers with different statuses
        let mut t1 = test_transfer();
        t1.status = TransferStatus::Queued;
        manager.add(t1);

        let mut t2 = test_transfer();
        t2.status = TransferStatus::Transferring;
        manager.add(t2);

        let mut t3 = test_transfer();
        t3.status = TransferStatus::Completed;
        manager.add(t3);

        let mut t4 = test_transfer();
        t4.status = TransferStatus::Failed;
        manager.add(t4);

        let mut t5 = test_transfer();
        t5.status = TransferStatus::Paused;
        manager.add(t5);

        assert_eq!(manager.count(), 5);
        assert_eq!(manager.queued().count(), 1);
        assert_eq!(manager.active().count(), 1);
        assert_eq!(manager.completed().count(), 1);
        assert_eq!(manager.failed().count(), 1);
        assert_eq!(manager.paused().count(), 1);
        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn test_transfer_manager_clear_completed() {
        let mut manager = TransferManager::new();

        let mut t1 = test_transfer();
        t1.status = TransferStatus::Completed;
        manager.add(t1);

        let mut t2 = test_transfer();
        t2.status = TransferStatus::Completed;
        manager.add(t2);

        let mut t3 = test_transfer();
        t3.status = TransferStatus::Transferring;
        manager.add(t3);

        assert_eq!(manager.count(), 3);

        manager.clear_completed();

        assert_eq!(manager.count(), 1);
        assert_eq!(manager.completed().count(), 0);
        assert_eq!(manager.active().count(), 1);
    }

    #[test]
    fn test_transfer_manager_clear_failed() {
        let mut manager = TransferManager::new();

        let mut t1 = test_transfer();
        t1.status = TransferStatus::Failed;
        manager.add(t1);

        let mut t2 = test_transfer();
        t2.status = TransferStatus::Queued;
        manager.add(t2);

        manager.clear_failed();

        assert_eq!(manager.count(), 1);
        assert_eq!(manager.failed().count(), 0);
    }

    #[test]
    fn test_transfers_path_format() {
        if let Some(path) = TransferManager::transfers_path() {
            assert!(
                path.ends_with("nexus/transfers.json"),
                "Transfers path should end with nexus/transfers.json, got: {:?}",
                path
            );
        }
    }

    #[test]
    fn test_transfers_file_serialization() {
        let mut t1 = test_transfer();
        t1.status = TransferStatus::Transferring;
        t1.total_bytes = 1000;
        t1.transferred_bytes = 500;

        let file = TransfersFile {
            transfers: vec![t1.clone()],
        };

        let json = serde_json::to_string(&file).expect("serialize");
        let deserialized: TransfersFile = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.transfers.len(), 1);
        assert_eq!(deserialized.transfers[0].id, t1.id);
        assert_eq!(deserialized.transfers[0].total_bytes, 1000);
    }

    #[test]
    fn test_transfer_manager_dirty_flag() {
        let mut manager = TransferManager::new();
        assert!(!manager.is_dirty());

        let transfer = test_transfer();
        let id = transfer.id;
        manager.add(transfer);
        assert!(manager.is_dirty());

        // Manually reset dirty flag to simulate save
        manager.dirty = false;
        assert!(!manager.is_dirty());

        // Getting mutable reference marks dirty
        let _ = manager.get_mut(id);
        assert!(manager.is_dirty());
    }

    #[test]
    fn test_transfer_manager_all_iterator() {
        let mut manager = TransferManager::new();
        manager.add(test_transfer());
        manager.add(test_transfer());
        manager.add(test_transfer());

        let all: Vec<_> = manager.all().collect();
        assert_eq!(all.len(), 3);
    }
}
