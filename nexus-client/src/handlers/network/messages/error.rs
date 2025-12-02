//! Error message handler

use crate::NexusApp;
use crate::types::{ActivePanel, ChatMessage, Message};
use iced::Task;

// Protocol command names (must match server exactly)
const CMD_USER_EDIT: &str = "UserEdit";
const CMD_USER_UPDATE: &str = "UserUpdate";

impl NexusApp {
    /// Handle error message from server
    pub fn handle_error(
        &mut self,
        connection_id: usize,
        message: String,
        command: Option<String>,
    ) -> Task<Message> {
        // Show error in edit user form if it's for user management commands
        if self.is_user_edit_error(&command, connection_id) {
            let Some(conn) = self.connections.get_mut(&connection_id) else {
                return Task::none();
            };
            conn.user_management.edit_error = Some(message);
            return Task::none();
        }

        // For other errors (including UserDelete), show in chat
        self.add_chat_message(connection_id, ChatMessage::error(message))
    }

    /// Check if error should be shown in user edit form
    fn is_user_edit_error(&self, command: &Option<String>, connection_id: usize) -> bool {
        let Some(cmd) = command else {
            return false;
        };

        (cmd == CMD_USER_EDIT || cmd == CMD_USER_UPDATE)
            && self.ui_state.active_panel == ActivePanel::EditUser
            && self.active_connection == Some(connection_id)
    }
}
