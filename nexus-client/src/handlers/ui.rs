//! UI panel toggles

use crate::NexusApp;
use crate::config::ThemePreference;
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, InputId, Message};
use iced::Task;
use iced::widget::text_input;

impl NexusApp {
    // ==================== Fingerprint Handling ====================

    /// Accept new certificate fingerprint (update stored fingerprint and complete connection)
    pub fn handle_accept_new_fingerprint(&mut self) -> Task<Message> {
        if let Some(mismatch) = self.fingerprint_mismatch_queue.pop_front() {
            // Update the stored fingerprint (handle case where bookmark was deleted)
            if let Some(bookmark) = self.config.bookmarks.get_mut(mismatch.bookmark_index) {
                bookmark.certificate_fingerprint = Some(mismatch.received);
                let _ = self.config.save();
            }

            // Complete the connection that was pending
            return self.handle_bookmark_connection_result(
                Ok(mismatch.connection),
                Some(mismatch.bookmark_index),
                mismatch.display_name,
            );
        }
        Task::none()
    }

    /// Reject new certificate fingerprint (cancel connection)
    pub fn handle_cancel_fingerprint_mismatch(&mut self) -> Task<Message> {
        self.fingerprint_mismatch_queue.pop_front();

        if self.fingerprint_mismatch_queue.is_empty() {
            self.connection_form.error = Some(t("msg-connection-cancelled"));
        }

        Task::none()
    }

    // ==================== Panel Actions ====================

    /// Show Add User panel (does nothing if already shown)
    pub fn handle_toggle_add_user(&mut self) -> Task<Message> {
        if self.ui_state.active_panel == ActivePanel::AddUser {
            return Task::none();
        }

        self.ui_state.active_panel = ActivePanel::AddUser;

        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.clear_add_user();
            }
            self.focused_field = InputId::AdminUsername;
            return text_input::focus(text_input::Id::from(InputId::AdminUsername));
        }
        Task::none()
    }

    /// Show Edit User panel (does nothing if already shown)
    pub fn handle_toggle_edit_user(&mut self) -> Task<Message> {
        if self.ui_state.active_panel == ActivePanel::EditUser {
            return Task::none();
        }

        self.ui_state.active_panel = ActivePanel::EditUser;

        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.user_management.start_editing();
            }
            self.focused_field = InputId::EditUsername;
            return text_input::focus(text_input::Id::from(InputId::EditUsername));
        }
        Task::none()
    }

    // ==================== Sidebar Toggles ====================

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

    // ==================== Theme ====================

    /// Toggle between light and dark theme
    pub fn handle_toggle_theme(&mut self) -> Task<Message> {
        self.config.theme = match self.config.theme {
            ThemePreference::Light => ThemePreference::Dark,
            ThemePreference::Dark => ThemePreference::Light,
        };

        if let Err(e) = self.config.save() {
            self.connection_form.error = Some(t_args(
                "err-failed-save-theme",
                &[("error", &e.to_string())],
            ));
        }
        Task::none()
    }
}
