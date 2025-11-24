//! Connection and chat message handlers

use crate::types::{InputId, Message};
use crate::{NexusApp, network};
use iced::Task;
use iced::widget::text_input;
use nexus_common::protocol::ClientMessage;

impl NexusApp {
    // Connection form field update handlers
    pub fn handle_server_name_changed(&mut self, name: String) -> Task<Message> {
        self.connection_form.server_name = name;
        self.focused_field = InputId::ServerName;
        Task::none()
    }

    pub fn handle_server_address_changed(&mut self, addr: String) -> Task<Message> {
        self.connection_form.server_address = addr;
        self.focused_field = InputId::ServerAddress;
        Task::none()
    }

    pub fn handle_port_changed(&mut self, port: String) -> Task<Message> {
        self.connection_form.port = port;
        self.focused_field = InputId::Port;
        Task::none()
    }

    pub fn handle_username_changed(&mut self, username: String) -> Task<Message> {
        self.connection_form.username = username;
        self.focused_field = InputId::Username;
        Task::none()
    }

    pub fn handle_password_changed(&mut self, password: String) -> Task<Message> {
        self.connection_form.password = password;
        self.focused_field = InputId::Password;
        Task::none()
    }

    // Connection and chat handlers
    pub fn handle_connect_pressed(&mut self) -> Task<Message> {
        self.connection_form.error = None;

        let server_address = self.connection_form.server_address.clone();
        let port_str = self.connection_form.port.clone();
        let username = self.connection_form.username.clone();
        let password = self.connection_form.password.clone();
        let connection_id = self.next_connection_id;
        self.next_connection_id += 1;

        Task::perform(
            async move {
                // Parse port to u16
                let port: u16 = match port_str.parse() {
                    Ok(p) => p,
                    Err(_) => return Err(format!("Invalid port number '{}'", port_str)),
                };

                network::connect_to_server(
                    server_address.clone(),
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

    pub fn handle_message_input_changed(&mut self, input: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.message_input = input;
            }
        }
        Task::none()
    }

    pub fn handle_send_message_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if !conn.message_input.trim().is_empty() {
                    let msg = ClientMessage::ChatSend {
                        message: conn.message_input.clone(),
                    };
                    let _ = conn.tx.send(msg);
                    conn.message_input.clear();
                }
            }
        }
        Task::none()
    }

    pub fn handle_request_user_info(&mut self, session_id: u32) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get(&conn_id) {
                let _ = conn.tx.send(ClientMessage::UserInfo { session_id });
            }
        }
        Task::none()
    }

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

    pub fn handle_switch_to_connection(&mut self, connection_id: usize) -> Task<Message> {
        if self.connections.contains_key(&connection_id) {
            self.active_connection = Some(connection_id);
            // Focus chat input when switching to a connection
            return text_input::focus(text_input::Id::from(InputId::ChatInput));
        }
        Task::none()
    }
}
