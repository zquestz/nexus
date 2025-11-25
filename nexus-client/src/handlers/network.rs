//! Network events

use crate::NexusApp;
use crate::types::{ChatMessage, InputId, Message, ServerConnection, UserInfo};
use chrono::Local;
use iced::Task;
use iced::widget::{scrollable, text_input};
use nexus_common::protocol::{ClientMessage, ServerMessage};

// Message username constants
const MSG_USERNAME_SYSTEM: &str = "System";
const MSG_USERNAME_ERROR: &str = "Error";
const MSG_USERNAME_INFO: &str = "Info";
const MSG_USERNAME_BROADCAST_PREFIX: &str = "[BROADCAST]";

// Error message constants
const ERR_CONNECTION_BROKEN: &str = "Connection error";
const ERR_NO_SHUTDOWN_HANDLE: &str = "Connection error: No shutdown handle";
const ERR_USERLIST_FAILED: &str = "Failed to refresh user list";

impl NexusApp {
    /// Handle connection attempt result (success or failure)
    pub fn handle_connection_result(
        &mut self,
        result: Result<crate::types::NetworkConnection, String>,
    ) -> Task<Message> {
        match result {
            Ok(conn) => {
                self.connection_form.error = None;

                // Find if this connection matches a bookmark
                let bookmark_index = self.config.bookmarks.iter().position(|b| {
                    b.address == self.connection_form.server_address
                        && b.port == self.connection_form.port
                        && b.username == self.connection_form.username
                });

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

                // Create server connection
                let server_conn = ServerConnection {
                    bookmark_index,
                    session_id,
                    username,
                    display_name,
                    chat_messages: Vec::new(),
                    online_users: Vec::new(),
                    tx: conn.tx,
                    shutdown_handle: match conn.shutdown {
                        Some(handle) => handle,
                        None => {
                            self.connection_form.error = Some(ERR_NO_SHUTDOWN_HANDLE.to_string());
                            return Task::none();
                        }
                    },
                    connection_id,
                    message_input: String::new(),
                    broadcast_message: String::new(),
                    user_management: crate::types::UserManagementState::default(),
                    is_admin: conn.is_admin,
                    permissions: conn.permissions,
                };

                // Add to connections and make it active
                self.connections.insert(connection_id, server_conn);
                self.active_connection = Some(connection_id);

                // Request initial user list (only if user has permission)
                if should_request_userlist {
                    if let Err(e) = conn_tx.send(ClientMessage::UserList) {
                        // Channel send failed - connection is broken, show error
                        self.connection_form.error =
                            Some(format!("{}: {}", ERR_CONNECTION_BROKEN, e));
                        // Remove the connection we just added since it's broken
                        self.connections.remove(&connection_id);
                        self.active_connection = None;
                        return Task::none();
                    }
                }

                // Clear connection form
                self.connection_form.clear();

                // Focus chat input
                text_input::focus(text_input::Id::from(InputId::ChatInput))
            }
            Err(error) => {
                self.connection_form.error = Some(error);
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
            if self.active_connection == Some(connection_id) {
                return scrollable::snap_to(
                    crate::types::ScrollableId::ChatMessages.into(),
                    scrollable::RelativeOffset::END,
                );
            }
        }
        Task::none()
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
                        message: "Broadcast sent successfully".to_string(),
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
                if let Some(existing_user) = conn.online_users.iter_mut().find(|u| u.username == user.username) {
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
                    conn.online_users.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
                }
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: format!("{} connected", user.username),
                        timestamp: Local::now(),
                    },
                )
            }
            ServerMessage::UserDisconnected {
                session_id,
                username,
            } => {
                // Remove the specific session_id from the user's sessions
                if let Some(user) = conn.online_users.iter_mut().find(|u| u.username == username) {
                    user.session_ids.retain(|&sid| sid != session_id);
                    
                    // If user has no more sessions, remove them entirely
                    if user.session_ids.is_empty() {
                        conn.online_users.retain(|u| u.username != username);
                    }
                }
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: format!("{} disconnected", username),
                        timestamp: Local::now(),
                    },
                )
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
                Task::none()
            }
            ServerMessage::UserInfoResponse { user, error } => {
                let message = if let Some(err) = error {
                    ChatMessage {
                        username: MSG_USERNAME_INFO.to_string(),
                        message: format!("Error: {}", err),
                        timestamp: Local::now(),
                    }
                } else if let Some(user) = user {
                    // Calculate session duration
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let session_duration_secs = now.saturating_sub(user.login_time);
                    let duration_str = Self::format_duration(session_duration_secs);

                    // Build info message
                    let mut info = String::new();

                    // Start with username
                    info.push_str(&user.username);

                    // Add admin status if present (only visible to admins)
                    if let Some(is_admin) = user.is_admin {
                        if is_admin {
                            info.push_str(" • Admin");
                        }
                    }

                    // Add session count
                    let session_count = user.session_ids.len();
                    if session_count > 1 {
                        info.push_str(&format!(" • Sessions: {}", session_count));
                    }

                    // Add online duration
                    info.push_str(&format!(" • Online: {}", duration_str));

                    // Add features if any
                    if !user.features.is_empty() {
                        info.push_str(&format!(" • Features: {}", user.features.join(", ")));
                    }

                    // Add addresses if present (only visible to admins)
                    if let Some(addresses) = user.addresses {
                        if !addresses.is_empty() {
                            info.push_str(&format!(" • IPs: {}", addresses.join(", ")));
                        }
                    }

                    ChatMessage {
                        username: MSG_USERNAME_INFO.to_string(),
                        message: info,
                        timestamp: Local::now(),
                    }
                } else {
                    return Task::none();
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::UserCreateResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: "User created successfully".to_string(),
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
                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: "User deleted successfully".to_string(),
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
                permissions,
            } => {
                // Load the user details into edit form (stage 2)
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management
                        .load_user_for_editing(username, is_admin, permissions);
                }
                Task::none()
            }
            ServerMessage::UserUpdateResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: "User updated successfully".to_string(),
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
                    if !had_user_list && has_user_list {
                        if let Err(e) = conn.tx.send(ClientMessage::UserList) {
                            // Channel send failed - add error to chat
                            let error_msg = format!("{}: {}", ERR_USERLIST_FAILED, e);
                            let chat_msg = ChatMessage {
                                username: MSG_USERNAME_ERROR.to_string(),
                                message: error_msg,
                                timestamp: Local::now(),
                            };
                            conn.chat_messages.push(chat_msg);
                        }
                    }

                    // Notify user in chat
                    let message = ChatMessage {
                        username: MSG_USERNAME_SYSTEM.to_string(),
                        message: "Your permissions have been updated".to_string(),
                        timestamp: Local::now(),
                    };
                    return self.add_chat_message(connection_id, message);
                }
                Task::none()
            }
            ServerMessage::Error { message, .. } => self.add_chat_message(
                connection_id,
                ChatMessage {
                    username: MSG_USERNAME_ERROR.to_string(),
                    message,
                    timestamp: Local::now(),
                },
            ),
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
}
