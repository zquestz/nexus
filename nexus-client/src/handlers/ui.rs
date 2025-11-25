//! UI panel toggles

use crate::NexusApp;
use crate::types::{InputId, Message};
use iced::Task;
use iced::widget::text_input;

impl NexusApp {
    /// Toggle bookmarks sidebar visibility
    pub fn handle_toggle_bookmarks(&mut self) -> Task<Message> {
        self.ui_state.show_bookmarks = !self.ui_state.show_bookmarks;
        Task::none()
    }

    /// Toggle user list sidebar visibility
    pub fn handle_toggle_user_list(&mut self) -> Task<Message> {
        self.ui_state.show_user_list = !self.ui_state.show_user_list;
        Task::none()
    }

    /// Toggle Add User panel visibility
    pub fn handle_toggle_add_user(&mut self) -> Task<Message> {
        // Toggle Add User, and turn off Edit User and Broadcast
        self.ui_state.show_add_user = !self.ui_state.show_add_user;
        if self.ui_state.show_add_user {
            self.ui_state.show_edit_user = false;
            self.ui_state.show_broadcast = false;
            // Clear form and set focus (only if connected)
            if let Some(conn_id) = self.active_connection {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.user_management.clear_add_user();
                }
                self.focused_field = InputId::AdminUsername;
                return text_input::focus(text_input::Id::from(InputId::AdminUsername));
            }
        } else {
            // Closing panel - focus chat input if connected
            if self.active_connection.is_some() {
                return text_input::focus(text_input::Id::from(InputId::ChatInput));
            }
        }
        Task::none()
    }

    /// Toggle Edit User panel visibility
    pub fn handle_toggle_edit_user(&mut self) -> Task<Message> {
        // Toggle Edit User, and turn off Add User and Broadcast
        self.ui_state.show_edit_user = !self.ui_state.show_edit_user;
        if self.ui_state.show_edit_user {
            self.ui_state.show_add_user = false;
            self.ui_state.show_broadcast = false;
            // Start editing and set focus (only if connected)
            if let Some(conn_id) = self.active_connection {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.user_management.start_editing();
                }
                self.focused_field = InputId::EditUsername;
                return text_input::focus(text_input::Id::from(InputId::EditUsername));
            }
        } else {
            // Closing panel - clear and focus chat input if connected
            if let Some(conn_id) = self.active_connection {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.user_management.clear_edit_user();
                }
                return text_input::focus(text_input::Id::from(InputId::ChatInput));
            }
        }
        Task::none()
    }

    /// Toggle between light and dark theme
    pub fn handle_toggle_theme(&mut self) -> Task<Message> {
        use crate::config::ThemePreference;
        
        // Toggle theme preference
        self.config.theme = match self.config.theme {
            ThemePreference::Light => ThemePreference::Dark,
            ThemePreference::Dark => ThemePreference::Light,
        };
        
        // Save config to persist theme preference
        if let Err(e) = self.config.save() {
            eprintln!("Failed to save theme preference: {}", e);
        }
        
        Task::none()
    }
}
