//! Connection and chat message handlers

use crate::types::{ChatMessage, InputId, Message, ScrollableId};
use crate::{network, NexusApp};
use chrono::Local;
use iced::widget::{scrollable, text_input};
use iced::Task;
use nexus_common::protocol::ClientMessage;

// Constants
const MAX_CHAT_LENGTH: usize = 1024;

// Error messages
const ERR_PORT_INVALID: &str = "Port must be a valid number (1-65535)";
const ERR_MESSAGE_TOO_LONG: &str = "Chat message too long";
const ERR_SEND_FAILED: &str = "Failed to send message";

impl NexusApp {
    /// Handle server name field change
    pub fn handle_server_name_changed(&mut self, name: String) -> Task<Message> {
        self.connection_form.server_name = name;
        self.connection_form.error = None;
        self.focused_field = InputId::ServerName;
        Task::none()
    }

    /// Handle server address field change
    pub fn handle_server_address_changed(&mut self, addr: String) -> Task<Message> {
        self.connection_form.server_address = addr;
        self.connection_form.error = None;
        self.focused_field = InputId::ServerAddress;
        Task::none()
    }

    /// Handle port field change
    pub fn handle_port_changed(&mut self, port: String) -> Task<Message> {
        self.connection_form.port = port;
        self.connection_form.error = None;
        self.focused_field = InputId::Port;
        Task::none()
    }

    /// Handle username field change
    pub fn handle_username_changed(&mut self, username: String) -> Task<Message> {
        self.connection_form.username = username;
        self.connection_form.error = None;
        self.focused_field = InputId::Username;
        Task::none()
    }

    /// Handle password field change
    pub fn handle_password_changed(&mut self, password: String) -> Task<Message> {
        self.connection_form.password = password;
        self.connection_form.error = None;
        self.focused_field = InputId::Password;
        Task::none()
    }

    /// Handle connect button press
    pub fn handle_connect_pressed(&mut self) -> Task<Message> {
        self.connection_form.error = None;

        // Validate port early
        let port: u16 = match self.connection_form.port.parse() {
            Ok(p) => p,
            Err(_) => {
                self.connection_form.error = Some(ERR_PORT_INVALID.to_string());
                return Task::none();
            }
        };

        let server_address = self.connection_form.server_address.clone();
        let username = self.connection_form.username.clone();
        let password = self.connection_form.password.clone();
        let connection_id = self.next_connection_id;
        self.next_connection_id += 1;

        Task::perform(
            async move {
                network::connect_to_server(
                    server_address,
                    port,
                    username,
                    password,
                    connection_id,
                )
                .await
            },
            Message::ConnectionResult,
        )
    }

    /// Handle chat message input change
    pub fn handle_message_input_changed(&mut self, input: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.message_input = input;
            }
        }
        self.focused_field = InputId::ChatInput;
        Task::none()
    }

    /// Handle send chat message button press
    pub fn handle_send_message_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get(&conn_id) {
                let message = conn.message_input.trim();

                // Validate message is not empty
                if message.is_empty() {
                    return Task::none();
                }

                // Validate message length
                if message.len() > MAX_CHAT_LENGTH {
                    let error_msg = format!(
                        "{} ({} characters, max {})",
                        ERR_MESSAGE_TOO_LONG,
                        message.len(),
                        MAX_CHAT_LENGTH
                    );
                    return self.add_chat_error(conn_id, error_msg);
                }

                let msg = ClientMessage::ChatSend {
                    message: message.to_string(),
                };

                // Send message and handle errors
                if let Err(e) = conn.tx.send(msg) {
                    let error_msg = format!("{}: {}", ERR_SEND_FAILED, e);
                    return self.add_chat_error(conn_id, error_msg);
                }

                // Clear message after successful send
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.message_input.clear();
                }
            }
        }
        Task::none()
    }

    /// Request detailed user information from server
    pub fn handle_request_user_info(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get(&conn_id) {
                if let Err(e) = conn.tx.send(ClientMessage::UserInfo { username }) {
                    let error_msg = format!("Failed to request user info: {}", e);
                    return self.add_chat_error(conn_id, error_msg);
                }
            }
        }
        Task::none()
    }

    /// Disconnect from a server and clean up resources
    pub fn handle_disconnect_from_server(&mut self, connection_id: usize) -> Task<Message> {
        if let Some(conn) = self.connections.remove(&connection_id) {
            // Signal the network task to shutdown
            let shutdown_arc = conn.shutdown_handle.clone();
            tokio::spawn(async move {
                let mut guard = shutdown_arc.lock().await;
                if let Some(shutdown) = guard.take() {
                    shutdown.shutdown();
                }
            });

            // Clean up the receiver from the global registry
            let conn_id = conn.connection_id;
            let registry = network::NETWORK_RECEIVERS.clone();
            tokio::spawn(async move {
                let mut receivers = registry.lock().await;
                receivers.remove(&conn_id);
            });

            // If this was the active connection, clear active
            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
            }
        }
        Task::none()
    }

    /// Switch active view to a different connection
    pub fn handle_switch_to_connection(&mut self, connection_id: usize) -> Task<Message> {
        if self.connections.contains_key(&connection_id) {
            self.active_connection = Some(connection_id);
            // Focus chat input when switching to a connection
            return text_input::focus(text_input::Id::from(InputId::ChatInput));
        }
        Task::none()
    }

    /// Add an error message to the chat and auto-scroll
    fn add_chat_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.chat_messages.push(ChatMessage {
                username: "Error".to_string(),
                message,
                timestamp: Local::now(),
            });

            // Auto-scroll if this is the active connection
            if self.active_connection == Some(connection_id) {
                return scrollable::snap_to(
                    ScrollableId::ChatMessages.into(),
                    scrollable::RelativeOffset::END,
                );
            }
        }
        Task::none()
    }
}
