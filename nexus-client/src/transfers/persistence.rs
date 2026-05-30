//! Transfer persistence and management
//!
//! Handles saving/loading transfers to disk and managing the transfer queue.
//! Transfers are stored in `transfers.json` in the same directory as `config.json`.

use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

use nexus_common::names::fold_name;
use uuid::Uuid;

use super::types::{Transfer, TransferError, TransferStatus};
use crate::constants::{APP_DIR_NAME, TRANSFERS_FILE_NAME};
use crate::i18n::{t, t_args};
use crate::types::ConnectionInfo;

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
        dirs::config_dir().map(|dir| dir.join(APP_DIR_NAME).join(TRANSFERS_FILE_NAME))
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

        let path = Self::transfers_path().ok_or_else(|| t("transfer-save-no-config-dir"))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                t_args(
                    "transfer-save-create-dir-failed",
                    &[("error", &e.to_string())],
                )
            })?;
        }

        // Build the file structure
        let file = TransfersFile {
            transfers: self.transfers.values().cloned().collect(),
        };

        // Serialize to pretty JSON
        let json = serde_json::to_string_pretty(&file).map_err(|e| {
            t_args(
                "transfer-save-serialize-failed",
                &[("error", &e.to_string())],
            )
        })?;

        // On Unix, create empty file and set permissions before writing content
        // This avoids a race condition where the file is briefly world-readable
        #[cfg(unix)]
        {
            // Create empty file (or truncate existing)
            fs::File::create(&path)
                .map_err(|e| t_args("transfer-save-write-failed", &[("error", &e.to_string())]))?;

            // Set restrictive permissions while file is empty
            Self::set_transfers_permissions(&path)?;
        }

        // Write content (file already has correct permissions on Unix)
        fs::write(&path, json)
            .map_err(|e| t_args("transfer-save-write-failed", &[("error", &e.to_string())]))?;

        self.dirty = false;
        Ok(())
    }

    /// Set transfers file permissions to owner read/write only on Unix systems
    #[cfg(unix)]
    fn set_transfers_permissions(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path).map_err(|e| {
            t_args(
                "transfer-save-metadata-failed",
                &[("error", &e.to_string())],
            )
        })?;
        let mut perms = metadata.permissions();
        perms.set_mode(TRANSFERS_FILE_MODE);

        fs::set_permissions(path, perms).map_err(|e| {
            t_args(
                "transfer-save-permissions-failed",
                &[("error", &e.to_string())],
            )
        })?;

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

    /// Get all transfers
    pub fn all(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers.values()
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
    ///
    /// Assigns the transfer to the end of the queue (highest queue_position + 1).
    pub fn queue(&mut self, id: Uuid) -> bool {
        // Calculate position before taking mutable borrow
        let new_position = self.next_queue_position_internal();
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.queue();
            transfer.queue_position = new_position;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Get the next queue position (max + 1, or 0 if empty)
    pub fn next_queue_position(&self) -> u32 {
        self.next_queue_position_internal()
    }

    /// Internal helper to calculate next queue position
    fn next_queue_position_internal(&self) -> u32 {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Queued)
            .map(|t| t.queue_position)
            .max()
            .map(|max| max.saturating_add(1))
            .unwrap_or(0)
    }

    /// Move a queued transfer up (lower position = higher priority)
    ///
    /// Returns false if transfer not found, not queued, or already first.
    pub fn move_up(&mut self, id: Uuid) -> bool {
        // Get the transfer's current position
        let current_pos = match self.transfers.get(&id) {
            Some(t) if t.status == TransferStatus::Queued => t.queue_position,
            _ => return false,
        };

        // Find the transfer with the next lower position (the one above)
        // Capture both id and position to avoid double-lookup
        let Some((swap_id, swap_pos)) = self
            .transfers
            .iter()
            .filter(|(_, t)| t.status == TransferStatus::Queued && t.queue_position < current_pos)
            .max_by_key(|(_, t)| t.queue_position)
            .map(|(id, t)| (*id, t.queue_position))
        else {
            return false; // Already first
        };

        // Swap positions
        if let Some(t) = self.transfers.get_mut(&id) {
            t.queue_position = swap_pos;
        }
        if let Some(t) = self.transfers.get_mut(&swap_id) {
            t.queue_position = current_pos;
        }
        self.dirty = true;
        true
    }

    /// Move a queued transfer down (higher position = lower priority)
    ///
    /// Returns false if transfer not found, not queued, or already last.
    pub fn move_down(&mut self, id: Uuid) -> bool {
        // Get the transfer's current position
        let current_pos = match self.transfers.get(&id) {
            Some(t) if t.status == TransferStatus::Queued => t.queue_position,
            _ => return false,
        };

        // Find the transfer with the next higher position (the one below)
        // Capture both id and position to avoid double-lookup
        let Some((swap_id, swap_pos)) = self
            .transfers
            .iter()
            .filter(|(_, t)| t.status == TransferStatus::Queued && t.queue_position > current_pos)
            .min_by_key(|(_, t)| t.queue_position)
            .map(|(id, t)| (*id, t.queue_position))
        else {
            return false; // Already last
        };

        // Swap positions
        if let Some(t) = self.transfers.get_mut(&id) {
            t.queue_position = swap_pos;
        }
        if let Some(t) = self.transfers.get_mut(&swap_id) {
            t.queue_position = current_pos;
        }
        self.dirty = true;
        true
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

    /// Get all queued transfers
    pub fn queued(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers
            .values()
            .filter(|t| t.status == TransferStatus::Queued)
    }

    /// Get all active transfers (connecting or transferring)
    pub fn active(&self) -> impl Iterator<Item = &Transfer> {
        self.transfers.values().filter(|t| t.status.is_active())
    }

    /// Update certificate fingerprint for resume-eligible transfers belonging to a bookmark.
    ///
    /// Called when a user accepts a new certificate fingerprint. Any transfer
    /// that could be resumed against the new fingerprint needs to be updated
    /// so it doesn't fail stage-1 with a stale snapshot. This includes
    /// `Queued`, `Paused`, `Connecting`, and `Failed` — `handle_transfer_resume`
    /// allows resuming `Paused | Failed`, so `Failed` must be in this set.
    pub fn update_fingerprint_for_bookmark(&mut self, bookmark_id: Uuid, new_fingerprint: &str) {
        for transfer in self.transfers.values_mut() {
            if transfer.bookmark_id == Some(bookmark_id)
                && (transfer.status == TransferStatus::Queued
                    || transfer.status == TransferStatus::Paused
                    || transfer.status == TransferStatus::Connecting
                    || transfer.status == TransferStatus::Failed)
            {
                transfer.connection_info.certificate_fingerprint = new_fingerprint.to_string();
                self.dirty = true;
            }
        }
    }

    /// Update saved transfer credentials after the connected account is renamed.
    ///
    /// Resume opens a fresh transfer-port login using the transfer's persisted
    /// `ConnectionInfo`, so records that can still be retried must follow a self
    /// rename. Active records are included because a later pause turns the same
    /// persisted row into a resumable transfer.
    pub fn update_username_for_connection(
        &mut self,
        connection: &ConnectionInfo,
        old_username: &str,
        new_username: &str,
    ) -> bool {
        let old_folded = fold_name(old_username);
        let mut changed = false;

        for transfer in self.transfers.values_mut() {
            if !transfer_status_can_reconnect(transfer.status) {
                continue;
            }

            let transfer_info = &mut transfer.connection_info;
            if transfer_info.address == connection.address
                && transfer_info.port == connection.port
                && transfer_info.transfer_port == connection.transfer_port
                && transfer_info.certificate_fingerprint == connection.certificate_fingerprint
                && fold_name(&transfer_info.username) == old_folded
            {
                transfer_info.username = new_username.to_string();
                if fold_name(&transfer_info.nickname) == old_folded {
                    transfer_info.nickname = new_username.to_string();
                }
                changed = true;
            }
        }

        if changed {
            self.dirty = true;
        }
        changed
    }
}

fn transfer_status_can_reconnect(status: TransferStatus) -> bool {
    matches!(
        status,
        TransferStatus::Queued
            | TransferStatus::Connecting
            | TransferStatus::Transferring
            | TransferStatus::Paused
            | TransferStatus::Failed
    )
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
    use crate::types::ConnectionInfo;

    fn test_connection_info() -> ConnectionInfo {
        ConnectionInfo {
            server_name: "Test Server".to_string(),
            address: "192.168.1.100".to_string(),
            port: 7500,
            transfer_port: 7501,
            certificate_fingerprint: "AA:BB:CC:DD".to_string(),
            username: "alice".to_string(),
            password: "secret".to_string(),
            nickname: String::new(),
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
            0,
        )
    }

    fn test_transfer_with_position(position: u32) -> Transfer {
        Transfer::new_download(
            test_connection_info(),
            "/Games/app.zip".to_string(),
            false,
            false,
            PathBuf::from("/home/user/Downloads/app.zip"),
            None,
            position,
        )
    }

    #[test]
    fn test_transfer_manager_new() {
        let manager = TransferManager::new();
        assert_eq!(manager.transfers.len(), 0);
        assert!(!manager.dirty);
    }

    #[test]
    fn test_transfer_manager_add() {
        let mut manager = TransferManager::new();
        let transfer = test_transfer();
        let id = transfer.id;

        let returned_id = manager.add(transfer);

        assert_eq!(returned_id, id);
        assert_eq!(manager.transfers.len(), 1);
        assert!(manager.dirty);
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
        assert_eq!(manager.transfers.len(), 0);
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
    fn test_transfer_manager_file_progress() {
        let mut manager = TransferManager::new();
        let mut transfer = test_transfer();
        transfer.status = TransferStatus::Transferring;
        transfer.current_file = Some("file1.txt".to_string());
        let id = transfer.id;
        manager.add(transfer);

        // Simulate completing a file by updating progress
        assert!(manager.update_progress(id, 1000, 1, None));

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

        assert_eq!(manager.transfers.len(), 5);
        assert_eq!(manager.queued().count(), 1);
        assert_eq!(manager.active().count(), 1);
        assert_eq!(manager.completed().count(), 1);
        assert_eq!(manager.failed().count(), 1);
        assert_eq!(
            manager
                .transfers
                .values()
                .filter(|t| t.status == TransferStatus::Paused)
                .count(),
            1
        );
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

        assert_eq!(manager.transfers.len(), 3);

        manager.clear_completed();

        assert_eq!(manager.transfers.len(), 1);
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

        assert_eq!(manager.transfers.len(), 1);
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
        assert!(!manager.dirty);

        let transfer = test_transfer();
        let id = transfer.id;
        manager.add(transfer);
        assert!(manager.dirty);

        // Manually reset dirty flag to simulate save
        manager.dirty = false;
        assert!(!manager.dirty);

        // Updating progress marks dirty
        manager.update_progress(id, 100, 0, None);
        assert!(manager.dirty);
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

    #[test]
    fn test_next_queue_position_empty() {
        let manager = TransferManager::new();
        assert_eq!(manager.next_queue_position(), 0);
    }

    #[test]
    fn test_next_queue_position_with_transfers() {
        let mut manager = TransferManager::new();
        manager.add(test_transfer_with_position(0));
        manager.add(test_transfer_with_position(5));
        manager.add(test_transfer_with_position(3));
        assert_eq!(manager.next_queue_position(), 6);
    }

    #[test]
    fn test_queue_assigns_position_at_end() {
        let mut manager = TransferManager::new();

        // Add a queued transfer at position 0
        let t1 = test_transfer_with_position(0);
        let id1 = t1.id;
        manager.add(t1);

        // Add another at position 5
        let t2 = test_transfer_with_position(5);
        manager.add(t2);

        // Pause the first transfer, then re-queue it
        manager.pause(id1);
        manager.queue(id1);

        // Should now be at position 6 (max + 1)
        assert_eq!(manager.get(id1).unwrap().queue_position, 6);
    }

    #[test]
    fn test_move_up() {
        let mut manager = TransferManager::new();

        let t1 = test_transfer_with_position(0);
        let id1 = t1.id;
        manager.add(t1);

        let t2 = test_transfer_with_position(1);
        let id2 = t2.id;
        manager.add(t2);

        let t3 = test_transfer_with_position(2);
        let id3 = t3.id;
        manager.add(t3);

        // Move t3 up - should swap with t2
        assert!(manager.move_up(id3));
        assert_eq!(manager.get(id2).unwrap().queue_position, 2);
        assert_eq!(manager.get(id3).unwrap().queue_position, 1);

        // Move t3 up again - should swap with t1
        assert!(manager.move_up(id3));
        assert_eq!(manager.get(id1).unwrap().queue_position, 1);
        assert_eq!(manager.get(id3).unwrap().queue_position, 0);

        // Move t3 up again - should fail (already first)
        assert!(!manager.move_up(id3));
    }

    #[test]
    fn test_move_down() {
        let mut manager = TransferManager::new();

        let t1 = test_transfer_with_position(0);
        let id1 = t1.id;
        manager.add(t1);

        let t2 = test_transfer_with_position(1);
        let id2 = t2.id;
        manager.add(t2);

        let t3 = test_transfer_with_position(2);
        let id3 = t3.id;
        manager.add(t3);

        // Move t1 down - should swap with t2
        assert!(manager.move_down(id1));
        assert_eq!(manager.get(id1).unwrap().queue_position, 1);
        assert_eq!(manager.get(id2).unwrap().queue_position, 0);

        // Move t1 down again - should swap with t3
        assert!(manager.move_down(id1));
        assert_eq!(manager.get(id1).unwrap().queue_position, 2);
        assert_eq!(manager.get(id3).unwrap().queue_position, 1);

        // Move t1 down again - should fail (already last)
        assert!(!manager.move_down(id1));
    }

    #[test]
    fn test_move_up_not_queued() {
        let mut manager = TransferManager::new();

        let t = test_transfer_with_position(0);
        let id = t.id;
        manager.add(t);

        // Complete the transfer
        manager.complete(id);

        // Should fail - not queued
        assert!(!manager.move_up(id));
    }

    #[test]
    fn test_move_down_not_queued() {
        let mut manager = TransferManager::new();

        let t = test_transfer_with_position(0);
        let id = t.id;
        manager.add(t);

        // Pause the transfer
        manager.set_connecting(id);
        manager.pause(id);

        // Should fail - not queued (paused)
        assert!(!manager.move_down(id));
    }

    #[test]
    fn test_move_nonexistent() {
        let mut manager = TransferManager::new();
        let fake_id = Uuid::new_v4();

        assert!(!manager.move_up(fake_id));
        assert!(!manager.move_down(fake_id));
    }

    #[test]
    fn test_update_fingerprint_for_bookmark() {
        let mut manager = TransferManager::new();
        let bookmark_id = Uuid::new_v4();
        let other_bookmark_id = Uuid::new_v4();
        let old_fingerprint = "AA:BB:CC:DD";
        let new_fingerprint = "11:22:33:44";

        // Create transfers with different statuses
        let mut queued = test_transfer();
        queued.bookmark_id = Some(bookmark_id);
        queued.connection_info.certificate_fingerprint = old_fingerprint.to_string();
        queued.status = TransferStatus::Queued;
        let queued_id = queued.id;

        let mut paused = test_transfer();
        paused.bookmark_id = Some(bookmark_id);
        paused.connection_info.certificate_fingerprint = old_fingerprint.to_string();
        paused.status = TransferStatus::Paused;
        let paused_id = paused.id;

        let mut connecting = test_transfer();
        connecting.bookmark_id = Some(bookmark_id);
        connecting.connection_info.certificate_fingerprint = old_fingerprint.to_string();
        connecting.status = TransferStatus::Connecting;
        let connecting_id = connecting.id;

        let mut failed = test_transfer();
        failed.bookmark_id = Some(bookmark_id);
        failed.connection_info.certificate_fingerprint = old_fingerprint.to_string();
        failed.status = TransferStatus::Failed;
        let failed_id = failed.id;

        let mut completed = test_transfer();
        completed.bookmark_id = Some(bookmark_id);
        completed.connection_info.certificate_fingerprint = old_fingerprint.to_string();
        completed.status = TransferStatus::Completed;
        let completed_id = completed.id;

        let mut other_bookmark = test_transfer();
        other_bookmark.bookmark_id = Some(other_bookmark_id);
        other_bookmark.connection_info.certificate_fingerprint = old_fingerprint.to_string();
        other_bookmark.status = TransferStatus::Queued;
        let other_id = other_bookmark.id;

        manager.add(queued);
        manager.add(paused);
        manager.add(connecting);
        manager.add(failed);
        manager.add(completed);
        manager.add(other_bookmark);
        manager.dirty = false;

        // Update fingerprint for the bookmark
        manager.update_fingerprint_for_bookmark(bookmark_id, new_fingerprint);

        // Queued, Paused, Connecting, and Failed should be updated
        // (Failed is a resume-eligible status — handle_transfer_resume allows
        // resuming Paused | Failed, so a stale fingerprint there would fail
        // stage-1 on the resume attempt.)
        assert_eq!(
            manager
                .get(queued_id)
                .unwrap()
                .connection_info
                .certificate_fingerprint,
            new_fingerprint
        );
        assert_eq!(
            manager
                .get(paused_id)
                .unwrap()
                .connection_info
                .certificate_fingerprint,
            new_fingerprint
        );
        assert_eq!(
            manager
                .get(connecting_id)
                .unwrap()
                .connection_info
                .certificate_fingerprint,
            new_fingerprint
        );
        assert_eq!(
            manager
                .get(failed_id)
                .unwrap()
                .connection_info
                .certificate_fingerprint,
            new_fingerprint
        );

        // Completed should NOT be updated
        assert_eq!(
            manager
                .get(completed_id)
                .unwrap()
                .connection_info
                .certificate_fingerprint,
            old_fingerprint
        );

        // Other bookmark should NOT be updated
        assert_eq!(
            manager
                .get(other_id)
                .unwrap()
                .connection_info
                .certificate_fingerprint,
            old_fingerprint
        );

        // Should be marked dirty
        assert!(manager.dirty);
    }

    #[test]
    fn test_update_username_for_connection_updates_resume_eligible_transfers() {
        let mut manager = TransferManager::new();
        let mut current_connection = test_connection_info();
        current_connection.username = "alicia".to_string();

        let mut queued = test_transfer();
        queued.status = TransferStatus::Queued;
        queued.connection_info.username = "alice".to_string();
        queued.connection_info.nickname = "alice".to_string();
        let queued_id = queued.id;

        let mut paused_shared = test_transfer();
        paused_shared.status = TransferStatus::Paused;
        paused_shared.connection_info.username = "alice".to_string();
        paused_shared.connection_info.nickname = "Visitor".to_string();
        let paused_shared_id = paused_shared.id;

        let mut connecting = test_transfer();
        connecting.status = TransferStatus::Connecting;
        let connecting_id = connecting.id;

        let mut transferring = test_transfer();
        transferring.status = TransferStatus::Transferring;
        let transferring_id = transferring.id;

        let mut failed = test_transfer();
        failed.status = TransferStatus::Failed;
        let failed_id = failed.id;

        let mut completed = test_transfer();
        completed.status = TransferStatus::Completed;
        let completed_id = completed.id;

        let mut other_server = test_transfer();
        other_server.connection_info.address = "192.168.1.101".to_string();
        let other_server_id = other_server.id;

        let mut other_user = test_transfer();
        other_user.connection_info.username = "bob".to_string();
        let other_user_id = other_user.id;

        manager.add(queued);
        manager.add(paused_shared);
        manager.add(connecting);
        manager.add(transferring);
        manager.add(failed);
        manager.add(completed);
        manager.add(other_server);
        manager.add(other_user);
        manager.dirty = false;

        assert!(manager.update_username_for_connection(&current_connection, "alice", "alicia"));

        let queued = manager.get(queued_id).unwrap();
        assert_eq!(queued.connection_info.username, "alicia");
        assert_eq!(queued.connection_info.nickname, "alicia");

        let paused_shared = manager.get(paused_shared_id).unwrap();
        assert_eq!(paused_shared.connection_info.username, "alicia");
        assert_eq!(paused_shared.connection_info.nickname, "Visitor");

        assert_eq!(
            manager.get(connecting_id).unwrap().connection_info.username,
            "alicia"
        );
        assert_eq!(
            manager
                .get(transferring_id)
                .unwrap()
                .connection_info
                .username,
            "alicia"
        );
        assert_eq!(
            manager.get(failed_id).unwrap().connection_info.username,
            "alicia"
        );
        assert_eq!(
            manager.get(completed_id).unwrap().connection_info.username,
            "alice"
        );
        assert_eq!(
            manager
                .get(other_server_id)
                .unwrap()
                .connection_info
                .username,
            "alice"
        );
        assert_eq!(
            manager.get(other_user_id).unwrap().connection_info.username,
            "bob"
        );
        assert!(manager.dirty);
    }
}
