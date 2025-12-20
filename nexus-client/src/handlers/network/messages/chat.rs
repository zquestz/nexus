//! Chat message handlers

use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ServerInfo;

impl NexusApp {
    /// Handle incoming chat message
    pub fn handle_chat_message(
        &mut self,
        connection_id: usize,
        nickname: String,
        message: String,
        is_admin: bool,
        is_shared: bool,
    ) -> Task<Message> {
        let chat_message = ChatMessage::with_timestamp_and_status(
            nickname,
            message,
            chrono::Local::now(),
            is_admin,
            is_shared,
        );
        self.add_chat_message(connection_id, chat_message)
    }

    /// Handle chat topic change notification
    pub fn handle_chat_topic(
        &mut self,
        connection_id: usize,
        topic: String,
        username: String,
    ) -> Task<Message> {
        // Store the topic and who set it
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        conn.chat_topic = if topic.is_empty() {
            None
        } else {
            Some(topic.clone())
        };
        conn.chat_topic_set_by = if username.is_empty() {
            None
        } else {
            Some(username.clone())
        };

        // Build message after releasing mutable borrow
        let message = if topic.is_empty() {
            t_args("msg-topic-cleared", &[("username", &username)])
        } else {
            t_args(
                "msg-topic-set",
                &[("username", &username), ("topic", &topic)],
            )
        };
        self.add_chat_message(connection_id, ChatMessage::system(message))
    }

    /// Handle chat topic update response
    pub fn handle_chat_topic_update_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            ChatMessage::info(t("msg-topic-updated"))
        } else {
            ChatMessage::error(t_args(
                "err-failed-update-topic",
                &[("error", &error.unwrap_or_default())],
            ))
        };
        self.add_chat_message(connection_id, message)
    }

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
        // Update server image and cached version if provided
        if let Some(image) = server_info.image {
            conn.server_image = image.clone();
            conn.cached_server_image = if image.is_empty() {
                None
            } else {
                decode_data_uri_max_width(&image, SERVER_IMAGE_MAX_CACHE_WIDTH)
            };
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
