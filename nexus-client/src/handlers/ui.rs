//! UI panel toggles

use crate::NexusApp;
use crate::config::settings::{AVATAR_MAX_SIZE, CHAT_FONT_SIZE_MAX, CHAT_FONT_SIZE_MIN};
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, InputId, Message, SettingsFormState};
use crate::views::constants::{
    PERMISSION_USER_BROADCAST, PERMISSION_USER_CREATE, PERMISSION_USER_EDIT, PERMISSION_USER_INFO,
};
use iced::Task;
use iced::widget::{Id, markdown, operation};
use rfd::AsyncFileDialog;

impl NexusApp {
    // ==================== About ====================

    /// Show About panel (does nothing if already shown)
    pub fn handle_show_about(&mut self) -> Task<Message> {
        if self.ui_state.active_panel == ActivePanel::About {
            return Task::none();
        }

        self.ui_state.active_panel = ActivePanel::About;
        Task::none()
    }

    /// Close About panel
    pub fn handle_close_about(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    /// Open a URL in the default browser
    pub fn handle_open_url(&mut self, url: markdown::Uri) -> Task<Message> {
        let _ = open::that(url.as_str());
        Task::none()
    }

    // ==================== Server Info ====================

    /// Show Server Info panel
    pub fn handle_show_server_info(&mut self) -> Task<Message> {
        if self.ui_state.active_panel == ActivePanel::ServerInfo {
            return Task::none();
        }

        self.ui_state.active_panel = ActivePanel::ServerInfo;
        Task::none()
    }

    /// Close Server Info panel
    pub fn handle_close_server_info(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    // ==================== User Info ====================

    /// Close User Info panel
    pub fn handle_close_user_info(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    // ==================== Panel Permission Helpers ====================

    /// Close any active panel that the user doesn't have permission for.
    ///
    /// Called when switching connections or when a new connection is established
    /// to ensure the user doesn't see panels they can't access.
    pub fn close_panels_without_permission(&mut self, is_admin: bool, permissions: &[String]) {
        let has_broadcast = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_BROADCAST);
        let has_user_create = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_CREATE);
        let has_user_edit = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_EDIT);
        let has_user_info = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_INFO);

