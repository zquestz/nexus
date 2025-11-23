//! User management

use crate::NexusApp;
use crate::types::{InputId, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;

impl NexusApp {
    // User management field update handlers
    pub fn handle_admin_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.username = username;
            }
        }
        self.focused_field = InputId::AdminUsername;
        Task::none()
    }

    pub fn handle_admin_password_changed(&mut self, password: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.password = password;
            }
        }
        self.focused_field = InputId::AdminPassword;
        Task::none()
    }

    pub fn handle_admin_is_admin_toggled(&mut self, is_admin: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.is_admin = is_admin;
            }
        }
        Task::none()
    }

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

    pub fn handle_delete_username_changed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.delete_username = username;
            }
        }
        self.focused_field = InputId::DeleteUsername;
        Task::none()
    }

    // User management operation handlers
    pub fn handle_create_user_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                let permissions: Vec<String> = conn
                    .user_management
                    .permissions
                    .iter()
                    .filter(|(_, enabled)| *enabled)
                    .map(|(name, _)| name.clone())
                    .collect();

                let msg = ClientMessage::UserCreate {
                    username: conn.user_management.username.clone(),
                    password: conn.user_management.password.clone(),
                    is_admin: conn.user_management.is_admin,
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

    pub fn handle_delete_user_pressed(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                let msg = ClientMessage::UserDelete { username };
                let _ = conn.tx.send(msg);
                // Clear the form and close the panel
                conn.user_management.clear_delete_user();
                self.ui_state.show_delete_user = false;
            }
        }
        Task::none()
    }
}
