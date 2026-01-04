//! Server info message handlers

use iced::Task;
use nexus_common::protocol::ServerInfo;

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

        // Update only the server info fields that were provided
        if let Some(name) = server_info.name {
            conn.server_name = Some(name);
        }
        if let Some(description) = server_info.description {
            conn.server_description = Some(description);
        }
        if let Some(version) = server_info.version {
            conn.server_version = Some(version);
        }
        // max_connections_per_ip is only sent to admins
        if server_info.max_connections_per_ip.is_some() {
            conn.max_connections_per_ip = server_info.max_connections_per_ip;
        }
        // max_transfers_per_ip is only sent to admins
        if server_info.max_transfers_per_ip.is_some() {
            conn.max_transfers_per_ip = server_info.max_transfers_per_ip;
        }
        // Update server image and cached version if provided
        if let Some(image) = server_info.image {
            // Decode first using reference, then move (avoids clone)
            conn.cached_server_image = if image.is_empty() {
                None
            } else {
                decode_data_uri_max_width(&image, SERVER_IMAGE_MAX_CACHE_WIDTH)
            };
            conn.server_image = image;
        }

        self.add_chat_message(connection_id, ChatMessage::system(system_message))
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
                edit_state.error = Some(t_args(
                    "err-failed-update-server-info",
                    &[("error", &error_msg)],
                ));
                Task::none()
            } else {
                self.add_chat_message(
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
