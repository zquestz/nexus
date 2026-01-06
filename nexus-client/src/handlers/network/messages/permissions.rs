//! Permissions update handler

use iced::Task;
use nexus_common::protocol::{ChatInfo, ClientMessage, ServerInfo};

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::t;
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::{ChatMessage, Message};
use crate::views::constants::PERMISSION_USER_LIST;

impl NexusApp {
    /// Handle permissions updated notification
    pub fn handle_permissions_updated(
        &mut self,
        connection_id: usize,
        is_admin: bool,
        permissions: Vec<String>,
        server_info: Option<ServerInfo>,
        chat_info: Option<ChatInfo>,
    ) -> Task<Message> {
        // Get server name before mutable borrow for notification
        let server_name = self
            .connections
            .get(&connection_id)
            .map(|c| c.connection_info.server_name.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| t("unknown-server"));

        // Emit permissions changed notification
        emit_event(
            self,
            EventType::PermissionsChanged,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_server_name(&server_name),
        );

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let had_user_list = conn.has_permission(PERMISSION_USER_LIST);

        let has_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        conn.is_admin = is_admin;
        conn.permissions = permissions;

        // Update only the server info fields that were provided
        // (PermissionsUpdated only sends fields that change with permissions, like max_connections_per_ip)
        if let Some(info) = server_info {
            if let Some(name) = info.name {
                conn.server_name = Some(name);
            }
            if let Some(description) = info.description {
                conn.server_description = Some(description);
            }
            if let Some(version) = info.version {
                conn.server_version = Some(version);
            }
            if info.max_connections_per_ip.is_some() {
                conn.max_connections_per_ip = info.max_connections_per_ip;
            }
            if let Some(image) = info.image {
                conn.server_image = image.clone();
                conn.cached_server_image = if image.is_empty() {
                    None
                } else {
                    decode_data_uri_max_width(&image, SERVER_IMAGE_MAX_CACHE_WIDTH)
                };
            }
        }

        // Update chat info separately
        if let Some(info) = chat_info {
            // Empty strings mean not set
            conn.chat_topic = if info.topic.is_empty() {
                None
            } else {
                Some(info.topic)
            };
            conn.chat_topic_set_by = if info.topic_set_by.is_empty() {
                None
            } else {
                Some(info.topic_set_by)
            };
        } else {
            // No chat_info means no permission to see topic
            conn.chat_topic = None;
            conn.chat_topic_set_by = None;
        }

        // If user just gained user_list permission, refresh the list
        // (it may be stale from missed join/leave events while permission was revoked)
        if !had_user_list
            && has_user_list
            && let Err(e) = conn.send(ClientMessage::UserList { all: false })
        {
            // Channel send failed - add error to chat
            let error_msg = format!("{}: {}", t("err-userlist-failed"), e);
            return self.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }

        // Show notification message
        self.add_chat_message(
            connection_id,
            ChatMessage::system(t("msg-permissions-updated")),
        )
    }
}
