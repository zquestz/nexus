//! User management

use crate::NexusApp;
use crate::types::{InputId, Message, UserEditState};
use iced::Task;
use nexus_common::protocol::ClientMessage;

impl NexusApp {
    /// Handle admin panel username field change
    pub fn handle_admin_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.username = username;
            }
        }
        self.focused_field = InputId::AdminUsername;
        Task::none()
    }

    /// Handle admin panel password field change
    pub fn handle_admin_password_changed(&mut self, password: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.password = password;
            }
        }
        self.focused_field = InputId::AdminPassword;
        Task::none()
    }

    /// Handle admin panel Is Admin checkbox toggle
    pub fn handle_admin_is_admin_toggled(&mut self, is_admin: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.is_admin = is_admin;
            }
        }
        Task::none()
    }

    /// Handle admin panel permission checkbox toggle
    pub fn handle_admin_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let Some(perm) = conn
                    .user_management
                    .permissions
                    .iter_mut()
                    .find(|(p, _)| p == &permission)
                {
                    perm.1 = enabled;
                }
            }
        }
        Task::none()
    }

    /// Handle Create User button press
    pub fn handle_create_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
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
                    permissions,
                };
                let _ = conn.tx.send(msg);

                // Clear the form and close the panel
                conn.user_management.clear_add_user();
                self.ui_state.show_add_user = false;
            }
        }
        Task::none()
    }

    /// Handle Delete User button press
    pub fn handle_delete_user_pressed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                let msg = ClientMessage::UserDelete { username };
                let _ = conn.tx.send(msg);
                // Note: This is called from the Delete button in the User Edit form
                // The edit form will handle closing itself
            }
        }
        Task::none()
    }

    /// Handle edit user username field change (stage 1)
    pub fn handle_edit_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::SelectingUser { username: ref mut u } = conn.user_management.edit_state {
                    *u = username;
                }
            }
        }
        self.focused_field = InputId::EditUsername;
        Task::none()
    }

    /// Handle edit user new username field change (stage 2)
    pub fn handle_edit_new_username_changed(&mut self, new_username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::EditingUser { new_username: ref mut nu, .. } = conn.user_management.edit_state {
                    *nu = new_username;
                }
            }
        }
        self.focused_field = InputId::EditNewUsername;
        Task::none()
    }

    /// Handle edit user new password field change (stage 2)
    pub fn handle_edit_new_password_changed(&mut self, new_password: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::EditingUser { new_password: ref mut np, .. } = conn.user_management.edit_state {
                    *np = new_password;
                }
            }
        }
        self.focused_field = InputId::EditNewPassword;
        Task::none()
    }

    /// Handle edit user Is Admin checkbox toggle (stage 2)
    pub fn handle_edit_is_admin_toggled(&mut self, is_admin: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::EditingUser { is_admin: ref mut ia, .. } = conn.user_management.edit_state {
                    *ia = is_admin;
                }
            }
        }
        Task::none()
    }

    /// Handle edit user permission checkbox toggle (stage 2)
    pub fn handle_edit_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::EditingUser { permissions: ref mut perms, .. } = conn.user_management.edit_state {
                    if let Some(perm) = perms.iter_mut().find(|(p, _)| p == &permission) {
                        perm.1 = enabled;
                    }
                }
            }
        }
        Task::none()
    }

    /// Handle Edit button press (stage 1 - request user details)
    pub fn handle_edit_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::SelectingUser { username } = &conn.user_management.edit_state {
                    let msg = ClientMessage::UserEdit {
                        username: username.clone(),
                    };
                    let _ = conn.tx.send(msg);
                    // Stay on this screen, wait for server response
                }
            }
        }
        Task::none()
    }

    /// Handle Update button press (stage 2 - submit changes)
    pub fn handle_update_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if let UserEditState::EditingUser {
                    original_username,
                    new_username,
                    new_password,
                    is_admin,
                    permissions,
                } = &conn.user_management.edit_state {
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
                    let requested_is_admin = if conn.is_admin {
                        Some(*is_admin)
                    } else {
                        None
                    };

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
                        requested_permissions: Some(requested_permissions),
                    };
                    let _ = conn.tx.send(msg);

                    // Clear the form and close the panel
                    conn.user_management.clear_edit_user();
                    self.ui_state.show_edit_user = false;
                }
            }
        }
        Task::none()
    }

    /// Handle Cancel button press in edit user panel
    pub fn handle_cancel_edit_user(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.clear_edit_user();
            }
        }
        self.ui_state.show_edit_user = false;
        Task::none()
    }
}
