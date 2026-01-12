//! Error message handler

use iced::Task;

use crate::NexusApp;
use crate::types::{ActivePanel, ChatMessage, Message, UserManagementMode};

// Protocol command names (must match server exactly)
const CMD_USER_EDIT: &str = "UserEdit";
const CMD_USER_UPDATE: &str = "UserUpdate";
const CMD_SERVER_INFO_UPDATE: &str = "ServerInfoUpdate";
const CMD_USER_KICK: &str = "UserKick";

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

        // Show error in server info edit form if it's for server info update
        if self.is_server_info_edit_error(&command, connection_id) {
            let Some(conn) = self.connections.get_mut(&connection_id) else {
                return Task::none();
            };
            if let Some(edit_state) = &mut conn.server_info_edit {
                edit_state.error = Some(message);
                return Task::none();
            }
        }

        // Check if this is a kick notification - store for use on disconnect
        if command.as_deref() == Some(CMD_USER_KICK)
            && let Some(conn) = self.connections.get_mut(&connection_id)
        {
            conn.pending_kick_message = Some(message.clone());
        }

        // For other errors (including UserDelete), show in chat
        self.add_active_tab_message(connection_id, ChatMessage::error(message))
    }

    /// Check if error should be shown in user edit form
    fn is_user_edit_error(&self, command: &Option<String>, connection_id: usize) -> bool {
        let Some(cmd) = command else {
            return false;
        };

        let Some(conn) = self.connections.get(&connection_id) else {
            return false;
        };

        (cmd == CMD_USER_EDIT || cmd == CMD_USER_UPDATE)
            && conn.active_panel == ActivePanel::UserManagement
            && matches!(conn.user_management.mode, UserManagementMode::Edit { .. })
    }

    /// Check if error should be shown in server info edit form
    fn is_server_info_edit_error(&self, command: &Option<String>, connection_id: usize) -> bool {
        let Some(cmd) = command else {
            return false;
        };

        let Some(conn) = self.connections.get(&connection_id) else {
            return false;
        };

        // ServerInfo panel can be in display or edit mode, so also check edit state
        cmd == CMD_SERVER_INFO_UPDATE
            && conn.active_panel == ActivePanel::ServerInfo
            && conn.server_info_edit.is_some()
    }
}
