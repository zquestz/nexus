//! Permissions update handler

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ChatMessage, Message};
use crate::views::constants::PERMISSION_USER_LIST;
use iced::Task;
use nexus_common::protocol::{ClientMessage, ServerInfo};

impl NexusApp {
    /// Handle permissions updated notification
    pub fn handle_permissions_updated(
        &mut self,
        connection_id: usize,
        is_admin: bool,
        permissions: Vec<String>,
        server_info: Option<ServerInfo>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let had_user_list =
            conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        let has_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        conn.is_admin = is_admin;
        conn.permissions = permissions;

        // Update server info fields from the server's authoritative data
        if let Some(info) = server_info {
            conn.server_name = Some(info.name);
            conn.server_description = Some(info.description);
            conn.server_version = Some(info.version);
            // Empty strings mean no permission or not set
            conn.chat_topic = if info.chat_topic.is_empty() {
                None
            } else {
                Some(info.chat_topic)
            };
            conn.chat_topic_set_by = if info.chat_topic_set_by.is_empty() {
                None
            } else {
                Some(info.chat_topic_set_by)
            };
            conn.max_connections_per_ip = info.max_connections_per_ip;
        }

        // If user just gained user_list permission, refresh the list
        // (it may be stale from missed join/leave events while permission was revoked)
        if !had_user_list
            && has_user_list
            && let Err(e) = conn.tx.send(ClientMessage::UserList)
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
