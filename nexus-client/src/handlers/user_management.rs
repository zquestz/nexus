//! User management handlers

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    ActivePanel, ChatMessage, ChatTab, InputId, Message, PendingRequests, ResponseRouting,
    UserManagementMode,
};
use crate::views::constants::PERMISSION_USER_INFO;
use iced::Task;
use iced::widget::{Id, operation};

use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, PasswordError, UsernameError};

impl NexusApp {
    // ==================== Panel Toggle ====================

    /// Toggle the user management panel
    ///
    /// When opening, fetches the user list from the server.
    pub fn handle_toggle_user_management(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::UserManagement {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::UserManagement);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Reset to list mode and clear any previous state
        conn.user_management.reset_to_list();
        conn.user_management.all_users = None; // Trigger loading state

        // Request user list from server
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

    /// Handle cancel in user management panel
    ///
    /// In create/edit mode: returns to list view
    /// In list mode: closes the panel
    pub fn handle_cancel_user_management(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return self.handle_show_chat_view();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return self.handle_show_chat_view();
        };

        match &conn.user_management.mode {
            UserManagementMode::List => {
                // In list mode, close the panel
                self.handle_show_chat_view()
            }
            UserManagementMode::Create | UserManagementMode::Edit { .. } => {
                // In create/edit mode, return to list
                conn.user_management.reset_to_list();
                Task::none()
            }
            UserManagementMode::ConfirmDelete { .. } => {
                // Should not happen (modal handles its own cancel)
                conn.user_management.mode = UserManagementMode::List;
                Task::none()
            }
        }
    }

    // ==================== List View Actions ====================

    /// Show the create user form
    pub fn handle_user_management_show_create(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.enter_create_mode();
        self.focused_field = InputId::AdminUsername;
        operation::focus(Id::from(InputId::AdminUsername))
    }

