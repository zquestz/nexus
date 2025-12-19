//! Chat and user display types

use chrono::{DateTime, Local};

/// Chat tab type - represents different chat windows
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum ChatTab {
    /// Server chat (main channel)
    #[default]
    Server,
    /// User message conversation (1-on-1)
    UserMessage(String),
}

/// Scroll state for a chat tab
#[derive(Debug, Clone, Copy)]
pub struct ScrollState {
    /// Saved scroll position (relative offset 0.0-1.0)
    pub offset: f32,
    /// Whether to auto-scroll when new messages arrive
    pub auto_scroll: bool,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 1.0,       // Start at bottom
            auto_scroll: true, // Auto-scroll by default
        }
    }
}

/// Type of chat message (prevents username spoofing)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageType {
    /// Regular chat message from a user
    #[default]
    Chat,
    /// System message (user connect/disconnect, etc.)
    System,
    /// Error message
    Error,
    /// Info message (command responses, user info)
    Info,
    /// Broadcast message from a user
    Broadcast,
}

/// Chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Username of the sender (for Chat/Broadcast types)
    pub username: String,
    /// Message text
    pub message: String,
    /// Type of message (determines rendering style)
    pub message_type: MessageType,
    /// When the message was received (defaults to now if not specified)
    pub timestamp: Option<DateTime<Local>>,
    /// Whether the sender is an admin (for username coloring)
    pub is_admin: bool,
    /// Whether the sender is a shared account user (for muted coloring)
    pub is_shared: bool,
}

impl ChatMessage {
    /// Create a new chat message with a specific timestamp, admin status, and shared status
    pub fn with_timestamp_and_status(
        username: impl Into<String>,
        message: impl Into<String>,
        timestamp: DateTime<Local>,
        is_admin: bool,
        is_shared: bool,
    ) -> Self {
        Self {
            username: username.into(),
            message: message.into(),
            message_type: MessageType::Chat,
            timestamp: Some(timestamp),
            is_admin,
            is_shared,
        }
    }

    /// Create a system message
    pub fn system(message: impl Into<String>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::System,
            timestamp: None,
            is_admin: false,
            is_shared: false,
        }
    }

    /// Create an error message
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::Error,
            timestamp: None,
            is_admin: false,
            is_shared: false,
        }
    }

    /// Create an info message
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::Info,
            timestamp: None,
            is_admin: false,
            is_shared: false,
        }
    }

    /// Create an info message with a specific timestamp
    pub fn info_with_timestamp(message: impl Into<String>, timestamp: DateTime<Local>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::Info,
            timestamp: Some(timestamp),
            is_admin: false,
            is_shared: false,
        }
    }

    /// Create a broadcast message from a user
    pub fn broadcast(username: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            message: message.into(),
            message_type: MessageType::Broadcast,
            timestamp: None,
            is_admin: false,
            is_shared: false,
        }
    }

    /// Get the timestamp, using current time if not set
    pub fn get_timestamp(&self) -> DateTime<Local> {
        self.timestamp.unwrap_or_else(Local::now)
    }
}

/// User information for display
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// Username (account name)
    pub username: String,
    /// Nickname for shared account users (None for regular users)
    pub nickname: Option<String>,
    /// Whether user is admin
    pub is_admin: bool,
    /// Whether this is a shared account user
    pub is_shared: bool,
    /// All active session IDs for this user
    pub session_ids: Vec<u32>,
    /// SHA-256 hash of the avatar data URI for change detection (None = no avatar/identicon)
    ///
    /// We store a 32-byte hash instead of the full data URI (up to 176KB) to save memory.
    /// The actual decoded avatar is stored in `ServerConnection.avatar_cache`.
    pub avatar_hash: Option<[u8; 32]>,
}

impl UserInfo {
    /// Returns the display name for this user (nickname for shared, username for regular)
    pub fn display_name(&self) -> &str {
        self.nickname.as_deref().unwrap_or(&self.username)
    }
}
