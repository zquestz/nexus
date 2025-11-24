//! Chat and user display types

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
    /// Session ID assigned by server
    pub session_id: u32,
    /// Username
    pub username: String,
    /// Whether user is admin
    pub is_admin: bool,
}
