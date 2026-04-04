//! Server info message handlers

use iced::Task;
use nexus_common::protocol::ServerInfo;

use nexus_common::validators::PasswordStrength;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle server info updated notification
    pub fn handle_server_info_updated(
        &mut self,
        connection_id: usize,
        server_info: ServerInfo,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Build system message
        let system_message = t("msg-server-info-updated");

        // Update server info fields unconditionally (None means "cleared/not visible").
        // The server already does per-user permission filtering, so we trust the values.
        // Exception: image keeps a guard because the broadcast always includes it,
        // and we need to decode it before storing.
        conn.server_name = server_info.name;
        conn.server_description = server_info.description;
        conn.server_version = server_info.version;
        conn.max_connections_per_ip = server_info.max_connections_per_ip;
        conn.max_transfers_per_ip = server_info.max_transfers_per_ip;
        conn.file_reindex_interval = server_info.file_reindex_interval;
        conn.persistent_channels = server_info.persistent_channels;
        conn.auto_join_channels = server_info.auto_join_channels;
        conn.chat_burst_limit = server_info.chat_burst_limit;
        conn.chat_rate_limit = server_info.chat_rate_limit;
        if let Some(score) = server_info.min_password_strength {
            conn.min_password_strength = PasswordStrength::from(score);
        }
        conn.log_level = server_info.log_level;
        // Image needs special handling for decoding
        if let Some(image) = server_info.image {
            conn.cached_server_image = if image.is_empty() {
                None
            } else {
                decode_data_uri_max_width(&image, SERVER_IMAGE_MAX_CACHE_WIDTH)
            };
            conn.server_image = image;
        }

        self.add_console_message(connection_id, ChatMessage::system(system_message))
    }

    /// Handle server info update response
    pub fn handle_server_info_update_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        if success {
            // Exit edit mode on success (no message - the broadcast will show SYS message)
            conn.server_info_edit = None;
            Task::none()
        } else {
            // Show error in the edit form if still open, otherwise show in chat
            let error_msg = error.unwrap_or_default();
            if let Some(edit_state) = &mut conn.server_info_edit {
                edit_state.is_submitting = false;
                edit_state.error = Some(t_args(
                    "err-failed-update-server-info",
                    &[("error", &error_msg)],
                ));
                Task::none()
            } else {
                self.add_console_message(
                    connection_id,
                    ChatMessage::error(t_args(
                        "err-failed-update-server-info",
                        &[("error", &error_msg)],
                    )),
                )
            }
        }
    }
}
