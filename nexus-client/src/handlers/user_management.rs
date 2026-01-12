//! User management handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, PasswordError, UsernameError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    ActivePanel, ChatMessage, ChatTab, InputId, Message, PasswordChangeState, PendingRequests,
    ResponseRouting, UserManagementMode,
};
use crate::views::constants::PERMISSION_USER_INFO;

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
    /// In create/edit mode: returns to original panel if set, otherwise list view
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
                // Check if we should return to a different panel (e.g., UserInfo)
                if let Some(return_panel) = conn.user_management.return_to_panel {
                    conn.user_management.return_to_panel = None;
                    conn.user_management.mode = UserManagementMode::List;
                    conn.user_management.edit_error = None;
                    conn.active_panel = return_panel;
                    Task::none()
                } else {
                    // Return to list mode within User Management
                    conn.user_management.reset_to_list();
                    // Only fetch if we don't already have the list
                    if conn.user_management.all_users.is_none()
                        && let Ok(message_id) = conn.send(ClientMessage::UserList { all: true })
                    {
                        conn.pending_requests
                            .track(message_id, ResponseRouting::PopulateUserManagementList);
                    }
                    Task::none()
                }
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
    /// If called from outside the User Management panel, opens the panel first
    /// and stores the original panel to return to on cancel/save.
    pub fn handle_user_management_edit_clicked(&mut self, username: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Track where we came from so we can return there on cancel/save
        if conn.active_panel != ActivePanel::UserManagement {
            conn.user_management.return_to_panel = Some(conn.active_panel);
            conn.active_panel = ActivePanel::UserManagement;
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
    ///
    /// Keeps the dialog open until we get a response (success closes it, error shows in dialog).
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

        // Clear any previous error before sending
        conn.user_management.delete_error = None;

        // Send delete request (keep dialog open until response)
        match conn.send(ClientMessage::UserDelete {
            username: username.clone(),
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::UserManagementDeleteResult);
            }
            Err(e) => {
                // Show send error in the delete dialog
                conn.user_management.delete_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

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

        // Clear error and return to list
        conn.user_management.delete_error = None;
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
            // Shared accounts cannot be admins - ignore toggle if is_shared
            if !conn.user_management.is_shared {
                conn.user_management.is_admin = is_admin;
            }
        }
        Task::none()
    }

    /// Handle is_shared checkbox toggle in create form
    ///
    /// When toggled ON:
    /// - Unchecks admin (shared accounts cannot be admins)
    /// - Disables forbidden permissions (keeps only allowed ones)
    pub fn handle_user_management_is_shared_toggled(&mut self, is_shared: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.is_shared = is_shared;

            if is_shared {
                // Shared accounts cannot be admins
                conn.user_management.is_admin = false;

                // Disable forbidden permissions (only allow shared account permissions)
                for (perm_name, enabled) in &mut conn.user_management.permissions {
                    if !is_shared_account_permission(perm_name) {
                        *enabled = false;
                    }
                }
            }
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
            .filter(|(perm_name, enabled)| *enabled && conn.has_permission(perm_name))
            .map(|(name, _)| name.clone())
            .collect();

        // Only send shared flag if current user is admin (non-admins can't create shared accounts)
        let is_shared = if conn.is_admin {
            conn.user_management.is_shared
        } else {
            false
        };

        let msg = ClientMessage::UserCreate {
            username: conn.user_management.username.clone(),
            password: conn.user_management.password.clone(),
            is_admin,
            is_shared,
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
                    is_shared: _, // is_shared is immutable, not sent in update
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
            .filter(|(perm_name, perm_enabled)| *perm_enabled && conn.has_permission(perm_name))
            .map(|(name, _)| name.clone())
            .collect();

        let msg = ClientMessage::UserUpdate {
            username: original_username,
            current_password: None,
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
    ///
    /// The `nickname` parameter is the display name (always populated; equals username for regular accounts).
    pub fn handle_user_message_icon_clicked(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Create PM tab entry if it doesn't exist (keyed by display name)
            conn.user_messages.entry(nickname.clone()).or_default();

            // Switch to the PM tab
            let tab = ChatTab::UserMessage(nickname);
            return Task::done(Message::SwitchChatTab(tab));
        }
        Task::none()
    }

    /// Handle disconnect icon click - opens the disconnect dialog
    ///
    /// The `nickname` parameter is the display name (always populated; equals username for regular accounts).
    pub fn handle_disconnect_icon_clicked(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            use crate::types::DisconnectDialogState;
            conn.disconnect_dialog = Some(DisconnectDialogState::new(nickname));
        }
        Task::none()
    }

    /// Handle disconnect dialog action changed (kick or ban)
    pub fn handle_disconnect_dialog_action_changed(
        &mut self,
        action: crate::types::DisconnectAction,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(ref mut dialog) = conn.disconnect_dialog
        {
            dialog.action = action;
        }
        Task::none()
    }

    /// Handle disconnect dialog duration changed
    pub fn handle_disconnect_dialog_duration_changed(
        &mut self,
        duration: crate::types::BanDuration,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(ref mut dialog) = conn.disconnect_dialog
        {
            dialog.duration = duration;
        }
        Task::none()
    }

    /// Handle disconnect dialog reason changed
    pub fn handle_disconnect_dialog_reason_changed(&mut self, reason: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(ref mut dialog) = conn.disconnect_dialog
        {
            dialog.reason = reason;
        }
        Task::none()
    }

    /// Handle disconnect dialog cancel
    pub fn handle_disconnect_dialog_cancel(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.disconnect_dialog = None;
        }
        Task::none()
    }

    /// Handle disconnect dialog submit (kick or ban)
    pub fn handle_disconnect_dialog_submit(&mut self) -> Task<Message> {
        use crate::types::DisconnectAction;

        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(ref dialog) = conn.disconnect_dialog
        {
            let nickname = dialog.nickname.clone();

            match dialog.action {
                DisconnectAction::Kick => {
                    // Send UserKick request with optional reason
                    let reason = if dialog.reason.is_empty() {
                        None
                    } else {
                        Some(dialog.reason.clone())
                    };

                    if let Err(e) = conn.send(ClientMessage::UserKick {
                        nickname: nickname.clone(),
                        reason,
                    }) {
                        let error_msg = format!("{}: {}", t("err-send-failed"), e);
                        return self.add_active_tab_message(conn_id, ChatMessage::error(error_msg));
                    }
                    // Close dialog on success
                    if let Some(conn) = self.connections.get_mut(&conn_id) {
                        conn.disconnect_dialog = None;
                    }
                }
                DisconnectAction::Ban => {
                    // Send BanCreate request
                    let duration = dialog.duration.as_duration_string();
                    let reason = if dialog.reason.is_empty() {
                        None
                    } else {
                        Some(dialog.reason.clone())
                    };

                    if let Err(e) = conn.send(ClientMessage::BanCreate {
                        target: nickname.clone(),
                        duration,
                        reason,
                    }) {
                        let error_msg = format!("{}: {}", t("err-send-failed"), e);
                        return self.add_active_tab_message(conn_id, ChatMessage::error(error_msg));
                    }
                    // Close dialog on success
                    if let Some(conn) = self.connections.get_mut(&conn_id) {
                        conn.disconnect_dialog = None;
                    }
                }
            }

            return self.handle_show_chat_view();
        }
        Task::none()
    }

    /// Handle user list item click (expand/collapse accordion)
    ///
    /// The `nickname` parameter is the display name (always populated; equals username for regular accounts).
    pub fn handle_user_list_item_clicked(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Toggle expansion: if clicking the same user, collapse it; otherwise expand new user
            if conn.expanded_user.as_ref() == Some(&nickname) {
                conn.expanded_user = None;
            } else {
                conn.expanded_user = Some(nickname);
            }
        }
        Task::none()
    }

    /// Handle info icon click on expanded user
    ///
    /// Opens the UserInfo panel and sends a request to the server.
    /// The panel shows a loading state until the response arrives.
    /// Requires user_info permission.
    ///
    /// The `nickname` parameter is the display name (always populated; equals username for regular accounts).
    pub fn handle_user_info_icon_clicked(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Check permission (admins always have access)
            if !conn.has_permission(PERMISSION_USER_INFO) {
                return Task::none();
            }

            // Clear previous data and password change state, then open the panel (shows loading state)
            conn.user_info_data = None;
            conn.password_change_state = None;
            conn.active_panel = ActivePanel::UserInfo;

            // Send UserInfo request to server and track it
            match conn.send(ClientMessage::UserInfo {
                nickname: nickname.clone(),
            }) {
                Ok(message_id) => {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::PopulateUserInfoPanel(nickname));
                }
                Err(e) => {
                    let error_msg = format!("{}: {}", t("err-send-failed"), e);
                    conn.user_info_data = Some(Err(error_msg));
                }
            }
        }
        Task::none()
    }

    // ==================== Password Change ====================

    /// Enter password change mode - opens as its own panel
    pub fn handle_change_password_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Initialize password change state with return panel and switch to the panel
        conn.password_change_state = Some(PasswordChangeState::new(Some(conn.active_panel)));
        conn.active_panel = ActivePanel::ChangePassword;

        // Focus the current password field and track it
        self.focused_field = InputId::ChangePasswordCurrent;
        operation::focus(Id::from(InputId::ChangePasswordCurrent))
    }

    /// Handle current password field change
    pub fn handle_change_password_current_changed(&mut self, value: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if let Some(state) = &mut conn.password_change_state {
            state.current_password = value;
        }

        // Track focused field for Tab navigation
        self.focused_field = InputId::ChangePasswordCurrent;
        Task::none()
    }

    /// Handle new password field change
    pub fn handle_change_password_new_changed(&mut self, value: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if let Some(state) = &mut conn.password_change_state {
            state.new_password = value;
        }

        // Track focused field for Tab navigation
        self.focused_field = InputId::ChangePasswordNew;
        Task::none()
    }

    /// Handle confirm password field change
    pub fn handle_change_password_confirm_changed(&mut self, value: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if let Some(state) = &mut conn.password_change_state {
            state.confirm_password = value;
        }

        // Track focused field for Tab navigation
        self.focused_field = InputId::ChangePasswordConfirm;
        Task::none()
    }

    /// Handle Tab pressed in password change form
    ///
    /// Checks which field is actually focused using async operations,
    /// then moves to the next field in sequence.
    pub fn handle_change_password_tab_pressed(&mut self) -> Task<Message> {
        // Check focus state of all three password fields in parallel
        let check_current = operation::is_focused(Id::from(InputId::ChangePasswordCurrent));
        let check_new = operation::is_focused(Id::from(InputId::ChangePasswordNew));
        let check_confirm = operation::is_focused(Id::from(InputId::ChangePasswordConfirm));

        // Batch the checks and combine results
        Task::batch([
            check_current.map(|focused| (0, focused)),
            check_new.map(|focused| (1, focused)),
            check_confirm.map(|focused| (2, focused)),
        ])
        .collect()
        .map(|results: Vec<(u8, bool)>| {
            let current_focused = results.iter().any(|(i, f)| *i == 0 && *f);
            let new_focused = results.iter().any(|(i, f)| *i == 1 && *f);
            let confirm_focused = results.iter().any(|(i, f)| *i == 2 && *f);
            Message::ChangePasswordFocusResult(current_focused, new_focused, confirm_focused)
        })
    }

    /// Handle focus check result for password change Tab navigation
    pub fn handle_change_password_focus_result(
        &mut self,
        current_focused: bool,
        new_focused: bool,
        confirm_focused: bool,
    ) -> Task<Message> {
        // Determine next field based on which is currently focused
        let next_field = if current_focused {
            InputId::ChangePasswordNew
        } else if new_focused {
            InputId::ChangePasswordConfirm
        } else if confirm_focused {
            // Wrap around to first field
            InputId::ChangePasswordCurrent
        } else {
            // None focused, start at first field
            InputId::ChangePasswordCurrent
        };

        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
    }

    /// Cancel password change and return to original panel
    pub fn handle_change_password_cancel_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Get return panel before clearing state
        let return_panel = conn
            .password_change_state
            .as_ref()
            .and_then(|state| state.return_to_panel);

        // Clear password change state and return to original panel
        conn.password_change_state = None;
        conn.active_panel = return_panel.unwrap_or(ActivePanel::None);

        Task::none()
    }

    /// Submit password change form
    pub fn handle_change_password_save_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Get form values
        let (current_password, new_password, confirm_password) =
            if let Some(state) = &conn.password_change_state {
                (
                    state.current_password.clone(),
                    state.new_password.clone(),
                    state.confirm_password.clone(),
                )
            } else {
                return Task::none();
            };

        // Validate: current password required
        if current_password.is_empty() {
            if let Some(state) = &mut conn.password_change_state {
                state.error = Some(t("err-current-password-required"));
            }
            self.focused_field = InputId::ChangePasswordCurrent;
            return operation::focus(Id::from(InputId::ChangePasswordCurrent));
        }

        // Validate: new password required
        if new_password.is_empty() {
            if let Some(state) = &mut conn.password_change_state {
                state.error = Some(t("err-new-password-required"));
            }
            self.focused_field = InputId::ChangePasswordNew;
            return operation::focus(Id::from(InputId::ChangePasswordNew));
        }

        // Validate: confirm password required
        if confirm_password.is_empty() {
            if let Some(state) = &mut conn.password_change_state {
                state.error = Some(t("err-confirm-password-required"));
            }
            self.focused_field = InputId::ChangePasswordConfirm;
            return operation::focus(Id::from(InputId::ChangePasswordConfirm));
        }

        // Validate: passwords must match
        if new_password != confirm_password {
            if let Some(state) = &mut conn.password_change_state {
                state.error = Some(t("err-passwords-do-not-match"));
            }
            self.focused_field = InputId::ChangePasswordNew;
            return operation::focus(Id::from(InputId::ChangePasswordNew));
        }

        // Validate: new password format
        if let Err(e) = validators::validate_password(&new_password) {
            if let Some(state) = &mut conn.password_change_state {
                state.error = Some(match e {
                    PasswordError::Empty => t("err-new-password-required"),
                    PasswordError::TooLong => t_args(
                        "err-password-too-long",
                        &[("max", &validators::MAX_PASSWORD_LENGTH.to_string())],
                    ),
                });
            }
            self.focused_field = InputId::ChangePasswordNew;
            return operation::focus(Id::from(InputId::ChangePasswordNew));
        }

        // Get username for the request
        let username = conn.connection_info.username.clone();

        // Send UserUpdate with current_password for self-edit
        let msg = ClientMessage::UserUpdate {
            username,
            current_password: Some(current_password),
            requested_username: None,
            requested_password: Some(new_password),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
        };

        // Clear any previous error
        if let Some(state) = &mut conn.password_change_state {
            state.error = None;
        }

        // Send message and track for response routing
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PasswordChangeResult);
            }
            Err(e) => {
                if let Some(state) = &mut conn.password_change_state {
                    state.error = Some(format!("{}: {}", t("err-send-failed"), e));
                }
            }
        }

        Task::none()
    }

    // ==================== User Management Tab Navigation ====================

    /// Handle Tab pressed in user management create form
    ///
    /// Checks which field is actually focused using async operations,
    /// then moves to the next field in sequence.
    pub fn handle_user_management_create_tab_pressed(&mut self) -> Task<Message> {
        // Check focus state of both create form fields in parallel
        let check_username = operation::is_focused(Id::from(InputId::AdminUsername));
        let check_password = operation::is_focused(Id::from(InputId::AdminPassword));

        // Batch the checks and combine results
        Task::batch([
            check_username.map(|focused| (0, focused)),
            check_password.map(|focused| (1, focused)),
        ])
        .collect()
        .map(|results: Vec<(u8, bool)>| {
            let username_focused = results.iter().any(|(i, f)| *i == 0 && *f);
            let password_focused = results.iter().any(|(i, f)| *i == 1 && *f);
            Message::UserManagementCreateFocusResult(username_focused, password_focused)
        })
    }

    /// Handle focus check result for user management create Tab navigation
    pub fn handle_user_management_create_focus_result(
        &mut self,
        username_focused: bool,
        password_focused: bool,
    ) -> Task<Message> {
        // Determine next field based on which is currently focused
        let next_field = if username_focused {
            InputId::AdminPassword
        } else if password_focused {
            // Wrap around to first field
            InputId::AdminUsername
        } else {
            // None focused, start at first field
            InputId::AdminUsername
        };

        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
    }

    /// Handle Tab pressed in user management edit form
    ///
    /// Checks which field is actually focused using async operations,
    /// then moves to the next field in sequence.
    pub fn handle_user_management_edit_tab_pressed(&mut self) -> Task<Message> {
        // Check focus state of both edit form fields in parallel
        let check_username = operation::is_focused(Id::from(InputId::EditNewUsername));
        let check_password = operation::is_focused(Id::from(InputId::EditNewPassword));

        // Batch the checks and combine results
        Task::batch([
            check_username.map(|focused| (0, focused)),
            check_password.map(|focused| (1, focused)),
        ])
        .collect()
        .map(|results: Vec<(u8, bool)>| {
            let username_focused = results.iter().any(|(i, f)| *i == 0 && *f);
            let password_focused = results.iter().any(|(i, f)| *i == 1 && *f);
            Message::UserManagementEditFocusResult(username_focused, password_focused)
        })
    }

    /// Handle focus check result for user management edit Tab navigation
    pub fn handle_user_management_edit_focus_result(
        &mut self,
        username_focused: bool,
        password_focused: bool,
    ) -> Task<Message> {
        // Determine next field based on which is currently focused
        let next_field = if username_focused {
            InputId::EditNewPassword
        } else if password_focused {
            // Wrap around to first field
            InputId::EditNewUsername
        } else {
            // None focused, start at first field
            InputId::EditNewUsername
        };

        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
    }
}
