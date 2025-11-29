//! Network events

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    ChatMessage, ChatTab, InputId, Message, ScrollableId, ServerBookmark, ServerConnection,
    UserInfo,
};
use chrono::Local;
use iced::Task;
use iced::widget::{scrollable, text_input};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::collections::{HashMap, HashSet};

// Message username constants (pub for use in views) - use i18n
pub(crate) fn msg_username_system() -> String {
    t("msg-username-system")
}
pub(crate) fn msg_username_error() -> String {
    t("msg-username-error")
}
pub(crate) fn msg_username_info() -> String {
    t("msg-username-info")
}
pub(crate) fn msg_username_broadcast_prefix() -> String {
    t("msg-username-broadcast-prefix")
}

/// Helper function to sort user list alphabetically by username (case-insensitive)
fn sort_user_list(users: &mut [UserInfo]) {
    users.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
}

impl NexusApp {
    /// Handle connection attempt result (success or failure)
    pub fn handle_connection_result(
        &mut self,
        result: Result<crate::types::NetworkConnection, String>,
    ) -> Task<Message> {
        // Clear connecting flag
        self.connection_form.is_connecting = false;

        match result {
            Ok(conn) => {
                self.connection_form.error = None;

                // Find if this connection matches a bookmark
                let bookmark_index = self.config.bookmarks.iter().position(|b| {
                    b.address == self.connection_form.server_address
                        && b.port == self.connection_form.port
                        && b.username == self.connection_form.username
                });

                // Verify and save certificate fingerprint
                if let Err(boxed_mismatch) =
                    self.verify_and_save_fingerprint(bookmark_index, &conn.certificate_fingerprint)
                {
                    // Extract display name for manual connection
                    let display_name = if !self.connection_form.server_name.trim().is_empty() {
                        self.connection_form.server_name.clone()
                    } else if let Some(idx) = bookmark_index {
                        self.config.bookmarks[idx].name.clone()
                    } else {
                        format!(
                            "{}:{}",
                            self.connection_form.server_address, self.connection_form.port
                        )
                    };
                    return self.handle_fingerprint_mismatch(boxed_mismatch, conn, display_name);
                }

                let session_id = conn.session_id;
                let username = self.connection_form.username.clone();
                let connection_id = conn.connection_id;

                // Create display name
                let display_name = if !self.connection_form.server_name.trim().is_empty() {
                    self.connection_form.server_name.clone()
                } else if let Some(idx) = bookmark_index {
                    self.config.bookmarks[idx].name.clone()
                } else {
                    format!(
                        "{}:{}",
                        self.connection_form.server_address, self.connection_form.port
                    )
                };

                let conn_tx = conn.tx.clone();
                let should_request_userlist =
                    conn.is_admin || conn.permissions.contains(&"user_list".to_string());
                let chat_topic = conn.chat_topic.clone();

                // Create server connection
                let server_conn = ServerConnection {
                    bookmark_index,
                    session_id,
                    username,
                    display_name,
                    connection_id,
                    is_admin: conn.is_admin,
                    permissions: conn.permissions,
                    locale: conn.locale,
                    active_chat_tab: ChatTab::Server,
                    chat_messages: Vec::new(),
                    user_messages: HashMap::new(),
                    unread_tabs: HashSet::new(),
                    online_users: Vec::new(),
                    expanded_user: None,
                    tx: conn.tx,
                    shutdown_handle: match conn.shutdown {
                        Some(handle) => handle,
                        None => {
                            self.connection_form.error = Some(t("err-no-shutdown-handle"));
                            return Task::none();
                        }
                    },
                    message_input: String::new(),
                    broadcast_message: String::new(),
                    chat_auto_scroll: true,
                    broadcast_error: None,
                    user_management: crate::types::UserManagementState::default(),
                };

                // Add to connections and make it active
                self.connections.insert(connection_id, server_conn);
                self.active_connection = Some(connection_id);

                // Request initial user list (only if user has permission)
                if should_request_userlist && let Err(e) = conn_tx.send(ClientMessage::UserList) {
                    // Channel send failed - connection is broken, show error
                    self.connection_form.error =
                        Some(format!("{}: {}", t("err-connection-broken"), e));
                    // Remove the connection we just added since it's broken
                    self.connections.remove(&connection_id);
                    self.active_connection = None;
                    return Task::none();
                }

                // Add chat topic message if present
                self.add_topic_message_if_present(connection_id, chat_topic);

                // Save as bookmark if checkbox was enabled (and not already a bookmark)
                if self.connection_form.add_bookmark && bookmark_index.is_none() {
                    let new_bookmark = ServerBookmark {
                        name: self.connection_form.server_name.clone(),
                        address: self.connection_form.server_address.clone(),
                        port: self.connection_form.port.clone(),
                        username: self.connection_form.username.clone(),
                        password: self.connection_form.password.clone(),
                        auto_connect: false,
                        certificate_fingerprint: Some(conn.certificate_fingerprint.clone()),
                    };
                    self.config.add_bookmark(new_bookmark);
                    let _ = self.config.save();

                    // Update the connection's bookmark_index to point to the new bookmark
                    if let Some(server_conn) = self.connections.get_mut(&connection_id) {
                        server_conn.bookmark_index = Some(self.config.bookmarks.len() - 1);
                    }
                }

                // Clear connection form
                self.connection_form.clear();

                // Focus the chat input
                text_input::focus(text_input::Id::from(InputId::ChatInput))
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
        result: Result<crate::types::NetworkConnection, String>,
        bookmark_index: Option<usize>,
        display_name: String,
    ) -> Task<Message> {
        match result {
            Ok(conn) => {
                let session_id = conn.session_id;
                let conn_id = conn.connection_id;

                // Clear the connecting lock for this bookmark
                if let Some(idx) = bookmark_index {
                    self.connecting_bookmarks.remove(&idx);
                }

                // Verify and save certificate fingerprint
                if let Err(boxed_mismatch) =
                    self.verify_and_save_fingerprint(bookmark_index, &conn.certificate_fingerprint)
                {
                    return self.handle_fingerprint_mismatch(boxed_mismatch, conn, display_name);
                }

                // Extract username from bookmark if we have one
                let username = if let Some(idx) = bookmark_index {
                    if let Some(bookmark) = self.config.get_bookmark(idx) {
                        bookmark.username.clone()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let conn_tx = conn.tx.clone();
                let should_request_userlist =
                    conn.is_admin || conn.permissions.contains(&"user_list".to_string());
                let chat_topic = conn.chat_topic.clone();

                // Create server connection with passed display_name
                let server_conn = ServerConnection {
                    bookmark_index,
                    session_id,
                    username,
                    display_name, // Use the display_name passed from the bookmark
                    connection_id: conn_id,
                    is_admin: conn.is_admin,
                    permissions: conn.permissions,
                    locale: conn.locale,
                    active_chat_tab: ChatTab::Server,
                    chat_messages: Vec::new(),
                    user_messages: HashMap::new(),
                    unread_tabs: HashSet::new(),
                    online_users: Vec::new(),
                    expanded_user: None,
                    tx: conn.tx,
                    shutdown_handle: match conn.shutdown {
                        Some(handle) => handle,
                        None => {
                            // For bookmark connections, we can't show error in connection_form
                            // since it might be used for something else
                            // The connection just fails silently or we could log it
                            return Task::none();
                        }
                    },
                    message_input: String::new(),
                    broadcast_message: String::new(),
                    chat_auto_scroll: true,
                    broadcast_error: None,
                    user_management: crate::types::UserManagementState::default(),
                };

                // Add to connections and make it active
                self.connections.insert(conn_id, server_conn);
                self.active_connection = Some(conn_id);

                // Add chat topic message if present
                self.add_topic_message_if_present(conn_id, chat_topic);

                // Request initial user list (only if user has permission)
                if should_request_userlist && let Err(_e) = conn_tx.send(ClientMessage::UserList) {
                    // Channel send failed - connection is broken
                    // Remove the connection we just added since it's broken
                    self.connections.remove(&conn_id);
                    self.active_connection = None;
                    return Task::none();
                }

                // Focus the chat input
                text_input::focus(text_input::Id::from(InputId::ChatInput))
            }
            Err(error) => {
                // Clear the connecting lock for this bookmark
                if let Some(idx) = bookmark_index {
                    self.connecting_bookmarks.remove(&idx);
                    // Store error in per-bookmark error map (transient, not saved to disk)
                    self.bookmark_errors.insert(idx, error);
                }

                Task::none()
            }
        }
    }

    /// Handle message received from server
    pub fn handle_server_message_received(
        &mut self,
        connection_id: usize,
        msg: ServerMessage,
    ) -> Task<Message> {
        if self.connections.contains_key(&connection_id) {
            self.handle_server_message(connection_id, msg)
        } else {
            Task::none()
        }
    }

    /// Handle network error or connection closure
    pub fn handle_network_error(&mut self, connection_id: usize, error: String) -> Task<Message> {
        // Connection has closed or errored - remove it from the list
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

            // If this was the active connection, clear it
            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
                self.connection_form.error = Some(t_args("msg-disconnected", &[("error", &error)]));
            }
        }
        Task::none()
    }

    /// Add chat message and auto-scroll if this is the active connection
    pub fn add_chat_message(
        &mut self,
        connection_id: usize,
        message: ChatMessage,
    ) -> Task<Message> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.chat_messages.push(message);

            // Mark Server tab as unread if not currently viewing it
            if conn.active_chat_tab != ChatTab::Server {
                conn.unread_tabs.insert(ChatTab::Server);
            }

            if self.active_connection == Some(connection_id) && conn.chat_auto_scroll {
                return scrollable::snap_to(
                    crate::types::ScrollableId::ChatMessages.into(),
                    scrollable::RelativeOffset::END,
                );
            }
        }
        Task::none()
    }

