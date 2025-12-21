//! Certificate fingerprint verification and handling

use crate::NexusApp;
use crate::types::{FingerprintMismatch, FingerprintMismatchDetails, Message, NetworkConnection};
use iced::Task;

impl NexusApp {
    /// Verify certificate fingerprint matches stored value, or save on first connection (TOFU)
    pub fn verify_and_save_fingerprint(
        &mut self,
        bookmark_index: Option<usize>,
        fingerprint: &str,
    ) -> Result<(), Box<FingerprintMismatchDetails>> {
        let Some(idx) = bookmark_index else {
            // No bookmark - nothing to verify
            return Ok(());
        };

        let Some(bookmark) = self.config.bookmarks.get_mut(idx) else {
            // Invalid bookmark index - nothing to verify
            return Ok(());
        };

        match &bookmark.certificate_fingerprint {
            None => {
                // First connection - save fingerprint (Trust On First Use)
                bookmark.certificate_fingerprint = Some(fingerprint.to_string());
                let _ = self.config.save();
                Ok(())
            }
            Some(stored) => {
                // Verify fingerprint matches
                if stored == fingerprint {
                    Ok(())
                } else {
                    Err(Box::new(FingerprintMismatchDetails {
                        bookmark_index: idx,
                        expected: stored.clone(),
                        received: fingerprint.to_string(),
                        bookmark_name: bookmark.name.clone(),
                        server_address: bookmark.address.clone(),
                        server_port: bookmark.port.to_string(),
                    }))
                }
            }
        }
    }

    /// Handle fingerprint mismatch by queuing it for user verification
    pub fn handle_fingerprint_mismatch(
        &mut self,
        details: FingerprintMismatchDetails,
        conn: NetworkConnection,
        display_name: String,
    ) -> Task<Message> {
        self.fingerprint_mismatch_queue
            .push_back(FingerprintMismatch {
                bookmark_index: details.bookmark_index,
                expected: details.expected,
                received: details.received,
                bookmark_name: details.bookmark_name,
                server_address: details.server_address,
                server_port: details.server_port,
                connection: conn,
                display_name,
            });

        self.connection_form.is_connecting = false;
        Task::none()
    }
}
