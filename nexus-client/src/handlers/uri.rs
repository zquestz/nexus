//! URI intent handler for nexus:// scheme navigation

use iced::Task;
use nexus_common::names::fold_name;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::validate_connection_address;

use crate::NexusApp;
use crate::handlers::FilesOpenIntent;
use crate::i18n::{get_locale, t, t_args};
use crate::network::{ConnectionParams, FEATURE_CHAT, FEATURE_FILES, FEATURE_NEWS, ProxyConfig};
use crate::types::{ChatMessage, ChatTab, Message, NetworkConnection};
use crate::uri::{NexusPath, NexusUri};

struct UriConnectionLookup {
    username: String,
    password: String,
    nickname: Option<String>,
    display_name: String,
    expected_fingerprint: Option<String>,
}

impl NexusApp {
    /// Handle a nexus:// URI
    ///
    /// This is the main entry point for URI handling. It:
    /// 1. Tries to find an existing connection to the host
    /// 2. If found with matching credentials, navigates to the path intent
    /// 3. If not found, initiates a new connection and stores the path for later
    pub fn handle_nexus_uri(&mut self, uri: NexusUri) -> Task<Message> {
        if let Err(error) = validate_connection_address(&uri.host) {
            let error = super::server_form_errors::translate_server_address_error(error);
            return self.show_uri_connection_error(&uri.host, error);
        }

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
        let uri_host = crate::config::canonical_bookmark_host(&uri.host);
        for (conn_id, conn) in &self.connections {
            // Check host matches using the same connection-time normalization
            // as bookmark lookup so bracketed IPv6 / IDN spelling variants
            // reuse the existing connection.
            if crate::config::canonical_bookmark_host(&conn.connection_info.address) != uri_host {
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
        let UriConnectionLookup {
            username,
            password,
            nickname,
            display_name,
            expected_fingerprint,
        } = self.resolve_uri_connection_lookup(&uri);

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

    fn resolve_uri_connection_lookup(&self, uri: &NexusUri) -> UriConnectionLookup {
        // If URI has no credentials, look for a matching bookmark to use its
        // credentials and stored fingerprint for the stage-1 TOFU check.
        if uri.user.is_none() {
            if let Some(bookmark) = self
                .config
                .find_bookmark_matching_uri(&uri.host, uri.port, None)
            {
                return UriConnectionLookup {
                    username: bookmark.username.clone(),
                    password: bookmark.password.clone(),
                    nickname: if bookmark.nickname.is_empty() {
                        self.config.settings.nickname.clone()
                    } else {
                        Some(bookmark.nickname.clone())
                    },
                    display_name: bookmark.name.clone(),
                    expected_fingerprint: bookmark.certificate_fingerprint.clone(),
                };
            }

            return UriConnectionLookup {
                username: String::new(),
                password: String::new(),
                nickname: self.config.settings.nickname.clone(),
                display_name: crate::uri::format_endpoint(&uri.host, uri.port),
                expected_fingerprint: None,
            };
        }

        // URI has username - find matching bookmark for password, display name,
        // nickname, and stored fingerprint.
        let uri_user = uri.user.clone().unwrap_or_default();
        if let Some(bookmark) =
            self.config
                .find_bookmark_matching_uri(&uri.host, uri.port, Some(&uri_user))
        {
            return UriConnectionLookup {
                username: uri_user,
                password: uri
                    .password
                    .clone()
                    .unwrap_or_else(|| bookmark.password.clone()),
                nickname: if bookmark.nickname.is_empty() {
                    self.config.settings.nickname.clone()
                } else {
                    Some(bookmark.nickname.clone())
                },
                display_name: bookmark.name.clone(),
                expected_fingerprint: bookmark.certificate_fingerprint.clone(),
            };
        }

        UriConnectionLookup {
            username: uri_user,
            password: uri.password.clone().unwrap_or_default(),
            nickname: self.config.settings.nickname.clone(),
            display_name: crate::uri::format_endpoint(&uri.host, uri.port),
            expected_fingerprint: None,
        }
    }

    fn show_uri_connection_error(&mut self, host: &str, error: String) -> Task<Message> {
        if let Some(connection_id) = self.active_connection {
            let error_msg = t_args(
                "err-uri-connection-failed",
                &[("host", host), ("error", &error)],
            );
            return self.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
        }

        self.connection_form.error = Some(error);
        Task::none()
    }

    fn show_chat_uri_error(&mut self, connection_id: usize, error: String) -> Task<Message> {
        let error_task = self.add_active_tab_message(connection_id, ChatMessage::error(error));
        let show_task = self.handle_show_chat_view();
        Task::batch([error_task, show_task])
    }

    /// Navigate to a path intent within an existing connection
    pub fn navigate_to_path(&mut self, connection_id: usize, path: NexusPath) -> Task<Message> {
        match path {
            NexusPath::Chat { target, is_channel } => {
                let Some(conn) = self.connections.get(&connection_id) else {
                    return Task::none();
                };

                if !conn.has_feature(FEATURE_CHAT) {
                    return self
                        .show_chat_uri_error(connection_id, t("err-chat-feature-not-enabled"));
                }

                if let Some(target) = target
                    && let Some(conn) = self.connections.get_mut(&connection_id)
                {
                    if is_channel {
                        let channel_lower = fold_name(&target);
                        let already_joined =
                            conn.channels.keys().any(|c| fold_name(c) == channel_lower);

                        if already_joined {
                            conn.active_chat_tab = ChatTab::Channel(target);
                        } else {
                            let _ = conn.send(ClientMessage::ChatJoin {
                                channel: target.clone(),
                            });
                        }
                    } else {
                        let user_lower = fold_name(&target);
                        let tab_name =
                            if let Some(existing_key) = conn.user_message_tab_key(&target) {
                                existing_key
                            } else if let Some(online_nickname) = conn
                                .online_users
                                .iter()
                                .find(|u| fold_name(&u.nickname) == user_lower)
                                .map(|u| u.nickname.clone())
                            {
                                online_nickname
                            } else {
                                let error_msg =
                                    t_args("cmd-focus-not-found", &[("name", target.as_str())]);
                                return self.show_chat_uri_error(connection_id, error_msg);
                            };

                        let key = conn.resolve_user_message_tab(&tab_name);
                        conn.active_chat_tab = ChatTab::UserMessage(key);
                    }
                }

                self.handle_show_chat_view()
            }

            NexusPath::Files { segments } => {
                let Some(conn) = self.connections.get(&connection_id) else {
                    return Task::none();
                };

                if !conn.has_feature(FEATURE_FILES) {
                    return self
                        .show_chat_uri_error(connection_id, t("err-files-feature-not-enabled"));
                }

                let Some(path) = (NexusPath::Files { segments }).file_path() else {
                    return self.show_chat_uri_error(connection_id, t("err-files-invalid-path"));
                };
                if path.is_empty() {
                    self.handle_toggle_files(FilesOpenIntent::Toolbar)
                } else {
                    self.handle_toggle_files(FilesOpenIntent::UriPath(path))
                }
            }

            NexusPath::News => {
                let Some(conn) = self.connections.get(&connection_id) else {
                    return Task::none();
                };

                if !conn.has_feature(FEATURE_NEWS) {
                    return self
                        .show_chat_uri_error(connection_id, t("err-news-feature-not-enabled"));
                }

                self.handle_toggle_news()
            }
            NexusPath::Info => self.handle_show_server_info(),
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
    /// On other failures, shows an error in the active chat tab.
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
                self.show_uri_connection_error(&params.server_address, error)
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
    use crate::network::types::{ConnectError, ConnectionParams};
    use crate::types::{
        ActivePanel, ConnectionInfo, NewsManagementMode, ResponseRouting, ServerBookmark,
        ServerConnection, ServerConnectionParams, UserInfo,
    };

    fn test_connection_with_receiver(
        connection_id: usize,
    ) -> (
        ServerConnection,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        test_connection_with_receiver_and_features(connection_id, vec![FEATURE_CHAT.to_string()])
    }

    fn test_connection_with_receiver_and_features(
        connection_id: usize,
        features: Vec<String>,
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
            features,
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

    fn test_connection_params(server_address: &str) -> ConnectionParams {
        ConnectionParams {
            server_address: server_address.to_string(),
            port: 7500,
            username: String::new(),
            password: String::new(),
            nickname: None,
            locale: "en".to_string(),
            avatar: None,
            connection_id: 2,
            proxy: None,
            expected_fingerprint: None,
        }
    }

    fn test_user(nickname: &str) -> UserInfo {
        UserInfo {
            id: 1,
            username: nickname.to_string(),
            nickname: nickname.to_string(),
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            avatar_hash: None,
            is_away: false,
            status: None,
        }
    }

    fn invalid_host_uri() -> NexusUri {
        NexusUri {
            user: None,
            password: None,
            host: "bad host".to_string(),
            port: 7500,
            path: None,
        }
    }

    fn uri_error_message(host: &str, error: &str) -> String {
        t_args(
            "err-uri-connection-failed",
            &[("host", host), ("error", error)],
        )
    }

    #[test]
    fn uri_connection_lookup_uses_bookmark_pin_for_ipv6_variant() {
        let mut app = NexusApp::default();
        app.config.add_bookmark(ServerBookmark {
            name: "Local".to_string(),
            address: "[::1]".to_string(),
            port: 7500,
            username: "alice".to_string(),
            password: "secret".to_string(),
            certificate_fingerprint: Some("fp-local".to_string()),
            ..Default::default()
        });
        let uri = NexusUri {
            user: None,
            password: None,
            host: "::1".to_string(),
            port: 7500,
            path: None,
        };

        let lookup = app.resolve_uri_connection_lookup(&uri);

        assert_eq!(lookup.username, "alice");
        assert_eq!(lookup.password, "secret");
        assert_eq!(lookup.display_name, "Local");
        assert_eq!(lookup.expected_fingerprint.as_deref(), Some("fp-local"));
    }

    #[test]
    fn uri_existing_connection_lookup_uses_canonical_host() {
        let mut app = NexusApp {
            active_connection: Some(1),
            next_connection_id: 2,
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.connection_info.address = "[::1]".to_string();
        conn.connection_info.port = 7500;
        app.connections.insert(1, conn);
        let uri = NexusUri {
            user: None,
            password: None,
            host: "::1".to_string(),
            port: 7500,
            path: None,
        };

        let task = app.handle_nexus_uri(uri);

        assert_eq!(app.active_connection, Some(1));
        assert_eq!(app.next_connection_id, 2);
        assert!(rx.try_recv().is_err());
        drop(task);
    }

    #[test]
    fn invalid_uri_host_with_active_connection_reports_to_active_chat_tab() {
        let mut app = NexusApp {
            active_connection: Some(1),
            next_connection_id: 2,
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        let key = conn.resolve_user_message_tab("alice");
        conn.active_chat_tab = ChatTab::UserMessage(key);
        app.connections.insert(1, conn);

        let _ = app.handle_nexus_uri(invalid_host_uri());

        let conn = &app.connections[&1];
        assert!(conn.console_messages.is_empty());
        let messages = conn.user_messages_for("alice").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].message,
            uri_error_message("bad host", &t("err-server-address-contains-whitespace"))
        );
        assert_eq!(app.next_connection_id, 2);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn invalid_uri_host_without_active_connection_reports_to_connection_form() {
        let mut app = NexusApp {
            next_connection_id: 2,
            ..NexusApp::default()
        };

        let _ = app.handle_nexus_uri(invalid_host_uri());

        assert_eq!(
            app.connection_form.error,
            Some(t("err-server-address-contains-whitespace"))
        );
        assert_eq!(app.next_connection_id, 2);
    }

    #[test]
    fn uri_connection_failure_reports_to_active_chat_tab() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, _rx) = test_connection_with_receiver(1);
        let key = conn.resolve_user_message_tab("alice");
        conn.active_chat_tab = ChatTab::UserMessage(key);
        app.connections.insert(1, conn);

        let _ = app.handle_uri_connection_result(
            Err(ConnectError::Other("boom".to_string())),
            test_connection_params("example.com"),
            String::new(),
            None,
        );

        let conn = &app.connections[&1];
        assert!(conn.console_messages.is_empty());
        let messages = conn.user_messages_for("alice").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].message,
            uri_error_message("example.com", "boom")
        );
    }

    #[test]
    fn uri_files_path_uses_single_parent_request_and_dismisses_overlay() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        let (mut conn, mut rx) = test_connection_with_receiver_and_features(
            1,
            vec![FEATURE_CHAT.to_string(), FEATURE_FILES.to_string()],
        );
        conn.active_panel = ActivePanel::News;
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Files {
                segments: vec!["Music".to_string(), "song.mp3".to_string()],
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::Files);
        assert!(app.connection_form.connect_origin.is_none());
        assert_eq!(conn.files_management.active_tab().current_path, "Music");

        let (message_id, message) = rx.try_recv().expect("expected FileList request");
        match message {
            ClientMessage::FileList {
                path,
                root,
                show_hidden,
            } => {
                assert_eq!(path, "Music");
                assert!(!root);
                assert_eq!(show_hidden, app.config.settings.show_hidden_files);
            }
            other => panic!("expected FileList, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            conn.pending_requests.get(&message_id),
            Some(ResponseRouting::PopulateFileList {
                uri_target: Some(target),
                ..
            }) if target == "song.mp3"
        ));
    }

    #[test]
    fn uri_files_empty_path_uses_toolbar_noop_when_already_open() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        let (mut conn, mut rx) = test_connection_with_receiver_and_features(
            1,
            vec![FEATURE_CHAT.to_string(), FEATURE_FILES.to_string()],
        );
        conn.active_panel = ActivePanel::Files;
        conn.files_management
            .active_tab_mut()
            .navigate_to("Documents".to_string());
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Files {
                segments: Vec::new(),
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::Files);
        assert_eq!(conn.files_management.active_tab().current_path, "Documents");
        assert!(app.connection_form.connect_origin.is_some());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_files_encoded_separator_is_invalid_path() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (conn, mut rx) = test_connection_with_receiver_and_features(
            1,
            vec![FEATURE_CHAT.to_string(), FEATURE_FILES.to_string()],
        );
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Files {
                segments: vec!["Music/Hidden".to_string(), "song.mp3".to_string()],
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::None);
        assert_eq!(conn.console_messages.len(), 1);
        assert_eq!(
            conn.console_messages[0].message,
            t("err-files-invalid-path")
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_news_uses_toolbar_open_behavior() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        app.news_body_content.insert(1, Default::default());
        let (mut conn, mut rx) = test_connection_with_receiver_and_features(
            1,
            vec![FEATURE_CHAT.to_string(), FEATURE_NEWS.to_string()],
        );
        conn.active_panel = ActivePanel::Files;
        conn.news_management.mode = NewsManagementMode::Create;
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(1, NexusPath::News);

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::News);
        assert!(matches!(
            conn.news_management.mode,
            NewsManagementMode::List
        ));
        assert!(!app.news_body_content.contains_key(&1));
        assert!(app.connection_form.connect_origin.is_none());

        let (message_id, message) = rx.try_recv().expect("expected NewsList request");
        assert!(matches!(message, ClientMessage::NewsList));
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            conn.pending_requests.get(&message_id),
            Some(ResponseRouting::PopulateNewsList)
        ));
    }

    #[test]
    fn uri_news_already_active_matches_toolbar_noop() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        app.news_body_content.insert(1, Default::default());
        let (mut conn, mut rx) = test_connection_with_receiver_and_features(
            1,
            vec![FEATURE_CHAT.to_string(), FEATURE_NEWS.to_string()],
        );
        conn.active_panel = ActivePanel::News;
        conn.news_management.mode = NewsManagementMode::Create;
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(1, NexusPath::News);

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::News);
        assert!(matches!(
            conn.news_management.mode,
            NewsManagementMode::Create
        ));
        assert!(app.news_body_content.contains_key(&1));
        assert!(app.connection_form.connect_origin.is_some());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_info_uses_server_info_helper_and_prefetches_trackers() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.active_panel = ActivePanel::Files;
        conn.permissions
            .push(crate::views::constants::PERMISSION_TRACKER_LIST.to_string());
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(1, NexusPath::Info);

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::ServerInfo);
        assert!(app.connection_form.connect_origin.is_none());

        let (message_id, message) = rx.try_recv().expect("expected TrackerList request");
        assert!(matches!(message, ClientMessage::TrackerList));
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            conn.pending_requests.get(&message_id),
            Some(ResponseRouting::PopulateTrackerManagementList)
        ));
    }

    #[test]
    fn uri_chat_user_dismisses_overlay_and_focuses_dm_tab() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.active_panel = ActivePanel::Files;
        conn.online_users.push(test_user("alice"));
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Chat {
                target: Some("alice".to_string()),
                is_channel: false,
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::None);
        assert!(matches!(&conn.active_chat_tab, ChatTab::UserMessage(name) if name == "alice"));
        assert!(app.connection_form.connect_origin.is_none());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_chat_user_uses_online_user_casing() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.online_users.push(test_user("Alice"));
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Chat {
                target: Some("alice".to_string()),
                is_channel: false,
            },
        );

        let conn = &app.connections[&1];
        assert!(matches!(&conn.active_chat_tab, ChatTab::UserMessage(name) if name == "Alice"));
        assert!(conn.user_messages.contains_key("Alice"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_chat_user_focuses_existing_tab_when_offline() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        let key = conn.resolve_user_message_tab("Alice");
        conn.active_chat_tab = ChatTab::Console;
        assert_eq!(key, "Alice");
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Chat {
                target: Some("alice".to_string()),
                is_channel: false,
            },
        );

        let conn = &app.connections[&1];
        assert!(matches!(&conn.active_chat_tab, ChatTab::UserMessage(name) if name == "Alice"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_chat_user_missing_reports_not_found_without_opening_tab() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connection_form.connect_origin = Some(ActivePanel::Settings);
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.active_panel = ActivePanel::Files;
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Chat {
                target: Some("missing".to_string()),
                is_channel: false,
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::None);
        assert_eq!(conn.active_chat_tab, ChatTab::Console);
        assert!(conn.user_message_tabs.is_empty());
        assert!(conn.user_messages.is_empty());
        assert_eq!(conn.console_messages.len(), 1);
        assert_eq!(
            conn.console_messages[0].message,
            t_args("cmd-focus-not-found", &[("name", "missing")])
        );
        assert!(app.connection_form.connect_origin.is_none());
        assert!(rx.try_recv().is_err());
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

    #[test]
    fn uri_user_message_requires_chat_feature() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (conn, mut rx) = test_connection_with_receiver_and_features(1, Vec::new());
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Chat {
                target: Some("alice".to_string()),
                is_channel: false,
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_chat_tab, ChatTab::Console);
        assert!(conn.user_message_tabs.is_empty());
        assert_eq!(conn.console_messages.len(), 1);
        assert_eq!(
            conn.console_messages[0].message,
            t("err-chat-feature-not-enabled")
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_files_requires_files_feature() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (conn, mut rx) = test_connection_with_receiver_and_features(1, Vec::new());
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(
            1,
            NexusPath::Files {
                segments: vec!["uploads".to_string(), "readme.txt".to_string()],
            },
        );

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::None);
        assert_eq!(conn.console_messages.len(), 1);
        assert_eq!(
            conn.console_messages[0].message,
            t("err-files-feature-not-enabled")
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn uri_news_requires_news_feature() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (conn, mut rx) = test_connection_with_receiver_and_features(1, Vec::new());
        app.connections.insert(1, conn);

        let _ = app.navigate_to_path(1, NexusPath::News);

        let conn = &app.connections[&1];
        assert_eq!(conn.active_panel, ActivePanel::None);
        assert_eq!(conn.console_messages.len(), 1);
        assert_eq!(
            conn.console_messages[0].message,
            t("err-news-feature-not-enabled")
        );
        assert!(rx.try_recv().is_err());
    }
}
