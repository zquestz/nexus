//! Certificate fingerprint mismatch handlers

use iced::Task;

use crate::NexusApp;
use crate::i18n::t;
use crate::transfers::update_registry_fingerprint;
use crate::types::Message;

impl NexusApp {
    /// Accept new certificate fingerprint (update stored fingerprint and complete connection)
    pub fn handle_accept_new_fingerprint(&mut self) -> Task<Message> {
        if let Some(mismatch) = self.fingerprint_mismatch_queue.pop_front() {
            let new_fingerprint = mismatch.received.clone();

            // Update the stored fingerprint (handle case where bookmark was deleted)
            if let Some(bookmark) = self.config.get_bookmark_mut(mismatch.bookmark_id) {
                bookmark.certificate_fingerprint = Some(new_fingerprint.clone());
                let _ = self.config.save();
            }

            // Update any queued/paused transfers for this bookmark with the new fingerprint
            self.transfer_manager
                .update_fingerprint_for_bookmark(mismatch.bookmark_id, &new_fingerprint);
            let _ = self.transfer_manager.save();

            // Also update the transfer registry (for transfers waiting to execute)
            update_registry_fingerprint(mismatch.bookmark_id, &new_fingerprint);

            // Complete the connection that was pending
            return self.handle_bookmark_connection_result(
                Ok(mismatch.connection),
                Some(mismatch.bookmark_id),
                mismatch.display_name,
            );
        }
        Task::none()
    }

    /// Reject new certificate fingerprint (cancel connection)
    pub fn handle_cancel_fingerprint_mismatch(&mut self) -> Task<Message> {
        self.fingerprint_mismatch_queue.pop_front();

        if self.fingerprint_mismatch_queue.is_empty() {
            self.connection_form.error = Some(t("msg-connection-cancelled"));
        }

        Task::none()
    }
}
