//! Connection result handlers

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;

use crate::types::{
    ActivePanel, InputId, Message, NetworkConnection, ServerBookmark, ServerConnection,
    ServerConnectionParams,
};
use crate::views::constants::PERMISSION_USER_LIST;
use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::ClientMessage;

/// Result of creating and registering a connection
struct ConnectionRegistration {
    chat_topic: Option<String>,
    chat_topic_set_by: Option<String>,
    should_request_userlist: bool,
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
                let bookmark_index = self.config.bookmarks.iter().position(|b| {
                    b.address == self.connection_form.server_address
                        && b.port == self.connection_form.port
                        && b.username.to_lowercase() == self.connection_form.username.to_lowercase()
                        && b.nickname.to_lowercase() == self.connection_form.nickname.to_lowercase()
                });

                // Verify and save certificate fingerprint
                if let Err(mismatch_details) =
                    self.verify_and_save_fingerprint(bookmark_index, &conn.certificate_fingerprint)
                {
                    let display_name = self.get_display_name(bookmark_index);
                    return self.handle_fingerprint_mismatch(*mismatch_details, conn, display_name);
                }

                let connection_id = conn.connection_id;
                let display_name = self.get_display_name(bookmark_index);
                let username = self.connection_form.username.clone();
                let certificate_fingerprint = conn.certificate_fingerprint.clone();

                // Create and register connection
                let Some(reg) = self.create_and_register_connection(
                    conn,
                    bookmark_index,
                    username,
                    display_name,
                ) else {
                    self.connection_form.error = Some(t("err-no-shutdown-handle"));
                    return Task::none();
                };

                // Request user list if we have permission
                if let Err(error_msg) =
                    self.request_initial_userlist(connection_id, reg.should_request_userlist)
                {
                    self.connection_form.error = Some(error_msg);
                    self.connections.remove(&connection_id);
                    self.active_connection = None;
                    return Task::none();
                }

                // Add chat topic message if present
                self.add_topic_message(connection_id, reg.chat_topic, reg.chat_topic_set_by);

                // Save as bookmark if checkbox was enabled (and not already a bookmark)
                if self.connection_form.add_bookmark && bookmark_index.is_none() {
                    self.save_new_bookmark(connection_id, certificate_fingerprint);
                }

                // Clear connection form
                self.connection_form.clear();

