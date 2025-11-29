//! Network events

use crate::NexusApp;
use crate::types::{
    ChatMessage, ChatTab, InputId, Message, ScrollableId, ServerConnection, UserInfo,
};
use chrono::Local;
use iced::Task;
use iced::widget::{scrollable, text_input};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::collections::{HashMap, HashSet};

// Message username constants (pub for use in views)
pub(crate) const MSG_USERNAME_SYSTEM: &str = "System";
pub(crate) const MSG_USERNAME_ERROR: &str = "Error";
pub(crate) const MSG_USERNAME_INFO: &str = "Info";
pub(crate) const MSG_USERNAME_BROADCAST_PREFIX: &str = "[BROADCAST]";

// Success message constants
const MSG_USER_KICKED_SUCCESS: &str = "User kicked successfully";
const MSG_USER_MESSAGE_FAILED: &str = "Failed to send message";
const MSG_BROADCAST_SENT: &str = "Broadcast sent successfully";
const MSG_USER_CREATED: &str = "User created successfully";
const MSG_USER_DELETED: &str = "User deleted successfully";
const MSG_USER_UPDATED: &str = "User updated successfully";
const MSG_PERMISSIONS_UPDATED: &str = "Your permissions have been updated";
const MSG_TOPIC_UPDATED: &str = "Topic updated successfully";

