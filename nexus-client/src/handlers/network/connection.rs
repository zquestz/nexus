//! Connection result handlers

use iced::Task;
use nexus_common::names::fold_name;
use nexus_common::protocol::{ChannelJoinInfo, ClientMessage};
use nexus_common::validators::ImageDecodeProfile;
use uuid::Uuid;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::history::HistoryManager;
use crate::i18n::{t, t_args};
use crate::image::decode_data_uri_max_width;
use crate::network::types::{ConnectError, ConnectionParams};
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::ChatMessage;
use crate::types::{
    ActivePanel, ChannelState, FingerprintMismatch, InputId, ManualConnectionIntent, Message,
    NetworkConnection, ReconnectAction, ServerBookmark, ServerConnection, ServerConnectionParams,
    normalize_certificate_fingerprint,
};
use crate::views::constants::PERMISSION_USER_LIST;

// Protocol command names (must match server exactly)
const CMD_BAN_CREATE: &str = "BanCreate";
const CMD_USER_KICK: &str = "UserKick";

/// Result of creating and registering a connection
struct ConnectionRegistration {
    /// Channels the user was auto-joined to on login
    channels: Vec<ChannelJoinInfo>,
    should_request_userlist: bool,
}

/// Context for connection success handling
pub struct ConnectionContext {
    pub bookmark_id: Option<Uuid>,
    pub display_name: String,
    pub certificate_fingerprint: String,
    pub connection_id: usize,
}

/// Source of the connection attempt
#[derive(Clone)]
pub enum ConnectionSource {
    /// Manual connection from the connection form
    Manual { intent: ManualConnectionIntent },
    /// Connection from clicking a bookmark
    Bookmark,
    /// Connection from a nexus:// URI (startup arg, IPC, or clicked link)
    Uri,
}

impl NexusApp {
    // =========================================================================
    // Public Handlers
    // =========================================================================

    /// Handle connection attempt result (success or failure)
    pub fn handle_connection_result(
        &mut self,
        result: Result<NetworkConnection, ConnectError>,
        params: ConnectionParams,
        intent: ManualConnectionIntent,
    ) -> Task<Message> {
        self.connection_form.is_connecting = false;

        match result {
            Ok(conn) => {
                self.connection_form.error = None;

                // Find if this connection matches a bookmark.
                let bookmark_id = self
                    .config
                    .find_bookmark_matching(
                        &params.server_address,
                        params.port,
                        &params.username,
                        &intent.form_nickname,
                    )
                    .map(|b| b.id);

                let display_name = self.get_display_name(
                    bookmark_id,
                    &intent.server_name,
                    &params.server_address,
                    params.port,
                );

                let ctx = ConnectionContext {
                    bookmark_id,
                    display_name,
                    certificate_fingerprint: conn.connection_info.certificate_fingerprint.clone(),
                    connection_id: conn.connection_id,
                };

                self.handle_successful_connection(conn, ctx, ConnectionSource::Manual { intent })
            }
            Err(ConnectError::FingerprintMismatch(details)) => {
                self.queue_fingerprint_mismatch(
                    *details,
                    ReconnectAction::Manual { params, intent },
                );
                Task::none()
            }
            Err(ConnectError::FingerprintInterception(mut details)) => {
                // Use the user-typed name from the form if any; empty falls
                // through to host:port in the dialog.
                details.server_name = intent.server_name;
                self.fingerprint_interception_queue.push_back(*details);
                Task::none()
            }
            Err(other) => {
                self.connection_form.error = Some(other.to_localized_string());
                Task::none()
            }
        }
    }

