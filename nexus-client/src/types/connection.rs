//! Server connection types

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::widget::markdown;
use nexus_common::framing::MessageId;
use nexus_common::protocol::{ClientMessage, UserInfoDetailed};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use nexus_common::protocol::ChannelJoinInfo;

use super::{
    ActivePanel, ChannelState, ChatMessage, ChatTab, DisconnectDialogState, FilesManagementState,
    NewsManagementState, PasswordChangeState, ResponseRouting, ScrollState, ServerInfoEditState,
    UserInfo, UserManagementState,
};
use crate::image::CachedImage;

// =============================================================================
// Connection Credentials
// =============================================================================

/// Credentials and connection info needed for authentication
///
/// This struct consolidates all the information needed to connect to a server,
/// both for the main BBS connection and for file transfers. It avoids
/// duplicating these fields across multiple structs.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Server name for display (resolved: server-provided name or address fallback)
    pub server_name: String,
    /// Server address (IP or hostname)
    pub address: String,
    /// Main BBS port (typically 7500)
    #[serde(default)]
    pub port: u16,
    /// Transfer port (typically 7501)
    pub transfer_port: u16,
    /// TLS certificate fingerprint (SHA-256)
    pub certificate_fingerprint: String,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Nickname for shared accounts (empty string if not used)
    pub nickname: String,
}

impl std::fmt::Debug for ConnectionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionInfo")
            .field("server_name", &self.server_name)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("transfer_port", &self.transfer_port)
            .field("certificate_fingerprint", &self.certificate_fingerprint)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("nickname", &self.nickname)
            .finish()
    }
}

// =============================================================================
// Tab Completion State
// =============================================================================

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

// =============================================================================
// Server Connection Parameters
// =============================================================================

/// Parameters for creating a new ServerConnection
pub struct ServerConnectionParams {
    /// Bookmark ID or None for ad-hoc connections
    pub bookmark_id: Option<Uuid>,
    /// Session ID assigned by server
    pub session_id: u32,
    /// Connection info (address, port, auth info)
    pub connection_info: ConnectionInfo,
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

    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (admin only)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admin only, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Command sender channel
    pub tx: CommandSender,
    /// Shutdown handle for graceful disconnect
    pub shutdown_handle: WrappedShutdownHandle,
}

/// Type alias for the wrapped shutdown handle
pub type WrappedShutdownHandle = Arc<Mutex<Option<crate::network::ShutdownHandle>>>;

/// Type alias for the command sender channel
pub type CommandSender = mpsc::UnboundedSender<(MessageId, ClientMessage)>;

// =============================================================================
// Server Connection
// =============================================================================

/// Active connection to a server
///
/// Contains connection state, chat history, user list, and UI state.
#[derive(Debug, Clone)]
pub struct ServerConnection {
    /// Bookmark ID or None for ad-hoc connections
    pub bookmark_id: Option<Uuid>,
    /// Session ID assigned by server (used to identify our user in online_users list)
    pub session_id: u32,
    /// Connection info (address, port, auth info)
    pub connection_info: ConnectionInfo,
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
    /// Max connections per IP (admin only, from ServerInfo)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (admin only, from ServerInfo)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admin only, from ServerInfo, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Active chat tab (Console, Channel, or UserMessage)
    pub active_chat_tab: ChatTab,
    /// Console messages (system, error, info, broadcast messages)
    pub console_messages: Vec<ChatMessage>,
    /// Channel tabs in join order (channel names, e.g., ["#nexus", "#support"])
    pub channel_tabs: Vec<String>,
    /// Channel state by lowercase channel name
    pub channels: HashMap<String, ChannelState>,
    /// User message tabs in creation order (nicknames)
    pub user_message_tabs: Vec<String>,
    /// User message history per user (keyed by nickname)
    pub user_messages: HashMap<String, Vec<ChatMessage>>,
    /// Pending channel leave request (to prevent double-send)
    pub pending_channel_leave: Option<String>,
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
    /// Pending kick message (set when we receive a kick error, used on disconnect)
    pub pending_kick_message: Option<String>,
    /// Disconnect dialog state (Some when dialog is open)
    pub disconnect_dialog: Option<DisconnectDialogState>,
}

impl ServerConnection {
    /// Check if the user has a specific permission
    ///
    /// Admins implicitly have all permissions. For non-admins, checks
    /// if the permission is in their permissions list.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.is_admin || self.permissions.iter().any(|p| p == permission)
    }

    /// Get channel state by name (case-insensitive lookup)
    pub fn get_channel_state(&self, channel: &str) -> Option<&ChannelState> {
        self.channels.get(&channel.to_lowercase())
    }

    /// Get mutable channel state by name (case-insensitive lookup)
    pub fn get_channel_state_mut(&mut self, channel: &str) -> Option<&mut ChannelState> {
        self.channels.get_mut(&channel.to_lowercase())
    }

    /// Get the display name for a channel (preserves original casing)
    ///
    /// Returns the channel name from `channel_tabs` which preserves the original casing,
    /// or falls back to the provided name if not found.
    pub fn get_channel_display_name(&self, channel: &str) -> String {
        let channel_lower = channel.to_lowercase();
        self.channel_tabs
            .iter()
            .find(|c| c.to_lowercase() == channel_lower)
            .cloned()
            .unwrap_or_else(|| channel.to_string())
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
            bookmark_id: params.bookmark_id,
            session_id: params.session_id,
            connection_info: params.connection_info,
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
            max_connections_per_ip: params.max_connections_per_ip,
            max_transfers_per_ip: params.max_transfers_per_ip,
            file_reindex_interval: params.file_reindex_interval,
            persistent_channels: params.persistent_channels,
            auto_join_channels: params.auto_join_channels,
            active_chat_tab: ChatTab::Console,
            console_messages: Vec::new(),
            channel_tabs: Vec::new(),
            channels: HashMap::new(),
            user_message_tabs: Vec::new(),
            user_messages: HashMap::new(),
            pending_channel_leave: None,
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
            pending_kick_message: None,
            disconnect_dialog: None,
        }
    }
}

// =============================================================================
// Network Connection
// =============================================================================

/// Network connection state returned from connection setup
///
/// This is an intermediate type created after successful TLS + login,
/// before the UI creates a full ServerConnection.
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
    /// Channels the user was auto-joined to on login
    pub channels: Vec<ChannelJoinInfo>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (admin only)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admin only, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Locale accepted by the server
    pub locale: String,
    /// Connection info (address, port, auth info)
    pub connection_info: ConnectionInfo,
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
