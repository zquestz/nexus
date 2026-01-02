//! Transfer message handlers

use iced::Task;
use uuid::Uuid;

use crate::NexusApp;
use crate::transfers::{TransferEvent, TransferStatus};
use crate::types::Message;

// Allow unused handlers - UI buttons for these will be added later
#[allow(dead_code)]
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
                self.transfer_manager.pause(id);
                self.save_transfers();
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
    /// TODO: Implement executor cancellation. Currently this only updates the status
    /// but the executor continues running. Both pause and cancel should abort the
    /// executor task immediately (cutting the connection). The transfer can then
    /// be resumed later using the .part file for resume support.
    pub fn handle_transfer_pause(&mut self, id: Uuid) -> Task<Message> {
        // TODO: Abort the executor task to stop the transfer immediately
        if let Some(transfer) = self.transfer_manager.get(id)
            && (transfer.status == TransferStatus::Transferring
                || transfer.status == TransferStatus::Connecting)
        {
            self.transfer_manager.pause(id);
            self.save_transfers();
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
    /// TODO: Implement executor cancellation. Currently this only updates the status
    /// but the executor continues running. Cancel should abort the executor task
    /// immediately (cutting the connection), same as pause but marking as Failed
    /// instead of Paused.
    pub fn handle_transfer_cancel(&mut self, id: Uuid) -> Task<Message> {
        // TODO: Abort the executor task to stop the transfer immediately
        if let Some(transfer) = self.transfer_manager.get(id)
            && transfer.status.is_active()
        {
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

    /// Save transfers to disk (helper to reduce repetition)
    fn save_transfers(&mut self) {
        if let Err(e) = self.transfer_manager.save() {
            eprintln!("Failed to save transfers: {e}");
        }
    }
}
