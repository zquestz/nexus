//! Connection result handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::ClientMessage;
use uuid::Uuid;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::{t, t_args};
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::{
    ActivePanel, InputId, Message, NetworkConnection, ServerBookmark, ServerConnection,
    ServerConnectionParams,
};
use crate::views::constants::PERMISSION_USER_LIST;

/// Result of creating and registering a connection
struct ConnectionRegistration {
    chat_topic: Option<String>,
    chat_topic_set_by: Option<String>,
    should_request_userlist: bool,
}

/// Context for connection success handling
struct ConnectionContext {
    bookmark_id: Option<Uuid>,
    display_name: String,
    certificate_fingerprint: String,
    connection_id: usize,
}

/// Source of the connection attempt
#[derive(Clone, Copy)]
enum ConnectionSource {
    /// Manual connection from the connection form
    Form,
    /// Connection from clicking a bookmark
    Bookmark,
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

                self.handle_successful_connection(conn, ctx, ConnectionSource::Form)
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

            // If this was the active connection, clear it
            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
                self.connection_form.error = Some(t_args("msg-disconnected", &[("error", &error)]));
            }
        }
        Task::none()
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Common handler for successful connections from any source
    fn handle_successful_connection(
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

        // Add chat topic message if present
        self.add_topic_message(ctx.connection_id, reg.chat_topic, reg.chat_topic_set_by);

        // Save as bookmark if checkbox was enabled (form connections only, not already a bookmark)
        if matches!(source, ConnectionSource::Form)
            && self.connection_form.add_bookmark
            && ctx.bookmark_id.is_none()
        {
            self.save_new_bookmark(ctx.connection_id, ctx.certificate_fingerprint);
        }

        // Clear connection form for form connections
        if matches!(source, ConnectionSource::Form) {
            self.connection_form.clear();
        }

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
            ConnectionSource::Form => {
                self.connection_form.error = Some(error);
            }
            ConnectionSource::Bookmark => {
                if let Some(id) = bookmark_id {
                    self.bookmark_errors.insert(id, error);
                }
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

        // Clone server_image once for both uses
        let server_image = conn.server_image.clone();
        let cached_server_image = if server_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(&server_image, SERVER_IMAGE_MAX_CACHE_WIDTH)
        };

        let server_conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id,
            session_id: conn.session_id,
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
            chat_topic: conn.chat_topic.clone(),
            chat_topic_set_by: conn.chat_topic_set_by.clone(),
            max_connections_per_ip: conn.max_connections_per_ip,
            max_transfers_per_ip: conn.max_transfers_per_ip,
            tx: conn.tx,
            shutdown_handle,
        });

        self.connections.insert(conn.connection_id, server_conn);
        self.active_connection = Some(conn.connection_id);

        // Always start on chat screen - close any app-wide panels (Settings/About)
        self.ui_state.active_panel = ActivePanel::None;

        Some(ConnectionRegistration {
            chat_topic: conn.chat_topic,
            chat_topic_set_by: conn.chat_topic_set_by,
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
