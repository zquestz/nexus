//! User administration response handlers

use nexus_common::framing::MessageId;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::InputId;
use crate::types::{ActivePanel, ChatMessage, Message, ResponseRouting, UserManagementMode};
use iced::Task;
use iced::widget::{Id, operation};

/// Data from a UserEditResponse message
pub struct UserEditResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub username: Option<String>,
    pub is_admin: Option<bool>,
    pub is_shared: Option<bool>,
    pub enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
}

impl NexusApp {
    /// Handle user create response
    ///
    /// If tracked via ResponseRouting::UserManagementCreateResult, returns to list view
    /// and refreshes the user list on success.
    pub fn handle_user_create_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        username: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from user management panel
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat with username if available
            let message = if let Some(ref name) = username {
                t_args("msg-user-created-name", &[("username", name)])
            } else {
                t("msg-user-created")
            };
            let task = self.add_chat_message(connection_id, ChatMessage::system(message));

            // If from user management panel, return to list and refresh
            if matches!(routing, Some(ResponseRouting::UserManagementCreateResult)) {
                return Task::batch([task, self.return_to_user_management_list(connection_id)]);
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::UserManagementCreateResult)) {
            // Show error in create form
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.create_error = Some(error.unwrap_or_default());
            }
        } else {
            // Show error in chat
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        Task::none()
    }

    /// Handle user delete response
    ///
    /// If tracked via ResponseRouting::UserManagementDeleteResult, closes the delete
    /// dialog on success and refreshes the user list, or shows error in dialog on failure.
    pub fn handle_user_delete_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        username: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from user management panel
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat with username if available
            let message = if let Some(ref name) = username {
                t_args("msg-user-deleted-name", &[("username", name)])
            } else {
                t("msg-user-deleted")
            };
            let task = self.add_chat_message(connection_id, ChatMessage::system(message));

            // If from user management panel, close dialog and refresh the list
            if matches!(routing, Some(ResponseRouting::UserManagementDeleteResult)) {
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.mode = UserManagementMode::List;
                    conn.user_management.delete_error = None;
                }
                return Task::batch([task, self.refresh_user_management_list_for(connection_id)]);
            }

            return task;
        }

        // On error, show in the delete dialog (keep it open for retry)
        if matches!(routing, Some(ResponseRouting::UserManagementDeleteResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.delete_error = Some(error.unwrap_or_default());
            }
        } else {
            // Show error in chat
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        Task::none()
    }

    /// Handle user edit response (loading user details for editing)
    ///
    /// If tracked via ResponseRouting::PopulateUserManagementEdit, populates the edit form.
    pub fn handle_user_edit_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        data: UserEditResponseData,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from user management panel
        let routing = conn.pending_requests.remove(&message_id);

        if data.success {
            // If from user management panel, load the user details into edit form
            if matches!(routing, Some(ResponseRouting::PopulateUserManagementEdit)) {
                conn.user_management.enter_edit_mode(
                    data.username.unwrap_or_default(),
                    data.is_admin.unwrap_or(false),
                    data.is_shared.unwrap_or(false),
                    data.enabled.unwrap_or(true),
                    data.permissions.unwrap_or_default(),
                );
            }
        } else {
            // On error, show in the appropriate place
            if matches!(routing, Some(ResponseRouting::PopulateUserManagementEdit)) {
                // Show error on list view
                conn.user_management.list_error = Some(data.error.unwrap_or_default());
            } else {
                // Show error in chat
                return self.add_chat_message(
                    connection_id,
                    ChatMessage::error(data.error.unwrap_or_default()),
                );
            }
        }

        Task::none()
    }

    /// Handle user update response
    ///
    /// If tracked via ResponseRouting::UserManagementUpdateResult, returns to list view
    /// and refreshes the user list on success.
    /// If tracked via ResponseRouting::PasswordChangeResult, closes the password change form
    /// on success, or shows error in the form on failure.
    pub fn handle_user_update_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        username: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from user management panel or password change
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Handle password change success
            if matches!(routing, Some(ResponseRouting::PasswordChangeResult)) {
                // Get return panel before clearing state
                let return_panel = self
                    .connections
                    .get(&connection_id)
                    .and_then(|conn| conn.password_change_state.as_ref())
                    .and_then(|state| state.return_to_panel);

                // Clear password change state and return to original panel
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.password_change_state = None;
                    conn.active_panel = return_panel.unwrap_or(ActivePanel::None);
                }
                // Show success message in chat
                return self.add_chat_message(
                    connection_id,
                    ChatMessage::system(t("msg-password-changed")),
                );
            }

            // Show success message in chat with username if available
            let message = if let Some(ref name) = username {
                t_args("msg-user-updated-name", &[("username", name)])
            } else {
                t("msg-user-updated")
            };
            let task = self.add_chat_message(connection_id, ChatMessage::system(message));

            // If from user management panel, return to list and refresh
            if matches!(routing, Some(ResponseRouting::UserManagementUpdateResult)) {
                return Task::batch([task, self.return_to_user_management_list(connection_id)]);
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::PasswordChangeResult)) {
            // Show error in password change form
            if let Some(conn) = self.connections.get_mut(&connection_id)
                && let Some(state) = &mut conn.password_change_state
            {
                state.error = Some(error.unwrap_or_default());
                // Clear password fields on error for security
                state.current_password.clear();
                state.new_password.clear();
                state.confirm_password.clear();
            }
            // Focus the current password field again
            return Task::batch([operation::focus(Id::from(InputId::ChangePasswordCurrent))]);
        } else if matches!(routing, Some(ResponseRouting::UserManagementUpdateResult)) {
            // Show error in edit form
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.edit_error = Some(error.unwrap_or_default());
            }
        } else {
            // Show error in chat
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        Task::none()
    }

    // ==================== Helper Functions ====================

    /// Return to user management list view (or original panel) and refresh the list
    fn return_to_user_management_list(&mut self, connection_id: usize) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if we should return to a different panel (e.g., UserInfo)
        if let Some(return_panel) = conn.user_management.return_to_panel {
            conn.user_management.return_to_panel = None;
            conn.user_management.mode = UserManagementMode::List;
            conn.user_management.edit_error = None;
            conn.active_panel = return_panel;
            return Task::none();
        }

        // Reset to list mode
        conn.user_management.reset_to_list();

        // Refresh the user list
        self.refresh_user_management_list_for(connection_id)
    }

    /// Refresh user management list for a specific connection
    fn refresh_user_management_list_for(&mut self, connection_id: usize) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Only refresh if we're in the user management panel
        if conn.active_panel != ActivePanel::UserManagement {
            return Task::none();
        }

        // Clear user list to show loading state
        conn.user_management.all_users = None;

        // Request user list from server
        use crate::types::PendingRequests;
        use nexus_common::protocol::ClientMessage;

        match conn.send(ClientMessage::UserList { all: true }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateUserManagementList);
            }
            Err(e) => {
                conn.user_management.all_users =
                    Some(Err(format!("{}: {}", t("err-send-failed"), e)));
            }
        }

        Task::none()
    }
}