    /// Handle edit button click on a user in the list (or from user info panel)
    ///
    /// Requests user details from server, then transitions to edit mode.
    /// If called from outside the User Management panel, opens the panel first.
    pub fn handle_user_management_edit_clicked(&mut self, username: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Open the User Management panel if not already open
        if conn.active_panel != ActivePanel::UserManagement {
            conn.active_panel = ActivePanel::UserManagement;
            conn.user_management.reset_to_list();
        }

        // Request user details from server
        match conn.send(ClientMessage::UserEdit {
            username: username.clone(),
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateUserManagementEdit);
            }
            Err(e) => {
                conn.user_management.list_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle delete button click on a user in the list
    ///
    /// Shows the delete confirmation modal.
    pub fn handle_user_management_delete_clicked(&mut self, username: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.enter_confirm_delete_mode(username);
        Task::none()
    }

    /// Handle confirm delete button in modal
    pub fn handle_user_management_confirm_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let username = match &conn.user_management.mode {
            UserManagementMode::ConfirmDelete { username } => username.clone(),
            _ => return Task::none(),
        };

        // Send delete request
        match conn.send(ClientMessage::UserDelete {
            username: username.clone(),
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::UserManagementDeleteResult);
            }
            Err(e) => {
                conn.user_management.mode = UserManagementMode::List;
                conn.user_management.list_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        // Return to list mode (will show result when response arrives)
        conn.user_management.mode = UserManagementMode::List;
        Task::none()
    }

    /// Handle cancel delete button in modal
    pub fn handle_user_management_cancel_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.mode = UserManagementMode::List;
        Task::none()
    }

    // ==================== Create Form Handlers ====================

    /// Handle username field change in create form
    pub fn handle_user_management_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.username = username;
        }
        self.focused_field = InputId::AdminUsername;
        Task::none()
    }

    /// Handle password field change in create form
    pub fn handle_user_management_password_changed(&mut self, password: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.password = password;
        }
        self.focused_field = InputId::AdminPassword;
        Task::none()
    }

    /// Handle is_admin checkbox toggle in create form
    pub fn handle_user_management_is_admin_toggled(&mut self, is_admin: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.is_admin = is_admin;
        }
        Task::none()
    }

    /// Handle enabled checkbox toggle in create form
    pub fn handle_user_management_enabled_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.enabled = enabled;
        }
        Task::none()
    }

    /// Handle permission checkbox toggle in create form
    pub fn handle_user_management_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(perm) = conn
                .user_management
                .permissions
                .iter_mut()
                .find(|(p, _)| p == &permission)
        {
            perm.1 = enabled;
        }
        Task::none()
    }

    /// Handle create user button press
    pub fn handle_user_management_create_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Validate username
        let username = &conn.user_management.username;
        if let Err(e) = validators::validate_username(username) {
            conn.user_management.create_error = Some(match e {
                UsernameError::Empty => t("err-username-empty"),
                UsernameError::TooLong => t_args(
                    "err-username-too-long",
                    &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
                ),
                UsernameError::InvalidCharacters => t("err-username-invalid"),
            });
            return Task::none();
        }

        // Validate password
        let password = &conn.user_management.password;
        if let Err(e) = validators::validate_password(password) {
            conn.user_management.create_error = Some(match e {
                PasswordError::Empty => t("err-password-required"),
                PasswordError::TooLong => t_args(
                    "err-password-too-long",
                    &[("max", &validators::MAX_PASSWORD_LENGTH.to_string())],
                ),
            });
            return Task::none();
        }

        // Only send admin flag if current user is admin
        let is_admin = if conn.is_admin {
            conn.user_management.is_admin
        } else {
            false
        };

        // Only send permissions that the current user has (or all if admin)
        let permissions: Vec<String> = conn
            .user_management
            .permissions
            .iter()
            .filter(|(perm_name, enabled)| {
                *enabled && (conn.is_admin || conn.permissions.contains(perm_name))
            })
            .map(|(name, _)| name.clone())
            .collect();

        let msg = ClientMessage::UserCreate {
            username: conn.user_management.username.clone(),
            password: conn.user_management.password.clone(),
            is_admin,
            enabled: conn.user_management.enabled,
            permissions,
        };

        // Clear any previous error on new submission
        conn.user_management.create_error = None;

        // Send message and track for response routing
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::UserManagementCreateResult);
            }
            Err(e) => {
                conn.user_management.create_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Validate create user form (called on Enter when form incomplete)
    pub fn handle_validate_user_management_create(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Validate username first
            if let Err(e) = validators::validate_username(&conn.user_management.username) {
                conn.user_management.create_error = Some(match e {
                    UsernameError::Empty => t("err-username-required"),
                    UsernameError::TooLong => t_args(
                        "err-username-too-long",
                        &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
                    ),
                    UsernameError::InvalidCharacters => t("err-username-invalid"),
                });
            } else if let Err(e) = validators::validate_password(&conn.user_management.password) {
                // Username is valid, check password
                conn.user_management.create_error = Some(match e {
                    PasswordError::Empty => t("err-password-required"),
                    PasswordError::TooLong => t_args(
                        "err-password-too-long",
                        &[("max", &validators::MAX_PASSWORD_LENGTH.to_string())],
                    ),
                });
            }
        }
        Task::none()
    }

    // ==================== Edit Form Handlers ====================

    /// Handle new username field change in edit form
    pub fn handle_user_management_edit_username_changed(
        &mut self,
        new_username: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserManagementMode::Edit {
                new_username: ref mut nu,
                ..
            } = conn.user_management.mode
        {
            *nu = new_username;
        }
        self.focused_field = InputId::EditNewUsername;
        Task::none()
    }

    /// Handle new password field change in edit form
    pub fn handle_user_management_edit_password_changed(
        &mut self,
        new_password: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserManagementMode::Edit {
                new_password: ref mut np,
                ..
            } = conn.user_management.mode
        {
            *np = new_password;
        }
        self.focused_field = InputId::EditNewPassword;
        Task::none()
    }

    /// Handle is_admin checkbox toggle in edit form
    pub fn handle_user_management_edit_is_admin_toggled(
        &mut self,
        is_admin: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserManagementMode::Edit {
                is_admin: ref mut ia,
                ..
            } = conn.user_management.mode
        {
            *ia = is_admin;
        }
        Task::none()
    }

    /// Handle enabled checkbox toggle in edit form
    pub fn handle_user_management_edit_enabled_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserManagementMode::Edit {
                enabled: ref mut e, ..
            } = conn.user_management.mode
        {
            *e = enabled;
        }
        Task::none()
    }

    /// Handle permission checkbox toggle in edit form
    pub fn handle_user_management_edit_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserManagementMode::Edit {
                permissions: ref mut perms,
                ..
            } = conn.user_management.mode
            && let Some(perm) = perms.iter_mut().find(|(p, _)| p == &permission)
        {
            perm.1 = enabled;
        }
        Task::none()
    }

    /// Handle update user button press
    pub fn handle_user_management_update_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let (original_username, new_username, new_password, is_admin, enabled, permissions) =
            match &conn.user_management.mode {
                UserManagementMode::Edit {
                    original_username,
                    new_username,
                    new_password,
                    is_admin,
                    enabled,
                    permissions,
                } => (
                    original_username.clone(),
                    new_username.clone(),
                    new_password.clone(),
                    *is_admin,
                    *enabled,
                    permissions.clone(),
                ),
                _ => return Task::none(),
            };

        // Validate new username
        if let Err(e) = validators::validate_username(&new_username) {
            conn.user_management.edit_error = Some(match e {
                UsernameError::Empty => t("err-username-empty"),
                UsernameError::TooLong => t_args(
                    "err-username-too-long",
                    &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
                ),
                UsernameError::InvalidCharacters => t("err-username-invalid"),
            });
            return Task::none();
        }

        // Validate new password if provided
        if !new_password.is_empty()
            && let Err(e) = validators::validate_password(&new_password)
        {
            conn.user_management.edit_error = Some(match e {
                PasswordError::Empty => t("err-password-required"),
                PasswordError::TooLong => t_args(
                    "err-password-too-long",
                    &[("max", &validators::MAX_PASSWORD_LENGTH.to_string())],
                ),
            });
            return Task::none();
        }

        let requested_username = if new_username != original_username {
            Some(new_username)
        } else {
            None
        };

        let requested_password = if !new_password.is_empty() {
            Some(new_password)
        } else {
            None
        };

        // Only send admin flag if current user is admin
        let requested_is_admin = if conn.is_admin { Some(is_admin) } else { None };

        // Only send enabled flag if current user is admin
        let requested_enabled = if conn.is_admin { Some(enabled) } else { None };

        // Only send permissions that the current user has (or all if admin)
        let requested_permissions: Vec<String> = permissions
            .iter()
            .filter(|(perm_name, perm_enabled)| {
                *perm_enabled && (conn.is_admin || conn.permissions.contains(perm_name))
            })
            .map(|(name, _)| name.clone())
            .collect();

        let msg = ClientMessage::UserUpdate {
            username: original_username,
            requested_username,
            requested_password,
            requested_is_admin,
            requested_enabled,
            requested_permissions: Some(requested_permissions),
        };

        // Clear any previous error on new submission
        conn.user_management.edit_error = None;

        // Send message and track for response routing
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::UserManagementUpdateResult);
            }
            Err(e) => {
                conn.user_management.edit_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Validate edit user form (called on Enter when form incomplete)
    pub fn handle_validate_user_management_edit(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserManagementMode::Edit { new_username, .. } = &conn.user_management.mode
            && let Err(e) = validators::validate_username(new_username)
        {
            conn.user_management.edit_error = Some(match e {
                UsernameError::Empty => t("err-username-required"),
                UsernameError::TooLong => t_args(
                    "err-username-too-long",
                    &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
                ),
                UsernameError::InvalidCharacters => t("err-username-invalid"),
            });
        }
        Task::none()
    }

    // ==================== User List Icon Handlers ====================

    /// Handle user message icon click (create/switch to PM tab)
    pub fn handle_user_message_icon_clicked(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Create PM tab entry if it doesn't exist
            conn.user_messages.entry(username.clone()).or_default();

            // Switch to the PM tab
            let tab = ChatTab::UserMessage(username);
            return Task::done(Message::SwitchChatTab(tab));
        }
        Task::none()
    }

    /// Handle user kick icon click (kick/disconnect user)
    pub fn handle_user_kick_icon_clicked(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Send UserKick request to server
            if let Err(e) = conn.send(ClientMessage::UserKick { username }) {
                let error_msg = format!("{}: {}", t("err-send-failed"), e);
                return self.add_chat_message(conn_id, ChatMessage::error(error_msg));
            }

            return self.handle_show_chat_view();
        }
        Task::none()
    }

    /// Handle user list item click (expand/collapse accordion)
    pub fn handle_user_list_item_clicked(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Toggle expansion: if clicking the same user, collapse it; otherwise expand new user
            if conn.expanded_user.as_ref() == Some(&username) {
                conn.expanded_user = None;
            } else {
                conn.expanded_user = Some(username);
            }
        }
        Task::none()
    }

    /// Handle info icon click on expanded user
    ///
    /// Opens the UserInfo panel and sends a request to the server.
    /// The panel shows a loading state until the response arrives.
    /// Requires user_info permission.
    pub fn handle_user_info_icon_clicked(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Check permission (admins always have access)
            let has_permission =
                conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_INFO);
            if !has_permission {
                return Task::none();
            }

            // Clear previous data and open the panel (shows loading state)
            conn.user_info_data = None;
            conn.active_panel = ActivePanel::UserInfo;

            // Send UserInfo request to server and track it
            match conn.send(ClientMessage::UserInfo {
                username: username.clone(),
            }) {
                Ok(message_id) => {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::PopulateUserInfoPanel(username));
                }
                Err(e) => {
                    let error_msg = format!("{}: {}", t("err-send-failed"), e);
                    conn.user_info_data = Some(Err(error_msg));
                }
            }
        }
        Task::none()
    }
}
