//! Chat and user display types

use chrono::{DateTime, Local};
use nexus_common::protocol::ChatAction;

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

/// Type of chat message (prevents nickname spoofing)
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
    /// Display name of the sender (nickname)
    ///
    /// For chat messages, this is the sender's nickname (display name).
    /// For broadcast messages, this is also the nickname (since shared
    /// accounts cannot broadcast, nickname always equals username for broadcasters).
    /// For system/error/info messages, this is an empty string.
    pub nickname: String,
    /// Message text
    pub message: String,
    /// Type of message (determines rendering style)
    pub message_type: MessageType,
    /// When the message was received (defaults to now if not specified)
    pub timestamp: Option<DateTime<Local>>,
    /// Whether the sender is an admin (for nickname coloring)
    pub is_admin: bool,
    /// Whether the sender is a shared account user (for muted coloring)
    pub is_shared: bool,
    /// Action type for chat messages (Normal or Me)
    pub action: ChatAction,
}

impl ChatMessage {
    /// Create a new chat message with a specific timestamp, admin status, shared status, and action
    pub fn with_timestamp_and_status(
        nickname: impl Into<String>,
        message: impl Into<String>,
        timestamp: DateTime<Local>,
        is_admin: bool,
        is_shared: bool,
        action: ChatAction,
    ) -> Self {
        Self {
            nickname: nickname.into(),
            message: message.into(),
            message_type: MessageType::Chat,
            timestamp: Some(timestamp),
            is_admin,
            is_shared,
            action,
        }
    }

    /// Create a system message
    pub fn system(message: impl Into<String>) -> Self {
        Self {
            nickname: String::new(),
            message: message.into(),
            message_type: MessageType::System,
            timestamp: None,
            is_admin: false,
            is_shared: false,
            action: ChatAction::Normal,
        }
    }

    /// Create an error message
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            nickname: String::new(),
            message: message.into(),
            message_type: MessageType::Error,
            timestamp: None,
            is_admin: false,
            is_shared: false,
            action: ChatAction::Normal,
        }
    }

    /// Create an info message
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            nickname: String::new(),
            message: message.into(),
            message_type: MessageType::Info,
            timestamp: None,
            is_admin: false,
            is_shared: false,
            action: ChatAction::Normal,
        }
    }

    /// Create an info message with a specific timestamp
    pub fn info_with_timestamp(message: impl Into<String>, timestamp: DateTime<Local>) -> Self {
        Self {
            nickname: String::new(),
            message: message.into(),
            message_type: MessageType::Info,
            timestamp: Some(timestamp),
            is_admin: false,
            is_shared: false,
            action: ChatAction::Normal,
        }
    }

    /// Create a broadcast message from a user
    ///
    /// Takes `username` from the protocol. Since shared accounts cannot broadcast,
    /// the sender's username always equals their nickname, so we store it in the
    /// nickname field for display.
    pub fn broadcast(username: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            nickname: username.into(),
            message: message.into(),
            message_type: MessageType::Broadcast,
            timestamp: None,
            is_admin: false,
            is_shared: false,
            action: ChatAction::Normal,
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
    /// Username (account name / database identifier)
    pub username: String,
    /// Display name (what users see and type)
    /// For regular accounts: nickname == username
    /// For shared accounts: nickname is session-specific
    pub nickname: String,
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
