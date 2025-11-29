//! User management

use crate::NexusApp;
use crate::handlers::network::msg_username_error;
use crate::i18n::t;
use crate::types::{ChatMessage, ChatTab, InputId, Message, ScrollableId, UserEditState};
use chrono::Local;
use iced::Task;
use iced::widget::scrollable;
use nexus_common::protocol::ClientMessage;

impl NexusApp {
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

    /// Handle Create User button press
    pub fn handle_create_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
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

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                return self.add_user_management_error(
                    conn_id,
                    format!("{}: {}", t("err-send-failed"), e),
                );
            }

            // Clear the form and close the panel
            conn.user_management.clear_add_user();
            self.ui_state.show_add_user = false;
        }
        Task::none()
    }

    /// Handle Delete User button press
    pub fn handle_delete_user_pressed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
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
            let msg = ClientMessage::UserEdit {
                username: username.clone(),
            };

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                return self.add_user_management_error(
                    conn_id,
                    format!("{}: {}", t("err-send-failed"), e),
                );
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
            let requested_username = if new_username != original_username {
                Some(new_username.clone())
            } else {
                None
            };

            let requested_password = if !new_password.trim().is_empty() {
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

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                return self.add_user_management_error(
                    conn_id,
                    format!("{}: {}", t("err-send-failed"), e),
                );
            }

            // Clear the form and close the panel
            conn.user_management.clear_edit_user();
            self.ui_state.show_edit_user = false;
        }
        Task::none()
    }

    /// Handle Cancel button press in edit user panel
    pub fn handle_cancel_edit_user(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.clear_edit_user();
        }
        self.ui_state.show_edit_user = false;
        Task::none()
    }

    /// Add an error message to the chat for user management errors and auto-scroll
    fn add_user_management_error(
        &mut self,
        connection_id: usize,
        message: String,
    ) -> Task<Message> {
        self.add_chat_message(
            connection_id,
            ChatMessage {
                username: msg_username_error(),
                message,
                timestamp: Local::now(),
            },
        )
    }

    /// Handle user message icon click (create/switch to PM tab)
    pub fn handle_user_message_icon_clicked(&mut self, username: String) -> Task<Message> {
        // Close all panels to show chat
        self.close_all_panels();

        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Create PM tab entry if it doesn't exist
            if !conn.user_messages.contains_key(&username) {
                conn.user_messages.insert(username.clone(), Vec::new());
            }

            // Switch to the PM tab
            let tab = ChatTab::UserMessage(username);
            return Task::done(Message::SwitchChatTab(tab));
        }
        Task::none()
    }

    /// Send a private message to a user
    #[allow(dead_code)]
    pub fn send_private_message(&mut self, to_username: String, message: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            let msg = ClientMessage::UserMessage {
                to_username: to_username.clone(),
                message: message.clone(),
            };

            if let Err(e) = conn.tx.send(msg) {
                return self.add_user_management_error(
                    conn_id,
                    format!("{}: {}", t("err-send-failed"), e),
                );
            }
        }
        Task::none()
    }

    /// Handle user kick icon click (kick/disconnect user)
    pub fn handle_user_kick_icon_clicked(&mut self, username: String) -> Task<Message> {
        // Close all panels to show chat
        self.close_all_panels();

        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Send UserKick request to server
            if let Err(e) = conn.tx.send(ClientMessage::UserKick { username }) {
                let error_msg = format!("{}: {}", t("err-send-failed"), e);
                return self.add_chat_message(
                    conn_id,
                    ChatMessage {
                        username: msg_username_error(),
                        message: error_msg,
                        timestamp: Local::now(),
                    },
                );
            }

            // Auto-scroll to show kick response
            return scrollable::snap_to(
                ScrollableId::ChatMessages.into(),
                scrollable::RelativeOffset::END,
            );
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
    pub fn handle_user_info_icon_clicked(&mut self, username: String) -> Task<Message> {
        // Close all panels to show chat
        self.close_all_panels();

        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Send UserInfo request to server
            if let Err(e) = conn.tx.send(ClientMessage::UserInfo { username }) {
                let error_msg = format!("{}: {}", t("err-send-failed"), e);
                return self.add_chat_message(
                    conn_id,
                    ChatMessage {
                        username: msg_username_error(),
                        message: error_msg,
                        timestamp: Local::now(),
                    },
                );
            }

            // Auto-scroll to show info response
            return scrollable::snap_to(
                ScrollableId::ChatMessages.into(),
                scrollable::RelativeOffset::END,
            );
        }
        Task::none()
    }
}
