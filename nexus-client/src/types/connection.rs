//! Server connection types

use crate::types::UserManagementState;
use nexus_common::protocol::ClientMessage;
use tokio::sync::mpsc;

use super::{ChatMessage, UserInfo};

/// Type alias for shutdown handle
type ShutdownHandle = std::sync::Arc<tokio::sync::Mutex<Option<crate::network::ShutdownHandle>>>;

/// Active connection to a server
///
/// Contains connection state, chat history, user list, and UI state.
#[derive(Debug, Clone)]
pub struct ServerConnection {
    /// Bookmark index or None for ad-hoc connections
    pub bookmark_index: Option<usize>,
    /// Session ID assigned by server
    #[allow(dead_code)]
    pub session_id: u32,
    /// Authenticated username
    #[allow(dead_code)]
    pub username: String,
    /// Display name (bookmark name or address:port)
    pub display_name: String,
    /// Unique connection identifier
    pub connection_id: usize,
    /// Whether user is admin on this server
    pub is_admin: bool,
    /// User's permissions on this server
    pub permissions: Vec<String>,
    /// Chat message history
    pub chat_messages: Vec<ChatMessage>,
    /// Currently online users
    pub online_users: Vec<UserInfo>,
    /// Username of expanded user in user list (None if no user expanded)
    pub expanded_user: Option<String>,
    /// Channel for sending commands to server
    pub tx: mpsc::UnboundedSender<ClientMessage>,
    /// Handle for graceful shutdown
    pub shutdown_handle: ShutdownHandle,
    /// Current chat message input
    pub message_input: String,
    /// Current broadcast message input
    pub broadcast_message: String,
    /// User management panel state
    pub user_management: UserManagementState,
}

/// Network connection handle returned by connect_to_server()
#[derive(Debug, Clone)]
pub struct NetworkConnection {
    /// Channel for sending messages to server
    pub tx: mpsc::UnboundedSender<nexus_common::protocol::ClientMessage>,
    /// Session ID from server
    pub session_id: String,
    /// Unique connection identifier
    pub connection_id: usize,
    /// Optional shutdown handle
    pub shutdown: Option<ShutdownHandle>,
    /// Whether user is admin
    pub is_admin: bool,
    /// User's permissions
    pub permissions: Vec<String>,
    /// Chat topic received on login (if user has ChatTopic permission)
    pub chat_topic: Option<String>,
}
