//! Server connection types

use crate::types::UserManagementState;
use nexus_common::protocol::ClientMessage;
use tokio::sync::mpsc;

use super::{ChatMessage, UserInfo};

/// Connection to a server
///
/// Represents an active connection with its associated state and UI.
/// Each connection maintains its own chat history, user list, and form state.
#[derive(Debug, Clone)]
pub struct ServerConnection {
    /// Index into bookmarks list, None for ad-hoc connections
    pub bookmark_index: Option<usize>,

    /// Session ID assigned by server (parsed from string to u32)
    #[allow(dead_code)]
    pub session_id: u32,

    /// Authenticated username for this connection
    #[allow(dead_code)]
    pub username: String,

    /// Display name (bookmark name or "address:port")
    pub display_name: String,

    /// Chat message history for this connection
    pub chat_messages: Vec<ChatMessage>,

    /// Currently online users on this server
    pub online_users: Vec<UserInfo>,

    /// Channel for sending commands to server
    pub tx: mpsc::UnboundedSender<ClientMessage>,

    /// Handle for graceful connection shutdown
    pub shutdown_handle: std::sync::Arc<tokio::sync::Mutex<Option<crate::network::ShutdownHandle>>>,

    /// Unique connection identifier
    pub connection_id: usize,

    // === Per-Connection UI State ===
    /// Current message being typed in chat input
    pub message_input: String,

    /// Current message being typed in broadcast input
    pub broadcast_message: String,

    /// Admin panel state (create/delete user forms)
    pub user_management: UserManagementState,
}

/// Network connection handle
///
/// Contains channels and identifiers for communicating with a connected server.
/// Returned by `connect_to_server()` and used to send messages.
#[derive(Debug, Clone)]
pub struct NetworkConnection {
    pub tx: mpsc::UnboundedSender<nexus_common::protocol::ClientMessage>,
    pub session_id: String,
    pub connection_id: usize,
    pub shutdown:
        Option<std::sync::Arc<tokio::sync::Mutex<Option<crate::network::ShutdownHandle>>>>,
}
