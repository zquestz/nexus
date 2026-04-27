//! Certificate fingerprint mismatch handlers

use iced::Task;

use crate::NexusApp;
use crate::history::rotate_fingerprint;
use crate::i18n::t;
use crate::transfers::update_registry_fingerprint;
use crate::types::{Message, ReconnectAction};

impl NexusApp {
    /// Accept new certificate fingerprint and replay the original connect
    /// attempt with the updated bookmark.
    ///
    /// The retry preserves the variant-specific intent: manual connects
    /// re-dispatch to `ConnectionResult`, bookmark connects to
    /// `BookmarkConnectionResult`, URI connects to `UriConnectionResult`
    /// (including the path-navigation hint).
    pub fn handle_accept_new_fingerprint(&mut self) -> Task<Message> {
        let Some(mismatch) = self.fingerprint_mismatch_queue.pop_front() else {
            return Task::none();
        };

        let new_fingerprint = mismatch.received.clone();
        let old_fingerprint = mismatch.expected.clone();

        // Rotate history files from old fingerprint to new.
        let _ = rotate_fingerprint(&old_fingerprint, &new_fingerprint);

        // Update the stored bookmark fingerprint (handle case where the
        // bookmark was deleted between mismatch and accept).
        if let Some(bookmark) = self.config.get_bookmark_mut(mismatch.bookmark_id) {
            bookmark.certificate_fingerprint = Some(new_fingerprint.clone());
            let _ = self.config.save();
        }

        // Update queued/paused transfers and the in-flight transfer registry.
        self.transfer_manager
            .update_fingerprint_for_bookmark(mismatch.bookmark_id, &new_fingerprint);
        let _ = self.transfer_manager.save();
        update_registry_fingerprint(mismatch.bookmark_id, &new_fingerprint);

        // Replay the original connect attempt. Allocate a fresh connection_id
        // and refresh the expected_fingerprint so stage 1 passes on retry.
        match mismatch.retry_action {
            ReconnectAction::Manual { mut params } => {
                params.connection_id = self.next_connection_id;
                self.next_connection_id += 1;
                params.expected_fingerprint = Some(new_fingerprint.clone());
                // Re-arm the form lock to mirror `handle_connect_pressed`;
                // released in `handle_connection_result` on entry.
                self.connection_form.is_connecting = true;
                let retry_params = params.clone();
                Task::perform(
                    async move { crate::network::connect_to_server(params).await },
                    move |result| Message::ConnectionResult {
                        result,
                        params: retry_params,
                    },
                )
            }
            ReconnectAction::Bookmark {
                mut params,
                bookmark_id,
                display_name,
            } => {
                params.connection_id = self.next_connection_id;
                self.next_connection_id += 1;
                params.expected_fingerprint = Some(new_fingerprint.clone());
                self.connecting_bookmarks.insert(bookmark_id);
                let retry_params = params.clone();
                Task::perform(
                    async move { crate::network::connect_to_server(params).await },
                    move |result| Message::BookmarkConnectionResult {
                        result,
                        params: retry_params,
                        bookmark_id,
                        display_name,
                    },
                )
            }
            ReconnectAction::Uri {
                mut params,
                display_name,
                path,
            } => {
                params.connection_id = self.next_connection_id;
                self.next_connection_id += 1;
                params.expected_fingerprint = Some(new_fingerprint.clone());
                let retry_params = params.clone();
                Task::perform(
                    async move { crate::network::connect_to_server(params).await },
                    move |result| Message::UriConnectionResult {
                        result,
                        params: retry_params,
                        display_name,
                        path,
                    },
                )
            }
        }
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
