//! Error message handler

use crate::i18n::t;
use crate::types::{ActivePanel, ChatMessage, Message};
use crate::NexusApp;
use chrono::Local;
use iced::Task;

impl NexusApp {
    /// Handle error message from server
    pub fn handle_error(
        &mut self,
        connection_id: usize,
        message: String,
        command: Option<String>,
    ) -> Task<Message> {
        // Show error in edit user form if it's for user management commands
        // Note: These command names come from the server protocol and must match exactly
        if let Some(ref cmd) = command
            && (cmd == "UserEdit" || cmd == "UserUpdate")
            && self.ui_state.active_panel == ActivePanel::EditUser
            && self.active_connection == Some(connection_id)
        {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.edit_error = Some(message);
            }
            return Task::none();
        }

        // For other errors (including UserDelete), show in chat
        self.add_chat_message(
            connection_id,
            ChatMessage {
                username: t("msg-username-error"),
                message,
                timestamp: Local::now(),
            },
        )
    }
}