//! UI panel toggles

use crate::NexusApp;
use crate::types::{InputId, Message};
use iced::Task;
use iced::widget::text_input;

impl NexusApp {
    // UI toggle handlers
    pub fn handle_toggle_bookmarks(&mut self) -> Task<Message> {
        self.ui_state.show_bookmarks = !self.ui_state.show_bookmarks;
        Task::none()
    }

    pub fn handle_toggle_user_list(&mut self) -> Task<Message> {
        self.ui_state.show_user_list = !self.ui_state.show_user_list;
        Task::none()
    }

    pub fn handle_toggle_add_user(&mut self) -> Task<Message> {
        // Toggle Add User, and turn off Delete User and Broadcast
        self.ui_state.show_add_user = !self.ui_state.show_add_user;
        if self.ui_state.show_add_user {
            self.ui_state.show_delete_user = false;
            self.ui_state.show_broadcast = false;
            // Clear form and set focus
            if let Some(conn_id) = self.active_connection {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.user_management.clear_add_user();
                }
            }
            self.focused_field = InputId::AdminUsername;
            return text_input::focus(text_input::Id::from(InputId::AdminUsername));
        } else {
            // Closing panel - focus chat input
            return text_input::focus(text_input::Id::from(InputId::ChatInput));
        }
    }

    pub fn handle_toggle_delete_user(&mut self) -> Task<Message> {
        // Toggle Delete User, and turn off Add User and Broadcast
        self.ui_state.show_delete_user = !self.ui_state.show_delete_user;
        if self.ui_state.show_delete_user {
            self.ui_state.show_add_user = false;
            self.ui_state.show_broadcast = false;
            // Clear form and set focus
            if let Some(conn_id) = self.active_connection {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.user_management.clear_delete_user();
                }
            }
            self.focused_field = InputId::DeleteUsername;
            return text_input::focus(text_input::Id::from(InputId::DeleteUsername));
        } else {
            // Closing panel - focus chat input
            return text_input::focus(text_input::Id::from(InputId::ChatInput));
        }
    }
}
