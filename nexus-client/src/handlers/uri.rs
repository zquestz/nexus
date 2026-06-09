//! URI intent handler for nexus:// scheme navigation

use iced::Task;
use nexus_common::names::fold_name;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::{get_locale, t_args};
use crate::network::{ConnectionParams, ProxyConfig};
use crate::types::{
    ActivePanel, ChatMessage, ChatTab, Message, NetworkConnection, PendingRequests, ResponseRouting,
};
use crate::uri::{NexusPath, NexusUri};

impl NexusApp {
    /// Handle a nexus:// URI
    ///
    /// This is the main entry point for URI handling. It:
    /// 1. Tries to find an existing connection to the host
    /// 2. If found with matching credentials, navigates to the path intent
    /// 3. If not found, initiates a new connection and stores the path for later
    pub fn handle_nexus_uri(&mut self, uri: NexusUri) -> Task<Message> {
        // Try to find an existing connection to this host
        let existing_conn = self.find_connection_for_uri(&uri);

        if let Some(connection_id) = existing_conn {
            // Found existing connection - switch to it and navigate
            self.active_connection = Some(connection_id);

            // Navigate to path intent if present
            if let Some(ref path) = uri.path {
                return self.navigate_to_path(connection_id, path.clone());
            }

            Task::none()
        } else {
            // No existing connection - initiate new connection
            self.connect_from_uri(uri)
        }
    }

    /// Find an existing connection that matches the URI
    ///
    /// Matching logic:
    /// - If URI has no credentials: match any connection to the same host:port
    /// - If URI has credentials: match connection with same host:port AND username
    fn find_connection_for_uri(&self, uri: &NexusUri) -> Option<usize> {
        for (conn_id, conn) in &self.connections {
            // Check host matches (case-insensitive)
            if conn.connection_info.address.to_lowercase() != uri.host.to_lowercase() {
                continue;
            }

            // Check port matches
            if conn.connection_info.port != uri.port {
                continue;
            }

            // If URI has credentials, also check username matches
            if let Some(ref uri_user) = uri.user
                && fold_name(&conn.connection_info.username) != fold_name(uri_user)
            {
                continue;
            }

            return Some(*conn_id);
        }

        None
    }

    /// Initiate a connection from a URI
    fn connect_from_uri(&mut self, uri: NexusUri) -> Task<Message> {
        let connection_id = self.next_connection_id;
        self.next_connection_id += 1;

        let server_address = uri.host.clone();
        let port = uri.port;

        // If URI has no credentials, look for a matching bookmark to use its credentials.
        // If a bookmark is matched, also pull its stored fingerprint so the
        // stage-1 TOFU check can run.
        let (username, password, nickname, display_name, expected_fingerprint) =
            if uri.user.is_none() {
                // Find bookmark matching host:port
                if let Some(bookmark) = self
                    .config
                    .find_bookmark_matching_uri(&uri.host, uri.port, None)
                {
                    (
                        bookmark.username.clone(),
                        bookmark.password.clone(),
                        if bookmark.nickname.is_empty() {
                            self.config.settings.nickname.clone()
                        } else {
                            Some(bookmark.nickname.clone())
                        },
                        bookmark.name.clone(),
                        bookmark.certificate_fingerprint.clone(),
                    )
                } else {
                    // No bookmark found - use guest login
                    (
                        String::new(),
                        String::new(),
                        self.config.settings.nickname.clone(),
                        crate::uri::format_endpoint(&uri.host, uri.port),
                        None,
                    )
                }
            } else {
                // URI has username - find matching bookmark for password and display name
                let uri_user = uri.user.clone().unwrap_or_default();

                if let Some(bookmark) =
                    self.config
                        .find_bookmark_matching_uri(&uri.host, uri.port, Some(&uri_user))
                {
                    // Use bookmark's password if URI didn't provide one
                    let password = uri
                        .password
                        .clone()
                        .unwrap_or_else(|| bookmark.password.clone());
                    (
                        uri_user,
                        password,
                        if bookmark.nickname.is_empty() {
                            self.config.settings.nickname.clone()
                        } else {
                            Some(bookmark.nickname.clone())
                        },
                        bookmark.name.clone(),
                        bookmark.certificate_fingerprint.clone(),
                    )
                } else {
                    // No matching bookmark - use URI credentials as-is
                    (
                        uri_user,
                        uri.password.clone().unwrap_or_default(),
                        self.config.settings.nickname.clone(),
                        crate::uri::format_endpoint(&uri.host, uri.port),
                        None,
                    )
                }
            };

        let locale = get_locale().to_string();
        let avatar = self.config.settings.avatar.clone();

        // Build proxy config if enabled
        let proxy = if self.config.settings.proxy.enabled {
            Some(ProxyConfig {
                address: self.config.settings.proxy.address.clone(),
                port: self.config.settings.proxy.port,
                username: self.config.settings.proxy.username.clone(),
                password: self.config.settings.proxy.password.clone(),
            })
        } else {
            None
        };

        let path = uri.path.clone();
        let params = ConnectionParams {
            server_address,
            port,
            username,
            password,
            nickname,
            locale,
            avatar,
            connection_id,
            proxy,
            expected_fingerprint,
        };
        // Clone for the result handler so an accept-after-mismatch can
        // replay the original intent (URI path navigation included).
        let retry_params = params.clone();

        Task::perform(
            async move { crate::network::connect_to_server(params).await },
            move |result| Message::UriConnectionResult {
                result,
                params: retry_params,
                display_name,
                path,
            },
        )
    }