    /// Handle bookmark connection attempt result (success or failure)
    ///
    /// This variant is used when connecting from bookmarks to avoid race conditions
    /// with the shared connection_form state.
    pub fn handle_bookmark_connection_result(
        &mut self,
        result: Result<NetworkConnection, ConnectError>,
        params: ConnectionParams,
        bookmark_id: Uuid,
        display_name: String,
    ) -> Task<Message> {
        match result {
            Ok(conn) => {
                // Clear the connecting lock and any previous error for this bookmark
                self.connecting_bookmarks.remove(&bookmark_id);
                self.bookmark_errors.remove(&bookmark_id);

                let ctx = ConnectionContext {
                    bookmark_id: Some(bookmark_id),
                    display_name,
                    certificate_fingerprint: conn.connection_info.certificate_fingerprint.clone(),
                    connection_id: conn.connection_id,
                };

                self.handle_successful_connection(conn, ctx, ConnectionSource::Bookmark)
            }
            Err(ConnectError::FingerprintMismatch(details)) => {
                self.connecting_bookmarks.remove(&bookmark_id);
                let action = ReconnectAction::Bookmark {
                    params,
                    bookmark_id,
                    display_name,
                };
                self.queue_fingerprint_mismatch(*details, action);
                Task::none()
            }
            Err(ConnectError::FingerprintInterception(mut details)) => {
                self.connecting_bookmarks.remove(&bookmark_id);
                // Bookmark connects always carry a non-empty display name.
                details.server_name = display_name;
                self.fingerprint_interception_queue.push_back(*details);
                Task::none()
            }
            Err(other) => {
                self.connecting_bookmarks.remove(&bookmark_id);
                self.bookmark_errors
                    .insert(bookmark_id, other.to_localized_string());
                Task::none()
            }
        }
    }

