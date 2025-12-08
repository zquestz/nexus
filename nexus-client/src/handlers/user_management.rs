//! User management

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, ChatMessage, ChatTab, InputId, Message, UserEditState};
use crate::views::constants::PERMISSION_USER_INFO;
use iced::Task;

use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, PasswordError, UsernameError};

impl NexusApp {
    // ==================== Add User Form Handlers ====================

    /// Handle admin panel username field change
    pub fn handle_admin_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.username = username;
        }
        self.focused_field = InputId::AdminUsername;
        Task::none()
    }

    /// Handle admin panel password field change
    pub fn handle_admin_password_changed(&mut self, password: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.password = password;
        }
        self.focused_field = InputId::AdminPassword;
        Task::none()
    }

    /// Handle admin panel Is Admin checkbox toggle
    pub fn handle_admin_is_admin_toggled(&mut self, is_admin: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.is_admin = is_admin;
        }
        Task::none()
    }

    /// Handle admin panel Enabled checkbox toggle
    pub fn handle_admin_enabled_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.enabled = enabled;
        }
        Task::none()
    }

    /// Handle admin panel permission checkbox toggle
    pub fn handle_admin_permission_toggled(
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

    // ==================== Add User Actions ====================

    /// Handle Create User button press
    pub fn handle_create_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
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

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                conn.user_management.create_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
                return Task::none();
            }

            // Don't close panel or clear form here - wait for server response
            // Panel will close on success, stay open with error on failure
        }
        Task::none()
    }

    /// Handle validation of create user form (called on Enter when form incomplete)
    pub fn handle_validate_create_user(&mut self) -> Task<Message> {
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

    // ==================== Edit User Form Handlers ====================

    /// Handle validation of edit user form (called on Enter when form incomplete)
    pub fn handle_validate_edit_user(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            match &conn.user_management.edit_state {
                UserEditState::SelectingUser { username } => {
                    if let Err(e) = validators::validate_username(username) {
                        conn.user_management.edit_error = Some(match e {
                            UsernameError::Empty => t("err-username-required"),
                            UsernameError::TooLong => t_args(
                                "err-username-too-long",
                                &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
                            ),
                            UsernameError::InvalidCharacters => t("err-username-invalid"),
                        });
                    }
                }
                UserEditState::EditingUser { new_username, .. } => {
                    if let Err(e) = validators::validate_username(new_username) {
                        conn.user_management.edit_error = Some(match e {
                            UsernameError::Empty => t("err-username-required"),
                            UsernameError::TooLong => t_args(
                                "err-username-too-long",
                                &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
                            ),
                            UsernameError::InvalidCharacters => t("err-username-invalid"),
                        });
                    }
                }
                UserEditState::None => {}
            }
        }
        Task::none()
    }

    // ==================== Edit User Actions ====================

    /// Handle Delete User button press
    pub fn handle_delete_user_pressed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Validate username
            if let Err(e) = validators::validate_username(&username) {
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

            let msg = ClientMessage::UserDelete { username };

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                return self.add_user_management_error(
                    conn_id,
                    format!("{}: {}", t("err-send-failed"), e),
                );
            }
            // Note: This is called from the Delete button in the User Edit form
            // The edit form will handle closing itself
        }
        Task::none()
    }

    /// Handle edit user username field change (stage 1)
    pub fn handle_edit_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::SelectingUser {
                username: ref mut u,
            } = conn.user_management.edit_state
        {
            *u = username;
        }
        self.focused_field = InputId::EditUsername;
        Task::none()
    }

    /// Handle edit user new username field change (stage 2)
    pub fn handle_edit_new_username_changed(&mut self, new_username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::EditingUser {
                new_username: ref mut nu,
                ..
            } = conn.user_management.edit_state
        {
            *nu = new_username;
        }
        self.focused_field = InputId::EditNewUsername;
        Task::none()
    }

    /// Handle edit user new password field change (stage 2)
    pub fn handle_edit_new_password_changed(&mut self, new_password: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::EditingUser {
                new_password: ref mut np,
                ..
            } = conn.user_management.edit_state
        {
            *np = new_password;
        }
        self.focused_field = InputId::EditNewPassword;
        Task::none()
    }

    /// Handle edit user Is Admin checkbox toggle (stage 2)
    pub fn handle_edit_is_admin_toggled(&mut self, is_admin: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::EditingUser {
                is_admin: ref mut ia,
                ..
            } = conn.user_management.edit_state
        {
            *ia = is_admin;
        }
        Task::none()
    }

    /// Handle edit user Enabled checkbox toggle (stage 2)
    pub fn handle_edit_enabled_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::EditingUser {
                enabled: ref mut e, ..
            } = conn.user_management.edit_state
        {
            *e = enabled;
        }
        Task::none()
    }

    /// Handle edit user permission checkbox toggle (stage 2)
    pub fn handle_edit_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::EditingUser {
                permissions: ref mut perms,
                ..
            } = conn.user_management.edit_state
            && let Some(perm) = perms.iter_mut().find(|(p, _)| p == &permission)
        {
            perm.1 = enabled;
        }
        Task::none()
    }

    /// Handle Edit button press (stage 1 - request user details)
    pub fn handle_edit_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::SelectingUser { username } = &conn.user_management.edit_state
        {
            // Validate username
            if let Err(e) = validators::validate_username(username) {
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

            let msg = ClientMessage::UserEdit {
                username: username.clone(),
            };

            // Clear any previous error on new submission
            conn.user_management.edit_error = None;

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                conn.user_management.edit_error = Some(format!("{}: {}", t("err-send-failed"), e));
                return Task::none();
            }
            // Stay on this screen, wait for server response
        }
        Task::none()
    }

    /// Handle Update button press (stage 2 - submit changes)
    pub fn handle_update_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let UserEditState::EditingUser {
                original_username,
                new_username,
                new_password,
                is_admin,
                enabled,
                permissions,
            } = &conn.user_management.edit_state
        {
            // Validate new username
            if let Err(e) = validators::validate_username(new_username) {
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
                && let Err(e) = validators::validate_password(new_password)
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
                Some(new_username.clone())
            } else {
                None
            };

            let requested_password = if !new_password.is_empty() {
                Some(new_password.clone())
            } else {
                None
            };

            // Only send admin flag if current user is admin
            let requested_is_admin = if conn.is_admin { Some(*is_admin) } else { None };

            // Only send enabled flag if current user is admin
            let requested_enabled = if conn.is_admin { Some(*enabled) } else { None };

            // Only send permissions that the current user has (or all if admin)
            let requested_permissions: Vec<String> = permissions
                .iter()
                .filter(|(perm_name, enabled)| {
                    *enabled && (conn.is_admin || conn.permissions.contains(perm_name))
                })
                .map(|(name, _)| name.clone())
                .collect();

            let msg = ClientMessage::UserUpdate {
                username: original_username.clone(),
                requested_username,
                requested_password,
                requested_is_admin,
                requested_enabled,
                requested_permissions: Some(requested_permissions),
            };

            // Clear any previous error on new submission
            conn.user_management.edit_error = None;

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                conn.user_management.edit_error = Some(format!("{}: {}", t("err-send-failed"), e));
                return Task::none();
            }

            // Don't close panel or clear form here - wait for server response
            // Panel will close on success, stay open with error on failure
        }
        Task::none()
    }

    // ==================== Cancel Handlers ====================

    /// Handle Cancel button press in add user panel
    pub fn handle_cancel_add_user(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.clear_add_user();
        }
        self.handle_show_chat_view()
    }

    /// Handle Cancel button press in edit user panel
    pub fn handle_cancel_edit_user(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.clear_edit_user();
        }
        self.handle_show_chat_view()
    }

    // ==================== Private Helpers ====================

    /// Add an error message to the chat for user management errors and auto-scroll
    fn add_user_management_error(
        &mut self,
        connection_id: usize,
        message: String,
    ) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::error(message))
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
            if let Err(e) = conn.tx.send(ClientMessage::UserKick { username }) {
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
            self.ui_state.active_panel = ActivePanel::UserInfo;

            // Send UserInfo request to server
            if let Err(e) = conn.tx.send(ClientMessage::UserInfo { username }) {
                let error_msg = format!("{}: {}", t("err-send-failed"), e);
                conn.user_info_data = Some(Err(error_msg));
            }
        }
        Task::none()
    }
}