    /// Navigate to a path intent within an existing connection
    pub fn navigate_to_path(&mut self, connection_id: usize, path: NexusPath) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        match path {
            NexusPath::Chat { target, is_channel } => {
                if let Some(target) = target {
                    if is_channel {
                        // Join or focus channel
                        let channel_lower = fold_name(&target);

                        // Check if we're already in this channel
                        let already_joined =
                            conn.channels.keys().any(|c| fold_name(c) == channel_lower);

                        if already_joined {
                            // Just switch to the tab
                            conn.active_chat_tab = ChatTab::Channel(target);
                        } else {
                            // Need to join the channel
                            let _ = conn.send(ClientMessage::ChatJoin {
                                channel: target.clone(),
                            });
                        }
                    } else {
                        // Open PM tab with user
                        // First check if the user exists (case-insensitive match)
                        let user_lower = fold_name(&target);
                        let actual_nickname = conn
                            .online_users
                            .iter()
                            .find(|u| fold_name(&u.nickname) == user_lower)
                            .map(|u| u.nickname.clone());

                        let tab_name = actual_nickname
                            .or_else(|| conn.user_message_tab_key(&target))
                            .unwrap_or(target);

                        // Create or focus the PM tab (resolved by folded identity).
                        let key = conn.resolve_user_message_tab(&tab_name);
                        conn.active_chat_tab = ChatTab::UserMessage(key);
                    }
                }
                // If target is None, just show chat panel (don't change active tab)

                // Make sure chat is visible (not hidden by another panel)
                conn.active_panel = ActivePanel::None;

                self.scroll_chat_if_visible(true)
            }

            NexusPath::Files { path } => {
                // Open Files panel and navigate to path
                conn.active_panel = ActivePanel::Files;

                // Get show_hidden from config
                let show_hidden = self.config.settings.show_hidden_files;

                if !path.is_empty() {
                    // Extract parent directory and target name
                    // Server resolves paths with folder type suffixes (e.g., "uploads" -> "uploads [NEXUS-UL]")
                    let (parent_path, target_name) = if let Some(slash_idx) = path.rfind('/') {
                        (path[..slash_idx].to_string(), &path[slash_idx + 1..])
                    } else {
                        (String::new(), path.as_str())
                    };

                    // Build uri_target if we have a target name
                    let uri_target = if !target_name.is_empty() {
                        Some(target_name.to_string())
                    } else {
                        None
                    };

                    // Get the active file tab and navigate to parent directory
                    let active_tab = conn.files_management.active_tab_mut();
                    active_tab.navigate_to(parent_path.clone());

                    // Request file list for the parent path, passing uri_target through routing
                    // URIs always use viewing_root: false - they're for users sharing files,
                    // not for admin root browsing
                    let tab_id = active_tab.id;
                    return self.send_file_list_request_for_tab(
                        connection_id,
                        tab_id,
                        parent_path,
                        false, // URIs always navigate within user's area, not root
                        show_hidden,
                        uri_target,
                    );
                } else {
                    // Empty path - just open files panel at current location
                    // Request file list if not already loaded
                    if conn.files_management.active_tab().entries.is_none() {
                        let path_str = conn.files_management.active_tab().current_path.clone();
                        return self.send_file_list_request(
                            connection_id,
                            path_str,
                            false, // URIs always navigate within user's area, not root
                            show_hidden,
                        );
                    }
                }

                Task::none()
            }

            NexusPath::News => {
                conn.active_panel = ActivePanel::News;

                // Reset to list mode
                conn.news_management.reset_to_list();

                // Request news list if not already loaded
                if conn.news_management.news_items.is_none()
                    && let Ok(message_id) = conn.send(ClientMessage::NewsList)
                {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::PopulateNewsList);
                }

                Task::none()
            }

