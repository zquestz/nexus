//! Server info edit handlers

use iced::Task;
use iced::widget::Id;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{
    self, MAX_PUBLIC_ADDRESS_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_NAME_LENGTH,
    PublicAddressError, ServerDescriptionError, ServerImageError, ServerNameError,
};
use rfd::AsyncFileDialog;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::image::{ImagePickerError, decode_data_uri_max_width};
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::style::SERVER_IMAGE_MAX_SIZE;
use crate::types::{InputId, Message, ServerInfoEditState, ServerInfoParams, ServerInfoTab};

impl NexusApp {
    // ==================== Panel Actions ====================

    /// Change the active tab in server info display mode
    pub fn handle_server_info_tab_changed(&mut self, tab: ServerInfoTab) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.server_info_tab = tab;
        Task::none()
    }

    /// Enter server info edit mode
    ///
    /// Creates a new edit state with the current server info values.
    /// Only admins can access this feature.
    pub fn handle_edit_server_info_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Only admins can edit server info
        if !conn.is_admin {
            return Task::none();
        }

        // Create edit state with current values
        conn.server_info_edit = Some(ServerInfoEditState::new(ServerInfoParams {
            auto_join_channels: conn.auto_join_channels.as_deref(),
            chat_burst_limit: conn.chat_burst_limit,
            chat_rate_limit: conn.chat_rate_limit,
            description: conn.server_description.as_deref(),
            file_reindex_interval: conn.file_reindex_interval,
            image: &conn.server_image,
            max_connections_per_ip: conn.max_connections_per_ip,
            max_outbound_rate: conn.max_outbound_rate,
            max_transfers_per_ip: conn.max_transfers_per_ip,
            min_password_strength: conn.min_password_strength,
            name: conn.server_name.as_deref(),
            persistent_channels: conn.persistent_channels.as_deref(),
            public_address: conn.public_address.as_deref(),
            scheduler_chunk_size: conn.scheduler_chunk_size,
        }));

        // Focus the name input
        self.focus_field(InputId::EditServerInfoName)
    }

    /// Cancel server info edit mode
    ///
    /// Clears the edit state and returns to the display view.
    pub fn handle_cancel_edit_server_info(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.server_info_edit = None;
        Task::none()
    }

    /// Save server info changes
    ///
    /// Validates the form and sends `ServerInfoUpdate` to the server.
    pub fn handle_update_server_info_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Get edit state
        let Some(edit_state) = &conn.server_info_edit else {
            return Task::none();
        };

        // Prevent double-submit
        if edit_state.is_submitting {
            return Task::none();
        }

        // Validate server name
        if let Err(e) = validators::validate_server_name(&edit_state.name) {
            let error_msg = match e {
                ServerNameError::Empty => t("err-server-name-empty"),
                ServerNameError::TooLong => t_args(
                    "err-server-name-too-long",
                    &[("max", &MAX_SERVER_NAME_LENGTH.to_string())],
                ),
                ServerNameError::ContainsNewlines => t("err-server-name-contains-newlines"),
                ServerNameError::InvalidCharacters => t("err-server-name-invalid-characters"),
            };
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(error_msg);
            }
            return Task::none();
        }

        // Validate server description
        if let Err(e) = validators::validate_server_description(&edit_state.description) {
            let error_msg = match e {
                ServerDescriptionError::TooLong => t_args(
                    "err-server-description-too-long",
                    &[("max", &MAX_SERVER_DESCRIPTION_LENGTH.to_string())],
                ),
                ServerDescriptionError::ContainsNewlines => {
                    t("err-server-description-contains-newlines")
                }
                ServerDescriptionError::InvalidCharacters => {
                    t("err-server-description-invalid-characters")
                }
            };
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(error_msg);
            }
            return Task::none();
        }

        // Validate public_address (empty string is allowed to clear)
        if let Err(e) = validators::validate_public_address(&edit_state.public_address) {
            let error_msg = match e {
                PublicAddressError::TooLong => t_args(
                    "err-public-address-too-long",
                    &[("max", &MAX_PUBLIC_ADDRESS_LENGTH.to_string())],
                ),
                PublicAddressError::ContainsScheme => t("err-public-address-contains-scheme"),
                PublicAddressError::ContainsBrackets => t("err-public-address-contains-brackets"),
                PublicAddressError::ContainsPath => t("err-public-address-contains-path"),
                PublicAddressError::ContainsUserinfo => t("err-public-address-contains-userinfo"),
                PublicAddressError::ContainsWhitespace => {
                    t("err-public-address-contains-whitespace")
                }
                PublicAddressError::ContainsPort => t("err-public-address-contains-port"),
                PublicAddressError::ContainsZoneId => t("err-public-address-contains-zone-id"),
                PublicAddressError::InvalidFormat => t("err-public-address-invalid-format"),
            };
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(error_msg);
            }
            return Task::none();
        }

        // Validate server image if not empty
        if !edit_state.image.is_empty()
            && let Err(e) = validators::validate_server_image(&edit_state.image)
        {
            let error_msg = match e {
                ServerImageError::TooLarge => t("err-server-image-too-large"),
                ServerImageError::InvalidFormat => t("err-server-image-invalid-format"),
                ServerImageError::UnsupportedType => t("err-server-image-unsupported-type"),
            };
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(error_msg);
            }
            return Task::none();
        }

        // Diff against the baseline captured when the form opened, not live
        // connection state. ServerInfoUpdated/PermissionsUpdated can arrive
        // while the form is open; live diffs would resend stale form values.
        let Some(fields) = edit_state.update_fields() else {
            return Task::none();
        };

        let msg = ClientMessage::ServerInfoUpdate {
            name: fields.name,
            description: fields.description,
            public_address: fields.public_address,
            max_connections_per_ip: fields.max_connections_per_ip,
            max_transfers_per_ip: fields.max_transfers_per_ip,
            image: fields.image,
            file_reindex_interval: fields.file_reindex_interval,
            persistent_channels: fields.persistent_channels,
            auto_join_channels: fields.auto_join_channels,
            chat_burst_limit: fields.chat_burst_limit,
            chat_rate_limit: fields.chat_rate_limit,
            min_password_strength: fields.min_password_strength,
            max_outbound_rate: fields.max_outbound_rate,
            scheduler_chunk_size: fields.scheduler_chunk_size,
        };

        // Mark as submitting to prevent double-submit
        if let Some(edit) = &mut conn.server_info_edit {
            edit.is_submitting = true;
        }

        if let Err(e) = conn.send(msg) {
            if let Some(edit) = &mut conn.server_info_edit {
                edit.is_submitting = false;
                edit.error = Some(t_args(
                    "err-failed-send-update",
                    &[("error", &e.to_string())],
                ));
            }
            return Task::none();
        }

        // Keep edit mode open until we get a response
        // The response handler will close it on success
        Task::none()
    }

    // ==================== Form Field Handlers ====================

    /// Handle server info name field change
    pub fn handle_edit_server_info_name_changed(&mut self, name: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.name = name;
        }
        self.focused_field = InputId::EditServerInfoName;
        Task::none()
    }

    /// Handle server info description field change
    pub fn handle_edit_server_info_description_changed(
        &mut self,
        description: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.description = description;
        }
        self.focused_field = InputId::EditServerInfoDescription;
        Task::none()
    }

    /// Handle server info public_address field change
    pub fn handle_edit_server_info_public_address_changed(
        &mut self,
        address: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.public_address = address;
        }
        self.focused_field = InputId::EditServerInfoPublicAddress;
        Task::none()
    }

    /// Copy the server's public `nexus://` URI to the clipboard and show a toast
    pub fn handle_copy_server_uri(&mut self, uri: String) -> Task<Message> {
        let toast_text = t("toast-link-copied");
        iced::clipboard::write(uri).chain(Task::done(Message::ShowToast(toast_text)))
    }

    /// Handle server info chat burst limit field change
    pub fn handle_edit_server_info_chat_burst_limit_changed(
        &mut self,
        limit: u32,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.chat_burst_limit = limit;
        }
        Task::none()
    }

    /// Handle server info chat rate limit field change
    pub fn handle_edit_server_info_chat_rate_limit_changed(&mut self, limit: u32) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.chat_rate_limit = limit;
        }
        Task::none()
    }

    /// Handle server info max connections field change
    pub fn handle_edit_server_info_max_connections_changed(
        &mut self,
        max_connections: u32,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.max_connections_per_ip = Some(max_connections);
        }
        Task::none()
    }

    /// Handle server info max transfers field change
    pub fn handle_edit_server_info_max_transfers_changed(
        &mut self,
        max_transfers: u32,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.max_transfers_per_ip = Some(max_transfers);
        }
        Task::none()
    }

    /// Handle server info max outbound rate (Mbps, NumberInput<f64>) field
    /// change. NumberInput already constrains to the configured bounds, so
    /// we just store the new value; conversion to bytes/sec happens at save
    /// time via `ServerInfoEditState::max_outbound_rate_bytes_per_sec`.
    pub fn handle_edit_server_info_max_outbound_rate_changed(
        &mut self,
        mbps: f64,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.max_outbound_rate_mbps = mbps;
        }
        Task::none()
    }

    /// Handle server info scheduler chunk size (bytes) field change.
    pub fn handle_edit_server_info_scheduler_chunk_size_changed(
        &mut self,
        size: Option<u32>,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.scheduler_chunk_size = size;
        }
        Task::none()
    }

    /// Handle server info file reindex interval field change
    pub fn handle_edit_server_info_file_reindex_interval_changed(
        &mut self,
        interval: u32,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.file_reindex_interval = Some(interval);
        }
        Task::none()
    }

    /// Handle server info persistent channels field change
    pub fn handle_edit_server_info_persistent_channels_changed(
        &mut self,
        persistent_channels: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.persistent_channels = persistent_channels;
        }
        self.focused_field = InputId::EditServerInfoPersistentChannels;
        Task::none()
    }

    /// Handle server info auto-join channels field change
    pub fn handle_edit_server_info_auto_join_channels_changed(
        &mut self,
        auto_join_channels: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.auto_join_channels = auto_join_channels;
        }
        self.focused_field = InputId::EditServerInfoAutoJoinChannels;
        Task::none()
    }

    /// Handle server info min password strength field change
    pub fn handle_edit_server_info_min_password_strength_changed(
        &mut self,
        strength: nexus_common::validators::PasswordStrength,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.min_password_strength = strength;
        }
        Task::none()
    }

    // ==================== Image Handlers ====================

    /// Handle pick server image button press
    ///
    /// Opens a file picker dialog to select an image file.
    pub fn handle_pick_server_image_pressed(&mut self) -> Task<Message> {
        // Clear any previous error when starting a new pick
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.error = None;
        }

        Task::perform(
            async {
                let handle = AsyncFileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "webp", "svg"])
                    .pick_file()
                    .await;

                match handle {
                    Some(file) => {
                        let path = file.path();
                        let extension = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        // Determine MIME type from extension
                        let mime_type = match extension.as_str() {
                            "png" => "image/png",
                            "jpg" | "jpeg" => "image/jpeg",
                            "webp" => "image/webp",
                            "svg" => "image/svg+xml",
                            _ => return Err(ImagePickerError::UnsupportedType),
                        };

                        // Read file contents
                        let bytes = file.read().await;

                        // Check file size
                        if bytes.len() > SERVER_IMAGE_MAX_SIZE {
                            return Err(ImagePickerError::TooLarge);
                        }

                        // Validate file content matches expected format
                        if !crate::image::validate_image_bytes(&bytes, mime_type) {
                            return Err(ImagePickerError::UnsupportedType);
                        }

                        // Encode as data URI
                        use base64::Engine;
                        let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        let data_uri = format!("data:{};base64,{}", mime_type, base64_data);

                        Ok(data_uri)
                    }
                    None => Err(ImagePickerError::Cancelled),
                }
            },
            Message::EditServerInfoImageLoaded,
        )
    }

    /// Handle server image loaded from file picker
    pub fn handle_edit_server_info_image_loaded(
        &mut self,
        result: Result<String, ImagePickerError>,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };
        let Some(edit_state) = &mut conn.server_info_edit else {
            return Task::none();
        };

        match result {
            Ok(data_uri) => {
                let cached = decode_data_uri_max_width(&data_uri, SERVER_IMAGE_MAX_CACHE_WIDTH);
                if cached.is_some() {
                    edit_state.image = data_uri;
                    edit_state.cached_image = cached;
                    edit_state.error = None;
                } else {
                    edit_state.error = Some(t("err-server-image-decode-failed"));
                }
            }
            Err(ImagePickerError::Cancelled) => {
                // User cancelled, do nothing
            }
            Err(ImagePickerError::TooLarge) => {
                edit_state.error = Some(t("err-server-image-too-large"));
            }
            Err(ImagePickerError::UnsupportedType) => {
                edit_state.error = Some(t("err-server-image-unsupported-type"));
            }
        }

        Task::none()
    }

    /// Handle clear server image button press
    pub fn handle_clear_server_image_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };
        let Some(edit_state) = &mut conn.server_info_edit else {
            return Task::none();
        };

        edit_state.image = String::new();
        edit_state.cached_image = None;
        edit_state.error = None;

        Task::none()
    }

    // ==================== Tab Navigation ====================

    /// Handle Tab pressed in server info edit form.
    pub fn handle_server_info_edit_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::ServerInfoEditTabResolved)
    }

    pub fn handle_server_info_edit_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        // Tab cycle: Name → Description → PublicAddress → Auto-join →
        // Persistent → Name. NumberInput fields (connections, transfers,
        // burst, rate, reindex, scheduler chunk size, max outbound rate)
        // are skipped because they consume Tab internally.
        const CYCLE: &[InputId] = &[
            InputId::EditServerInfoName,
            InputId::EditServerInfoDescription,
            InputId::EditServerInfoPublicAddress,
            InputId::EditServerInfoAutoJoinChannels,
            InputId::EditServerInfoPersistentChannels,
        ];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focus_field(next)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use nexus_common::framing::MessageId;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams};

    fn test_connection_with_receiver(
        connection_id: usize,
    ) -> (
        ServerConnection,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "admin".to_string(),
            connection_info: ConnectionInfo {
                server_name: "Original".to_string(),
                address: "example.com".to_string(),
                port: 7500,
                transfer_port: 7501,
                certificate_fingerprint: String::new(),
                username: "admin".to_string(),
                password: String::new(),
                nickname: "admin".to_string(),
            },
            display_name: "Original".to_string(),
            connection_id,
            is_admin: true,
            permissions: Vec::new(),
            features: Vec::new(),
            server_name: Some("Original".to_string()),
            server_description: Some("v1".to_string()),
            public_address: Some("bbs.example.com".to_string()),
            server_version: Some("0.9.1".to_string()),
            server_image: String::new(),
            cached_server_image: None,
            chat_burst_limit: Some(10),
            chat_rate_limit: Some(20),
            max_connections_per_ip: Some(30),
            max_outbound_rate: Some(40),
            max_transfers_per_ip: Some(50),
            file_reindex_interval: Some(60),
            persistent_channels: Some("#general".to_string()),
            auto_join_channels: Some("#general".to_string()),
            min_password_strength: PasswordStrength::Good,
            log_level: Some("info".to_string()),
            scheduler_chunk_size: Some(4096),
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        });
        (conn, rx)
    }

    #[test]
    fn update_server_info_diffs_against_edit_baseline_not_live_state() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);

        conn.server_info_edit = Some(ServerInfoEditState::new(ServerInfoParams {
            auto_join_channels: conn.auto_join_channels.as_deref(),
            chat_burst_limit: conn.chat_burst_limit,
            chat_rate_limit: conn.chat_rate_limit,
            description: conn.server_description.as_deref(),
            file_reindex_interval: conn.file_reindex_interval,
            image: &conn.server_image,
            max_connections_per_ip: conn.max_connections_per_ip,
            max_outbound_rate: conn.max_outbound_rate,
            max_transfers_per_ip: conn.max_transfers_per_ip,
            min_password_strength: conn.min_password_strength,
            name: conn.server_name.as_deref(),
            persistent_channels: conn.persistent_channels.as_deref(),
            public_address: conn.public_address.as_deref(),
            scheduler_chunk_size: conn.scheduler_chunk_size,
        }));

        conn.server_description = Some("v2".to_string());
        conn.server_info_edit.as_mut().unwrap().name = "Renamed".to_string();
        app.connections.insert(1, conn);

        let _ = app.handle_update_server_info_pressed();

        match rx.try_recv() {
            Ok((
                _,
                ClientMessage::ServerInfoUpdate {
                    name,
                    description,
                    public_address,
                    ..
                },
            )) => {
                assert_eq!(name.as_deref(), Some("Renamed"));
                assert!(description.is_none());
                assert!(public_address.is_none());
            }
            other => panic!("expected ServerInfoUpdate, got {other:?}"),
        }
    }
}
