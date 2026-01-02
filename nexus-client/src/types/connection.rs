//! Server connection types

use iced::widget::markdown;
use nexus_common::framing::MessageId;
use nexus_common::protocol::{ClientMessage, UserInfoDetailed};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

/// State for tab completion in chat input
#[derive(Debug, Clone)]
pub struct TabCompletionState {
    /// List of matching nicknames (sorted alphabetically)
    pub matches: Vec<String>,
    /// Current index in the matches list
    pub index: usize,
    /// Position in the input where the prefix starts (for truncation during cycling)
    pub start_pos: usize,
}

impl TabCompletionState {
    /// Create a new tab completion state
    pub fn new(matches: Vec<String>, start_pos: usize) -> Self {
        Self {
            matches,
            index: 0,
            start_pos,
        }
    }
}

use super::{
    ActivePanel, ChatMessage, ChatTab, FilesManagementState, NewsManagementState,
    PasswordChangeState, ResponseRouting, ScrollState, ServerInfoEditState, UserInfo,
    UserManagementState,
};
use crate::image::CachedImage;

/// Parameters for creating a new ServerConnection
pub struct ServerConnectionParams {
    /// Bookmark index or None for ad-hoc connections
    pub bookmark_index: Option<usize>,
    /// Session ID assigned by server
    pub session_id: u32,
    /// Authenticated username
    pub username: String,
    /// Display name (bookmark name or address:port)
    pub display_name: String,
    /// Unique connection identifier
    pub connection_id: usize,
    /// Whether user is admin on this server
    pub is_admin: bool,
    /// User's permissions on this server
    pub permissions: Vec<String>,
    /// Locale for this connection
    pub locale: String,
    /// Server name (from ServerInfo)
    pub server_name: Option<String>,
    /// Server description (from ServerInfo)
    pub server_description: Option<String>,
    /// Server version (from ServerInfo)
    pub server_version: Option<String>,
    /// Server image data URI
    pub server_image: String,
    /// Cached server image for display
    pub cached_server_image: Option<CachedImage>,
    /// Chat topic
    pub chat_topic: Option<String>,
    /// Who set the chat topic
    pub chat_topic_set_by: Option<String>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (admin only)
    pub max_transfers_per_ip: Option<u32>,
    /// Command sender channel
    pub tx: CommandSender,
    /// Shutdown handle for graceful disconnect
    pub shutdown_handle: WrappedShutdownHandle,
}

/// Type alias for the wrapped shutdown handle (Arc<Mutex<Option<...>>>)
type WrappedShutdownHandle =
    std::sync::Arc<tokio::sync::Mutex<Option<crate::network::ShutdownHandle>>>;

/// Type alias for the command channel sender (includes message ID)
pub type CommandSender = mpsc::UnboundedSender<(MessageId, ClientMessage)>;

/// Active connection to a server
///
/// Contains connection state, chat history, user list, and UI state.
#[derive(Debug, Clone)]
pub struct ServerConnection {
    /// Bookmark index or None for ad-hoc connections
    pub bookmark_index: Option<usize>,
    /// Session ID assigned by server (used to identify our user in online_users list)
    pub session_id: u32,
    /// Authenticated username (used for PM routing)
    pub username: String,
    /// Display name (bookmark name or address:port)
    pub display_name: String,
    /// Unique connection identifier
    pub connection_id: usize,
    /// Whether user is admin on this server
    pub is_admin: bool,
    /// User's permissions on this server
    pub permissions: Vec<String>,
    /// Locale for this connection (what the server accepted)
    ///
    /// Not yet used - waiting for translation infrastructure.
    /// Stored for future use when Fluent translations are implemented.
    #[allow(dead_code)]
    pub locale: String,
    /// Server name (from ServerInfo)
    pub server_name: Option<String>,
    /// Server description (from ServerInfo)
    pub server_description: Option<String>,
    /// Server version (from ServerInfo)
    pub server_version: Option<String>,
    /// Server image data URI (from ServerInfo, empty string if not set)
    pub server_image: String,
    /// Cached server image for rendering (decoded from server_image)
    pub cached_server_image: Option<CachedImage>,
    /// Current chat topic (None if no topic set)
    pub chat_topic: Option<String>,
    /// Username who set the current chat topic
    pub chat_topic_set_by: Option<String>,
    /// Max connections per IP (admin only, from ServerInfo)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (admin only, from ServerInfo)
    pub max_transfers_per_ip: Option<u32>,
    /// Active chat tab
    pub active_chat_tab: ChatTab,
    /// Chat message history for server chat
    pub chat_messages: Vec<ChatMessage>,
    /// User message history per user
    pub user_messages: HashMap<String, Vec<ChatMessage>>,
    /// Tabs with unread messages (for bold indicator)
    pub unread_tabs: HashSet<ChatTab>,
    /// Currently online users
    pub online_users: Vec<UserInfo>,
    /// Display name of expanded user in user list (None if no user expanded)
    /// For shared accounts this is the nickname, for regular accounts the username.
    pub expanded_user: Option<String>,
    /// Channel for sending commands to server
    tx: CommandSender,
    /// Handle for graceful shutdown
    pub shutdown_handle: WrappedShutdownHandle,
    /// Current chat message input
    pub message_input: String,
    /// Current broadcast message input
    pub broadcast_message: String,
    /// Scroll state per chat tab (offset and auto-scroll flag)
    pub scroll_states: HashMap<ChatTab, ScrollState>,
    /// Pending requests that need response routing
    pub pending_requests: HashMap<MessageId, ResponseRouting>,
    /// Error message for broadcast operations
    pub broadcast_error: Option<String>,
    /// User management panel state
    pub user_management: UserManagementState,
    /// User info panel data (None = loading, Some(Ok) = loaded, Some(Err) = error)
    pub user_info_data: Option<Result<UserInfoDetailed, String>>,
    /// Password change form state (Some when changing password, None otherwise)
    pub password_change_state: Option<PasswordChangeState>,
    /// Cached avatar handles for rendering (prevents flickering)
    pub avatar_cache: HashMap<String, CachedImage>,
    /// Server info edit state (Some when editing, None otherwise)
    pub server_info_edit: Option<ServerInfoEditState>,
    /// Currently active panel in the main content area (per-connection)
    pub active_panel: ActivePanel,
    /// News management panel state
    pub news_management: NewsManagementState,
    /// Cached news images for rendering (keyed by news item ID)
    pub news_image_cache: HashMap<i64, CachedImage>,
    /// Cached parsed markdown for news items (keyed by news item ID)
    pub news_markdown_cache: HashMap<i64, Vec<markdown::Item>>,
    /// Tab completion state for chat input (None when not completing)
    pub tab_completion: Option<TabCompletionState>,
    /// Files management panel state
    pub files_management: FilesManagementState,
}

