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

/// Chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Username of the sender
    pub username: String,
    /// Message text
    pub message: String,
    /// When the message was received (defaults to now if not specified)
    pub timestamp: Option<DateTime<Local>>,
}

impl ChatMessage {
    /// Create a new chat message with current timestamp
    pub fn new(username: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            message: message.into(),
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
            timestamp: Some(timestamp),
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
