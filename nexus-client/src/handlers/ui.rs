//! UI panel toggles

use crate::NexusApp;
use crate::config::{CHAT_FONT_SIZE_MAX, CHAT_FONT_SIZE_MIN};
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, InputId, Message, SettingsFormState};
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

    // ==================== Settings ====================

    /// Show Settings panel (does nothing if already shown)
    ///
    /// Takes a snapshot of the current config so it can be restored on cancel.
    pub fn handle_toggle_settings(&mut self) -> Task<Message> {
        if self.ui_state.active_panel == ActivePanel::Settings {
            return Task::none();
        }

        // Snapshot current config for potential cancel/restore
        self.settings_form = Some(SettingsFormState::new(&self.config));
        self.ui_state.active_panel = ActivePanel::Settings;
        Task::none()
    }

    /// Cancel settings panel and restore original config
    pub fn handle_cancel_settings(&mut self) -> Task<Message> {
        // Restore original config from snapshot
        if let Some(settings_form) = self.settings_form.take() {
            self.config = settings_form.original_config;
        }

        self.handle_show_chat_view()
    }

    /// Save settings to disk and close panel
    pub fn handle_save_settings(&mut self) -> Task<Message> {
        // Clear the snapshot (no need to restore)
        self.settings_form = None;

        // Save config to disk
        if let Err(e) = self.config.save() {
            self.connection_form.error = Some(t_args(
                "err-failed-save-settings",
                &[("error", &e.to_string())],
            ));
        }

        self.handle_show_chat_view()
    }

    /// Handle theme selection from the picker (live preview)
    ///
    /// Updates the config theme immediately for live preview.
    /// The change is persisted when Save is clicked, or reverted on Cancel.
    pub fn handle_theme_selected(&mut self, theme: iced::Theme) -> Task<Message> {
        self.config.theme = theme.into();
        Task::none()
    }

    /// Handle connection notifications toggle
    pub fn handle_connection_notifications_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.show_connection_notifications = enabled;
        Task::none()
    }

    /// Handle chat font size selection from the picker (live preview)
    pub fn handle_chat_font_size_selected(&mut self, size: u8) -> Task<Message> {
        self.config.chat_font_size = size.clamp(CHAT_FONT_SIZE_MIN, CHAT_FONT_SIZE_MAX);
        Task::none()
    }
}
