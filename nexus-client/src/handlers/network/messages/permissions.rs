//! Permissions update handler

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ChatMessage, Message};
use crate::views::constants::PERMISSION_USER_LIST;
use iced::Task;
use nexus_common::protocol::ClientMessage;

impl NexusApp {
    /// Handle permissions updated notification
    pub fn handle_permissions_updated(
        &mut self,
        connection_id: usize,
        is_admin: bool,
        permissions: Vec<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let had_user_list =
            conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        let has_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        conn.is_admin = is_admin;
        conn.permissions = permissions;

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
