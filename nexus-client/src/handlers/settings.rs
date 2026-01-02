//! Settings panel handlers

use iced::Task;
use iced::widget::{Id, operation};
use rfd::AsyncFileDialog;

use crate::NexusApp;
use crate::config::settings::{
    AVATAR_MAX_SIZE, CHAT_FONT_SIZE_MAX, CHAT_FONT_SIZE_MIN, default_download_path,
};
use crate::i18n::{t, t_args};
use crate::image::{ImagePickerError, decode_data_uri_square};
use crate::style::AVATAR_MAX_CACHE_SIZE;
use crate::types::{ActivePanel, InputId, Message, SettingsFormState, SettingsTab};

impl NexusApp {
    // ==================== Settings Panel ====================

    /// Show Settings panel (does nothing if already shown)
    ///
    /// Takes a snapshot of the current config so it can be restored on cancel.
    /// Focuses the appropriate field based on the active tab.
    pub fn handle_toggle_settings(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::Settings {
            return Task::none();
        }

        // Snapshot current config for potential cancel/restore
        self.settings_form = Some(SettingsFormState::new(&self.config, self.settings_tab));
        self.set_active_panel(ActivePanel::Settings);

        // Focus the appropriate field for the active tab
        self.focus_settings_tab_field()
    }

    /// Focus the appropriate input field for the current settings tab
    fn focus_settings_tab_field(&mut self) -> Task<Message> {
        match self.settings_tab {
            SettingsTab::General => {
                self.focused_field = InputId::SettingsNickname;
                operation::focus(Id::from(InputId::SettingsNickname))
            }
            SettingsTab::Chat => {
                // Chat tab has no text input fields
                Task::none()
            }
            SettingsTab::Network => {
                self.focused_field = InputId::ProxyAddress;
                operation::focus(Id::from(InputId::ProxyAddress))
            }
            SettingsTab::Files => {
                // Files tab has no text input fields (only browse button)
                Task::none()
            }
        }
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

    /// Handle settings tab selection
    ///
    /// Updates the active tab and focuses the appropriate field.
    pub fn handle_settings_tab_selected(&mut self, tab: SettingsTab) -> Task<Message> {
        // Update both the form state and the persistent tab state
        self.settings_tab = tab;
        if let Some(form) = &mut self.settings_form {
            form.active_tab = tab;
        }

        // Focus the appropriate field for the new tab
        self.focus_settings_tab_field()
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

    // ==================== Nickname ====================

    /// Handle nickname field change in settings panel
    pub fn handle_settings_nickname_changed(&mut self, nickname: String) -> Task<Message> {
        if nickname.is_empty() {
            self.config.settings.nickname = None;
        } else {
            self.config.settings.nickname = Some(nickname);
        }
        Task::none()
    }

    // ==================== Proxy ====================

    /// Handle proxy enabled toggle
    pub fn handle_proxy_enabled_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.proxy.enabled = enabled;
        Task::none()
    }

    /// Handle proxy address field change
    pub fn handle_proxy_address_changed(&mut self, address: String) -> Task<Message> {
        self.config.settings.proxy.address = address;
        Task::none()
    }

    /// Handle proxy port field change
    pub fn handle_proxy_port_changed(&mut self, port: u16) -> Task<Message> {
        self.config.settings.proxy.port = port;
        Task::none()
    }

    /// Handle proxy username field change
    pub fn handle_proxy_username_changed(&mut self, username: String) -> Task<Message> {
        if username.is_empty() {
            self.config.settings.proxy.username = None;
        } else {
            self.config.settings.proxy.username = Some(username);
        }
        Task::none()
    }

    /// Handle proxy password field change
    pub fn handle_proxy_password_changed(&mut self, password: String) -> Task<Message> {
        if password.is_empty() {
            self.config.settings.proxy.password = None;
        } else {
            self.config.settings.proxy.password = Some(password);
        }
        Task::none()
    }

    // ==================== Tab Navigation ====================

    /// Handle Tab key press in settings panel - check which field is focused
    ///
    /// Tab navigation is scoped to the active settings tab:
    /// - General tab: nickname (single field)
    /// - Chat tab: no focusable fields (only checkboxes/pickers)
    /// - Network tab: address -> username -> password (skips port NumberInput)
    /// - Files tab: no focusable fields (only browse button)
    pub fn handle_settings_tab_pressed(&mut self) -> Task<Message> {
        match self.settings_tab {
            SettingsTab::General => {
                // General tab only has nickname field - focus it
                self.focused_field = InputId::SettingsNickname;
                operation::focus(Id::from(InputId::SettingsNickname))
            }
            SettingsTab::Chat => {
                // Chat tab has no text input fields, just checkboxes and pickers
                Task::none()
            }
            SettingsTab::Files => {
                // Files tab has no text input fields, just a browse button
                Task::none()
            }
            SettingsTab::Network => {
                // Network tab: cycle through proxy fields
                let check_address = operation::is_focused(Id::from(InputId::ProxyAddress));
                let check_port = operation::is_focused(Id::from(InputId::ProxyPort));
                let check_username = operation::is_focused(Id::from(InputId::ProxyUsername));
                let check_password = operation::is_focused(Id::from(InputId::ProxyPassword));

                Task::batch([
                    check_address.map(|focused| (0, focused)),
                    check_port.map(|focused| (1, focused)),
                    check_username.map(|focused| (2, focused)),
                    check_password.map(|focused| (3, focused)),
                ])
                .collect()
                .map(|results: Vec<(u8, bool)>| {
                    let address = results.iter().any(|(i, f)| *i == 0 && *f);
                    let port = results.iter().any(|(i, f)| *i == 1 && *f);
                    let username = results.iter().any(|(i, f)| *i == 2 && *f);
                    let password = results.iter().any(|(i, f)| *i == 3 && *f);
                    Message::SettingsNetworkFocusResult(address, port, username, password)
                })
            }
        }
    }

    /// Handle focus check result for Network tab and move to next field
    pub fn handle_settings_network_focus_result(
        &mut self,
        address: bool,
        port: bool,
        username: bool,
        password: bool,
    ) -> Task<Message> {
        // Cycle through Network tab fields: address -> username -> password -> address
        // (skips port because NumberInput handles its own Tab key)
        if address {
            self.focused_field = InputId::ProxyUsername;
            operation::focus(Id::from(InputId::ProxyUsername))
        } else if port {
            // If somehow focused on port, move to username
            self.focused_field = InputId::ProxyUsername;
            operation::focus(Id::from(InputId::ProxyUsername))
        } else if username {
            self.focused_field = InputId::ProxyPassword;
            operation::focus(Id::from(InputId::ProxyPassword))
        } else if password {
            self.focused_field = InputId::ProxyAddress;
            operation::focus(Id::from(InputId::ProxyAddress))
        } else {
            // No field focused, start with address
            self.focused_field = InputId::ProxyAddress;
            operation::focus(Id::from(InputId::ProxyAddress))
        }
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

    // ==================== Download Path ====================

    /// Handle browse download path button pressed - opens folder picker
    pub fn handle_browse_download_path_pressed(&mut self) -> Task<Message> {
        // Get the current download path or system default for initial directory
        let initial_dir = self
            .config
            .settings
            .download_path
            .clone()
            .or_else(default_download_path);

        Task::future(async move {
            let mut dialog = AsyncFileDialog::new();

            // Set initial directory if available
            if let Some(ref path) = initial_dir {
                dialog = dialog.set_directory(path);
            }

            let folder = dialog.pick_folder().await;

            match folder {
                Some(handle) => {
                    let path = handle.path().to_string_lossy().into_owned();
                    Message::DownloadPathSelected(Some(path))
                }
                None => {
                    // User cancelled - no change
                    Message::DownloadPathSelected(None)
                }
            }
        })
    }

    /// Handle download path selected from folder picker
    pub fn handle_download_path_selected(&mut self, path: Option<String>) -> Task<Message> {
        if let Some(path) = path {
            self.config.settings.download_path = Some(path);
        }
        // If None, user cancelled - no change needed
        Task::none()
    }
}
