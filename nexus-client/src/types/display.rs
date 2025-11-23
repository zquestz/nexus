//! Chat and user display types

/// Chat message for display
///
/// Represents a single message in the chat area with timestamp.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub username: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
}

/// User information for display in the user list
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub session_id: u32,
    pub username: String,
}
