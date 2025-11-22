//! Nexus BBS Client - GUI Application

use iced::futures::{SinkExt, Stream};
use iced::stream;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Column};
use iced::{Center, Element, Fill, Subscription, Task, Theme};
use nexus_common::io::send_client_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::PROTOCOL_VERSION;
use std::net::Ipv6Addr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};

pub fn main() -> iced::Result {
    iced::application("Nexus BBS", NexusApp::update, NexusApp::view)
        .theme(|_| Theme::Dark)
        .subscription(NexusApp::subscription)
        .run_with(NexusApp::new)
}

#[derive(Debug, Clone)]
enum Message {
    // Connection screen
    ServerAddressChanged(String),
    PortChanged(String),
    UsernameChanged(String),
    PasswordChanged(String),
    ConnectPressed,

    // Main app
    MessageInputChanged(String),
    SendMessagePressed,
    RequestUserList,
    RequestUserInfo(u32),
    Disconnect,

    // Network events
    ConnectionResult(Result<NetworkConnection, String>),
    ServerMessageReceived(ServerMessage),
    NetworkError(String),
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    session_id: u32,
    username: String,
    message: String,
}

#[derive(Debug, Clone)]
struct UserInfo {
    session_id: u32,
    username: String,
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

    // Connected state
    session_id: Option<u32>,
    chat_messages: Vec<ChatMessage>,
    online_users: Vec<UserInfo>,
    message_input: String,

    // Network channels
    tx: Option<mpsc::UnboundedSender<ClientMessage>>,
    
    // Connection ID for subscription
    connection_id: Option<usize>,
}

// Network connection handle
#[derive(Debug, Clone)]
struct NetworkConnection {
    tx: mpsc::UnboundedSender<ClientMessage>,
    session_id: String,
    connection_id: usize,
}

// Global registry for network receivers
static NETWORK_RECEIVERS: once_cell::sync::Lazy<Arc<Mutex<std::collections::HashMap<usize, mpsc::UnboundedReceiver<ServerMessage>>>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashMap::new())));

static NEXT_CONNECTION_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Default for NexusApp {
    fn default() -> Self {
        Self {
            connection_state: ConnectionState::Disconnected,
            server_address: String::new(),
            port: String::from("7500"),
            username: String::new(),
            password: String::new(),
            connection_error: None,
            session_id: None,
            chat_messages: Vec::new(),
            online_users: Vec::new(),
            message_input: String::new(),
            tx: None,
            connection_id: None,
        }
    }
}

