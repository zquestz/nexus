//! Chat and user display types

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
    /// When the message was received
    pub timestamp: chrono::DateTime<chrono::Local>,
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
