//! Permissions update handler

use crate::i18n::t;
use crate::types::{ChatMessage, Message};
use crate::views::constants::PERMISSION_USER_LIST;
use crate::NexusApp;
use chrono::Local;
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
        // Update the connection's permissions and admin status
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            let had_user_list =
                conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_LIST);

            conn.is_admin = is_admin;
            conn.permissions = permissions.clone();

            let has_user_list =
                is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

            // If user just gained user_list permission, refresh the list
            // (it may be stale from missed join/leave events while permission was revoked)
            if !had_user_list
                && has_user_list
                && let Err(e) = conn.tx.send(ClientMessage::UserList)
            {
                // Channel send failed - add error to chat
                let error_msg = format!("{}: {}", t("err-userlist-failed"), e);
                return self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: t("msg-username-error"),
                        message: error_msg,
                        timestamp: Local::now(),
                    },
                );
            }

            // Show notification message
            let message = ChatMessage {
                username: t("msg-username-system"),
                message: t("msg-permissions-updated"),
                timestamp: Local::now(),
            };
            return self.add_chat_message(connection_id, message);
        }
        Task::none()
    }
}