//! Type definitions for the Nexus client

use iced::widget::text_input;
use nexus_common::protocol::ServerMessage;
use tokio::sync::mpsc;

/// Messages that drive the application
#[derive(Debug, Clone)]
pub enum Message {
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

    // Keyboard events
    TabPressed,
    Event(iced::Event),

    // Network events
    ConnectionResult(Result<NetworkConnection, String>),
    ServerMessageReceived(ServerMessage),
    NetworkError(String),
}

/// Connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

/// Chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub session_id: u32,
    pub username: String,
    pub message: String,
}

/// User information for display
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub session_id: u32,
    pub username: String,
}

/// Network connection handle
#[derive(Debug, Clone)]
pub struct NetworkConnection {
    pub tx: mpsc::UnboundedSender<nexus_common::protocol::ClientMessage>,
    pub session_id: String,
    pub connection_id: usize,
}

/// Text input IDs for focus management
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputId {
    ServerAddress,
    Port,
    Username,
    Password,
}

impl From<InputId> for text_input::Id {
    fn from(id: InputId) -> Self {
        text_input::Id::new(format!("{:?}", id))
    }
}
