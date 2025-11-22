//! Nexus BBS Client - GUI Application

mod network;
mod types;
mod views;

use iced::keyboard::{self, key};
use iced::widget::text_input;
use iced::{Element, Event, Subscription, Task, Theme};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use types::{ChatMessage, ConnectionState, InputId, Message, UserInfo};

pub fn main() -> iced::Result {
    iced::application("Nexus BBS", NexusApp::update, NexusApp::view)
        .theme(|_| Theme::Dark)
        .subscription(NexusApp::subscription)
        .run_with(NexusApp::new)
}

struct NexusApp {
    // Connection state
    connection_state: ConnectionState,

    // Connection screen inputs
    server_address: String,
    port: String,
    username: String,
    password: String,
    connection_error: Option<String>,
    focused_field: InputId,

    // Connected state
    session_id: Option<u32>,
    chat_messages: Vec<ChatMessage>,
    online_users: Vec<UserInfo>,
    message_input: String,

    // Network channels
    tx: Option<tokio::sync::mpsc::UnboundedSender<ClientMessage>>,
    shutdown_handle: Option<std::sync::Arc<tokio::sync::Mutex<Option<network::ShutdownHandle>>>>,

    // Connection ID for subscription
    connection_id: Option<usize>,
}

impl Default for NexusApp {
    fn default() -> Self {
        Self {
            connection_state: ConnectionState::Disconnected,
            server_address: String::new(),
            port: String::from("7500"),
            username: String::new(),
            password: String::new(),
            connection_error: None,
            focused_field: InputId::ServerAddress,
            session_id: None,
            chat_messages: Vec::new(),
            online_users: Vec::new(),
            message_input: String::new(),
            tx: None,
            shutdown_handle: None,
            connection_id: None,
        }
    }
}

