//! Server connection types

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::widget::markdown;
use nexus_common::framing::MessageId;
use nexus_common::names::fold_name;
use nexus_common::protocol::{ClientMessage, UserInfoDetailed};
use nexus_common::validators::PasswordStrength;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use nexus_common::protocol::ChannelJoinInfo;

use super::{
    ActivePanel, ChannelState, ChatMessage, ChatTab, ConnectionMonitorState, DisconnectDialogState,
    FilesManagementState, MessageType, NewsManagementState, PasswordChangeState, ResponseRouting,
    ScrollState, ServerInfoEditState, ServerInfoTab, TrackerManagementState, UserInfo,
    UserManagementState, VoiceState,
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
    /// List of matches (sorted alphabetically)
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
    /// Database user ID (for ID-based protocol messages like UserEdit, UserDelete, UserUpdate)
    pub user_id: Option<i64>,
    /// Server-confirmed nickname (equals username for regular accounts)
    pub nickname: String,
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
    /// Public address advertised for `nexus://` URI sharing (from ServerInfo)
    pub public_address: Option<String>,
    /// Server version (from ServerInfo)
    pub server_version: Option<String>,
    /// Server image data URI
    pub server_image: String,
    /// Cached server image for display
    pub cached_server_image: Option<CachedImage>,

    /// Chat burst limit (from ServerInfo, visible to all users)
    pub chat_burst_limit: Option<u32>,
    /// Chat rate limit in messages per minute (from ServerInfo, visible to all users)
    pub chat_rate_limit: Option<u32>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Maximum outbound bandwidth cap in bytes/sec (0 = unlimited).
    /// Visible to all users; settable by admin only.
    pub max_outbound_rate: Option<u64>,
    /// Max transfers per IP (admin only)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admin only, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Minimum password strength requirement from the server
    pub min_password_strength: PasswordStrength,
    /// Server log level (read-only, from ServerInfo)
    pub log_level: Option<String>,
    /// WF2Q+ scheduler chunk size in bytes (admin-only).
    pub scheduler_chunk_size: Option<u32>,
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
    /// Database user ID (for ID-based protocol messages like UserEdit, UserDelete, UserUpdate)
    pub user_id: Option<i64>,
    /// Server-confirmed nickname (equals username for regular accounts)
    pub nickname: String,
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
    /// Public address advertised for `nexus://` URI sharing (from ServerInfo)
    pub public_address: Option<String>,
    /// Server version (from ServerInfo)
    pub server_version: Option<String>,
    /// Server image data URI (from ServerInfo, empty string if not set)
    pub server_image: String,
    /// Cached server image for rendering (decoded from server_image)
    pub cached_server_image: Option<CachedImage>,
    /// Chat burst limit (from ServerInfo, visible to all users)
    pub chat_burst_limit: Option<u32>,
    /// Chat rate limit in messages per minute (from ServerInfo, visible to all users)
    pub chat_rate_limit: Option<u32>,
    /// Max connections per IP (admin only, from ServerInfo)
    pub max_connections_per_ip: Option<u32>,
    /// Maximum outbound bandwidth cap in bytes/sec (0 = unlimited).
    /// Visible to all users; settable by admin only.
    pub max_outbound_rate: Option<u64>,
    /// Max transfers per IP (admin only, from ServerInfo)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admin only, from ServerInfo, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Minimum password strength requirement from the server
    pub min_password_strength: PasswordStrength,
    /// Server log level (read-only, from ServerInfo)
    pub log_level: Option<String>,
    /// WF2Q+ scheduler chunk size in bytes (admin-only).
    pub scheduler_chunk_size: Option<u32>,
    /// Active chat tab (Console, Channel, or UserMessage)
    pub active_chat_tab: ChatTab,
    /// Console messages (system, error, info, broadcast messages)
    pub console_messages: Vec<ChatMessage>,
    /// Channel tabs in join order (channel names, e.g., ["#nexus", "#support"])
    pub channel_tabs: Vec<String>,
    /// Channel state by lowercase channel name
    pub channels: HashMap<String, ChannelState>,
    /// Known channels for tab completion (joined + seen from /channels, sorted)
    pub known_channels: Vec<String>,
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
    /// Tracker management panel state (Trackers tab inside Server Info)
    pub tracker_management: TrackerManagementState,
    /// User management panel state
    pub user_management: UserManagementState,
    /// User info panel data (None = loading, Some(Ok) = loaded, Some(Err) = error)
    pub user_info_data: Option<Result<UserInfoDetailed, String>>,

    /// Panel to return to when closing User Info (e.g., ConnectionMonitor)
    pub user_info_return_panel: Option<ActivePanel>,
    /// Password change form state (Some when changing password, None otherwise)
    pub password_change_state: Option<PasswordChangeState>,
    /// Cached avatar handles for rendering (prevents flickering)
    pub avatar_cache: HashMap<String, CachedImage>,
    /// Server info edit state (Some when editing, None otherwise)
    pub server_info_edit: Option<ServerInfoEditState>,
    /// Active tab in server info display mode (shown based on available data)
    pub server_info_tab: ServerInfoTab,
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
    /// Connection monitor panel state
    pub connection_monitor: ConnectionMonitorState,
    /// Pending kick message (set when we receive a kick error, used on disconnect)
    pub pending_kick_message: Option<String>,
    /// Disconnect dialog state (Some when dialog is open)
    pub disconnect_dialog: Option<DisconnectDialogState>,
    /// Active voice session (None if not in voice)
    pub voice_session: Option<VoiceState>,
    /// Nicknames currently in voice per channel (lowercase channel name -> set of nicknames)
    /// Tracked even when we're not in voice, so we can show voice indicators in user list
    pub channel_voiced: HashMap<String, HashSet<String>>,
    /// Last known activity time for this connection (reset on keyboard events only)
    pub last_activity: std::time::Instant,
    /// Our session's away state — updated only from our own AwayResponse/BackResponse,
    /// never from UserUpdated (avoids aggregated vs per-session mismatch)
    pub is_away: bool,
    /// Whether we set this away via auto-away (controls auto-back on activity)
    pub is_auto_away: bool,
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
        self.channels.get(&fold_name(channel))
    }

    /// Get mutable channel state by name (case-insensitive lookup)
    pub fn get_channel_state_mut(&mut self, channel: &str) -> Option<&mut ChannelState> {
        self.channels.get_mut(&fold_name(channel))
    }

    /// Re-key the connection state tied to a user's nickname that is neither
    /// channel-member- nor user-list-scoped: our own identity (when it's us being
    /// renamed), the DM conversation tab (visible tab, messages, scroll, unread,
    /// active), and the active voice session. Shared by both `handle_user_updated`
    /// (UserList-gated) and `handle_chat_user_renamed` (channel-scoped) so a rename
    /// lands regardless of which signal the viewer receives. Idempotent: once `old`
    /// is gone, repeat calls (e.g. one ChatUserRenamed per shared channel) no-op.
    pub fn apply_user_rename(&mut self, old_username: &str, new_username: &str) {
        // Our own identity. Each guard is independent: a shared login — whose
        // session nickname differs from the renamed account username — updates only
        // its stored auth username, never its session nickname.
        if fold_name(&self.connection_info.username) == fold_name(old_username) {
            self.connection_info.username = new_username.to_string();
        }
        // A regular account may also carry its username in the connect-form nickname
        // (canonical_nickname tolerates nickname == username); keep it in step. A
        // shared account's per-session nickname differs from the account username, so
        // this guard won't touch it.
        if fold_name(&self.connection_info.nickname) == fold_name(old_username) {
            self.connection_info.nickname = new_username.to_string();
        }
        if fold_name(&self.nickname) == fold_name(old_username) {
            self.nickname = new_username.to_string();
        }

        // DM tab/message state, re-keyed by folded identity (merge on collision).
        self.rekey_user_message_tab(old_username, new_username);

        // In-flight DM responses carry the request-time nickname. Re-key them
        // with the tab so success/away/error responses don't reopen the old name.
        let old_folded = fold_name(old_username);
        for routing in self.pending_requests.values_mut() {
            match routing {
                ResponseRouting::OpenMessageTab(nickname)
                | ResponseRouting::ShowErrorInMessageTab(nickname)
                    if fold_name(nickname) == old_folded =>
                {
                    *nickname = new_username.to_string();
                }
                _ => {}
            }
        }

        // Active voice session: DM peer / participant / speaking / muted state.
        if let Some(voice_session) = &mut self.voice_session {
            voice_session.rename_user(old_username, new_username);
        }

        // A moderation dialog (kick/ban) open against the renamed user must follow
        // the rename — submit sends UserKick/BanCreate by nickname, so a stale
        // target would hit the wrong or a now-reused name.
        if let Some(dialog) = &mut self.disconnect_dialog
            && fold_name(&dialog.nickname) == fold_name(old_username)
        {
            dialog.nickname = new_username.to_string();
        }
    }

    /// Find-or-create the DM tab for `nickname`, identifying conversations by folded
    /// nickname so case variants never split into two tabs. If a folded-equal tab
    /// already exists under a different display case, relabel it to `nickname` (the
    /// latest case), merging via [`Self::rekey_user_message_tab`]. Returns the
    /// canonical (current-case) display key callers should use.
    pub fn resolve_user_message_tab(&mut self, nickname: &str) -> String {
        let folded = fold_name(nickname);
        if let Some(existing) = self
            .user_message_tabs
            .iter()
            .find(|n| fold_name(n) == folded)
            .cloned()
        {
            if existing != nickname {
                self.rekey_user_message_tab(&existing, nickname);
            }
        } else {
            self.user_message_tabs.push(nickname.to_string());
            self.user_messages.entry(nickname.to_string()).or_default();
        }
        nickname.to_string()
    }

    /// The canonical (current-case) display key of an existing DM tab whose folded
    /// identity matches `nickname`, or `None` if there's no such tab. Read-only — use
    /// [`Self::resolve_user_message_tab`] when the tab should be created on miss.
    pub fn user_message_tab_key(&self, nickname: &str) -> Option<String> {
        let folded = fold_name(nickname);
        self.user_message_tabs
            .iter()
            .find(|n| fold_name(n) == folded)
            .cloned()
    }

    /// Whether the active tab is the DM conversation with `nickname`, compared by
    /// folded identity so a case difference between the active tab and `nickname`
    /// still matches.
    pub fn is_active_user_message_tab(&self, nickname: &str) -> bool {
        matches!(&self.active_chat_tab, ChatTab::UserMessage(n) if fold_name(n) == fold_name(nickname))
    }

    /// Whether the active tab is `channel`, compared by folded identity so display
    /// casing differences don't make notification suppression miss.
    pub fn is_active_channel_tab(&self, channel: &str) -> bool {
        matches!(&self.active_chat_tab, ChatTab::Channel(n) if fold_name(n) == fold_name(channel))
    }

    /// The messages of the DM conversation with `nickname` (folded identity), or `None`
    /// if no such tab exists. Read-only sibling of [`Self::user_messages_for_mut`].
    pub fn user_messages_for(&self, nickname: &str) -> Option<&Vec<ChatMessage>> {
        self.user_message_tab_key(nickname)
            .and_then(|key| self.user_messages.get(&key))
    }

    /// The mutable messages of the DM conversation with `nickname` (folded identity),
    /// or `None` if no such tab exists. Use [`Self::resolve_user_message_tab`] when the
    /// tab should be created on miss.
    pub fn user_messages_for_mut(&mut self, nickname: &str) -> Option<&mut Vec<ChatMessage>> {
        let key = self.user_message_tab_key(nickname)?;
        self.user_messages.get_mut(&key)
    }

    /// Re-key a DM conversation `old` -> `new` by folded identity, collapsing to a
    /// single tab displayed as `new` (the latest case). If a folded-equal conversation
    /// already exists, messages are merged time-ordered rather than overwritten or
    /// duplicated. Reconciles the tab list, message store, scroll/unread state, and
    /// the active tab.
    fn rekey_user_message_tab(&mut self, old: &str, new: &str) {
        let old_fold = fold_name(old);
        let new_fold = fold_name(new);

        // Stored display keys (case may differ from the args): the old identity, and —
        // only when `new` is a *different* identity — any existing `new` conversation.
        let old_key = self
            .user_messages
            .keys()
            .find(|k| fold_name(k) == old_fold)
            .cloned();
        let new_key = (new_fold != old_fold)
            .then(|| {
                self.user_messages
                    .keys()
                    .find(|k| fold_name(k) == new_fold)
                    .cloned()
            })
            .flatten();

        // Message store: merge old + any existing new under the canonical `new` key.
        let from_old = old_key.as_ref().and_then(|k| self.user_messages.remove(k));
        let from_new = new_key.as_ref().and_then(|k| self.user_messages.remove(k));
        let merged = match (from_old, from_new) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                a.sort_by_key(|m| m.timestamp);
                // Dedup the same way the on-disk merge does (history::merge_messages) so the
                // live tab can't show a duplicate that a reload would silently drop.
                // Set-based (not adjacent) because same-timestamp messages aren't guaranteed
                // adjacent after sort; only real chat lines are deduped — system/info notices
                // (e.g. voice join/leave) pass through, matching the disk merge's
                // UserMessage-only rule. Key mirrors disk's (timestamp, from, to, message):
                // `to` is implicit (the tab) and `from` is the sender nickname.
                let mut seen = HashSet::new();
                a.retain(|m| {
                    if m.message_type == MessageType::Chat {
                        seen.insert((
                            m.timestamp.map(|t| t.timestamp_millis()),
                            m.nickname.clone(),
                            m.message.clone(),
                        ))
                    } else {
                        true
                    }
                });
                Some(a)
            }
            (a, b) => a.or(b),
        };
        if let Some(messages) = merged {
            self.user_messages.insert(new.to_string(), messages);
        }

        // Tab list: collapse the old- and new-folded entries to one `new` at the
        // earliest of their positions (preserves order in the common single-tab case).
        let mut insert_at = None;
        let mut idx = 0;
        self.user_message_tabs.retain(|n| {
            let f = fold_name(n);
            let drop = f == old_fold || f == new_fold;
            if drop && insert_at.is_none() {
                insert_at = Some(idx);
            }
            idx += 1;
            !drop
        });
        if let Some(pos) = insert_at {
            let pos = pos.min(self.user_message_tabs.len());
            self.user_message_tabs.insert(pos, new.to_string());
        }

        // Scroll / unread / active: re-key the old & new ChatTabs onto ChatTab(new).
        let new_tab = ChatTab::UserMessage(new.to_string());
        let old_tabs: Vec<ChatTab> = [old_key.as_deref(), new_key.as_deref()]
            .into_iter()
            .flatten()
            .map(|k| ChatTab::UserMessage(k.to_string()))
            .collect();
        let mut scroll = None;
        let mut unread = false;
        for tab in &old_tabs {
            if let Some(s) = self.scroll_states.remove(tab) {
                scroll.get_or_insert(s);
            }
            if self.unread_tabs.remove(tab) {
                unread = true;
            }
            if self.active_chat_tab == *tab {
                self.active_chat_tab = new_tab.clone();
            }
        }
        if let Some(s) = scroll {
            self.scroll_states.entry(new_tab.clone()).or_insert(s);
        }
        if unread {
            self.unread_tabs.insert(new_tab);
        }
    }

    /// Get the display name for a channel (preserves original casing)
    ///
    /// Returns the channel name from `channel_tabs` which preserves the original casing,
    /// or falls back to the provided name if not found.
    pub fn get_channel_display_name(&self, channel: &str) -> String {
        let channel_lower = fold_name(channel);
        self.channel_tabs
            .iter()
            .find(|c| fold_name(c) == channel_lower)
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
            user_id: params.user_id,
            nickname: params.nickname,
            connection_info: params.connection_info,
            display_name: params.display_name,
            connection_id: params.connection_id,
            is_admin: params.is_admin,
            permissions: params.permissions,
            locale: params.locale,
            server_name: params.server_name,
            server_description: params.server_description,
            public_address: params.public_address,
            server_version: params.server_version,
            server_image: params.server_image,
            cached_server_image: params.cached_server_image,
            chat_burst_limit: params.chat_burst_limit,
            chat_rate_limit: params.chat_rate_limit,
            max_connections_per_ip: params.max_connections_per_ip,
            max_outbound_rate: params.max_outbound_rate,
            max_transfers_per_ip: params.max_transfers_per_ip,
            file_reindex_interval: params.file_reindex_interval,
            persistent_channels: params.persistent_channels,
            auto_join_channels: params.auto_join_channels,
            min_password_strength: params.min_password_strength,
            log_level: params.log_level,
            scheduler_chunk_size: params.scheduler_chunk_size,
            active_chat_tab: ChatTab::Console,
            console_messages: Vec::new(),
            channel_tabs: Vec::new(),
            channels: HashMap::new(),
            known_channels: Vec::new(),
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
            tracker_management: TrackerManagementState::default(),
            user_management: UserManagementState::default(),
            user_info_data: None,

            user_info_return_panel: None,
            password_change_state: None,
            avatar_cache: HashMap::new(),
            server_info_edit: None,
            server_info_tab: ServerInfoTab::default(),
            active_panel: ActivePanel::None,
            news_management: NewsManagementState::default(),
            news_image_cache: HashMap::new(),
            news_markdown_cache: HashMap::new(),
            tab_completion: None,
            files_management: FilesManagementState::default(),
            connection_monitor: ConnectionMonitorState::default(),
            pending_kick_message: None,
            disconnect_dialog: None,
            voice_session: None,
            channel_voiced: HashMap::new(),
            last_activity: std::time::Instant::now(),
            is_away: false,
            is_auto_away: false,
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
    /// Unique connection identifier
    pub connection_id: usize,
    /// Optional shutdown handle
    pub shutdown: Option<WrappedShutdownHandle>,
    /// Whether user is admin
    pub is_admin: bool,
    /// Database user ID (for ID-based protocol messages)
    pub user_id: Option<i64>,
    /// Server-confirmed nickname (equals username for regular accounts)
    pub nickname: String,
    /// User's permissions
    pub permissions: Vec<String>,
    /// Server name (if provided in ServerInfo)
    pub server_name: Option<String>,
    /// Server description (if provided in ServerInfo)
    pub server_description: Option<String>,
    /// Public address advertised for `nexus://` URI sharing (if provided in ServerInfo)
    pub public_address: Option<String>,
    /// Server version (if provided in ServerInfo)
    pub server_version: Option<String>,
    /// Server image (if provided in ServerInfo)
    pub server_image: String,
    /// Channels the user was auto-joined to on login
    pub channels: Vec<ChannelJoinInfo>,
    /// Chat burst limit (from ServerInfo, visible to all users)
    pub chat_burst_limit: Option<u32>,
    /// Chat rate limit in messages per minute (from ServerInfo, visible to all users)
    pub chat_rate_limit: Option<u32>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Maximum outbound bandwidth cap in bytes/sec (0 = unlimited).
    /// Visible to all users; settable by admin only.
    pub max_outbound_rate: Option<u64>,
    /// Max transfers per IP (admin only)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admin only, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Minimum password strength requirement from the server
    pub min_password_strength: PasswordStrength,
    /// Server log level (read-only, from ServerInfo)
    pub log_level: Option<String>,
    /// WF2Q+ scheduler chunk size in bytes (admin-only).
    pub scheduler_chunk_size: Option<u32>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    use nexus_common::protocol::ChatAction;

    /// Minimal connection for exercising the DM-tab helpers. The command channel's
    /// receiver is dropped (these tests never send); our own identity is "me" so a
    /// rename of some other party never trips the self-identity guards.
    fn test_connection() -> ServerConnection {
        let (tx, _rx) = mpsc::unbounded_channel();
        ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: "me".to_string(),
            },
            display_name: String::new(),
            connection_id: 0,
            is_admin: false,
            permissions: Vec::new(),
            locale: String::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            cached_server_image: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            max_connections_per_ip: None,
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Weak,
            log_level: None,
            scheduler_chunk_size: None,
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// An info message stamped `secs` past the epoch with `text` as its body, so tests
    /// can assert merge ordering by timestamp and identify messages by content.
    fn msg_at(secs: i64, text: &str) -> ChatMessage {
        let mut m = ChatMessage::info(text);
        m.timestamp = Some(
            Local
                .timestamp_opt(secs, 0)
                .single()
                .expect("valid timestamp"),
        );
        m
    }

    /// A chat-type message (the in-memory analog of a stored DM line) stamped `secs`
    /// past the epoch, from `nick`, with `text` — used to exercise merge dedup.
    fn chat_at(secs: i64, nick: &str, text: &str) -> ChatMessage {
        ChatMessage::with_timestamp_and_status(
            nick,
            text,
            Local
                .timestamp_opt(secs, 0)
                .single()
                .expect("valid timestamp"),
            false,
            false,
            ChatAction::Normal,
        )
    }

    fn dm(name: &str) -> ChatTab {
        ChatTab::UserMessage(name.to_string())
    }

    fn channel(name: &str) -> ChatTab {
        ChatTab::Channel(name.to_string())
    }

    #[test]
    fn active_channel_tab_matches_by_folded_identity() {
        let mut conn = test_connection();
        conn.active_chat_tab = channel("#General");

        assert!(conn.is_active_channel_tab("#general"));
        assert!(conn.is_active_channel_tab("#GENERAL"));
        assert!(!conn.is_active_channel_tab("#other"));
    }

    // Scenario 1: an incoming message whose sender case differs from the open tab
    // relabels the tab to the new case, carrying messages / active / unread / scroll.
    #[test]
    fn resolve_relabels_case_variant_tab() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("alice".to_string());
        conn.user_messages
            .insert("alice".to_string(), vec![msg_at(100, "hi")]);
        conn.active_chat_tab = dm("alice");
        conn.unread_tabs.insert(dm("alice"));
        conn.scroll_states.insert(
            dm("alice"),
            ScrollState {
                offset: 0.25,
                auto_scroll: false,
            },
        );

        let key = conn.resolve_user_message_tab("Alice");

        assert_eq!(key, "Alice");
        assert_eq!(conn.user_message_tabs, vec!["Alice".to_string()]);
        assert!(conn.user_messages.contains_key("Alice"));
        assert!(!conn.user_messages.contains_key("alice"));
        assert_eq!(conn.user_messages["Alice"].len(), 1);
        assert_eq!(conn.active_chat_tab, dm("Alice"));
        assert!(conn.unread_tabs.contains(&dm("Alice")));
        assert!(!conn.unread_tabs.contains(&dm("alice")));
        assert_eq!(
            conn.scroll_states.get(&dm("Alice")).map(|s| s.offset),
            Some(0.25)
        );
        assert!(!conn.scroll_states.contains_key(&dm("alice")));
    }

    // resolve creates a tab on miss and is a clean no-op when the case already matches.
    #[test]
    fn resolve_creates_then_noops_same_case() {
        let mut conn = test_connection();
        assert_eq!(conn.resolve_user_message_tab("alice"), "alice");
        assert_eq!(conn.user_message_tabs, vec!["alice".to_string()]);
        assert_eq!(conn.resolve_user_message_tab("alice"), "alice");
        assert_eq!(conn.user_message_tabs, vec!["alice".to_string()]);
    }

    // user_message_tab_key folds on lookup and returns None on a genuine miss.
    #[test]
    fn tab_key_folds_and_misses() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("Alice".to_string());
        conn.user_messages.insert("Alice".to_string(), Vec::new());
        assert_eq!(
            conn.user_message_tab_key("alice"),
            Some("Alice".to_string())
        );
        assert_eq!(
            conn.user_message_tab_key("ALICE"),
            Some("Alice".to_string())
        );
        assert_eq!(conn.user_message_tab_key("bob"), None);
    }

    // is_active_user_message_tab folds against the active DM tab, and is false for a
    // different person or when the active tab is not a DM at all.
    #[test]
    fn is_active_user_message_tab_folds() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("Alice".to_string());
        conn.user_messages.insert("Alice".to_string(), Vec::new());
        conn.active_chat_tab = dm("Alice");

        assert!(conn.is_active_user_message_tab("Alice"));
        assert!(conn.is_active_user_message_tab("alice"));
        assert!(conn.is_active_user_message_tab("ALICE"));
        assert!(!conn.is_active_user_message_tab("bob"));

        conn.active_chat_tab = ChatTab::Console;
        assert!(!conn.is_active_user_message_tab("Alice"));
    }

    // Scenario 2: rename bob -> charlie moves the tab and re-keys the active tab.
    #[test]
    fn apply_user_rename_moves_tab() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("bob".to_string());
        conn.user_messages
            .insert("bob".to_string(), vec![msg_at(100, "a")]);
        conn.active_chat_tab = dm("bob");

        conn.apply_user_rename("bob", "charlie");

        assert_eq!(conn.user_message_tabs, vec!["charlie".to_string()]);
        assert!(conn.user_messages.contains_key("charlie"));
        assert!(!conn.user_messages.contains_key("bob"));
        assert_eq!(conn.active_chat_tab, dm("charlie"));
    }

    #[test]
    fn apply_user_rename_rekeys_pending_dm_routes() {
        let mut conn = test_connection();
        let open_id = MessageId::new();
        let error_id = MessageId::new();
        let unrelated_id = MessageId::new();
        conn.pending_requests
            .insert(open_id, ResponseRouting::OpenMessageTab("bob".to_string()));
        conn.pending_requests.insert(
            error_id,
            ResponseRouting::ShowErrorInMessageTab("BOB".to_string()),
        );
        conn.pending_requests.insert(
            unrelated_id,
            ResponseRouting::OpenMessageTab("alice".to_string()),
        );

        conn.apply_user_rename("bob", "charlie");

        assert!(matches!(
            conn.pending_requests.get(&open_id),
            Some(ResponseRouting::OpenMessageTab(name)) if name == "charlie"
        ));
        assert!(matches!(
            conn.pending_requests.get(&error_id),
            Some(ResponseRouting::ShowErrorInMessageTab(name)) if name == "charlie"
        ));
        assert!(matches!(
            conn.pending_requests.get(&unrelated_id),
            Some(ResponseRouting::OpenMessageTab(name)) if name == "alice"
        ));
    }

    // Scenario 3: a case-only rename bob -> Bob still relabels the tab.
    #[test]
    fn apply_user_rename_case_only_relabels() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("bob".to_string());
        conn.user_messages
            .insert("bob".to_string(), vec![msg_at(100, "a")]);
        conn.active_chat_tab = dm("bob");

        conn.apply_user_rename("bob", "Bob");

        assert_eq!(conn.user_message_tabs, vec!["Bob".to_string()]);
        assert!(conn.user_messages.contains_key("Bob"));
        assert!(!conn.user_messages.contains_key("bob"));
        assert_eq!(conn.active_chat_tab, dm("Bob"));
    }

    // Fold-collision: renaming into a name whose tab already exists merges both
    // conversations time-ordered under the single surviving (new-case) tab.
    #[test]
    fn apply_user_rename_merges_into_existing_tab() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("bob".to_string());
        conn.user_message_tabs.push("charlie".to_string());
        conn.user_messages.insert(
            "bob".to_string(),
            vec![msg_at(100, "b1"), msg_at(300, "b3")],
        );
        conn.user_messages
            .insert("charlie".to_string(), vec![msg_at(200, "c2")]);
        conn.active_chat_tab = dm("bob");
        conn.unread_tabs.insert(dm("charlie"));

        conn.apply_user_rename("bob", "charlie");

        // One surviving tab, at bob's (earlier) position.
        assert_eq!(conn.user_message_tabs, vec!["charlie".to_string()]);
        // Messages merged and sorted by timestamp.
        let texts: Vec<&str> = conn.user_messages["charlie"]
            .iter()
            .map(|m| m.message.as_str())
            .collect();
        assert_eq!(texts, vec!["b1", "c2", "b3"]);
        // active (was bob) and unread (was charlie) both land on the merged tab.
        assert_eq!(conn.active_chat_tab, dm("charlie"));
        assert!(conn.unread_tabs.contains(&dm("charlie")));
    }

    // Both rename signals (UserUpdated, ChatUserRenamed) call apply_user_rename; the
    // second to arrive must be a no-op, not duplicate or drop the tab.
    #[test]
    fn apply_user_rename_is_idempotent() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("bob".to_string());
        conn.user_messages
            .insert("bob".to_string(), vec![msg_at(100, "a")]);
        conn.active_chat_tab = dm("bob");

        conn.apply_user_rename("bob", "charlie");
        conn.apply_user_rename("bob", "charlie");

        assert_eq!(conn.user_message_tabs, vec!["charlie".to_string()]);
        assert_eq!(conn.user_messages["charlie"].len(), 1);
        assert_eq!(conn.active_chat_tab, dm("charlie"));
    }

    // The merge dedups duplicate chat lines the same way the on-disk merge does
    // (history::merge_messages), while non-chat notices (system/info) pass through —
    // matching the disk merge's UserMessage-only rule, so the live tab equals a reload.
    #[test]
    fn apply_user_rename_merge_dedups_chat_lines_only() {
        let mut conn = test_connection();
        conn.user_message_tabs.push("bob".to_string());
        conn.user_message_tabs.push("charlie".to_string());
        // Both tabs carry the SAME chat line ("dup") and the SAME info notice ("note"),
        // plus a unique chat line each.
        conn.user_messages.insert(
            "bob".to_string(),
            vec![
                chat_at(100, "x", "dup"),
                msg_at(100, "note"),
                chat_at(101, "x", "b"),
            ],
        );
        conn.user_messages.insert(
            "charlie".to_string(),
            vec![
                chat_at(100, "x", "dup"),
                msg_at(100, "note"),
                chat_at(102, "x", "c"),
            ],
        );

        conn.apply_user_rename("bob", "charlie");

        let msgs = &conn.user_messages["charlie"];
        // The duplicate chat line collapses to one copy...
        assert_eq!(msgs.iter().filter(|m| m.message == "dup").count(), 1);
        // ...while the duplicated info notice passes through (both copies kept).
        assert_eq!(msgs.iter().filter(|m| m.message == "note").count(), 2);
        // Both unique chat lines survive; total 1 dup + 2 notes + b + c = 5.
        assert!(msgs.iter().any(|m| m.message == "b"));
        assert!(msgs.iter().any(|m| m.message == "c"));
        assert_eq!(msgs.len(), 5);
    }
}