        match self.ui_state.active_panel {
            ActivePanel::Broadcast if !has_broadcast => {
                self.ui_state.active_panel = ActivePanel::None;
            }
            ActivePanel::AddUser if !has_user_create => {
                self.ui_state.active_panel = ActivePanel::None;
            }
            ActivePanel::EditUser if !has_user_edit => {
                self.ui_state.active_panel = ActivePanel::None;
            }
            ActivePanel::UserInfo if !has_user_info => {
                self.ui_state.active_panel = ActivePanel::None;
            }
            _ => {}
        }
    }
}

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

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.clear_add_user();
        self.focused_field = InputId::AdminUsername;
        operation::focus(Id::from(InputId::AdminUsername))
    }

    /// Show Edit User panel (does nothing if already shown)
    ///
    /// If `username` is provided, pre-fills the username field.
    pub fn handle_toggle_edit_user(&mut self, username: Option<String>) -> Task<Message> {
        if self.ui_state.active_panel == ActivePanel::EditUser {
            return Task::none();
        }

        self.ui_state.active_panel = ActivePanel::EditUser;

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.start_editing(username);
        self.focused_field = InputId::EditUsername;
        operation::focus(Id::from(InputId::EditUsername))
    }

    // ==================== Sidebar Toggles ====================

    /// Toggle bookmarks sidebar visibility
    pub fn handle_toggle_bookmarks(&mut self) -> Task<Message> {
        self.ui_state.show_bookmarks = !self.ui_state.show_bookmarks;
        self.scroll_chat_if_visible(false)
    }

    /// Toggle user list sidebar visibility
    pub fn handle_toggle_user_list(&mut self) -> Task<Message> {
        self.ui_state.show_user_list = !self.ui_state.show_user_list;
        self.scroll_chat_if_visible(false)
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
        self.config.settings.theme = theme.into();
        Task::none()
    }

    /// Handle connection notifications toggle
    pub fn handle_connection_notifications_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.show_connection_notifications = enabled;
        Task::none()
    }

    /// Handle chat font size selection from the picker (live preview)
    pub fn handle_chat_font_size_selected(&mut self, size: u8) -> Task<Message> {
        self.config.settings.chat_font_size = size.clamp(CHAT_FONT_SIZE_MIN, CHAT_FONT_SIZE_MAX);
        Task::none()
    }

    /// Handle show timestamps toggle
    pub fn handle_show_timestamps_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.show_timestamps = enabled;
        Task::none()
    }

    /// Handle 24-hour time format toggle
    pub fn handle_use_24_hour_time_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.use_24_hour_time = enabled;
        Task::none()
    }

    /// Handle show seconds toggle
    pub fn handle_show_seconds_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.show_seconds = enabled;
        Task::none()
    }

    // ==================== Avatar ====================

    /// Handle pick avatar button pressed - opens file dialog
    pub fn handle_pick_avatar_pressed(&mut self) -> Task<Message> {
        // Clear any previous error when starting a new pick
        if let Some(form) = &mut self.settings_form {
            form.error = None;
        }

        Task::future(async {
            let file = AsyncFileDialog::new()
                .add_filter("Images", &["png", "webp", "svg"])
                .pick_file()
                .await;

            match file {
                Some(handle) => {
                    let path = handle.path();
                    let extension = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();

                    // Determine MIME type from extension
                    let mime_type = match extension.as_str() {
                        "png" => "image/png",
                        "webp" => "image/webp",
                        "svg" => "image/svg+xml",
                        _ => {
                            return Message::AvatarLoaded(Err(AvatarError::UnsupportedType));
                        }
                    };

                    // Read file contents
                    let bytes = handle.read().await;

                    // Check size
                    if bytes.len() > AVATAR_MAX_SIZE {
                        return Message::AvatarLoaded(Err(AvatarError::TooLarge));
                    }

                    // Validate file content matches expected format
                    if !crate::user_avatar::validate_image_bytes(&bytes, mime_type) {
                        return Message::AvatarLoaded(Err(AvatarError::UnsupportedType));
                    }

                    // Build data URI
                    use base64::Engine;
                    let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let data_uri = format!("data:{};base64,{}", mime_type, base64_data);

                    Message::AvatarLoaded(Ok(data_uri))
                }
                None => {
                    // User cancelled - no change
                    Message::AvatarLoaded(Err(AvatarError::Cancelled))
                }
            }
        })
    }

    /// Handle avatar loaded from file picker
    pub fn handle_avatar_loaded(&mut self, result: Result<String, AvatarError>) -> Task<Message> {
        match result {
            Ok(data_uri) => {
                // Clear error and update avatar
                if let Some(form) = &mut self.settings_form {
                    form.error = None;
                    form.cached_avatar = crate::user_avatar::decode_data_uri(&data_uri);
                }
                self.config.settings.avatar = Some(data_uri);
            }
            Err(AvatarError::Cancelled) => {
                // User cancelled - no error to show
            }
            Err(AvatarError::UnsupportedType) => {
                if let Some(form) = &mut self.settings_form {
                    form.error = Some(t("err-avatar-unsupported-type"));
                }
            }
            Err(AvatarError::TooLarge) => {
                if let Some(form) = &mut self.settings_form {
                    let max_kb = (AVATAR_MAX_SIZE / 1024).to_string();
                    form.error = Some(t_args("err-avatar-too-large", &[("max_kb", &max_kb)]));
                }
            }
        }
        Task::none()
    }

    /// Handle clear avatar button pressed
    pub fn handle_clear_avatar_pressed(&mut self) -> Task<Message> {
        // Clear error and avatar when clearing
        if let Some(form) = &mut self.settings_form {
            form.error = None;
            form.cached_avatar = None;
        }
        self.config.settings.avatar = None;
        Task::none()
    }
}

// =============================================================================
// Avatar Error Type
// =============================================================================

/// Errors that can occur when loading an avatar
#[derive(Debug, Clone)]
pub enum AvatarError {
    /// User cancelled the file picker
    Cancelled,
    /// File type not supported (not PNG, WebP, or SVG)
    UnsupportedType,
    /// File exceeds maximum size
    TooLarge,
}