    /// Add chat topic message if present and not empty
    fn add_topic_message_if_present(&mut self, connection_id: usize, chat_topic: Option<String>) {
        if let Some(topic) = chat_topic
            && !topic.is_empty()
        {
            let _ = self.add_chat_message(
                connection_id,
                ChatMessage {
                    username: msg_username_info(),
                    message: t_args("msg-topic-display", &[("topic", &topic)]),
                    timestamp: Local::now(),
                },
            );
        }
    }

    /// Process a specific server message and update state
    pub fn handle_server_message(
        &mut self,
        connection_id: usize,
        msg: ServerMessage,
    ) -> Task<Message> {
        let conn = match self.connections.get_mut(&connection_id) {
            Some(c) => c,
            None => return Task::none(),
        };

        match msg {
            ServerMessage::ChatTopic { topic, username } => {
                let message = if topic.is_empty() {
                    t_args("msg-topic-cleared", &[("username", &username)])
                } else {
                    t_args(
                        "msg-topic-set",
                        &[("username", &username), ("topic", &topic)],
                    )
                };
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: msg_username_info(),
                        message,
                        timestamp: Local::now(),
                    },
                )
            }
            ServerMessage::ChatMessage {
                session_id: _,
                username,
                message,
            } => self.add_chat_message(
                connection_id,
                ChatMessage {
                    username,
                    message,
                    timestamp: Local::now(),
                },
            ),
            ServerMessage::ServerBroadcast {
                session_id: _,
                username,
                message,
            } => self.add_chat_message(
                connection_id,
                ChatMessage {
                    username: format!("{} {}", msg_username_broadcast_prefix(), username),
                    message,
                    timestamp: Local::now(),
                },
            ),
            ServerMessage::UserBroadcastResponse { success, error } => {
                if success {
                    // Close broadcast panel on success
                    self.ui_state.show_broadcast = false;
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.broadcast_error = None;
                    }
                    Task::none()
                } else {
                    // On error, keep panel open and show error in form
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.broadcast_error = Some(error.unwrap_or_default());
                    }
                    Task::none()
                }
            }
            ServerMessage::UserConnected { user } => {
                // Check if user already exists (multi-device connection)
                let is_new_user = !conn
                    .online_users
                    .iter()
                    .any(|u| u.username == user.username);

                if let Some(existing_user) = conn
                    .online_users
                    .iter_mut()
                    .find(|u| u.username == user.username)
                {
                    // User already exists - merge session_ids
                    for session_id in &user.session_ids {
                        if !existing_user.session_ids.contains(session_id) {
                            existing_user.session_ids.push(*session_id);
                        }
                    }
                } else {
                    // New user - add to list
                    conn.online_users.push(UserInfo {
                        username: user.username.clone(),
                        is_admin: user.is_admin,
                        session_ids: user.session_ids.clone(),
                    });
                    // Sort to maintain alphabetical order
                    sort_user_list(&mut conn.online_users);
                }

                // Only announce if this is their first session (new user)
                if is_new_user {
                    self.add_chat_message(
                        connection_id,
                        ChatMessage {
                            username: msg_username_system(),
                            message: t_args("msg-user-connected", &[("username", &user.username)]),
                            timestamp: Local::now(),
                        },
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserDisconnected {
                session_id,
                username,
            } => {
                // Remove the specific session_id from the user's sessions
                let mut is_last_session = false;
                if let Some(user) = conn
                    .online_users
                    .iter_mut()
                    .find(|u| u.username == username)
                {
                    user.session_ids.retain(|&sid| sid != session_id);

                    // If user has no more sessions, remove them entirely
                    if user.session_ids.is_empty() {
                        conn.online_users.retain(|u| u.username != username);
                        is_last_session = true;

                        // Clear expanded_user if the disconnected user was expanded
                        if conn.expanded_user.as_ref() == Some(&username) {
                            conn.expanded_user = None;
                        }
                    }
                }

                // Only announce if this was their last session (fully offline)
                if is_last_session {
                    self.add_chat_message(
                        connection_id,
                        ChatMessage {
                            username: msg_username_system(),
                            message: t_args("msg-user-disconnected", &[("username", &username)]),
                            timestamp: Local::now(),
                        },
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserListResponse {
                success,
                error: _,
                users,
            } => {
                if !success {
                    return Task::none();
                }
                conn.online_users = users
                    .unwrap_or_default()
                    .into_iter()
                    .map(|u| UserInfo {
                        username: u.username,
                        is_admin: u.is_admin,
                        session_ids: u.session_ids,
                    })
                    .collect();
                // Sort to maintain alphabetical order
                sort_user_list(&mut conn.online_users);

                // Clear expanded_user if the user is no longer in the list
                if let Some(expanded) = &conn.expanded_user
                    && !conn.online_users.iter().any(|u| &u.username == expanded)
                {
                    conn.expanded_user = None;
                }
                Task::none()
            }
            ServerMessage::UserUpdated {
                previous_username,
                user,
            } => {
                // Update the user's info in the online_users list
                // Use previous_username to find the user (in case username changed)
                if let Some(existing_user) = conn
                    .online_users
                    .iter_mut()
                    .find(|u| u.username == previous_username)
                {
                    existing_user.username = user.username.clone();
                    existing_user.is_admin = user.is_admin;
                    existing_user.session_ids = user.session_ids;

                    // Re-sort the list since username may have changed
                    sort_user_list(&mut conn.online_users);
                }

                // If username changed, update user_messages HashMap and active tab
                if previous_username != user.username {
                    // If this is our own username changing, update conn.username
                    if conn.username == previous_username {
                        conn.username = user.username.clone();
                    }

                    // Rename the user_messages entry
                    if let Some(messages) = conn.user_messages.remove(&previous_username) {
                        conn.user_messages.insert(user.username.clone(), messages);
                    }

                    // Update unread_tabs if present
                    let old_tab = ChatTab::UserMessage(previous_username.clone());
                    if conn.unread_tabs.remove(&old_tab) {
                        conn.unread_tabs
                            .insert(ChatTab::UserMessage(user.username.clone()));
                    }

                    // Update active_chat_tab if it's for this user
                    if conn.active_chat_tab == old_tab {
                        conn.active_chat_tab = ChatTab::UserMessage(user.username.clone());
                    }

                    // Update expanded_user if it was set to the old username
                    if conn.expanded_user.as_ref() == Some(&previous_username) {
                        conn.expanded_user = Some(user.username.clone());
                    }
                }

                Task::none()
            }
            ServerMessage::UserInfoResponse {
                success,
                error,
                user,
            } => {
                if !success {
                    let err = error.unwrap_or_default();
                    let message = ChatMessage {
                        username: msg_username_info(),
                        message: t_args("user-info-error", &[("error", &err)]),
                        timestamp: Local::now(),
                    };
                    self.add_chat_message(connection_id, message)
                } else if let Some(user) = user {
                    // Calculate session duration
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    let session_duration_secs = now.saturating_sub(user.login_time) as u64;
                    let duration_str = Self::format_duration(session_duration_secs);

                    // Format account creation time (ISO 8601)
                    let created = chrono::DateTime::from_timestamp(user.created_at, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| t("user-info-unknown"));

                    // Build multi-line IRC WHOIS-style output
                    let mut lines = Vec::new();

                    // Header
                    lines.push(t_args("user-info-header", &[("username", &user.username)]));

                    // Admin status (only visible to admins)
                    if let Some(is_admin) = user.is_admin
                        && is_admin
                    {
                        lines.push(format!("  {}", t("user-info-is-admin")));
                    }

                    // Sessions
                    let session_count = user.session_ids.len();
                    if session_count == 1 {
                        lines.push(format!(
                            "  {}",
                            t_args("user-info-connected-ago", &[("duration", &duration_str)])
                        ));
                    } else {
                        lines.push(format!(
                            "  {}",
                            t_args(
                                "user-info-connected-sessions",
                                &[
                                    ("duration", &duration_str),
                                    ("count", &session_count.to_string())
                                ]
                            )
                        ));
                    }

                    // Features
                    if !user.features.is_empty() {
                        lines.push(format!(
                            "  {}",
                            t_args(
                                "user-info-features",
                                &[("features", &user.features.join(", "))]
                            )
                        ));
                    }

                    // Locale
                    lines.push(format!(
                        "  {}",
                        t_args("user-info-locale", &[("locale", &user.locale)])
                    ));

                    // IP Addresses (only visible to admins)
                    if let Some(addresses) = user.addresses
                        && !addresses.is_empty()
                    {
                        if addresses.len() == 1 {
                            lines.push(format!(
                                "  {}",
                                t_args("user-info-address", &[("address", &addresses[0])])
                            ));
                        } else {
                            lines.push(format!("  {}", t("user-info-addresses")));
                            for addr in &addresses {
                                lines.push(format!(
                                    "  {}",
                                    t_args("user-info-address-item", &[("address", addr)])
                                ));
                            }
                        }
                    }

                    // Account created (last field)
                    lines.push(format!(
                        "  {}",
                        t_args("user-info-created", &[("created", &created)])
                    ));

                    lines.push(format!("  {}", t("user-info-end")));

                    // Add each line as a separate chat message
                    let timestamp = Local::now();
                    let mut task = Task::none();
                    for line in lines {
                        task = self.add_chat_message(
                            connection_id,
                            ChatMessage {
                                username: msg_username_info(),
                                message: line,
                                timestamp,
                            },
                        );
                    }
                    // Last add_chat_message will handle auto-scroll
                    task
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserKickResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: msg_username_system(),
                        message: t("msg-user-kicked-success"),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: msg_username_error(),
                        message: t_args(
                            "err-failed-send-message",
                            &[("error", &error.unwrap_or_default())],
                        ),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::UserMessage {
                from_username,
                to_username,
                message,
            } => {
                // Route PM to the appropriate tab
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    // Determine which user we're chatting with (the other person)
                    let other_user = if from_username == conn.username {
                        // We sent this message, so we're chatting with to_username
                        to_username.clone()
                    } else {
                        // We received this message, so we're chatting with from_username
                        from_username.clone()
                    };

                    // Create PM tab entry if it doesn't exist
                    if !conn.user_messages.contains_key(&other_user) {
                        conn.user_messages.insert(other_user.clone(), Vec::new());
                    }

                    // Add message to PM tab history
                    let chat_msg = ChatMessage {
                        username: from_username.clone(),
                        message: message.clone(),
                        timestamp: Local::now(),
                    };

                    if let Some(messages) = conn.user_messages.get_mut(&other_user) {
                        messages.push(chat_msg);
                    }

                    // Mark as unread if not currently viewing this tab
                    let pm_tab = ChatTab::UserMessage(other_user.clone());
                    if conn.active_chat_tab != pm_tab {
                        conn.unread_tabs.insert(pm_tab);
                    }

                    // Auto-scroll if viewing this tab and at bottom
                    if conn.active_chat_tab == ChatTab::UserMessage(other_user.clone())
                        && conn.chat_auto_scroll
                    {
                        return scrollable::snap_to(
                            ScrollableId::ChatMessages.into(),
                            scrollable::RelativeOffset::END,
                        );
                    }
                }
                Task::none()
            }
            ServerMessage::UserMessageResponse { success, error } => {
                // Only show error messages - success is obvious from the PM tab
                if !success {
                    let message = ChatMessage {
                        username: msg_username_error(),
                        message: t_args(
                            "err-failed-send-message",
                            &[("error", &error.unwrap_or_default())],
                        ),
                        timestamp: Local::now(),
                    };
                    self.add_chat_message(connection_id, message)
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserCreateResponse { success, error } => {
                if success {
                    // Close add user panel on success
                    if self.ui_state.show_add_user && self.active_connection == Some(connection_id)
                    {
                        self.ui_state.show_add_user = false;
                        if let Some(conn) = self.connections.get_mut(&connection_id) {
                            conn.user_management.clear_add_user();
                        }
                    }

                    self.add_chat_message(
                        connection_id,
                        ChatMessage {
                            username: msg_username_system(),
                            message: t("msg-user-created"),
                            timestamp: Local::now(),
                        },
                    )
                } else {
                    // On error, keep panel open and show error in form
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.create_error = Some(error.unwrap_or_default());
                    }
                    Task::none()
                }
            }
            ServerMessage::UserDeleteResponse { success, error } => {
                if success {
                    // Close edit panel on success
                    if self.ui_state.show_edit_user && self.active_connection == Some(connection_id)
                    {
                        self.ui_state.show_edit_user = false;
                        if let Some(conn) = self.connections.get_mut(&connection_id) {
                            conn.user_management.clear_edit_user();
                        }
                    }

                    self.add_chat_message(
                        connection_id,
                        ChatMessage {
                            username: msg_username_system(),
                            message: t("msg-user-deleted"),
                            timestamp: Local::now(),
                        },
                    )
                } else {
                    // On error, keep panel open and show error in form
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.edit_error = Some(error.unwrap_or_default());
                    }
                    Task::none()
                }
            }
            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                is_admin,
                enabled,
                permissions,
            } => {
                if success {
                    // Load the user details into edit form (stage 2)
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.load_user_for_editing(
                            username.unwrap_or_default(),
                            is_admin.unwrap_or(false),
                            enabled.unwrap_or(true),
                            permissions.unwrap_or_default(),
                        );
                    }
                    Task::none()
                } else {
                    // On error, keep panel open and show error in form
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.edit_error =
                            Some(error.unwrap_or_else(|| t("error-unknown")));
                    }
                    Task::none()
                }
            }
            ServerMessage::UserUpdateResponse { success, error } => {
                if success {
                    // Close edit panel on success
                    if self.ui_state.show_edit_user && self.active_connection == Some(connection_id)
                    {
                        self.ui_state.show_edit_user = false;
                        if let Some(conn) = self.connections.get_mut(&connection_id) {
                            conn.user_management.clear_edit_user();
                        }
                    }

                    self.add_chat_message(
                        connection_id,
                        ChatMessage {
                            username: msg_username_system(),
                            message: t("msg-user-updated"),
                            timestamp: Local::now(),
                        },
                    )
                } else {
                    // On error, keep panel open and show error in form
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.edit_error = Some(error.unwrap_or_default());
                    }
                    Task::none()
                }
            }
            ServerMessage::PermissionsUpdated {
                is_admin,
                permissions,
            } => {
                // Update the connection's permissions and admin status
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    let had_user_list =
                        conn.is_admin || conn.permissions.contains(&"user_list".to_string());

                    conn.is_admin = is_admin;
                    conn.permissions = permissions.clone();

                    let has_user_list = is_admin || permissions.contains(&"user_list".to_string());

                    // If user just gained user_list permission, refresh the list
                    // (it may be stale from missed join/leave events while permission was revoked)
                    if !had_user_list
                        && has_user_list
                        && let Err(e) = conn.tx.send(ClientMessage::UserList)
                    {
                        // Channel send failed - add error to chat
                        let error_msg = format!("{}: {}", t("err-userlist-failed"), e);
                        return self.add_chat_message(
                            connection_id,
                            ChatMessage {
                                username: msg_username_error(),
                                message: error_msg,
                                timestamp: Local::now(),
                            },
                        );
                    }

                    // Show notification message
                    let message = ChatMessage {
                        username: msg_username_system(),
                        message: t("msg-permissions-updated"),
                        timestamp: Local::now(),
                    };
                    return self.add_chat_message(connection_id, message);
                }
                Task::none()
            }
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: msg_username_system(),
                        message: t("msg-topic-updated"),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: msg_username_error(),
                        message: t_args(
                            "err-failed-update-topic",
                            &[("error", &error.unwrap_or_default())],
                        ),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::Error { message, command } => {
                // Show error in edit user form if it's for user management commands
                if let Some(ref cmd) = command
                    && (cmd == "UserEdit" || cmd == "UserUpdate")
                    && self.ui_state.show_edit_user
                    && self.active_connection == Some(connection_id)
                {
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.edit_error = Some(message);
                    }
                    return Task::none();
                }

                // For other errors (including UserDelete), show in chat
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: msg_username_error(),
                        message,
                        timestamp: Local::now(),
                    },
                )
            }
            _ => Task::none(),
        }
    }

    /// Format session duration in human-readable form
    fn format_duration(seconds: u64) -> String {
        if seconds < 60 {
            format!("{}s", seconds)
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
        }
    }

    /// Verify certificate fingerprint matches stored value, or save on first connection (TOFU)
    fn verify_and_save_fingerprint(
        &mut self,
        bookmark_index: Option<usize>,
        fingerprint: &str,
    ) -> Result<(), Box<crate::types::FingerprintMismatch>> {
        if let Some(idx) = bookmark_index {
            match &self.config.bookmarks[idx].certificate_fingerprint {
                None => {
                    // First connection - save fingerprint (Trust On First Use)
                    self.config.bookmarks[idx].certificate_fingerprint =
                        Some(fingerprint.to_string());
                    let _ = self.config.save();
                    Ok(())
                }
                Some(stored) => {
                    // Verify fingerprint matches
                    if stored == fingerprint {
                        Ok(())
                    } else {
                        // Note: connection and display_name will be filled in by caller
                        Err(Box::new(crate::types::FingerprintMismatch {
                            bookmark_index: idx,
                            expected: stored.clone(),
                            received: fingerprint.to_string(),
                            bookmark_name: self.config.bookmarks[idx].name.clone(),
                            server_address: self.config.bookmarks[idx].address.clone(),
                            server_port: self.config.bookmarks[idx].port.clone(),
                            connection: crate::types::NetworkConnection {
                                tx: tokio::sync::mpsc::unbounded_channel().0,
                                session_id: 0,
                                connection_id: 0,
                                shutdown: None,
                                is_admin: false,
                                permissions: Vec::new(),
                                chat_topic: None,
                                certificate_fingerprint: String::new(),
                                locale: String::new(),
                            },
                            display_name: String::new(),
                        }))
                    }
                }
            }
        } else {
            // No bookmark - nothing to verify
            Ok(())
        }
    }

    /// Handle fingerprint mismatch by queuing it for user verification
    fn handle_fingerprint_mismatch(
        &mut self,
        boxed_mismatch: Box<crate::types::FingerprintMismatch>,
        conn: crate::types::NetworkConnection,
        display_name: String,
    ) -> Task<Message> {
        let crate::types::FingerprintMismatch {
            bookmark_index,
            expected,
            received,
            bookmark_name,
            server_address,
            server_port,
            ..
        } = *boxed_mismatch;

        self.fingerprint_mismatch_queue
            .push_back(crate::types::FingerprintMismatch {
                bookmark_index,
                expected,
                received,
                bookmark_name,
                server_address,
                server_port,
                connection: conn,
                display_name,
            });

        self.ui_state.show_fingerprint_mismatch = true;
        self.connection_form.is_connecting = false;
        Task::none()
    }
}
