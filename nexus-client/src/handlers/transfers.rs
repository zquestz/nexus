//! Transfer message handlers

use iced::Task;
use uuid::Uuid;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::t;
use crate::transfers::{TransferDirection, TransferEvent, TransferStatus, request_cancel};
use crate::types::{ActivePanel, Message};

impl NexusApp {
    /// Handle transfer progress event from executor
    pub fn handle_transfer_progress(&mut self, event: TransferEvent) -> Task<Message> {
        match event {
            TransferEvent::Connecting { id } => {
                self.transfer_manager.set_connecting(id);
                self.save_transfers();
            }

            TransferEvent::Started {
                id,
                total_bytes,
                file_count,
                server_transfer_id,
            } => {
                self.transfer_manager.set_transferring(
                    id,
                    total_bytes,
                    file_count,
                    Some(server_transfer_id),
                );
                self.save_transfers();
            }

            TransferEvent::Progress {
                id,
                transferred_bytes,
                files_completed,
                current_file,
            } => {
                self.transfer_manager.update_progress(
                    id,
                    transferred_bytes,
                    files_completed,
                    current_file,
                );
                // Don't save on every progress update - too expensive
            }

            TransferEvent::FileCompleted { id: _, path: _ } => {
                // File completion is already tracked via Progress events
            }

            TransferEvent::Completed { id } => {
                // Get transfer info before marking complete for notification
                let transfer_info = self
                    .transfer_manager
                    .get(id)
                    .map(|t| (t.direction, t.remote_path.clone()));

                // Check if we should refresh the file list before marking complete
                let should_refresh = self.should_refresh_after_upload(id);

                self.transfer_manager.complete(id);
                self.save_transfers();

                // Emit transfer complete notification
                if let Some((direction, path)) = transfer_info {
                    emit_event(
                        self,
                        EventType::TransferComplete,
                        EventContext::new()
                            .with_path(&path)
                            .with_is_upload(direction == TransferDirection::Upload),
                    );
                }

                // Refresh file list if upload completed to current directory
                if should_refresh {
                    return self.update(Message::FileRefresh);
                }
            }

            TransferEvent::Failed {
                id,
                error,
                error_kind,
            } => {
                // Get transfer info before marking failed for notification
                let transfer_info = self
                    .transfer_manager
                    .get(id)
                    .map(|t| (t.direction, t.remote_path.clone()));

                self.transfer_manager.fail(id, error.clone(), error_kind);
                self.save_transfers();

                // Emit transfer failed notification
                if let Some((direction, path)) = transfer_info {
                    emit_event(
                        self,
                        EventType::TransferFailed,
                        EventContext::new()
                            .with_path(&path)
                            .with_error(&error)
                            .with_is_upload(direction == TransferDirection::Upload),
                    );
                }
            }

            TransferEvent::Paused { id } => {
                // Only update to Paused if not already Failed (cancel sets Failed immediately)
                if let Some(transfer) = self.transfer_manager.get(id)
                    && !transfer.status.is_failed()
                {
                    self.transfer_manager.pause(id);
                    self.save_transfers();
                }
            }
        }

        Task::none()
    }

    /// Check if we should refresh the file list after an upload completes
    ///
    /// Returns true if:
    /// - The transfer is an upload
    /// - We're connected to the same server (address:port match)
    /// - Files panel is active
    /// - Current path matches the upload destination
    fn should_refresh_after_upload(&self, transfer_id: Uuid) -> bool {
        let Some(transfer) = self.transfer_manager.get(transfer_id) else {
            return false;
        };

        // Only for uploads
        if transfer.direction != TransferDirection::Upload {
            return false;
        }

        let Some(conn_id) = self.active_connection else {
            return false;
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return false;
        };

        // Must be viewing Files panel
        if conn.active_panel != ActivePanel::Files {
            return false;
        }

        // Check if connected to the same server
        if conn.connection_info.address != transfer.connection_info.address
            || conn.connection_info.port != transfer.connection_info.port
        {
            return false;
        }

        // Check if viewing the upload destination directory.
        //
        // `remote_path` means different things depending on the upload kind:
        //   - File: it's the destination directory (where the file lands).
        //   - Directory: it's the directory the server creates — the actual
        //     destination is its parent.
        let upload_parent: &str = if transfer.is_directory {
            match transfer.remote_path.rfind('/') {
                // Root case: "/MyFolder" → parent is "/"
                Some(0) => "/",
                // Nested: "/Uploads/MyFolder" → parent is "/Uploads"
                Some(idx) => &transfer.remote_path[..idx],
                // No slash at all — treat as empty / root
                None => "",
            }
        } else {
            &transfer.remote_path
        };
        let current_path = &conn.files_management.active_tab().current_path;
        current_path == upload_parent
    }