impl NexusApp {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Connection screen events
            Message::ServerAddressChanged(addr) => {
                self.server_address = addr;
                Task::none()
            }
            Message::PortChanged(port) => {
                self.port = port;
                Task::none()
            }
            Message::UsernameChanged(username) => {
                self.username = username;
                Task::none()
            }
            Message::PasswordChanged(password) => {
                self.password = password;
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
                        connect_to_server(server_address, port, username, password).await
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
                self.connection_state = ConnectionState::Disconnected;
                self.tx = None;
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
        if let Some(connection_id) = self.connection_id {
            Subscription::run_with_id(
                connection_id,
                network_stream(connection_id),
            )
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<Message> {
        match self.connection_state {
            ConnectionState::Disconnected | ConnectionState::Connecting => {
                self.connection_screen()
            }
            ConnectionState::Connected => self.main_screen(),
        }
    }

    fn connection_screen(&self) -> Element<Message> {
        let title = text("Nexus BBS Client")
            .size(32)
            .width(Fill)
            .align_x(Center);

        let server_input = text_input("Server IPv6 Address", &self.server_address)
            .on_input(Message::ServerAddressChanged)
            .padding(10)
            .size(16);

        let port_input = text_input("Port", &self.port)
            .on_input(Message::PortChanged)
            .padding(10)
            .size(16);

        let username_input = text_input("Username", &self.username)
            .on_input(Message::UsernameChanged)
            .padding(10)
            .size(16);

        let password_input = text_input("Password", &self.password)
            .on_input(Message::PasswordChanged)
            .secure(true)
            .padding(10)
            .size(16);

        let connect_button = if self.connection_state == ConnectionState::Connecting {
            button(text("Connecting...")).padding(10)
        } else {
            button(text("Connect"))
                .on_press(Message::ConnectPressed)
                .padding(10)
        };

        let mut content = column![
            title,
            text("").size(20),
            server_input,
            port_input,
            username_input,
            password_input,
            text("").size(10),
            connect_button,
        ]
        .spacing(10)
        .padding(20)
        .max_width(400);

        if let Some(error) = &self.connection_error {
            content = content.push(text("").size(10));
            content = content.push(text(error).size(14).color([1.0, 0.0, 0.0]));
        }

        container(content)
            .width(Fill)
            .height(Fill)
            .padding(20)
            .center(Fill)
            .into()
    }

    fn main_screen(&self) -> Element<Message> {
        // Left side: user list
        let user_list_title = text("Online Users").size(20);
        let mut user_list = Column::new().spacing(5).padding(10);
        for user in &self.online_users {
            user_list = user_list.push(
                button(text(format!("{} ({})", user.username, user.session_id)))
                    .on_press(Message::RequestUserInfo(user.session_id))
                    .width(Fill),
            );
        }
        let user_list_panel = column![
            user_list_title,
            scrollable(user_list).height(Fill),
            button(text("Refresh Users"))
                .on_press(Message::RequestUserList)
                .padding(10)
                .width(Fill),
        ]
        .spacing(10)
        .padding(10)
        .width(250);

        // Right side: chat
        let chat_title = text("Chat - #server").size(20);

        let mut chat_messages = Column::new().spacing(5).padding(10);
        for msg in &self.chat_messages {
            let display = if msg.username == "System" {
                text(format!("*** {}", msg.message)).color([0.7, 0.7, 0.7])
            } else if msg.username == "Error" {
                text(format!("Error: {}", msg.message)).color([1.0, 0.0, 0.0])
            } else if msg.username == "Info" {
                text(format!("ℹ {}", msg.message)).color([0.5, 0.8, 1.0])
            } else {
                text(format!("[{}] {}: {}", msg.session_id, msg.username, msg.message))
            };
            chat_messages = chat_messages.push(display);
        }

        let message_input = text_input("Type a message...", &self.message_input)
            .on_input(Message::MessageInputChanged)
            .on_submit(Message::SendMessagePressed)
            .padding(10)
            .size(16);

        let send_button = button(text("Send"))
            .on_press(Message::SendMessagePressed)
            .padding(10);

        let chat_panel = column![
            chat_title,
            scrollable(chat_messages).height(Fill),
            row![message_input, send_button,].spacing(10),
        ]
        .spacing(10)
        .padding(10);

        // Top bar with disconnect button
        let top_bar = row![
            text(format!(
                "Connected as {} (session {})",
                self.username,
                self.session_id.unwrap_or(0)
            ))
            .size(16),
            button(text("Disconnect"))
                .on_press(Message::Disconnect)
                .padding(10),
        ]
        .spacing(20)
        .padding(10);

        let main_content = row![user_list_panel, chat_panel].spacing(10);

        container(column![top_bar, main_content])
            .width(Fill)
            .height(Fill)
            .into()
    }
}

// Async function to connect to server and perform handshake/login
async fn connect_to_server(
    server_address: String,
    port: String,
    username: String,
    password: String,
) -> Result<NetworkConnection, String> {
    // Parse address and port
    let addr: Ipv6Addr = server_address
        .parse()
        .map_err(|_| "Invalid IPv6 address".to_string())?;
    let port: u16 = port
        .parse()
        .map_err(|_| "Invalid port number".to_string())?;

    // Connect to server
    let socket_addr = std::net::SocketAddr::from((addr, port));
    let stream = TcpStream::connect(socket_addr)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send handshake
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(&mut writer, &handshake)
        .await
        .map_err(|e| format!("Failed to send handshake: {}", e))?;

    // Wait for handshake response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read handshake response: {}", e))?;

    match serde_json::from_str::<ServerMessage>(&line.trim()) {
        Ok(ServerMessage::HandshakeResponse {
            success,
            version: _,
            error,
        }) => {
            if !success {
                return Err(format!(
                    "Handshake failed: {}",
                    error.unwrap_or_default()
                ));
            }
        }
        _ => return Err("Unexpected handshake response".to_string()),
    }

    // Send login
    let login = ClientMessage::Login {
        username: username.clone(),
        password: password.clone(),
        features: vec!["chat".to_string()],
    };
    send_client_message(&mut writer, &login)
        .await
        .map_err(|e| format!("Failed to send login: {}", e))?;

    // Wait for login response
    line.clear();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read login response: {}", e))?;

    let session_id = match serde_json::from_str::<ServerMessage>(&line.trim()) {
        Ok(ServerMessage::LoginResponse {
            success,
            session_id,
            error,
        }) => {
            if !success {
                return Err(format!("Login failed: {}", error.unwrap_or_default()));
            }
            session_id.ok_or_else(|| "No session ID received".to_string())?
        }
        _ => return Err("Unexpected login response".to_string()),
    };

    // Create channels for bidirectional communication
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Spawn task to handle bidirectional communication
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            tokio::select! {
                // Read from server
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => break, // Connection closed
                        Ok(_) => {
                            // Parse and send message to UI
                            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(line.trim()) {
                                if msg_tx.send(server_msg).is_err() {
                                    break; // UI closed
                                }
                            }
                            line.clear();
                        }
                        Err(_) => break,
                    }
                }
                // Send to server
                Some(msg) = cmd_rx.recv() => {
                    if send_client_message(&mut writer, &msg).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Generate unique connection ID and store receiver
    let connection_id = NEXT_CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    {
        let mut receivers = NETWORK_RECEIVERS.lock().await;
        receivers.insert(connection_id, msg_rx);
    }

    Ok(NetworkConnection {
        tx: cmd_tx,
        session_id: session_id.to_string(),
        connection_id,
    })
}

// Stream that reads messages from the network receiver
fn network_stream(connection_id: usize) -> impl Stream<Item = Message> {
    stream::channel(100, move |mut output| async move {
        // Get the receiver from the registry
        let mut rx = {
            let mut receivers = NETWORK_RECEIVERS.lock().await;
            receivers.remove(&connection_id)
        };

        if let Some(ref mut receiver) = rx {
            while let Some(msg) = receiver.recv().await {
                let _ = output.send(Message::ServerMessageReceived(msg)).await;
            }
        }

        // Connection closed
        let _ = output.send(Message::NetworkError("Connection closed".to_string())).await;
        
        // Keep the stream alive but do nothing
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    })
}