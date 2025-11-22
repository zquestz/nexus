//! Type definitions for the Nexus client

use iced::widget::{scrollable, text_input};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use tokio::sync::mpsc;

/// Messages that drive the application
#[derive(Debug, Clone)]
pub enum Message {
    // Connection screen
    ServerNameChanged(String),
    ServerAddressChanged(String),
    PortChanged(String),
    UsernameChanged(String),
    PasswordChanged(String),
    ConnectPressed,

    // Server bookmarks

    ShowAddBookmark,
    ShowEditBookmark(usize),
    CancelBookmarkEdit,
    BookmarkNameChanged(String),
    BookmarkAddressChanged(String),
    BookmarkPortChanged(String),
    BookmarkUsernameChanged(String),
    BookmarkPasswordChanged(String),
    SaveBookmark,
    DeleteBookmark(usize),

    // Main app
    MessageInputChanged(String),
    SendMessagePressed,
    RequestUserInfo(u32),
    DisconnectFromServer(usize), // Disconnect from specific server by connection_id
    
    // Server switching
    SwitchToConnection(usize), // Switch view to connection by connection_id
    ConnectToBookmark(usize), // Connect to a bookmark by bookmark index

    // Admin panel
    AdminUsernameChanged(String),
    AdminPasswordChanged(String),
    AdminIsAdminToggled(bool),
    AdminPermissionToggled(String, bool),
    CreateUserPressed,
    DeleteUserPressed(String),
    DeleteUsernameChanged(String),

    // UI toggles
    ToggleBookmarks,
    ToggleUserlist,
    ToggleAddUser,
    ToggleDeleteUser,

    // Keyboard events
    TabPressed,
    Event(iced::Event),

    // Network events
    ConnectionResult(Result<NetworkConnection, String>),
    ServerMessageReceived(usize, ServerMessage), // (connection_id, message)
    NetworkError(usize, String), // (connection_id, error)
}

/// Connection state
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

/// Connection to a server
#[derive(Debug, Clone)]
pub struct ServerConnection {
    pub bookmark_index: Option<usize>, // None for ad-hoc connections
    pub session_id: u32,
    pub username: String,
    pub display_name: String, // Bookmark name or "address:port"
    pub chat_messages: Vec<ChatMessage>,
    pub online_users: Vec<UserInfo>,
    pub tx: mpsc::UnboundedSender<ClientMessage>,
    pub shutdown_handle: std::sync::Arc<tokio::sync::Mutex<Option<crate::network::ShutdownHandle>>>,
    pub connection_id: usize,
}

/// Server bookmark
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerBookmark {
    pub name: String,
    pub address: String,
    pub port: String,
    pub username: String, // Optional, can be empty
    pub password: String, // Optional, can be empty
}

impl ServerBookmark {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            name: String::new(),
            address: String::new(),
            port: String::from("7500"),
            username: String::new(),
            password: String::new(),
        }
    }
}

/// Bookmark editing state
#[derive(Debug, Clone, PartialEq)]
pub enum BookmarkEditMode {
    None,
    Add,
    Edit(usize), // editing bookmark at index
}

/// Chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    #[allow(dead_code)]
    pub session_id: u32,
    pub username: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
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
    pub shutdown: Option<std::sync::Arc<tokio::sync::Mutex<Option<crate::network::ShutdownHandle>>>>,
}

/// Text input IDs for focus management
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputId {
    ServerName,
    ServerAddress,
    Port,
    Username,
    Password,
    BookmarkName,
    BookmarkAddress,
    BookmarkPort,
    BookmarkUsername,
    BookmarkPassword,
    AdminUsername,
    AdminPassword,
    DeleteUsername,
}

impl From<InputId> for text_input::Id {
    fn from(id: InputId) -> Self {
        text_input::Id::new(format!("{:?}", id))
    }
}

/// Scrollable area IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollableId {
    ChatMessages,
}

impl From<ScrollableId> for scrollable::Id {
    fn from(id: ScrollableId) -> Self {
        scrollable::Id::new(format!("{:?}", id))
    }
}