    /// Handle network error or connection closure
    pub fn handle_network_error(&mut self, connection_id: usize, error: String) -> Task<Message> {
        let Some(conn) = self.connections.get(&connection_id) else {
            return Task::none();
        };

        // Get server name and pending terminal error before removing connection
        let server_name = conn.connection_info.server_name.clone();
        let pending_disconnect = conn.pending_disconnect_error.clone();

        let display_name = if server_name.is_empty() {
            t("unknown-server")
        } else {
            server_name.clone()
        };
        let (event_type, disconnect_message) = match pending_disconnect {
            Some(pending) => {
                let event_type = match pending.command.as_deref() {
                    Some(CMD_USER_KICK) => EventType::UserKicked,
                    Some(CMD_BAN_CREATE) => EventType::UserBanned,
                    _ => EventType::ConnectionLost,
                };
                (event_type, pending.message)
            }
            None => (EventType::ConnectionLost, error),
        };

        emit_event(
            self,
            event_type,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_server_name(&display_name)
                .with_message(&disconnect_message),
        );

        if self.cleanup_connection_resources(connection_id) {
            // Same disconnected-default-state focus rule as
            // `handle_disconnect_from_server`: if the layout now shows
            // the connection form as its fallback, auto-focus the
            // first field.
            if self.active_connection.is_none() && self.active_panel() == ActivePanel::None {
                return self.focus_field(InputId::ServerName);
            }
        }
        Task::none()
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Common handler for successful connections from any source.
    ///
    /// By the time this runs, both fingerprint stages have already passed
    /// (stage 1 in `connect_to_server` against the bookmark's stored value,
    /// stage 2 against the server's self-report from `HandshakeResponse`).
    /// The only fingerprint work left here is the TOFU save: if the bookmark
    /// has no stored fingerprint yet, commit the TLS-observed one now that
    /// stage 2 has confirmed the server agrees with itself.
    pub fn handle_successful_connection(
        &mut self,
        conn: NetworkConnection,
        ctx: ConnectionContext,
        source: ConnectionSource,
    ) -> Task<Message> {
        // TOFU save: existing bookmark with no stored fingerprint commits now.
        // (Brand-new bookmarks created via "save as bookmark" are handled below
        // in `save_new_bookmark`, which already records the fingerprint.)
        let mut background_tasks = Vec::new();
        let mut tofu_save_error = None;
        if let Some(id) = ctx.bookmark_id
            && let Some(bookmark) = self.config.get_bookmark_mut(id)
            && bookmark.certificate_fingerprint.is_none()
        {
            bookmark.certificate_fingerprint =
                normalize_certificate_fingerprint(Some(ctx.certificate_fingerprint.clone()));
            if let Err(e) = self.config.save() {
                tofu_save_error = Some(t_args("err-failed-save-config", &[("error", &e)]));
            }
        }

        // Create and register connection
        let Some(reg) =
            self.create_and_register_connection(conn, ctx.bookmark_id, ctx.display_name)
        else {
            self.report_connection_error(source, ctx.bookmark_id, t("err-no-shutdown-handle"));
            return Task::none();
        };

        // Request user list if we have permission
        if let Err(error_msg) =
            self.request_initial_userlist(ctx.connection_id, reg.should_request_userlist)
        {
            let cleaned = self.cleanup_connection_resources(ctx.connection_id);
            self.report_connection_error(source, ctx.bookmark_id, error_msg);
            if cleaned
                && self.active_connection.is_none()
                && self.active_panel() == ActivePanel::None
            {
                return self.focus_field(InputId::ServerName);
            }
            return Task::none();
        }

        // Initialize channel state from auto-joined channels
        if let Some(conn) = self.connections.get_mut(&ctx.connection_id) {
            for channel_info in &reg.channels {
                let channel_lower = fold_name(&channel_info.channel);

                // Create channel state
                let channel_state = ChannelState::new(
                    channel_info.topic.clone(),
                    channel_info.topic_set_by.clone(),
                    channel_info.secret,
                    channel_info.members.clone(),
                );

                // Add to channels map and tabs list
                conn.channels.insert(channel_lower.clone(), channel_state);
                conn.channel_tabs.push(channel_info.channel.clone());

                // Populate voiced nicknames if provided (requires voice feature + voice_listen).
                if let Some(ref voiced) = channel_info.voiced {
                    let voiced_set = voiced.iter().map(|n| fold_name(n)).collect();
                    conn.channel_voiced
                        .insert(channel_lower.clone(), voiced_set);
                }

                // Add to known_channels for tab completion (sorted, deduplicated)
                if !conn
                    .known_channels
                    .iter()
                    .any(|c| fold_name(c) == channel_lower)
                {
                    let pos = conn
                        .known_channels
                        .iter()
                        .position(|c| fold_name(c) > channel_lower)
                        .unwrap_or(conn.known_channels.len());
                    conn.known_channels
                        .insert(pos, channel_info.channel.clone());
                }
            }

            // Set active tab to last joined channel, or stay on Console if no channels
            if let Some(last_channel) = conn.channel_tabs.last() {
                conn.active_chat_tab = crate::types::ChatTab::Channel(last_channel.clone());
            }

            // Add welcome message to Console with server info
            let mut welcome_lines = Vec::new();

            // Server name (or address if no name)
            let server_display = conn
                .server_name
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned()
                .unwrap_or_else(|| conn.connection_info.address.clone());
            welcome_lines.push(t_args("msg-connected-to", &[("server", &server_display)]));

            // Server description (if present)
            if let Some(ref desc) = conn.server_description
                && !desc.is_empty()
            {
                welcome_lines.push(desc.clone());
            }

            // Server version (if present)
            if let Some(ref version) = conn.server_version
                && !version.is_empty()
            {
                welcome_lines.push(t_args("msg-server-version", &[("version", version)]));
            }

            // Logged in identity: nickname [admin] or nickname (username) [admin] for shared accounts
            // Compare case-insensitively since username comes from user input (may differ in case)
            // and nickname comes from server (database-stored case)
            let username = &conn.connection_info.username;
            let nickname = &conn.nickname;
            let is_shared = fold_name(nickname) != fold_name(username);
            let login_info = match (is_shared, conn.is_admin) {
                (true, true) => t_args(
                    "msg-logged-in-as-shared-admin",
                    &[("nickname", nickname), ("username", username)],
                ),
                (true, false) => t_args(
                    "msg-logged-in-as-shared",
                    &[("nickname", nickname), ("username", username)],
                ),
                (false, true) => t_args("msg-logged-in-as-admin", &[("nickname", nickname)]),
                (false, false) => t_args("msg-logged-in-as", &[("nickname", nickname)]),
            };
            welcome_lines.push(login_info);

            let welcome_message = welcome_lines.join("\n");
            conn.console_messages
                .push(ChatMessage::system(welcome_message));
        }

        // Add topic messages for each channel
        for channel_info in &reg.channels {
            self.add_topic_message(
                ctx.connection_id,
                &channel_info.channel,
                channel_info.topic.clone(),
                channel_info.topic_set_by.clone(),
            );
        }

        if let Some(error) = tofu_save_error {
            background_tasks.push(self.add_background_error_message(ctx.connection_id, error));
        }

        // Save as bookmark if checkbox was enabled (form connections only, not already a bookmark)
        if let ConnectionSource::Manual { intent } = &source
            && intent.add_bookmark
            && ctx.bookmark_id.is_none()
        {
            background_tasks.push(self.save_new_bookmark(
                ctx.connection_id,
                ctx.certificate_fingerprint.clone(),
                intent,
            ));
        }

        // Connection succeeded — dismiss both layout-level overlays
        // unconditionally. Pre-overlay-refactor this only fired for
        // `ConnectionSource::Manual` (and only cleared the connection
        // form), which left both overlays summoned over the new chat
        // for Bookmark / URI / Reconnect sources. The user's intent
        // on a successful connection is "go to the chat I just
        // connected to," so any overlay is stale.
        self.dismiss_connection_form();
        self.dismiss_bookmark_edit();

        // Update tray icon state (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        self.update_tray_state();

        background_tasks.push(self.focus_field(InputId::ChatInput));
        Task::batch(background_tasks)
    }

    /// Report a connection error to the appropriate place based on source
    fn report_connection_error(
        &mut self,
        source: ConnectionSource,
        bookmark_id: Option<Uuid>,
        error: String,
    ) {
        match source {
            ConnectionSource::Manual { .. } => {
                self.connection_form.error = Some(error);
            }
            ConnectionSource::Bookmark => {
                if let Some(id) = bookmark_id {
                    self.bookmark_errors.insert(id, error);
                }
            }
            ConnectionSource::Uri => {
                // URI connection errors are handled in handle_uri_connection_result
                // This branch shouldn't be reached, but if it is, show in form
                self.connection_form.error = Some(error);
            }
        }
    }

    /// Create a ServerConnection from NetworkConnection and register it
    ///
    /// Returns `Some(ConnectionRegistration)` on success, or `None` if the
    /// connection has no shutdown handle.
    fn create_and_register_connection(
        &mut self,
        conn: NetworkConnection,
        bookmark_id: Option<Uuid>,
        display_name: String,
    ) -> Option<ConnectionRegistration> {
        let should_request_userlist = conn.has_permission(PERMISSION_USER_LIST);
        let shutdown_handle = conn.shutdown?;

        // Extract values needed for history manager before moving connection_info
        let fingerprint = conn.connection_info.certificate_fingerprint.clone();
        let username = conn.connection_info.username.clone();
        // Immutable account id keys the history dir, so a rename never moves it.
        let user_id = conn.user_id;
        // Use server-confirmed nickname (equals username for regular accounts)
        let nickname = conn.nickname.clone();
        let connection_id = conn.connection_id;

        // Clone server_image once for both uses
        let server_image = conn.server_image.clone();
        let cached_server_image = if server_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(
                &server_image,
                SERVER_IMAGE_MAX_CACHE_WIDTH,
                ImageDecodeProfile::Server,
            )
        };

        let server_conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id,
            user_id: conn.user_id,
            nickname: nickname.clone(),
            connection_info: conn.connection_info,
            display_name,
            connection_id: conn.connection_id,
            is_admin: conn.is_admin,
            permissions: conn.permissions,
            features: conn.features,
            server_name: conn.server_name,
            server_description: conn.server_description,
            public_address: conn.public_address,
            server_version: conn.server_version,
            server_image,
            cached_server_image,
            chat_burst_limit: conn.chat_burst_limit,
            chat_rate_limit: conn.chat_rate_limit,
            max_connections_per_ip: conn.max_connections_per_ip,
            max_outbound_rate: conn.max_outbound_rate,
            max_transfers_per_ip: conn.max_transfers_per_ip,
            file_reindex_interval: conn.file_reindex_interval,
            persistent_channels: conn.persistent_channels,
            auto_join_channels: conn.auto_join_channels,
            min_password_strength: conn.min_password_strength,
            log_level: conn.log_level,
            scheduler_chunk_size: conn.scheduler_chunk_size,
            tx: conn.tx,
            shutdown_handle,
        });

        self.connections.insert(connection_id, server_conn);
        self.active_connection = Some(connection_id);

        // History is keyed by the immutable account id (survives renames); skip if
        // somehow unauthenticated (shouldn't happen post-login).
        if let Some(user_id) = user_id {
            // One-time move of any pre-id (username-keyed) history into the id-keyed dir.
            HistoryManager::migrate_legacy_user_dir(&fingerprint, &username, user_id);

            // Get or create shared history manager for this server+account combination
            let base_dir = HistoryManager::build_base_dir(&fingerprint, user_id);
            let is_new_manager = !self.history_managers.contains_key(&base_dir);

            if is_new_manager {
                let manager = HistoryManager::new(
                    &fingerprint,
                    user_id,
                    self.config.settings.chat_history_retention,
                );
                self.history_managers.insert(base_dir.clone(), manager);
            } else {
                // Update existing manager's retention to current setting
                if let Some(manager) = self.history_managers.get_mut(&base_dir) {
                    manager.update_retention(self.config.settings.chat_history_retention);
                }
            }

            // Record which manager this connection uses
            self.connection_history_keys
                .insert(connection_id, base_dir.clone());

            // Load history and restore tabs (only loads from disk on first access)
            if let Some(history_manager) = self.history_managers.get_mut(&base_dir)
                && let Ok(conversations) = history_manager.load_all()
                && let Some(server_conn) = self.connections.get_mut(&connection_id)
            {
                for (other_nickname, messages) in conversations {
                    // Intentionally restore local DM history even when this
                    // login did not activate `chat`. The history is read-only
                    // until send, and send remains feature-gated.
                    // Convert ServerMessage::UserMessage to ChatMessage for display
                    let chat_messages: Vec<_> = messages
                        .iter()
                        .filter_map(|msg| {
                            if let nexus_common::protocol::ServerMessage::UserMessage {
                                from_nickname,
                                from_admin,
                                from_shared,
                                message,
                                action,
                                timestamp,
                                ..
                            } = msg
                            {
                                let datetime = if *timestamp > 0 {
                                    chrono::TimeZone::timestamp_opt(&chrono::Local, *timestamp, 0)
                                        .single()
                                        .unwrap_or_else(chrono::Local::now)
                                } else {
                                    chrono::Local::now()
                                };
                                Some(crate::types::ChatMessage::with_timestamp_and_status(
                                    from_nickname.clone(),
                                    message.clone(),
                                    datetime,
                                    *from_admin,
                                    *from_shared,
                                    *action,
                                ))
                            } else {
                                None
                            }
                        })
                        .collect();

                    // Resolve to the canonical DM tab (folded identity), then store.
                    let key = server_conn.resolve_user_message_tab(&other_nickname);
                    server_conn.user_messages.insert(key, chat_messages);
                }
            }
        }

        // Always start on chat screen - close any app-wide panels (Settings/About)
        self.ui_state.active_panel = ActivePanel::None;

        Some(ConnectionRegistration {
            channels: conn.channels,
            should_request_userlist,
        })
    }

    /// Get display name from the manual submit snapshot or matched bookmark.
    fn get_display_name(
        &self,
        bookmark_id: Option<Uuid>,
        server_name: &str,
        server_address: &str,
        port: u16,
    ) -> String {
        if !server_name.trim().is_empty() {
            server_name.to_string()
        } else if let Some(name) = bookmark_id
            .and_then(|id| self.config.get_bookmark(id))
            .map(|b| b.name.clone())
        {
            name
        } else {
            format!("{}:{}", server_address, port)
        }
    }

    /// Request initial user list if the user has permission
    fn request_initial_userlist(
        &self,
        connection_id: usize,
        should_request: bool,
    ) -> Result<(), String> {
        if should_request
            && let Some(conn) = self.connections.get(&connection_id)
            && let Err(e) = conn.send(ClientMessage::UserList { all: false })
        {
            return Err(format!("{}: {}", t("err-connection-broken"), e));
        }
        Ok(())
    }

    /// Queue a stage-1 fingerprint mismatch for user verification.
    ///
    /// Decorates the wire-format `FingerprintMismatchDetails` with the
    /// matched bookmark identity (for dialog display) and a `ReconnectAction`
    /// (for the accept-and-retry path).
    pub(crate) fn queue_fingerprint_mismatch(
        &mut self,
        details: crate::network::FingerprintMismatchDetails,
        retry_action: ReconnectAction,
    ) {
        // Look up the bookmark from the params/action so we can populate the
        // dialog's bookmark identity. Stage-1 mismatch can only happen when
        // there's a stored fingerprint, which can only come from a bookmark,
        // so a match is expected — but if the bookmark was deleted between
        // initiating the connect and the mismatch firing, fall back to nil.
        let (bookmark_id, bookmark_name) = match &retry_action {
            ReconnectAction::Manual { params, intent } => self
                .config
                .find_bookmark_matching(
                    &params.server_address,
                    params.port,
                    &params.username,
                    &intent.form_nickname,
                )
                .map(|b| (b.id, b.name.clone()))
                .unwrap_or((Uuid::nil(), String::new())),
            ReconnectAction::Bookmark {
                bookmark_id,
                display_name: name,
                ..
            } => (*bookmark_id, name.clone()),
            ReconnectAction::Uri {
                params,
                display_name: name,
                ..
            } => self
                .config
                .find_bookmark_matching_uri(
                    &params.server_address,
                    params.port,
                    (!params.username.is_empty()).then_some(params.username.as_str()),
                )
                .map(|b| (b.id, b.name.clone()))
                .unwrap_or((Uuid::nil(), name.clone())),
        };

        self.fingerprint_mismatch_queue
            .push_back(FingerprintMismatch {
                bookmark_id,
                expected: details.expected,
                received: details.received,
                bookmark_name,
                server_address: details.server_address,
                server_port: details.server_port,
                retry_action,
            });

        self.connection_form.is_connecting = false;
    }

    /// Save a new bookmark from the current connection form
    fn save_new_bookmark(
        &mut self,
        connection_id: usize,
        certificate_fingerprint: String,
        intent: &ManualConnectionIntent,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get(&connection_id) else {
            return Task::none();
        };
        let bookmark_nickname =
            if fold_name(&conn.nickname) == fold_name(&conn.connection_info.username) {
                String::new()
            } else {
                conn.nickname.clone()
            };

        let new_bookmark = ServerBookmark {
            id: Uuid::new_v4(),
            name: intent.server_name.clone(),
            address: conn.connection_info.address.clone(),
            port: conn.connection_info.port,
            username: conn.connection_info.username.clone(),
            password: conn.connection_info.password.clone(),
            nickname: bookmark_nickname,
            auto_connect: false,
            certificate_fingerprint: normalize_certificate_fingerprint(Some(
                certificate_fingerprint,
            )),
        };
        let bookmark_id = new_bookmark.id;
        self.config.add_bookmark(new_bookmark);

        let task = match self.config.save() {
            Ok(()) => Task::none(),
            Err(e) => self.add_background_error_message(
                connection_id,
                t_args("err-failed-save-config", &[("error", &e)]),
            ),
        };

        // Update the connection's bookmark_id to point to the new bookmark
        if let Some(server_conn) = self.connections.get_mut(&connection_id) {
            server_conn.bookmark_id = Some(bookmark_id);
        }

        task
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use nexus_common::framing::MessageId;
    use nexus_common::protocol::ServerMessage;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc, oneshot};

    use crate::config::Config;
    use crate::config::events::EventType;
    use crate::network::ShutdownHandle;
    use crate::network::types::ProxyConfig;
    use crate::types::ConnectionInfo;

    const TEST_FINGERPRINT: &str = "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA";

    fn manual_intent(
        server_name: &str,
        form_nickname: &str,
        add_bookmark: bool,
    ) -> ManualConnectionIntent {
        ManualConnectionIntent {
            server_name: server_name.to_string(),
            form_nickname: form_nickname.to_string(),
            add_bookmark,
        }
    }

    fn connection_params(
        connection_id: usize,
        address: &str,
        username: &str,
        nickname: Option<&str>,
    ) -> ConnectionParams {
        ConnectionParams {
            server_address: address.to_string(),
            port: 7500,
            username: username.to_string(),
            password: "secret".to_string(),
            nickname: nickname.map(str::to_string),
            locale: "en".to_string(),
            avatar: None,
            connection_id,
            proxy: Option::<ProxyConfig>::None,
            expected_fingerprint: None,
        }
    }

    fn network_connection(params: &ConnectionParams, server_nickname: &str) -> NetworkConnection {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        NetworkConnection {
            tx,
            connection_id: params.connection_id,
            shutdown: Some(Arc::new(Mutex::new(Some(ShutdownHandle::new_for_test(
                shutdown_tx,
            ))))),
            is_admin: false,
            user_id: Some(1),
            nickname: server_nickname.to_string(),
            permissions: Vec::new(),
            features: Vec::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            channels: Vec::<ChannelJoinInfo>::new(),
            chat_burst_limit: None,
            chat_rate_limit: None,
            max_connections_per_ip: None,
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Weak,
            log_level: None,
            scheduler_chunk_size: None,
            connection_info: ConnectionInfo {
                server_name: params.server_address.clone(),
                address: params.server_address.clone(),
                port: params.port,
                transfer_port: 7501,
                certificate_fingerprint: TEST_FINGERPRINT.to_string(),
                username: params.username.clone(),
                password: params.password.clone(),
                nickname: params.nickname.clone().unwrap_or_default(),
            },
        }
    }

    async fn register_dummy_receiver(connection_id: usize) {
        let (_tx, rx) =
            mpsc::unbounded_channel::<(MessageId, ServerMessage, Option<std::time::Instant>)>();
        crate::network::NETWORK_RECEIVERS
            .lock()
            .await
            .insert(connection_id, rx);
    }

    async fn assert_receiver_removed(connection_id: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if !crate::network::NETWORK_RECEIVERS
                    .lock()
                    .await
                    .contains_key(&connection_id)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("receiver registry entry should be removed");
    }

    fn enable_connection_lost_toast(app: &mut NexusApp) {
        let config = app
            .config
            .settings
            .event_settings
            .get_mut(EventType::ConnectionLost);
        config.show_notification = false;
        config.show_toast = true;
        config.play_sound = false;
    }

    async fn clear_receiver(connection_id: usize) {
        crate::network::NETWORK_RECEIVERS
            .lock()
            .await
            .remove(&connection_id);
    }

    #[test]
    fn manual_success_uses_submit_snapshot_for_bookmark_match_and_display() {
        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        let bookmark_id = Uuid::new_v4();
        app.config.bookmarks.push(ServerBookmark {
            id: bookmark_id,
            name: "Original Bookmark".to_string(),
            address: "bbs.example".to_string(),
            port: 7500,
            username: "alice".to_string(),
            password: "secret".to_string(),
            nickname: String::new(),
            auto_connect: false,
            certificate_fingerprint: None,
        });
        app.connection_form.server_name = "Edited Name".to_string();
        app.connection_form.server_address = "other.example".to_string();
        app.connection_form.port = 7600;
        app.connection_form.username = "mallory".to_string();
        app.connection_form.nickname = "MalloryNick".to_string();

        let params = connection_params(1, "bbs.example", "alice", Some("DefaultNick"));
        let conn = network_connection(&params, "alice");
        let task = app.handle_connection_result(Ok(conn), params, manual_intent("", "", false));
        drop(task);

        let conn = app.connections.get(&1).expect("connection registered");
        assert_eq!(conn.bookmark_id, Some(bookmark_id));
        assert_eq!(conn.display_name, "Original Bookmark");
    }

    #[test]
    fn manual_success_uses_submit_snapshot_for_shared_bookmark_match() {
        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        let bookmark_id = Uuid::new_v4();
        app.config.bookmarks.push(ServerBookmark {
            id: bookmark_id,
            name: "Shared Bookmark".to_string(),
            address: "bbs.example".to_string(),
            port: 7500,
            username: "shared".to_string(),
            password: "secret".to_string(),
            nickname: "Bob".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });
        app.connection_form.server_name = "Edited Name".to_string();
        app.connection_form.server_address = "other.example".to_string();
        app.connection_form.port = 7600;
        app.connection_form.username = "mallory".to_string();
        app.connection_form.nickname = "MalloryNick".to_string();

        let params = connection_params(1, "bbs.example", "shared", Some("WrongEffectiveNick"));
        let conn = network_connection(&params, "Bob");
        let task = app.handle_connection_result(Ok(conn), params, manual_intent("", "Bob", false));
        drop(task);

        let conn = app.connections.get(&1).expect("connection registered");
        assert_eq!(conn.bookmark_id, Some(bookmark_id));
        assert_eq!(conn.display_name, "Shared Bookmark");
    }

    #[test]
    fn manual_fingerprint_retry_preserves_submit_snapshot() {
        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        let bookmark_id = Uuid::new_v4();
        app.config.bookmarks.push(ServerBookmark {
            id: bookmark_id,
            name: "Shared Bookmark".to_string(),
            address: "bbs.example".to_string(),
            port: 7500,
            username: "shared".to_string(),
            password: "secret".to_string(),
            nickname: "Bob".to_string(),
            auto_connect: false,
            certificate_fingerprint: Some("AA:AA".to_string()),
        });

        let params = connection_params(1, "bbs.example", "shared", Some("WrongEffectiveNick"));
        let intent = manual_intent("Original Name", "Bob", false);
        app.queue_fingerprint_mismatch(
            crate::network::FingerprintMismatchDetails {
                expected: "AA:AA".to_string(),
                received: "BB:BB".to_string(),
                server_address: "bbs.example".to_string(),
                server_port: 7500,
            },
            ReconnectAction::Manual { params, intent },
        );

        let mismatch = app
            .fingerprint_mismatch_queue
            .pop_back()
            .expect("fingerprint mismatch queued");
        assert_eq!(mismatch.bookmark_id, bookmark_id);
        assert_eq!(mismatch.bookmark_name, "Shared Bookmark");
        match mismatch.retry_action {
            ReconnectAction::Manual { params, intent } => {
                assert_eq!(params.nickname.as_deref(), Some("WrongEffectiveNick"));
                assert_eq!(intent.server_name, "Original Name");
                assert_eq!(intent.form_nickname, "Bob");
            }
            _ => panic!("expected manual reconnect action"),
        }
    }

    #[test]
    fn manual_bookmark_save_uses_empty_nickname_for_regular_account() {
        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        let params = connection_params(1, "bbs.example", "alice", Some("DefaultNick"));
        let conn = network_connection(&params, "alice");
        let task =
            app.handle_connection_result(Ok(conn), params, manual_intent("Saved Server", "", true));
        drop(task);

        let bookmark = app.config.bookmarks.first().expect("bookmark saved");
        assert_eq!(bookmark.name, "Saved Server");
        assert_eq!(bookmark.address, "bbs.example");
        assert_eq!(bookmark.username, "alice");
        assert_eq!(bookmark.nickname, "");
        assert_eq!(app.connections[&1].bookmark_id, Some(bookmark.id));
    }

    #[test]
    fn manual_bookmark_save_uses_server_confirmed_shared_nickname() {
        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        let params = connection_params(1, "bbs.example", "shared", Some("SharedNick"));
        let conn = network_connection(&params, "ServerNick");
        let task = app.handle_connection_result(
            Ok(conn),
            params,
            manual_intent("Shared Server", "SharedNick", true),
        );
        drop(task);

        let bookmark = app.config.bookmarks.first().expect("bookmark saved");
        assert_eq!(bookmark.username, "shared");
        assert_eq!(bookmark.nickname, "ServerNick");
    }

    #[tokio::test]
    async fn initial_userlist_send_failure_cleans_registered_connection_resources() {
        const CONNECTION_ID: usize = 98_321;
        clear_receiver(CONNECTION_ID).await;
        register_dummy_receiver(CONNECTION_ID).await;

        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        let params = connection_params(CONNECTION_ID, "bbs.example", "alice", None);
        let mut conn = network_connection(&params, "alice");
        conn.permissions.push(PERMISSION_USER_LIST.to_string());
        let ctx = ConnectionContext {
            bookmark_id: None,
            display_name: "Test Server".to_string(),
            certificate_fingerprint: TEST_FINGERPRINT.to_string(),
            connection_id: CONNECTION_ID,
        };

        let task = app.handle_successful_connection(
            conn,
            ctx,
            ConnectionSource::Manual {
                intent: manual_intent("", "", false),
            },
        );
        drop(task);

        assert!(!app.connections.contains_key(&CONNECTION_ID));
        assert_eq!(app.active_connection, None);
        assert!(!app.connection_history_keys.contains_key(&CONNECTION_ID));
        assert!(
            app.connection_form
                .error
                .as_deref()
                .is_some_and(|error| error.contains(&t("err-connection-broken")))
        );
        assert_receiver_removed(CONNECTION_ID).await;
    }

    #[tokio::test]
    async fn late_network_error_after_setup_failure_is_noop() {
        const CONNECTION_ID: usize = 98_322;
        clear_receiver(CONNECTION_ID).await;
        register_dummy_receiver(CONNECTION_ID).await;

        let mut app = NexusApp {
            config: Config::default(),
            ..NexusApp::default()
        };
        enable_connection_lost_toast(&mut app);
        let params = connection_params(CONNECTION_ID, "bbs.example", "alice", None);
        let mut conn = network_connection(&params, "alice");
        conn.permissions.push(PERMISSION_USER_LIST.to_string());
        let ctx = ConnectionContext {
            bookmark_id: None,
            display_name: "Test Server".to_string(),
            certificate_fingerprint: TEST_FINGERPRINT.to_string(),
            connection_id: CONNECTION_ID,
        };

        let task = app.handle_successful_connection(
            conn,
            ctx,
            ConnectionSource::Manual {
                intent: manual_intent("", "", false),
            },
        );
        drop(task);
        assert_receiver_removed(CONNECTION_ID).await;
        let toasts_before = format!("{:?}", app.toasts);

        let task = app.handle_network_error(CONNECTION_ID, "late error".to_string());
        drop(task);

        assert_eq!(format!("{:?}", app.toasts), toasts_before);
        assert!(!app.connections.contains_key(&CONNECTION_ID));
    }
}
