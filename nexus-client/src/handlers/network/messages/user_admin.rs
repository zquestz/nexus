//! User administration response handlers

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ActivePanel, ChatMessage, Message};
use iced::Task;

/// Data from a UserEditResponse message
pub struct UserEditResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub username: Option<String>,
    pub is_admin: Option<bool>,
    pub enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
}

impl NexusApp {
    /// Handle user create response
    pub fn handle_user_create_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if success {
            // Close add user panel on success
            if self.ui_state.active_panel == ActivePanel::AddUser
                && self.active_connection == Some(connection_id)
            {
                self.ui_state.active_panel = ActivePanel::None;
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.clear_add_user();
                }
            }

            // add_chat_message handles scrolling when this is the active connection
            return self
                .add_chat_message(connection_id, ChatMessage::system(t("msg-user-created")));
        }

        // On error, keep panel open and show error in form
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };
        conn.user_management.create_error = Some(error.unwrap_or_default());
        Task::none()
    }

    /// Handle user delete response
    pub fn handle_user_delete_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if success {
            // Close edit panel on success
            if self.ui_state.active_panel == ActivePanel::EditUser
                && self.active_connection == Some(connection_id)
            {
                self.ui_state.active_panel = ActivePanel::None;
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.clear_edit_user();
                }
            }

            // add_chat_message handles scrolling when this is the active connection
            return self
                .add_chat_message(connection_id, ChatMessage::system(t("msg-user-deleted")));
        }

        // On error, keep panel open and show error in form
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };
        conn.user_management.edit_error = Some(error.unwrap_or_default());
        Task::none()
    }

    /// Handle user edit response (stage 2 - loading user details)
    pub fn handle_user_edit_response(
        &mut self,
        connection_id: usize,
        data: UserEditResponseData,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        if data.success {
            // Load the user details into edit form (stage 2)
            conn.user_management.load_user_for_editing(
                data.username.unwrap_or_default(),
                data.is_admin.unwrap_or(false),
                data.enabled.unwrap_or(true),
                data.permissions.unwrap_or_default(),
            );
        } else {
            // On error, keep panel open and show error in form
            conn.user_management.edit_error = Some(data.error.unwrap_or_default());
        }

        Task::none()
    }

    /// Handle user update response
    pub fn handle_user_update_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if success {
            // Close edit panel on success
            if self.ui_state.active_panel == ActivePanel::EditUser
                && self.active_connection == Some(connection_id)
            {
                self.ui_state.active_panel = ActivePanel::None;
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.clear_edit_user();
                }
            }

            // add_chat_message handles scrolling when this is the active connection
            return self
                .add_chat_message(connection_id, ChatMessage::system(t("msg-user-updated")));
        }

        // On error, keep panel open and show error in form
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };
        conn.user_management.edit_error = Some(error.unwrap_or_default());
        Task::none()
    }
}