impl ServerConnection {
    /// Check if the user has a specific permission
    ///
    /// Admins implicitly have all permissions. For non-admins, checks
    /// if the permission is in their permissions list.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.is_admin || self.permissions.iter().any(|p| p == permission)
    }

    /// Check if the user has any of the specified permissions
    ///
    /// Returns true if:
    /// - The permissions slice is empty (no permissions required)
    /// - The user is an admin (admins have all permissions)
    /// - The user has any of the specified permissions
    pub fn has_any_permission(&self, permissions: &[&str]) -> bool {
        permissions.is_empty()
            || self.is_admin
            || permissions
                .iter()
                .any(|req| self.permissions.iter().any(|p| p == *req))
    }

    /// Send a message to the server
    ///
    /// Generates a new message ID and sends the message through the channel.
    /// Returns the message ID on success for optional tracking.
    pub fn send(&self, message: ClientMessage) -> Result<MessageId, String> {
        let message_id = MessageId::new();
        self.tx
            .send((message_id, message))
            .map_err(|e| e.to_string())?;
        Ok(message_id)
    }

    /// Create a new ServerConnection with the given parameters
    pub fn new(params: ServerConnectionParams) -> Self {
        Self {
            bookmark_index: params.bookmark_index,
            session_id: params.session_id,
            username: params.username,
            display_name: params.display_name,
            connection_id: params.connection_id,
            is_admin: params.is_admin,
            permissions: params.permissions,
            locale: params.locale,
            server_name: params.server_name,
            server_description: params.server_description,
            server_version: params.server_version,
            server_image: params.server_image,
            cached_server_image: params.cached_server_image,
            chat_topic: params.chat_topic,
            chat_topic_set_by: params.chat_topic_set_by,
            max_connections_per_ip: params.max_connections_per_ip,
            max_transfers_per_ip: params.max_transfers_per_ip,
            active_chat_tab: ChatTab::Server,
            chat_messages: Vec::new(),
            user_messages: HashMap::new(),
            unread_tabs: HashSet::new(),
            online_users: Vec::new(),
            expanded_user: None,
            tx: params.tx,
            shutdown_handle: params.shutdown_handle,
            message_input: String::new(),
            broadcast_message: String::new(),
            scroll_states: HashMap::new(),
            pending_requests: HashMap::new(),
            broadcast_error: None,
            user_management: UserManagementState::default(),
            user_info_data: None,
            password_change_state: None,
            avatar_cache: HashMap::new(),
            server_info_edit: None,
            active_panel: ActivePanel::None,
            news_management: NewsManagementState::default(),
            news_image_cache: HashMap::new(),
            news_markdown_cache: HashMap::new(),
            tab_completion: None,
            files_management: FilesManagementState::default(),
        }
    }
}

/// Network connection handle returned by connect_to_server()
#[derive(Debug, Clone)]
pub struct NetworkConnection {
    /// Channel for sending messages to server
    pub tx: CommandSender,
    /// Session ID from server
    pub session_id: u32,
    /// Unique connection identifier
    pub connection_id: usize,
    /// Optional shutdown handle
    pub shutdown: Option<WrappedShutdownHandle>,
    /// Whether user is admin
    pub is_admin: bool,
    /// User's permissions
    pub permissions: Vec<String>,
    /// Server name (if provided in ServerInfo)
    pub server_name: Option<String>,
    /// Server description (if provided in ServerInfo)
    pub server_description: Option<String>,
    /// Server version (if provided in ServerInfo)
    pub server_version: Option<String>,
    /// Server image (if provided in ServerInfo)
    pub server_image: String,
    /// Chat topic received on login (if user has ChatTopic permission)
    pub chat_topic: Option<String>,
    /// Username who set the chat topic
    pub chat_topic_set_by: Option<String>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (admin only)
    pub max_transfers_per_ip: Option<u32>,
    /// Certificate fingerprint (SHA-256) for TOFU verification
    pub certificate_fingerprint: String,
    /// Locale accepted by the server
    pub locale: String,
}

impl NetworkConnection {
    /// Check if the user has a specific permission
    ///
    /// Admins implicitly have all permissions. For non-admins, checks
    /// if the permission is in their permissions list.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.is_admin || self.permissions.iter().any(|p| p == permission)
    }
}
