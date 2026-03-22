//! Connection result handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::{ChannelJoinInfo, ClientMessage};
use uuid::Uuid;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::history::HistoryManager;
use crate::i18n::{t, t_args};
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::ChatMessage;
use crate::types::{
    ActivePanel, ChannelState, InputId, Message, NetworkConnection, ServerBookmark,
    ServerConnection, ServerConnectionParams,
};
use crate::views::constants::PERMISSION_USER_LIST;

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
#[derive(Clone, Copy)]
pub enum ConnectionSource {
    /// Manual connection from the connection form
    Manual,
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
        result: Result<NetworkConnection, String>,
    ) -> Task<Message> {
        self.connection_form.is_connecting = false;

        match result {
            Ok(conn) => {
                self.connection_form.error = None;

                // Find if this connection matches a bookmark (username/nickname case-insensitive)
                let bookmark_id = self
                    .config
                    .bookmarks
                    .iter()
                    .find(|b| {
                        b.address == self.connection_form.server_address
                            && b.port == self.connection_form.port
                            && b.username.to_lowercase()
                                == self.connection_form.username.to_lowercase()
                            && b.nickname.to_lowercase()
                                == self.connection_form.nickname.to_lowercase()
                    })
                    .map(|b| b.id);

                let display_name = self.get_display_name(bookmark_id);

                let ctx = ConnectionContext {
                    bookmark_id,
                    display_name,
                    certificate_fingerprint: conn.connection_info.certificate_fingerprint.clone(),
                    connection_id: conn.connection_id,
                };

                self.handle_successful_connection(conn, ctx, ConnectionSource::Manual)
            }
            Err(error) => {
                self.connection_form.error = Some(error);
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
        result: Result<NetworkConnection, String>,
        bookmark_id: Option<Uuid>,
        display_name: String,
    ) -> Task<Message> {
        match result {
            Ok(conn) => {
                // Clear the connecting lock and any previous error for this bookmark
                if let Some(id) = bookmark_id {
                    self.connecting_bookmarks.remove(&id);
                    self.bookmark_errors.remove(&id);
                }

                let ctx = ConnectionContext {
                    bookmark_id,
                    display_name,
                    certificate_fingerprint: conn.connection_info.certificate_fingerprint.clone(),
                    connection_id: conn.connection_id,
                };

                self.handle_successful_connection(conn, ctx, ConnectionSource::Bookmark)
            }
            Err(error) => {
                if let Some(id) = bookmark_id {
                    self.connecting_bookmarks.remove(&id);
                    self.bookmark_errors.insert(id, error);
                }
                Task::none()
            }
        }
    }

    /// Handle network error or connection closure
    pub fn handle_network_error(&mut self, connection_id: usize, error: String) -> Task<Message> {
        // Clean up voice session first (before removing connection)
        self.cleanup_voice_session(connection_id);

        // Get server name and pending kick message before removing connection
        let (server_name, pending_kick) = self
            .connections
            .get(&connection_id)
            .map(|c| {
                (
                    c.connection_info.server_name.clone(),
                    c.pending_kick_message.clone(),
                )
            })
            .unwrap_or((String::new(), None));

        // Emit UserKicked if we received a kick error, otherwise ConnectionLost
        if let Some(kick_message) = pending_kick {
            emit_event(
                self,
                EventType::UserKicked,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_server_name(&server_name)
                    .with_message(&kick_message),
            );
        } else {
            let display_name = if server_name.is_empty() {
                t("unknown-server")
            } else {
                server_name.clone()
            };
            emit_event(
                self,
                EventType::ConnectionLost,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_server_name(&display_name)
                    .with_message(&error),
            );
        }

        if let Some(conn) = self.connections.remove(&connection_id) {
            // Clean up receiver and signal shutdown in a single spawn
            let registry = crate::network::NETWORK_RECEIVERS.clone();
            let shutdown_arc = conn.shutdown_handle.clone();
            tokio::spawn(async move {
                // Clean up the receiver from the global registry
                let mut receivers = registry.lock().await;
                receivers.remove(&connection_id);

                // Signal the network task to shutdown
                let mut guard = shutdown_arc.lock().await;
                if let Some(shutdown) = guard.take() {
                    shutdown.shutdown();
                }
            });

            // Clean up text editor content for this connection
            self.news_body_content.remove(&connection_id);

            // Clean up history key mapping (but keep the manager - it may be shared)
            self.connection_history_keys.remove(&connection_id);

            // If this was the active connection, clear it
            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
                self.connection_form.error = Some(t_args("msg-disconnected", &[("error", &error)]));
            }

            // Update tray icon state (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            self.update_tray_state();
        }
        Task::none()
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Common handler for successful connections from any source
    pub fn handle_successful_connection(
        &mut self,
        conn: NetworkConnection,
        ctx: ConnectionContext,
        source: ConnectionSource,
    ) -> Task<Message> {
        // Verify and save certificate fingerprint
        if let Err(mismatch_details) =
            self.verify_and_save_fingerprint(ctx.bookmark_id, &ctx.certificate_fingerprint)
        {
            // Clear bookmark connecting lock on fingerprint mismatch
            if let Some(id) = ctx.bookmark_id {
                self.connecting_bookmarks.remove(&id);
            }
            return self.handle_fingerprint_mismatch(*mismatch_details, conn, ctx.display_name);
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
            self.connections.remove(&ctx.connection_id);
            self.active_connection = None;
            self.report_connection_error(source, ctx.bookmark_id, error_msg);
            return Task::none();
        }

        // Initialize channel state from auto-joined channels
        if let Some(conn) = self.connections.get_mut(&ctx.connection_id) {
            for channel_info in &reg.channels {
                let channel_lower = channel_info.channel.to_lowercase();

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

                // Populate voiced nicknames if provided (requires voice_listen permission)
                if let Some(ref voiced) = channel_info.voiced {
                    let voiced_set = voiced.iter().map(|n| n.to_lowercase()).collect();
                    conn.channel_voiced
                        .insert(channel_lower.clone(), voiced_set);
                }

                // Add to known_channels for tab completion (sorted, deduplicated)
                if !conn
                    .known_channels
                    .iter()
                    .any(|c| c.to_lowercase() == channel_lower)
                {
                    let pos = conn
                        .known_channels
                        .iter()
                        .position(|c| c.to_lowercase() > channel_lower)
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
            let is_shared = nickname.to_lowercase() != username.to_lowercase();
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

        // Save as bookmark if checkbox was enabled (form connections only, not already a bookmark)
        if matches!(source, ConnectionSource::Manual)
            && self.connection_form.add_bookmark
            && ctx.bookmark_id.is_none()
        {
            self.save_new_bookmark(ctx.connection_id, ctx.certificate_fingerprint);
        }

        // Clear connection form for form connections
        if matches!(source, ConnectionSource::Manual) {
            self.connection_form.clear();
        }

        // Update tray icon state (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        self.update_tray_state();

        operation::focus(Id::from(InputId::ChatInput))
    }

    /// Report a connection error to the appropriate place based on source
    fn report_connection_error(
        &mut self,
        source: ConnectionSource,
        bookmark_id: Option<Uuid>,
        error: String,
    ) {
        match source {
            ConnectionSource::Manual => {
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
        // Use server-confirmed nickname (equals username for regular accounts)
        let nickname = conn.nickname.clone();
        let connection_id = conn.connection_id;

        // Clone server_image once for both uses
        let server_image = conn.server_image.clone();
        let cached_server_image = if server_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(&server_image, SERVER_IMAGE_MAX_CACHE_WIDTH)
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
            locale: conn.locale,
            server_name: conn.server_name,
            server_description: conn.server_description,
            server_version: conn.server_version,
            server_image,
            cached_server_image,

            max_connections_per_ip: conn.max_connections_per_ip,
            max_transfers_per_ip: conn.max_transfers_per_ip,
            file_reindex_interval: conn.file_reindex_interval,
            persistent_channels: conn.persistent_channels,
            auto_join_channels: conn.auto_join_channels,
            tx: conn.tx,
            shutdown_handle,
        });

        self.connections.insert(connection_id, server_conn);
        self.active_connection = Some(connection_id);

        // Get or create shared history manager for this server+account combination
        let base_dir = HistoryManager::build_base_dir(&fingerprint, &username);
        let is_new_manager = !self.history_managers.contains_key(&base_dir);

        if is_new_manager {
            let manager = HistoryManager::new(
                &fingerprint,
                &username,
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
                // Use the other user's nickname as the tab name
                let tab_name = other_nickname.clone();

                // Create tab if not exists
                if !server_conn.user_message_tabs.contains(&tab_name) {
                    server_conn.user_message_tabs.push(tab_name.clone());
                }

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
                                chrono::TimeZone::timestamp_opt(
                                    &chrono::Local,
                                    *timestamp as i64,
                                    0,
                                )
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

                server_conn.user_messages.insert(tab_name, chat_messages);
            }
        }

        // Always start on chat screen - close any app-wide panels (Settings/About)
        self.ui_state.active_panel = ActivePanel::None;

        Some(ConnectionRegistration {
            channels: conn.channels,
            should_request_userlist,
        })
    }

    /// Get display name from connection form or bookmark
    fn get_display_name(&self, bookmark_id: Option<Uuid>) -> String {
        if !self.connection_form.server_name.trim().is_empty() {
            self.connection_form.server_name.clone()
        } else if let Some(name) = bookmark_id
            .and_then(|id| self.config.get_bookmark(id))
            .map(|b| b.name.clone())
        {
            name
        } else {
            format!(
                "{}:{}",
                self.connection_form.server_address, self.connection_form.port
            )
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

    /// Save a new bookmark from the current connection form
    fn save_new_bookmark(&mut self, connection_id: usize, certificate_fingerprint: String) {
        let new_bookmark = ServerBookmark {
            id: Uuid::new_v4(),
            name: self.connection_form.server_name.clone(),
            address: self.connection_form.server_address.clone(),
            port: self.connection_form.port,
            username: self.connection_form.username.clone(),
            password: self.connection_form.password.clone(),
            nickname: self.connection_form.nickname.clone(),
            auto_connect: false,
            certificate_fingerprint: Some(certificate_fingerprint),
        };
        let bookmark_id = new_bookmark.id;
        self.config.add_bookmark(new_bookmark);

        let _ = self.config.save();

        // Update the connection's bookmark_id to point to the new bookmark
        if let Some(server_conn) = self.connections.get_mut(&connection_id) {
            server_conn.bookmark_id = Some(bookmark_id);
        }
    }
}
