//! Settings panel handlers

use iced_toasts::{ToastLevel, toast};

use crate::config::audio::PttReleaseDelay;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators;

#[cfg(all(unix, not(target_os = "macos")))]
use std::time::Instant;

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::voice::VoiceQuality;
use rfd::AsyncFileDialog;

use crate::NexusApp;
use crate::config::audio::PttMode;
use crate::config::events::{EventType, NotificationContent, SoundChoice};
use crate::config::settings::{
    AVATAR_MAX_SIZE, CHAT_FONT_SIZE_MAX, CHAT_FONT_SIZE_MIN, default_download_path,
};
use crate::i18n::{t, t_args};
use crate::image::{ImagePickerError, decode_data_uri_square};
use crate::style::AVATAR_MAX_CACHE_SIZE;
use crate::types::{
    ActivePanel, ChatMessage, InputId, Message, PendingRequests, ResponseRouting,
    SettingsFormState, SettingsTab,
};
use crate::voice::audio::AudioDevice;

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
        self.settings_form = Some(SettingsFormState::new(
            &self.config,
            self.settings_tab,
            self.selected_event_type,
        ));
        self.set_active_panel(ActivePanel::Settings);

        // Focus the appropriate field for the active tab
        self.focus_settings_tab_field()
    }

    /// Focus the appropriate input field for the current settings tab.
    fn focus_settings_tab_field(&mut self) -> Task<Message> {
        match self.settings_tab {
            SettingsTab::General => {
                self.focused_field = InputId::SettingsNickname;
                operation::focus(Id::from(InputId::SettingsNickname))
            }
            SettingsTab::Chat => {
                // Auto-away message is the only text input. Focus it
                // unconditionally; when auto-away is disabled the
                // input has no `on_input` so typing is a no-op, which
                // matches the user's visual cue (greyed-out input).
                self.focused_field = InputId::SettingsAutoAwayMessage;
                operation::focus(Id::from(InputId::SettingsAutoAwayMessage))
            }
            SettingsTab::Network => {
                // Only focus the address input when the proxy is
                // enabled — otherwise the field is rendered as
                // disabled (no `on_input`) and focusing a non-
                // editable field would be a confusing UX.
                if self.config.settings.proxy.enabled {
                    self.focused_field = InputId::ProxyAddress;
                    operation::focus(Id::from(InputId::ProxyAddress))
                } else {
                    Task::none()
                }
            }
            SettingsTab::Files => {
                // Files tab has no text input fields (only browse button)
                Task::none()
            }
            SettingsTab::Events => {
                // Events tab has no text input fields (only pickers and checkboxes)
                Task::none()
            }
            SettingsTab::Audio => {
                // Audio tab has no text input fields (only pickers and key capture)
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
        // Validate nickname (if not empty)
        if let Some(ref nickname) = self.config.settings.nickname
            && let Err(e) = validators::validate_nickname(nickname)
        {
            let error_msg = match e {
                // The outer `if let Some(ref nickname)` doesn't filter
                // empty strings (only `None`), so a `Some("")` from
                // config could reach the validator and produce `Empty`.
                // Surface the existing translated error instead of
                // panicking — `unreachable!()` would crash the client
                // when saving settings with an empty configured nickname.
                validators::NicknameError::Empty => t("err-nickname-empty"),
                validators::NicknameError::TooLong => t_args(
                    "err-nickname-too-long",
                    &[("max", &validators::MAX_NICKNAME_LENGTH.to_string())],
                ),
                validators::NicknameError::InvalidCharacters => t("err-nickname-invalid"),
            };
            if let Some(form) = &mut self.settings_form {
                form.error = Some(error_msg);
            }
            return Task::none();
        }

        // Validate auto-away message (if not empty)
        if !self.config.settings.auto_away_message.is_empty()
            && let Err(e) = validators::validate_status(&self.config.settings.auto_away_message)
        {
            let error_msg = match e {
                validators::StatusError::TooLong => t_args(
                    "err-status-too-long",
                    &[("max", &validators::MAX_STATUS_LENGTH.to_string())],
                ),
                validators::StatusError::ContainsNewlines => t("err-status-contains-newlines"),
                validators::StatusError::InvalidCharacters => t("err-status-invalid-characters"),
            };
            if let Some(form) = &mut self.settings_form {
                form.error = Some(error_msg);
            }
            return Task::none();
        }

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

    // ==================== Auto-Away ====================

    /// Handle auto-away timer tick (fires every 30s when auto-away is enabled)
    ///
    /// For each connection that has been idle long enough, sends UserAway
    /// with the configured message. Checks: elapsed >= timeout, not already away,
    /// not already auto-awayed, no pending auto-away request.
    pub fn handle_auto_away_tick(&mut self) -> Task<Message> {
        let Some(timeout) = self.config.settings.auto_away_timeout.as_duration() else {
            return Task::none();
        };

        let message = if self.config.settings.auto_away_message.is_empty() {
            None
        } else {
            Some(self.config.settings.auto_away_message.clone())
        };

        // Collect connection IDs that need auto-away
        let conn_ids: Vec<usize> = self
            .connections
            .values()
            .filter(|conn| {
                conn.last_activity.elapsed() >= timeout
                    && !conn.is_away
                    && !conn.is_auto_away
                    && conn.voice_session.is_none()
                    && !conn
                        .pending_requests
                        .values()
                        .any(|r| matches!(r, ResponseRouting::AutoAwayResult(_)))
            })
            .map(|conn| conn.connection_id)
            .collect();

        for conn_id in conn_ids {
            let Some(conn) = self.connections.get_mut(&conn_id) else {
                continue;
            };

            let client_msg = ClientMessage::UserAway {
                message: message.clone(),
            };
            if let Ok(message_id) = conn.send(client_msg) {
                conn.pending_requests
                    .track(message_id, ResponseRouting::AutoAwayResult(message.clone()));
            }
        }

        Task::none()
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
        self.config.settings.show_connection_events = enabled;
        Task::none()
    }

    /// Handle channel join/leave notifications toggle
    pub fn handle_channel_notifications_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.show_join_leave_events = enabled;
        Task::none()
    }

    /// Handle chat font size selection from the picker (live preview)
    pub fn handle_chat_font_size_selected(&mut self, size: u8) -> Task<Message> {
        self.config.settings.chat_font_size = size.clamp(CHAT_FONT_SIZE_MIN, CHAT_FONT_SIZE_MAX);
        Task::none()
    }

    /// Handle max scrollback lines change
    pub fn handle_max_scrollback_changed(&mut self, value: usize) -> Task<Message> {
        self.config.settings.max_scrollback = value;
        Task::none()
    }

    /// Handle chat history retention selection from the picker
    pub fn handle_chat_history_retention_selected(
        &mut self,
        retention: crate::config::settings::ChatHistoryRetention,
    ) -> Task<Message> {
        self.config.settings.chat_history_retention = retention;

        // Update all existing history managers to use new retention setting
        for manager in self.history_managers.values_mut() {
            manager.update_retention(retention);
        }

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
        self.focused_field = InputId::SettingsNickname;
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
        self.focused_field = InputId::ProxyAddress;
        Task::none()
    }

    /// Handle proxy port field change
    pub fn handle_proxy_port_changed(&mut self, port: u16) -> Task<Message> {
        self.config.settings.proxy.port = port;
        self.focused_field = InputId::ProxyPort;
        Task::none()
    }

    /// Handle proxy username field change
    pub fn handle_proxy_username_changed(&mut self, username: String) -> Task<Message> {
        if username.is_empty() {
            self.config.settings.proxy.username = None;
        } else {
            self.config.settings.proxy.username = Some(username);
        }
        self.focused_field = InputId::ProxyUsername;
        Task::none()
    }

    /// Handle proxy password field change
    pub fn handle_proxy_password_changed(&mut self, password: String) -> Task<Message> {
        if password.is_empty() {
            self.config.settings.proxy.password = None;
        } else {
            self.config.settings.proxy.password = Some(password);
        }
        self.focused_field = InputId::ProxyPassword;
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
            // Tabs without text inputs — Tab is a no-op.
            SettingsTab::Files | SettingsTab::Events | SettingsTab::Audio => Task::none(),
            // Tabs with text inputs — query iced for the actually-
            // focused widget id and let the resolved handler do the
            // cycle (settings_tab scopes the cycle since only one tab
            // is rendered at a time).
            SettingsTab::General | SettingsTab::Chat | SettingsTab::Network => {
                super::focus::dispatch_find_focused(Message::SettingsTabResolved)
            }
        }
    }

    pub fn handle_settings_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        let next = match self.settings_tab {
            SettingsTab::General => {
                // Single field — always focus the nickname input.
                InputId::SettingsNickname
            }
            SettingsTab::Chat => {
                // Single text input (auto-away message). Other Chat
                // tab widgets are checkboxes / pickers / NumberInput.
                InputId::SettingsAutoAwayMessage
            }
            SettingsTab::Network => {
                // Skip Port (NumberInput handles its own Tab).
                const CYCLE: &[InputId] = &[
                    InputId::ProxyAddress,
                    InputId::ProxyUsername,
                    InputId::ProxyPassword,
                ];
                super::focus::next_in_cycle(&focused, CYCLE)
            }
            SettingsTab::Files | SettingsTab::Events | SettingsTab::Audio => {
                return Task::none();
            }
        };
        self.focused_field = next;
        operation::focus(Id::from(next))
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

    // ==================== Transfer Queue ====================

    /// Handle queue transfers checkbox toggle
    pub fn handle_queue_transfers_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.config.settings.queue_transfers = enabled;
        Task::none()
    }

    /// Handle download limit change
    pub fn handle_download_limit_changed(&mut self, limit: u8) -> Task<Message> {
        self.config.settings.download_limit = limit;
        Task::none()
    }

    /// Handle upload limit change
    pub fn handle_upload_limit_changed(&mut self, limit: u8) -> Task<Message> {
        self.config.settings.upload_limit = limit;
        Task::none()
    }

    // =========================================================================
    // Event Settings Handlers
    // =========================================================================

    /// Handle event type selection in Events tab
    pub fn handle_event_type_selected(&mut self, event_type: EventType) -> Task<Message> {
        // Save to app state and config so it persists across panel opens and restarts
        self.selected_event_type = event_type;
        self.config.settings.selected_event_type = event_type;
        if let Some(form) = &mut self.settings_form {
            form.selected_event_type = event_type;
        }
        Task::none()
    }

    /// Handle show notification checkbox toggle for selected event
    pub fn handle_event_show_notification_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .show_notification = enabled;
        }
        Task::none()
    }

    /// Handle notification content level selection for selected event
    pub fn handle_event_notification_content_selected(
        &mut self,
        content: NotificationContent,
    ) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .notification_content = content;
        }
        Task::none()
    }

    /// Handle test notification button press
    pub fn handle_test_notification(&mut self) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            let config = self.config.settings.event_settings.get(event_type);

            // Build a sample notification with current content level
            let (summary, body) =
                crate::events::build_test_event_content(event_type, config.notification_content);

            // Show the notification
            let mut notification = notify_rust::Notification::new();
            notification
                .appname("Nexus BBS")
                .summary(&summary)
                .body(body.as_deref().unwrap_or(""))
                .auto_icon()
                .timeout(notify_rust::Timeout::Milliseconds(5000));

            // On Linux, keep handle alive to prevent GNOME/Cinnamon from dismissing
            // notifications when the D-Bus connection would otherwise be dropped.
            #[cfg(all(unix, not(target_os = "macos")))]
            if let Ok(handle) = notification.show()
                && let Ok(mut handles) = crate::events::NOTIFICATION_HANDLES.lock()
            {
                let now = Instant::now();
                handles.retain(|(created, _)| {
                    now.duration_since(*created) < crate::events::HANDLE_LIFETIME
                });
                handles.push((now, handle));
            }

            // On non-Linux platforms, just show and ignore result
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            let _ = notification.show();
        }
        Task::none()
    }

    /// Handle show toast checkbox toggle for selected event
    pub fn handle_event_show_toast_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .show_toast = enabled;
        }
        Task::none()
    }

    /// Handle toast content level selection for selected event
    pub fn handle_event_toast_content_selected(
        &mut self,
        content: NotificationContent,
    ) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .toast_content = content;
        }
        Task::none()
    }

    /// Handle test toast button press
    pub fn handle_test_toast(&mut self) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            let config = self.config.settings.event_settings.get(event_type);

            let (summary, body) =
                crate::events::build_test_event_content(event_type, config.toast_content);

            let toast_text = if let Some(body) = body {
                format!("{}: {}", summary, body)
            } else {
                summary
            };

            self.toasts.push(toast(&toast_text).level(ToastLevel::Info));
        }
        Task::none()
    }

    /// Handle play sound checkbox toggle for selected event
    pub fn handle_event_play_sound_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .play_sound = enabled;
        }
        Task::none()
    }

    /// Handle sound selection for selected event
    pub fn handle_event_sound_selected(&mut self, sound: SoundChoice) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .sound = sound;
        }
        Task::none()
    }

    /// Handle always play sound checkbox toggle for selected event
    pub fn handle_event_always_play_sound_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            self.config
                .settings
                .event_settings
                .get_mut(event_type)
                .always_play_sound = enabled;
        }
        Task::none()
    }

    /// Handle test sound button press
    pub fn handle_test_sound(&mut self) -> Task<Message> {
        if let Some(form) = &self.settings_form {
            let event_type = form.selected_event_type;
            let config = self.config.settings.event_settings.get(event_type);

            // Play the selected sound at the current volume on the selected output device
            crate::sound::play_sound_on_device(
                &config.sound,
                self.config.settings.sound_volume,
                &self.config.settings.audio.output_device,
            );
        }
        Task::none()
    }

    // =========================================================================
    // Audio Settings Handlers
    // =========================================================================

    /// Handle refresh audio devices button
    ///
    /// Re-enumerates audio devices and updates the cached lists in SettingsFormState.
    /// This allows users to see newly connected devices without closing/reopening settings.
    pub fn handle_audio_refresh_devices(&mut self) -> Task<Message> {
        if let Some(form) = &mut self.settings_form {
            form.output_devices = crate::voice::audio::list_output_devices();
            form.input_devices = crate::voice::audio::list_input_devices();
        }
        Task::none()
    }

    /// Handle output device selection
    pub fn handle_audio_output_device_selected(&mut self, device: AudioDevice) -> Task<Message> {
        // Store empty string for system default, otherwise the device name
        self.config.settings.audio.output_device = if device.is_default {
            String::new()
        } else {
            device.name
        };
        Task::none()
    }

    /// Handle input device selection
    pub fn handle_audio_input_device_selected(&mut self, device: AudioDevice) -> Task<Message> {
        // Store empty string for system default, otherwise the device name
        self.config.settings.audio.input_device = if device.is_default {
            String::new()
        } else {
            device.name
        };
        Task::none()
    }

    /// Handle voice quality selection
    ///
    /// Updates the config and also applies the change to any active voice session
    /// immediately, so users don't need to leave and rejoin to change quality.
    pub fn handle_audio_quality_selected(&mut self, quality: VoiceQuality) -> Task<Message> {
        self.config.settings.audio.voice_quality = quality;

        // Update active voice session if one exists
        if let Some(ref handle) = self.voice_session_handle {
            handle.set_quality(quality);
        }

        Task::none()
    }

    /// Update active voice session with current processor settings
    ///
    /// Called when noise suppression, echo cancellation, or AGC settings change.
    /// Applies the change to any active voice session immediately.
    pub fn update_voice_processor_settings(&self) {
        if let Some(ref handle) = self.voice_session_handle {
            handle.set_processor_settings(crate::voice::processor::AudioProcessorSettings {
                noise_suppression: self.config.settings.audio.noise_suppression,
                noise_suppression_level: self.config.settings.audio.noise_suppression_level,
                echo_cancellation: self.config.settings.audio.echo_cancellation,
                agc: self.config.settings.audio.agc,
                transient_suppression: self.config.settings.audio.transient_suppression,
                mic_boost: self.config.settings.audio.mic_boost,
            });
        }
    }

    /// Handle PTT key capture mode toggle
    pub fn handle_audio_ptt_key_capture(&mut self) -> Task<Message> {
        if let Some(form) = &mut self.settings_form {
            form.ptt_capturing = true;
        }
        Task::none()
    }

    /// Handle PTT key captured
    pub fn handle_audio_ptt_key_captured(&mut self, key: String) -> Task<Message> {
        if let Some(form) = &mut self.settings_form {
            form.ptt_capturing = false;
        }
        self.config.settings.audio.ptt_key = key.clone();

        // Re-register hotkey immediately if in voice (applies without rejoin)
        if let Some(ref mut ptt) = self.ptt_manager
            && let Some(connection_id) = self.active_voice_connection
            && let Err(e) = ptt.register_hotkey(&key)
        {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-voice-ptt-failed", &[("error", &e)])),
            );
        }

        Task::none()
    }

    /// Handle PTT mode selection
    pub fn handle_audio_ptt_mode_selected(&mut self, mode: PttMode) -> Task<Message> {
        self.config.settings.audio.ptt_mode = mode;

        // Update active PTT manager if in voice (applies immediately, no rejoin needed)
        if let Some(ref mut ptt) = self.ptt_manager {
            ptt.set_mode(mode);
        }

        // Stop transmitting if mode changed while transmitting (e.g., toggle mode was on)
        // set_mode() resets the active flag, so we need to sync the voice session
        if let Some(ref handle) = self.voice_session_handle {
            handle.stop_transmitting();
        }
        self.is_local_speaking = false;

        Task::none()
    }

    /// Handle PTT release delay selection
    ///
    /// Updates the setting immediately. The new delay will apply to the next
    /// PTT release (no need to rejoin voice).
    pub fn handle_audio_ptt_release_delay_selected(
        &mut self,
        delay: PttReleaseDelay,
    ) -> Task<Message> {
        self.config.settings.audio.ptt_release_delay = delay;
        let _ = self.config.save();

        // No need to update anything else - the delay is read from config
        // each time PTT is released in handle_voice_ptt_state_changed()

        Task::none()
    }

    /// Handle microphone test start
    ///
    /// Sets mic_testing to true, which triggers the mic_test_subscription
    /// in the main subscription function to start capturing audio.
    pub fn handle_audio_test_mic_start(&mut self) -> Task<Message> {
        if let Some(form) = &mut self.settings_form {
            form.mic_testing = true;
            form.mic_level = 0.0;
            form.mic_error = None; // Clear any previous error
        }
        Task::none()
    }

    /// Handle microphone test stop
    ///
    /// Sets mic_testing to false, which causes the mic_test_subscription
    /// to be dropped and audio capture stops.
    pub fn handle_audio_test_mic_stop(&mut self) -> Task<Message> {
        if let Some(form) = &mut self.settings_form {
            form.mic_testing = false;
            form.mic_level = 0.0;
        }
        Task::none()
    }

    /// Handle microphone level update (from audio capture)
    pub fn handle_audio_mic_level(&mut self, level: f32) -> Task<Message> {
        if let Some(form) = &mut self.settings_form
            && form.mic_testing
        {
            form.mic_level = level.clamp(0.0, 1.0);
        }
        Task::none()
    }

    /// Handle microphone test error
    ///
    /// Displays the error message and stops the mic test.
    pub fn handle_audio_mic_error(&mut self, error: String) -> Task<Message> {
        // Stop the mic test
        if let Some(form) = &mut self.settings_form {
            form.mic_testing = false;
            form.mic_level = 0.0;
            form.mic_error = Some(error);
        }
        Task::none()
    }
}
