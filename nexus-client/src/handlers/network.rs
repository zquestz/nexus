//! Network events

use crate::NexusApp;
use crate::types::{ChatMessage, InputId, Message, ServerConnection, UserInfo};
use chrono::Local;
use iced::Task;
use iced::widget::{scrollable, text_input};
use nexus_common::protocol::{ClientMessage, ServerMessage};

impl NexusApp {
    // Network event handlers
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

                // Request initial user list (only if user has permission)
                if conn.is_admin || conn.permissions.contains(&"user_list".to_string()) {
                    let _ = conn.tx.send(ClientMessage::UserList);
                }

                // Create server connection
                let server_conn = ServerConnection {
                    bookmark_index,
                    session_id,
                    username,
                    display_name,
                    chat_messages: Vec::new(),
                    online_users: Vec::new(),
                    tx: conn.tx,
                    shutdown_handle: conn.shutdown.unwrap(),
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

    /// Helper method to add a chat message and auto-scroll if this is the active connection
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
                    username: format!("[BROADCAST] {}", username),
                    message,
                    timestamp: Local::now(),
                },
            ),
            ServerMessage::UserBroadcastReply { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: "System".to_string(),
                        message: "Broadcast sent successfully".to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: "Error".to_string(),
                        message: format!("Failed to send broadcast: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::UserConnected { user } => {
                conn.online_users.push(UserInfo {
                    session_id: user.session_id,
                    username: user.username.clone(),
                });
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: "System".to_string(),
                        message: format!("{} connected", user.username),
                        timestamp: Local::now(),
                    },
                )
            }
            ServerMessage::UserDisconnected {
                session_id,
                username,
            } => {
                conn.online_users.retain(|u| u.session_id != session_id);
                self.add_chat_message(
                    connection_id,
                    ChatMessage {
                        username: "System".to_string(),
                        message: format!("{} disconnected", username),
                        timestamp: Local::now(),
                    },
                )
            }
            ServerMessage::UserListResponse { users } => {
                conn.online_users = users
                    .into_iter()
                    .map(|u| UserInfo {
                        session_id: u.session_id,
                        username: u.username,
                    })
                    .collect();
                Task::none()
            }
            ServerMessage::UserInfoResponse { user, error } => {
                let message = if let Some(err) = error {
                    ChatMessage {
                        username: "Info".to_string(),
                        message: format!("Error: {}", err),
                        timestamp: Local::now(),
                    }
                } else if let Some(user) = user {
                    ChatMessage {
                        username: "Info".to_string(),
                        message: format!(
                            "User {} (session {}): created {}, features: {:?}",
                            user.username, user.session_id, user.created_at, user.features
                        ),
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
                        username: "System".to_string(),
                        message: "User created successfully".to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: "Error".to_string(),
                        message: format!("Failed to create user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::UserDeleteResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: "System".to_string(),
                        message: "User deleted successfully".to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: "Error".to_string(),
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
                    conn.user_management.load_user_for_editing(username, is_admin, permissions);
                }
                Task::none()
            }
            ServerMessage::UserUpdateResponse { success, error } => {
                let message = if success {
                    ChatMessage {
                        username: "System".to_string(),
                        message: "User updated successfully".to_string(),
                        timestamp: Local::now(),
                    }
                } else {
                    ChatMessage {
                        username: "Error".to_string(),
                        message: format!("Failed to update user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    }
                };
                self.add_chat_message(connection_id, message)
            }
            ServerMessage::Error { message, .. } => self.add_chat_message(
                connection_id,
                ChatMessage {
                    username: "Error".to_string(),
                    message,
                    timestamp: Local::now(),
                },
            ),
            _ => Task::none(),
        }
    }
}