    /// Handle request to pause a transfer
    ///
    /// Requests cancellation of the executor task via the cancellation flag.
    /// The executor will check this flag and abort, sending a Paused event.
    /// The transfer can then be resumed later using the .part file for resume support.
    pub fn handle_transfer_pause(&mut self, id: Uuid) -> Task<Message> {
        if let Some(transfer) = self.transfer_manager.get(id)
            && (transfer.status == TransferStatus::Transferring
                || transfer.status == TransferStatus::Connecting)
        {
            // Request the executor to stop
            request_cancel(id);
            // Status will be updated when we receive the Paused event from executor
        }
        Task::none()
    }

    /// Handle request to resume a paused transfer
    pub fn handle_transfer_resume(&mut self, id: Uuid) -> Task<Message> {
        // Re-queue the transfer so the subscription picks it up
        if let Some(transfer) = self.transfer_manager.get(id)
            && (transfer.status == TransferStatus::Paused
                || transfer.status == TransferStatus::Failed)
        {
            self.transfer_manager.queue(id);
            self.save_transfers();
        }
        Task::none()
    }

    /// Handle request to cancel a transfer
    ///
    /// For active transfers: requests cancellation via the flag and marks as failed.
    /// For queued/paused transfers: just removes them (no executor running).
    pub fn handle_transfer_cancel(&mut self, id: Uuid) -> Task<Message> {
        let Some(transfer) = self.transfer_manager.get(id) else {
            return Task::none();
        };

        let is_active = transfer.status.is_active();
        let is_queued = transfer.status == TransferStatus::Queued;
        let is_paused = transfer.status == TransferStatus::Paused;

        if is_active {
            // Request the executor to stop
            request_cancel(id);
            // Mark as failed
            self.transfer_manager
                .fail(id, t("transfer-cancelled"), None);
            self.save_transfers();
        } else if is_queued || is_paused {
            // Not started yet - just remove it
            self.transfer_manager.remove(id);
            self.save_transfers();
        }
        Task::none()
    }

    /// Handle request to remove a transfer from the list
    pub fn handle_transfer_remove(&mut self, id: Uuid) -> Task<Message> {
        // Only allow removing completed or failed transfers
        if let Some(transfer) = self.transfer_manager.get(id)
            && (transfer.status.is_completed() || transfer.status.is_failed())
        {
            self.transfer_manager.remove(id);
            self.save_transfers();
        }
        Task::none()
    }

    /// Handle request to open the folder containing a transfer's local path
    pub fn handle_transfer_open_folder(&mut self, id: Uuid) -> Task<Message> {
        if let Some(transfer) = self.transfer_manager.get(id) {
            // Get the parent directory of the transfer's local path
            if let Some(parent) = transfer.local_path.parent() {
                let _ = open::that(parent);
            }
        }
        Task::none()
    }

    /// Handle request to clear all inactive (completed and failed) transfers
    pub fn handle_transfer_clear_inactive(&mut self) -> Task<Message> {
        self.transfer_manager.clear_completed();
        self.transfer_manager.clear_failed();
        self.save_transfers();
        Task::none()
    }

    /// Handle request to move a queued transfer up (higher priority)
    pub fn handle_transfer_move_up(&mut self, id: Uuid) -> Task<Message> {
        if self.transfer_manager.move_up(id) {
            self.save_transfers();
        }
        Task::none()
    }

    /// Handle request to move a queued transfer down (lower priority)
    pub fn handle_transfer_move_down(&mut self, id: Uuid) -> Task<Message> {
        if self.transfer_manager.move_down(id) {
            self.save_transfers();
        }
        Task::none()
    }

    /// Handle request to retry a failed transfer (re-queue)
    pub fn handle_transfer_retry(&mut self, id: Uuid) -> Task<Message> {
        // Same as resume - re-queue the failed transfer
        if let Some(transfer) = self.transfer_manager.get(id)
            && transfer.status.is_failed()
        {
            self.transfer_manager.queue(id);
            self.save_transfers();
        }
        Task::none()
    }

    /// Save transfers to disk (helper to reduce repetition)
    fn save_transfers(&mut self) {
        let _ = self.transfer_manager.save();
    }
}
