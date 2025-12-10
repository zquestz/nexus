//! Settings panel handlers

use crate::NexusApp;
use crate::config::settings::{AVATAR_MAX_SIZE, CHAT_FONT_SIZE_MAX, CHAT_FONT_SIZE_MIN};
use crate::i18n::{t, t_args};
use crate::image::{ImagePickerError, decode_data_uri_square};
use crate::style::AVATAR_MAX_CACHE_SIZE;
use crate::types::{ActivePanel, Message, SettingsFormState};
use iced::Task;
use rfd::AsyncFileDialog;

impl NexusApp {
    // ==================== Settings Panel ====================

    /// Show Settings panel (does nothing if already shown)
    ///
    /// Takes a snapshot of the current config so it can be restored on cancel.
    pub fn handle_toggle_settings(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::Settings {
            return Task::none();
        }

        // Snapshot current config for potential cancel/restore
        self.settings_form = Some(SettingsFormState::new(&self.config));
        self.set_active_panel(ActivePanel::Settings);
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

    // ==================== Theme & Display ====================

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

    // ==================== Timestamps ====================

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
                .add_filter("Images", &["png", "webp", "svg", "jpg", "jpeg"])
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
                        "jpg" | "jpeg" => "image/jpeg",
                        _ => {
                            return Message::AvatarLoaded(Err(ImagePickerError::UnsupportedType));
                        }
                    };

                    // Read file contents
                    let bytes = handle.read().await;

                    // Check size
                    if bytes.len() > AVATAR_MAX_SIZE {
                        return Message::AvatarLoaded(Err(ImagePickerError::TooLarge));
                    }

                    // Validate file content matches expected format
                    if !crate::image::validate_image_bytes(&bytes, mime_type) {
                        return Message::AvatarLoaded(Err(ImagePickerError::UnsupportedType));
                    }

                    // Build data URI
                    use base64::Engine;
                    let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let data_uri = format!("data:{};base64,{}", mime_type, base64_data);

                    Message::AvatarLoaded(Ok(data_uri))
                }
                None => {
                    // User cancelled - no change
                    Message::AvatarLoaded(Err(ImagePickerError::Cancelled))
                }
            }
        })
    }

    /// Handle avatar loaded from file picker
    pub fn handle_avatar_loaded(
        &mut self,
        result: Result<String, ImagePickerError>,
    ) -> Task<Message> {
        match result {
            Ok(data_uri) => {
                if let Some(form) = &mut self.settings_form {
                    let cached = decode_data_uri_square(&data_uri, AVATAR_MAX_CACHE_SIZE);
                    if cached.is_some() {
                        form.error = None;
                        form.cached_avatar = cached;
                        self.config.settings.avatar = Some(data_uri);
                    } else {
                        form.error = Some(t("err-avatar-decode-failed"));
                    }
                }
            }
            Err(ImagePickerError::Cancelled) => {
                // User cancelled - no error to show
            }
            Err(ImagePickerError::UnsupportedType) => {
                if let Some(form) = &mut self.settings_form {
                    form.error = Some(t("err-avatar-unsupported-type"));
                }
            }
            Err(ImagePickerError::TooLarge) => {
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
