//! Transfer message handlers

use iced::Task;
use uuid::Uuid;

use crate::NexusApp;
use crate::transfers::{TransferEvent, TransferStatus, request_cancel};
use crate::types::Message;

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
                self.transfer_manager.complete(id);
                self.save_transfers();
            }

            TransferEvent::Failed {
                id,
                error,
                error_kind,
            } => {
                self.transfer_manager.fail(id, error, error_kind);
                self.save_transfers();
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

    /// Handle request to start next queued transfer
    ///
    /// This is called after a transfer completes to check for more queued transfers.
    /// The subscription will pick up the next queued transfer automatically.
    pub fn handle_transfer_start_next(&mut self) -> Task<Message> {
        // Nothing to do here - the subscription automatically picks up
        // the next queued transfer via next_queued()
        Task::none()
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
    /// Requests cancellation of the executor task via the cancellation flag.
    /// The executor will check this flag and abort. We mark it as failed
    /// with a "Cancelled by user" message.
    pub fn handle_transfer_cancel(&mut self, id: Uuid) -> Task<Message> {
        if let Some(transfer) = self.transfer_manager.get(id)
            && transfer.status.is_active()
        {
            // Request the executor to stop
            request_cancel(id);
            // Mark as failed immediately (executor will also send Paused, but we want Failed)
            self.transfer_manager
                .fail(id, "Cancelled by user".to_string(), None);
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

    /// Handle request to open the folder containing a completed transfer
    pub fn handle_transfer_open_folder(&mut self, id: Uuid) -> Task<Message> {
        if let Some(transfer) = self.transfer_manager.get(id) {
            // Get the parent directory of the local path
            let folder = if transfer.is_directory {
                // For directory downloads, the local_path is the directory itself
                transfer.local_path.clone()
            } else {
                // For file downloads, get the parent directory
                transfer
                    .local_path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| transfer.local_path.clone())
            };

            // Open the folder in the system file manager
            if let Err(e) = open::that(&folder) {
                eprintln!("Failed to open folder {:?}: {e}", folder);
            }
        }
        Task::none()
    }

    /// Handle request to clear all completed transfers
    pub fn handle_transfer_clear_completed(&mut self) -> Task<Message> {
        self.transfer_manager.clear_completed();
        self.save_transfers();
        Task::none()
    }

    /// Handle request to clear all failed transfers
    pub fn handle_transfer_clear_failed(&mut self) -> Task<Message> {
        self.transfer_manager.clear_failed();
        self.save_transfers();
        Task::none()
    }

    /// Save transfers to disk (helper to reduce repetition)
    fn save_transfers(&mut self) {
        if let Err(e) = self.transfer_manager.save() {
            eprintln!("Failed to save transfers: {e}");
        }
    }
}
