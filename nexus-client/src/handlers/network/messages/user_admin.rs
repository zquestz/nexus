//! User administration response handlers

use iced::Task;
use nexus_common::framing::MessageId;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::InputId;
use crate::types::{
    ActivePanel, ChatMessage, Message, ResponseRouting, UserEditInit, UserManagementMode,
};

/// Data from a UserEditResponse message
pub struct UserEditResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub id: Option<i64>,
    pub username: Option<String>,
    pub is_admin: Option<bool>,
    pub is_shared: Option<bool>,
    pub enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub group_id: Option<i64>,
    #[allow(dead_code)] // Informational — client looks up name from available_groups
    pub group_name: Option<String>,
    pub group_permissions: Option<Vec<String>>,
    pub revoked_permissions: Option<Vec<String>>,
    pub available_groups: Option<Vec<nexus_common::protocol::GroupInfo>>,
    /// Per-user bandwidth weight override from the server (None = inherit).
    pub bandwidth_weight: Option<u16>,
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
            let task = self.add_active_tab_message(connection_id, ChatMessage::system(message));

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
                conn.user_management.is_submitting = false;
            }
        } else {
            // Show error in chat
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
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
            let task = self.add_active_tab_message(connection_id, ChatMessage::system(message));

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
                conn.user_management.is_delete_submitting = false;
            }
        } else {
            // Show error in chat
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
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
                // Update available_groups cache if provided
                if let Some(groups) = data.available_groups {
                    conn.user_management.available_groups = Some(Ok(groups));
                }

                conn.user_management.enter_edit_mode(UserEditInit {
                    id: data.id.unwrap_or(0),
                    username: data.username.unwrap_or_default(),
                    is_admin: data.is_admin.unwrap_or(false),
                    is_shared: data.is_shared.unwrap_or(false),
                    enabled: data.enabled.unwrap_or(true),
                    permissions: data.permissions.unwrap_or_default(),
                    group_id: data.group_id,
                    group_permissions: data.group_permissions.unwrap_or_default(),
                    revoked_permissions: data.revoked_permissions.unwrap_or_default(),
                    bandwidth_weight: data.bandwidth_weight,
                });
                return self.focus_field(InputId::EditNewUsername);
            }
        } else {
            // On error, show in the appropriate place
            if matches!(routing, Some(ResponseRouting::PopulateUserManagementEdit)) {
                // Show error in the panel-level banner above the tabs
                conn.user_management
                    .set_user_list_error(data.error.unwrap_or_default());
            } else {
                // Show error in chat
                return self.add_active_tab_message(
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
                return self.add_active_tab_message(
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
            let task = self.add_active_tab_message(connection_id, ChatMessage::system(message));

            // If from user management panel, return to where we came from and refresh.
            if matches!(routing, Some(ResponseRouting::UserManagementUpdateResult)) {
                // Returning to the User Info view? Refetch the full record so it
                // reflects every edited field (group, perms, admin, bandwidth,
                // enabled, name) — not stale pre-edit values. Key: regular → the
                // current/new username from the response; shared → the session's own
                // (unchanged) nickname, still a live lookup. Checked before
                // return_to_user_management_list resets return_to_panel.
                let refetch_nick = self.connections.get(&connection_id).and_then(|conn| {
                    if conn.user_management.return_to_panel != Some(ActivePanel::UserInfo) {
                        return None;
                    }
                    match &conn.user_info_data {
                        Some(Ok(d)) if d.is_shared => Some(d.nickname.clone()),
                        Some(Ok(_)) => username.clone(),
                        _ => None,
                    }
                });
                let return_task = self.return_to_user_management_list(connection_id);
                return match refetch_nick {
                    Some(nick) => Task::batch([
                        task,
                        return_task,
                        self.handle_user_info_icon_clicked(connection_id, nick),
                    ]),
                    None => Task::batch([task, return_task]),
                };
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::PasswordChangeResult)) {
            // Show error in password change form
            if let Some(conn) = self.connections.get_mut(&connection_id)
                && let Some(state) = &mut conn.password_change_state
            {
                state.is_submitting = false;
                state.error = Some(error.unwrap_or_default());
                // Clear password fields on error for security
                state.current_password.clear();
                state.new_password.clear();
                state.confirm_password.clear();
            }
            // Focus the current password field again
            return self.focus_field(InputId::ChangePasswordCurrent);
        } else if matches!(routing, Some(ResponseRouting::UserManagementUpdateResult)) {
            // Show error in edit form
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.edit_error = Some(error.unwrap_or_default());
                conn.user_management.is_submitting = false;
            }
        } else {
            // Show error in chat
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
        }

        Task::none()
    }

    // ==================== Helper Functions ====================

    /// Return to user management list view (or original panel) and refresh the list
    fn return_to_user_management_list(&mut self, connection_id: usize) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Full reset first — clears mode, errors, AND the submit flags. Capture the
        // return panel beforehand since reset_to_list() clears it. (Previously the
        // return-to-panel branch below reset only mode/error, leaving is_submitting
        // stuck true, which disabled Save on the next edit opened from that panel.)
        let return_panel = conn.user_management.return_to_panel;
        conn.user_management.reset_to_list();

        // If we came from a different panel (e.g., UserInfo), return there without
        // refreshing the user-management list.
        if let Some(return_panel) = return_panel {
            conn.active_panel = return_panel;
            return Task::none();
        }

        // Otherwise refresh the user list in place.
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