impl NexusApp {
    fn new() -> (Self, Task<Message>) {
        (
            Self::default(),
            text_input::focus(text_input::Id::from(InputId::ServerAddress)),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Handle keyboard events for Tab navigation
            Message::TabPressed => {
                if self.connection_state != ConnectionState::Connected {
                    // On connection screen, cycle through fields
                    let next_field = match self.focused_field {
                        InputId::ServerAddress => InputId::Port,
                        InputId::Port => InputId::Username,
                        InputId::Username => InputId::Password,
                        InputId::Password => InputId::ServerAddress,
                    };
                    self.focused_field = next_field.clone();
                    return text_input::focus(text_input::Id::from(next_field));
                }
                Task::none()
            }

            Message::Event(event) => {
                if let Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Tab),
                    modifiers,
                    ..
                }) = event
                {
                    if !modifiers.shift() {
                        return self.update(Message::TabPressed);
                    }
                }
                Task::none()
            }

            // Connection screen
            // Connection screen events
            Message::ServerAddressChanged(addr) => {
                self.server_address = addr;
                self.focused_field = InputId::ServerAddress;
                Task::none()
            }
            Message::PortChanged(port) => {
                self.port = port;
                self.focused_field = InputId::Port;
                Task::none()
            }
            Message::UsernameChanged(username) => {
                self.username = username;
                self.focused_field = InputId::Username;
                Task::none()
            }
            Message::PasswordChanged(password) => {
                self.password = password;
                self.focused_field = InputId::Password;
                Task::none()
            }
            Message::ConnectPressed => {
                self.connection_state = ConnectionState::Connecting;
                self.connection_error = None;

                let server_address = self.server_address.clone();
                let port = self.port.clone();
                let username = self.username.clone();
                let password = self.password.clone();

                Task::perform(
                    async move {
                        network::connect_to_server(server_address, port, username, password).await
                    },
                    Message::ConnectionResult,
                )
            }

            // Main app events
            Message::MessageInputChanged(input) => {
                self.message_input = input;
                Task::none()
            }
            Message::SendMessagePressed => {
                if let Some(tx) = &self.tx {
                    if !self.message_input.trim().is_empty() {
                        let msg = ClientMessage::ChatSend {
                            message: self.message_input.clone(),
                        };
                        let _ = tx.send(msg);
                        self.message_input.clear();
                    }
                }
                Task::none()
            }
            Message::RequestUserList => {
                if let Some(tx) = &self.tx {
                    let _ = tx.send(ClientMessage::UserList);
                }
                Task::none()
            }
            Message::RequestUserInfo(session_id) => {
                if let Some(tx) = &self.tx {
                    let _ = tx.send(ClientMessage::UserInfo { session_id });
                }
                Task::none()
            }
            Message::Disconnect => {
                // Signal the network task to shutdown (drops TCP writer)
                if let Some(shutdown_arc) = self.shutdown_handle.take() {
                    let shutdown_arc_clone = shutdown_arc.clone();
                    tokio::spawn(async move {
                        let mut guard = shutdown_arc_clone.lock().await;
                        if let Some(shutdown) = guard.take() {
                            shutdown.shutdown();
                        }
                    });
                }
                
                // Drop the tx channel
                self.tx = None;
                
                // Clean up the receiver from the global registry
                if let Some(connection_id) = self.connection_id {
                    let registry = network::NETWORK_RECEIVERS.clone();
                    tokio::spawn(async move {
                        let mut receivers = registry.lock().await;
                        receivers.remove(&connection_id);
                    });
                }
                
                self.connection_state = ConnectionState::Disconnected;
                self.session_id = None;
                self.connection_id = None;
                self.chat_messages.clear();
                self.online_users.clear();
                Task::none()
            }

            // Network events
            Message::ConnectionResult(result) => match result {
                Ok(conn) => {
                    self.connection_state = ConnectionState::Connected;
                    self.session_id = Some(conn.session_id.parse().unwrap_or(0));
                    self.tx = Some(conn.tx.clone());
                    self.shutdown_handle = conn.shutdown.clone();
                    self.connection_id = Some(conn.connection_id);
                    self.connection_error = None;

                    // Request initial user list
                    let _ = conn.tx.send(ClientMessage::UserList);

                    Task::none()
                }
                Err(error) => {
                    self.connection_state = ConnectionState::Disconnected;
                    self.connection_error = Some(error);
                    Task::none()
                }
            },
            Message::ServerMessageReceived(msg) => {
                self.handle_server_message(msg);
                Task::none()
            }
            Message::NetworkError(error) => {
                self.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "Error".to_string(),
                    message: error,
                });
                Task::none()
            }
        }
    }

    fn handle_server_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::ChatMessage {
                session_id,
                username,
                message,
            } => {
                self.chat_messages.push(ChatMessage {
                    session_id,
                    username,
                    message,
                });
            }
            ServerMessage::UserConnected { user } => {
                self.online_users.push(UserInfo {
                    session_id: user.session_id,
                    username: user.username.clone(),
                });
                // Add system message
                self.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "System".to_string(),
                    message: format!("{} connected", user.username),
                });
            }
            ServerMessage::UserDisconnected {
                session_id,
                username,
            } => {
                self.online_users.retain(|u| u.session_id != session_id);
                // Add system message
                self.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "System".to_string(),
                    message: format!("{} disconnected", username),
                });
            }
            ServerMessage::UserListResponse { users } => {
                self.online_users = users
                    .into_iter()
                    .map(|u| UserInfo {
                        session_id: u.session_id,
                        username: u.username,
                    })
                    .collect();
            }
            ServerMessage::UserInfoResponse { user, error } => {
                if let Some(err) = error {
                    self.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Info".to_string(),
                        message: format!("Error: {}", err),
                    });
                } else if let Some(user) = user {
                    self.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Info".to_string(),
                        message: format!(
                            "User {} (session {}): created {}, features: {:?}",
                            user.username, user.session_id, user.created_at, user.features
                        ),
                    });
                }
            }
            ServerMessage::UserCreateResponse { success, error } => {
                if success {
                    self.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "System".to_string(),
                        message: "User created successfully".to_string(),
                    });
                } else {
                    self.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Error".to_string(),
                        message: format!("Failed to create user: {}", error.unwrap_or_default()),
                    });
                }
            }
            ServerMessage::Error { message, .. } => {
                self.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "Error".to_string(),
                    message,
                });
            }
            _ => {}
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let network_subscription = if let Some(connection_id) = self.connection_id {
            Subscription::run_with_id(connection_id, network::network_stream(connection_id))
        } else {
            Subscription::none()
        };

        let keyboard_subscription = iced::event::listen().map(Message::Event);

        Subscription::batch([network_subscription, keyboard_subscription])
    }

    fn view(&self) -> Element<Message> {
        match self.connection_state {
            ConnectionState::Disconnected | ConnectionState::Connecting => {
                views::connection_screen(
                    &self.connection_state,
                    &self.server_address,
                    &self.port,
                    &self.username,
                    &self.password,
                    &self.connection_error,
                )
            }
            ConnectionState::Connected => views::main_screen(
                &self.username,
                self.session_id,
                &self.chat_messages,
                &self.online_users,
                &self.message_input,
            ),
        }
    }
}