                operation::focus(Id::from(InputId::ChatInput))
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
        bookmark_index: Option<usize>,
        display_name: String,
    ) -> Task<Message> {
        match result {
            Ok(conn) => {
                let connection_id = conn.connection_id;

                // Clear the connecting lock and any previous error for this bookmark
                if let Some(idx) = bookmark_index {
                    self.connecting_bookmarks.remove(&idx);
                    self.bookmark_errors.remove(&idx);
                }

                // Verify and save certificate fingerprint
                if let Err(mismatch_details) =
                    self.verify_and_save_fingerprint(bookmark_index, &conn.certificate_fingerprint)
                {
                    return self.handle_fingerprint_mismatch(*mismatch_details, conn, display_name);
                }

                // Extract username from bookmark
                let username = bookmark_index
                    .and_then(|idx| self.config.get_bookmark(idx))
                    .map(|b| b.username.clone())
                    .unwrap_or_default();

                // Create and register connection
                let Some(reg) = self.create_and_register_connection(
                    conn,
                    bookmark_index,
                    username,
                    display_name,
                ) else {
                    if let Some(idx) = bookmark_index {
                        self.bookmark_errors
                            .insert(idx, t("err-no-shutdown-handle"));
                    }
                    return Task::none();
                };

                // Request initial user list
                if let Err(error_msg) =
                    self.request_initial_userlist(connection_id, reg.should_request_userlist)
                {
                    self.connections.remove(&connection_id);
                    self.active_connection = None;
                    if let Some(idx) = bookmark_index {
                        self.bookmark_errors.insert(idx, error_msg);
                    }
                    return Task::none();
                }

                // Add chat topic message if present
                self.add_topic_message(connection_id, reg.chat_topic, reg.chat_topic_set_by);

                operation::focus(Id::from(InputId::ChatInput))
            }
            Err(error) => {
                if let Some(idx) = bookmark_index {
                    self.connecting_bookmarks.remove(&idx);
                    self.bookmark_errors.insert(idx, error);
                }
                Task::none()
            }
        }
    }

    /// Handle network error or connection closure
    pub fn handle_network_error(&mut self, connection_id: usize, error: String) -> Task<Message> {
        if let Some(conn) = self.connections.remove(&connection_id) {
            // Clean up the receiver from the global registry
            let registry = crate::network::NETWORK_RECEIVERS.clone();
            tokio::spawn(async move {
                let mut receivers = registry.lock().await;
                receivers.remove(&connection_id);
            });

            // Signal the network task to shutdown
            let shutdown_arc = conn.shutdown_handle.clone();
            tokio::spawn(async move {
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

    /// Create a ServerConnection from NetworkConnection and register it
    ///
    /// Returns `Some(ConnectionRegistration)` on success, or `None` if the
    /// connection has no shutdown handle.
    fn create_and_register_connection(
        &mut self,
        conn: NetworkConnection,
        bookmark_index: Option<usize>,
        username: String,
        display_name: String,
    ) -> Option<ConnectionRegistration> {
        let shutdown_handle = conn.shutdown?;
        let chat_topic = conn.chat_topic.clone();
        let chat_topic_set_by = conn.chat_topic_set_by.clone();
        let should_request_userlist =
            conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        let cached_server_image = if conn.server_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(&conn.server_image, SERVER_IMAGE_MAX_CACHE_WIDTH)
        };

        let server_conn = ServerConnection::new(ServerConnectionParams {
            bookmark_index,
            session_id: conn.session_id,
            username,
            display_name,
            connection_id: conn.connection_id,
            is_admin: conn.is_admin,
            permissions: conn.permissions,
            locale: conn.locale,
            server_name: conn.server_name,
            server_description: conn.server_description,
            server_version: conn.server_version,
            server_image: conn.server_image.clone(),
            cached_server_image,
            chat_topic: chat_topic.clone(),
            chat_topic_set_by: chat_topic_set_by.clone(),
            max_connections_per_ip: conn.max_connections_per_ip,
            tx: conn.tx,
            shutdown_handle,
        });

        self.connections.insert(conn.connection_id, server_conn);
        self.active_connection = Some(conn.connection_id);

        // Always start on chat screen - close any app-wide panels (Settings/About)
        self.ui_state.active_panel = ActivePanel::None;

        Some(ConnectionRegistration {
            chat_topic,
            chat_topic_set_by,
            should_request_userlist,
        })
    }

    /// Get display name from connection form or bookmark
    fn get_display_name(&self, bookmark_index: Option<usize>) -> String {
        if !self.connection_form.server_name.trim().is_empty() {
            self.connection_form.server_name.clone()
        } else if let Some(name) = bookmark_index
            .and_then(|idx| self.config.bookmarks.get(idx))
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
            name: self.connection_form.server_name.clone(),
            address: self.connection_form.server_address.clone(),
            port: self.connection_form.port,
            username: self.connection_form.username.clone(),
            password: self.connection_form.password.clone(),
            nickname: self.connection_form.nickname.clone(),
            auto_connect: false,
            certificate_fingerprint: Some(certificate_fingerprint),
        };
        self.config.add_bookmark(new_bookmark);
        let _ = self.config.save();

        // Update the connection's bookmark_index to point to the new bookmark
        if let Some(server_conn) = self.connections.get_mut(&connection_id) {
            server_conn.bookmark_index = Some(self.config.bookmarks.len() - 1);
        }
    }
}