// Error message constants
const ERR_CONNECTION_BROKEN: &str = "Connection error";
const ERR_USER_KICK_FAILED: &str = "Failed to kick user";
const ERR_NO_SHUTDOWN_HANDLE: &str = "Connection error: No shutdown handle";
const ERR_USERLIST_FAILED: &str = "Failed to refresh user list";

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

                let session_id = conn.session_id.parse().unwrap_or(0);
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
                            self.connection_form.error = Some(ERR_NO_SHUTDOWN_HANDLE.to_string());
                            return Task::none();
                        }
                    },
                    message_input: String::new(),
                    broadcast_message: String::new(),
                    user_management: crate::types::UserManagementState::default(),
                };

                // Add to connections and make it active
                self.connections.insert(connection_id, server_conn);
                self.active_connection = Some(connection_id);

                // Request initial user list (only if user has permission)
                if should_request_userlist && let Err(e) = conn_tx.send(ClientMessage::UserList) {
                    // Channel send failed - connection is broken, show error
                    self.connection_form.error = Some(format!("{}: {}", ERR_CONNECTION_BROKEN, e));
                    // Remove the connection we just added since it's broken
                    self.connections.remove(&connection_id);
                    self.active_connection = None;
                    return Task::none();
                }

                // Add chat topic message if present
                self.add_topic_message_if_present(connection_id, chat_topic);

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
                let session_id = conn.session_id.parse().unwrap_or(0);
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
                }

                // For auto-connect failures, we could show a system message
                // but for now we just fail silently
                // TODO: Consider showing failed auto-connect attempts in a notification area
                eprintln!("Bookmark connection failed: {}", error);

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
                self.connection_form.error = Some(format!("Disconnected: {}", error));
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

            if self.active_connection == Some(connection_id) {
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
                    username: MSG_USERNAME_INFO.to_string(),
                    message: format!("Topic: {}", topic),
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
                    format!("Topic cleared by {}", username)
                } else {
                    format!("Topic set by {}: {}", username, topic)
                };
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: MSG_USERNAME_INFO.to_string(),
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
                    username: format!("{} {}", MSG_USERNAME_BROADCAST_PREFIX, username),
                    message,
                    timestamp: Local::now(),
                },
            ),
            ServerMessage::UserBroadcastReply { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_BROADCAST_SENT.to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!("Failed to send broadcast: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
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
                            username: MSG_USERNAME_SYSTEM.to_string(),
                            message: format!("{} connected", user.username),
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
                    }
                }

                // Only announce if this was their last session (fully offline)
                if is_last_session {
                    self.add_chat_message(
                        connection_id,
                        ChatMessage {
                            username: MSG_USERNAME_SYSTEM.to_string(),
                            message: format!("{} disconnected", username),
                            timestamp: Local::now(),
                        },
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserListResponse { users } => {
                conn.online_users = users
                    .into_iter()
                    .map(|u| UserInfo {
                        username: u.username,
                        is_admin: u.is_admin,
                        session_ids: u.session_ids,
                    })
                    .collect();
                // Sort to maintain alphabetical order
                sort_user_list(&mut conn.online_users);
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
                }

                Task::none()
            }
            ServerMessage::UserInfoResponse { user, error } => {
                if let Some(err) = error {
                    let message = ChatMessage {
                        username: MSG_USERNAME_INFO.to_string(),
                        message: format!("Error: {}", err),
                        timestamp: Local::now(),
                    };
                    self.add_chat_message(connection_id, message)
                } else if let Some(user) = user {
                    // Calculate session duration
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let session_duration_secs = now.saturating_sub(user.login_time);
                    let duration_str = Self::format_duration(session_duration_secs);

                    // Format account creation time
                    let created = chrono::DateTime::from_timestamp(user.created_at, 0)
                        .map(|dt| dt.format("%b %d %Y %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Unknown".to_string());

                    // Build multi-line IRC WHOIS-style output
                    let mut lines = Vec::new();

                    // Header
                    lines.push(format!("[{}]", user.username));

                    // Admin status (only visible to admins)
                    if let Some(is_admin) = user.is_admin
                        && is_admin
                    {
                        lines.push("  is an Administrator".to_string());
                    }

                    // Sessions
                    let session_count = user.session_ids.len();
                    if session_count == 1 {
                        lines.push(format!("  connected: {} ago", duration_str));
                    } else {
                        lines.push(format!(
                            "  connected: {} ago ({} sessions)",
                            duration_str, session_count
                        ));
                    }

                    // Features
                    if !user.features.is_empty() {
                        lines.push(format!("  features: {}", user.features.join(", ")));
                    }

                    // Locale
                    lines.push(format!("  locale: {}", user.locale));

                    // IP Addresses (only visible to admins)
                    if let Some(addresses) = user.addresses
                        && !addresses.is_empty()
                    {
                        if addresses.len() == 1 {
                            lines.push(format!("  address: {}", addresses[0]));
                        } else {
                            lines.push("  addresses:".to_string());
                            for addr in &addresses {
                                lines.push(format!("    - {}", addr));
                            }
                        }
                    }

                    // Account created (last field)
                    lines.push(format!("  created: {}", created));

                    lines.push("  End of user info".to_string());

                    // Add each line as a separate chat message
                    let timestamp = Local::now();
                    let mut task = Task::none();
                    for line in lines {
                        task = self.add_chat_message(
                            connection_id,
                            ChatMessage {
                                username: MSG_USERNAME_INFO.to_string(),
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
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_USER_KICKED_SUCCESS.to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!("{}: {}", ERR_USER_KICK_FAILED, error.unwrap_or_default()),
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

                    // Auto-scroll if viewing this tab
                    if conn.active_chat_tab == ChatTab::UserMessage(other_user.clone()) {
                        return scrollable::snap_to(
                            ScrollableId::ChatMessages.into(),
                            scrollable::RelativeOffset::END,
                        );
                    }
                }
                Task::none()
            }
            ServerMessage::UserMessageReply { success, error } => {
                // Only show error messages - success is obvious from the PM tab
                if !success {
                    let message = ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!(
                            "{}: {}",
                            MSG_USER_MESSAGE_FAILED,
                            error.unwrap_or_default()
                        ),
                        timestamp: Local::now(),
                    };
                    self.add_chat_message(connection_id, message)
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserCreateResponse { success, error } => {
                // Close add user panel on any response (success or error)
                if self.ui_state.show_add_user && self.active_connection == Some(connection_id) {
                    self.ui_state.show_add_user = false;
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.clear_add_user();
                    }
                }

                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_USER_CREATED.to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!("Failed to create user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::UserDeleteResponse { success, error } => {
                // Close edit panel on any response (success or error)
                if self.ui_state.show_edit_user && self.active_connection == Some(connection_id) {
                    self.ui_state.show_edit_user = false;
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.clear_edit_user();
                    }
                }

                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_USER_DELETED.to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!("Failed to delete user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::UserEditResponse {
                username,
                is_admin,
                enabled,
                permissions,
            } => {
                // Load the user details into edit form (stage 2)
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.load_user_for_editing(
                        username,
                        is_admin,
                        enabled,
                        permissions,
                    );
                }
                Task::none()
            }
            ServerMessage::UserUpdateResponse { success, error } => {
                // Close edit panel on any response (success or error)
                if self.ui_state.show_edit_user && self.active_connection == Some(connection_id) {
                    self.ui_state.show_edit_user = false;
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.clear_edit_user();
                    }
                }

                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_USER_UPDATED.to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!("Failed to update user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
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
                        let error_msg = format!("{}: {}", ERR_USERLIST_FAILED, e);
                        return self.add_chat_message(
                            connection_id,
                            ChatMessage {
                                username: MSG_USERNAME_ERROR.to_string(),
                                message: error_msg,
                                timestamp: Local::now(),
                            },
                        );
                    }

                    // Show notification message
                    let message = ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_PERMISSIONS_UPDATED.to_string(),
                        timestamp: Local::now(),
                    };
                    return self.add_chat_message(connection_id, message);
                }
                Task::none()
            }
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: MSG_TOPIC_UPDATED.to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
                        message: format!("Failed to update topic: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::Error { message, command } => {
                // Close edit user panel if the error is for user management commands
                // and it's for the active connection
                if let Some(cmd) = command
                    && (cmd == "UserEdit" || cmd == "UserUpdate" || cmd == "UserDelete")
                    && self.ui_state.show_edit_user
                    && self.active_connection == Some(connection_id)
                {
                    self.ui_state.show_edit_user = false;
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        conn.user_management.clear_edit_user();
                    }
                }

                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: MSG_USERNAME_ERROR.to_string(),
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
                                session_id: String::new(),
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