            NexusPath::Info => {
                conn.active_panel = ActivePanel::ServerInfo;
                Task::none()
            }
        }
    }

    /// Called after a successful connection to process a URI path intent
    pub fn process_uri_path(
        &mut self,
        connection_id: usize,
        path: Option<NexusPath>,
    ) -> Task<Message> {
        if let Some(path) = path {
            return self.navigate_to_path(connection_id, path);
        }
        Task::none()
    }

    /// Handle URI connection result (success or failure)
    ///
    /// On success, creates the connection with proper display name.
    /// On fingerprint mismatch, queues an accept/reject dialog whose retry
    /// preserves the URI path-navigation intent.
    /// On other failures, shows error in the current connection's console.
    pub fn handle_uri_connection_result(
        &mut self,
        result: Result<NetworkConnection, crate::network::types::ConnectError>,
        params: crate::network::types::ConnectionParams,
        display_name: String,
        path: Option<NexusPath>,
    ) -> Task<Message> {
        use super::network::{ConnectionContext, ConnectionSource};
        use crate::network::types::ConnectError;
        use crate::types::ReconnectAction;

        match result {
            Ok(conn) => {
                // Try to find a matching bookmark for this connection.
                // Same URI-shape lookup as connect_from_uri: 2-tuple if no
                // creds were used, 3-tuple if username is set.
                let bookmark_id = self
                    .config
                    .find_bookmark_matching_uri(
                        &conn.connection_info.address,
                        conn.connection_info.port,
                        (!conn.connection_info.username.is_empty())
                            .then_some(conn.connection_info.username.as_str()),
                    )
                    .map(|b| b.id);

                // Use bookmark name as display name if we found a match
                let display_name = bookmark_id
                    .and_then(|id| self.config.get_bookmark(id))
                    .map(|b| b.name.clone())
                    .unwrap_or(display_name);

                let connection_id = conn.connection_id;
                let ctx = ConnectionContext {
                    bookmark_id,
                    display_name,
                    certificate_fingerprint: conn.connection_info.certificate_fingerprint.clone(),
                    connection_id,
                };

                let connect_task =
                    self.handle_successful_connection(conn, ctx, ConnectionSource::Uri);
                let path_task = self.process_uri_path(connection_id, path);
                Task::batch([connect_task, path_task])
            }
            Err(ConnectError::FingerprintMismatch(details)) => {
                let action = ReconnectAction::Uri {
                    params,
                    display_name,
                    path,
                };
                self.queue_fingerprint_mismatch(*details, action);
                Task::none()
            }
            Err(ConnectError::FingerprintInterception(mut details)) => {
                // Use the same URI-shape lookup as connect_from_uri so we
                // resolve to the same bookmark identity. URIs without creds
                // were matched by (host, port); with creds by (host, port,
                // username). Empty params.username means "no creds at connect
                // time" → 2-tuple match. Non-empty → 3-tuple match. If no
                // bookmark, server_name stays empty and the dialog falls
                // through to host:port.
                details.server_name = self
                    .config
                    .find_bookmark_matching_uri(
                        &params.server_address,
                        params.port,
                        (!params.username.is_empty()).then_some(params.username.as_str()),
                    )
                    .map(|b| b.name.clone())
                    .unwrap_or_default();
                self.fingerprint_interception_queue.push_back(*details);
                Task::none()
            }
            Err(other) => {
                let error = other.to_localized_string();
                // Show error in current connection's console if we have one
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get_mut(&conn_id) {
                        let error_msg = t_args(
                            "err-uri-connection-failed",
                            &[("host", &params.server_address), ("error", &error)],
                        );
                        conn.console_messages.push(ChatMessage::error(error_msg));
                    }
                } else {
                    // No active connection - put error in connection form
                    self.connection_form.error = Some(error);
                }

                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nexus_common::framing::MessageId;
    use nexus_common::protocol::ClientMessage;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use super::*;
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
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: "me".to_string(),
            },
            display_name: String::new(),
            connection_id,
            is_admin: false,
            permissions: Vec::new(),
            features: Vec::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            cached_server_image: None,
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
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        });
        (conn, rx)
    }

    #[test]
    fn uri_channel_join_waits_for_server_response_before_focusing_tab() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (conn, mut rx) = test_connection_with_receiver(1);
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Chat {
                target: Some("#general".to_string()),
                is_channel: true,
            },
        );

        assert_eq!(app.connections[&1].active_chat_tab, ChatTab::Console);
        match rx.try_recv() {
            Ok((_, ClientMessage::ChatJoin { channel })) => assert_eq!(channel, "#general"),
            other => panic!("expected ChatJoin, got {other:?}"),
        }
    }
}
