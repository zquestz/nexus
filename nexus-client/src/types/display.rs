//! Chat and user display types

use chrono::{DateTime, Local};

/// Chat tab type - represents different chat windows
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChatTab {
    /// Server chat (main channel)
    Server,
    /// User message conversation (1-on-1)
    UserMessage(String),
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
}

impl ChatMessage {
    /// Create a new chat message from a user
    pub fn new(username: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            message: message.into(),
            message_type: MessageType::Chat,
            timestamp: None,
        }
    }

    /// Create a new chat message with a specific timestamp
    pub fn with_timestamp(
        username: impl Into<String>,
        message: impl Into<String>,
        timestamp: DateTime<Local>,
    ) -> Self {
        Self {
            username: username.into(),
            message: message.into(),
            message_type: MessageType::Chat,
            timestamp: Some(timestamp),
        }
    }

    /// Create a system message
    pub fn system(message: impl Into<String>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::System,
            timestamp: None,
        }
    }

    /// Create an error message
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::Error,
            timestamp: None,
        }
    }

    /// Create an info message
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::Info,
            timestamp: None,
        }
    }

    /// Create an info message with a specific timestamp
    pub fn info_with_timestamp(message: impl Into<String>, timestamp: DateTime<Local>) -> Self {
        Self {
            username: String::new(),
            message: message.into(),
            message_type: MessageType::Info,
            timestamp: Some(timestamp),
        }
    }

    /// Create a broadcast message from a user
    pub fn broadcast(username: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            message: message.into(),
            message_type: MessageType::Broadcast,
            timestamp: None,
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
    /// Username
    pub username: String,
    /// Whether user is admin
    pub is_admin: bool,
    /// All active session IDs for this user
    pub session_ids: Vec<u32>,
}
