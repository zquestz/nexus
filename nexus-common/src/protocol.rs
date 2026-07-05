//! Protocol definitions for Nexus BBS
//!
//! ## Username vs Nickname
//!
//! - **Username**: Account identifier (database key). Used for admin operations.
//! - **Nickname**: Display name. Always populated; equals username for regular accounts.
//!
//! Rule: "Users type what they see" - user-facing commands use nicknames,
//! admin operations use usernames.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Action type for chat and user messages
///
/// Determines how a message is rendered:
/// - `Normal`: Standard message with brackets (e.g., `<alice> hello`)
/// - `Me`: Action format (e.g., `*** alice waves`)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatAction {
    #[default]
    Normal,
    Me,
}

/// Direction of an active transfer as exposed in `ConnectionMonitorResponse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferInfoDirection {
    /// Server sending bytes to the client.
    Download,
    /// Client sending bytes to the server.
    Upload,
}

impl TransferInfoDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Upload => "upload",
        }
    }
}

impl std::fmt::Display for TransferInfoDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Directory type metadata for `FileEntry::dir_type`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileEntryDirType {
    Default,
    Upload,
    DropBox,
    UserDropBox(String),
    /// Forward-compatible folder type unknown to this client/server version.
    Other(String),
}

impl FileEntryDirType {
    pub fn as_wire_value(&self) -> String {
        match self {
            Self::Default => "default".to_string(),
            Self::Upload => "upload".to_string(),
            Self::DropBox => "dropbox".to_string(),
            Self::UserDropBox(owner) => format!("dropbox:{owner}"),
            Self::Other(value) => value.clone(),
        }
    }
}

impl Serialize for FileEntryDirType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.as_wire_value())
    }
}

impl<'de> Deserialize<'de> for FileEntryDirType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "default" => Ok(Self::Default),
            "upload" => Ok(Self::Upload),
            "dropbox" => Ok(Self::DropBox),
            _ => match value.strip_prefix("dropbox:") {
                Some(owner) => Ok(Self::UserDropBox(owner.to_string())),
                None => Ok(Self::Other(value)),
            },
        }
    }
}

/// Default group bandwidth weight when missing on the wire.
/// Sourced from `validators::DEFAULT_BANDWIDTH_WEIGHT` (single Rust-side source
/// of truth; the schema default on `groups.bandwidth_weight` mirrors it).
fn default_group_weight() -> u16 {
    crate::validators::DEFAULT_BANDWIDTH_WEIGHT
}

/// Client request messages
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
// Test-only variant-name iteration: the limits-table fixture in
// `framing/limits.rs` asserts every variant has a MESSAGE_TYPE_LIMITS row.
#[cfg_attr(test, derive(strum::EnumDiscriminants))]
#[cfg_attr(test, strum_discriminants(derive(strum::EnumIter)))]
pub enum ClientMessage {
    /// Create or update an IP ban
    BanCreate {
        /// Target: nickname, IP address, or hostname
        target: String,
        /// Duration: "10m", "4h", "7d", "0" (permanent), or None (permanent)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<String>,
        /// Reason for the ban (admin notes)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Delete an IP ban
    BanDelete {
        /// Target: nickname (removes all IPs with that annotation) or IP address
        target: String,
    },
    /// Request list of active bans
    BanList,
    /// Join or create a channel
    ChatJoin {
        channel: String,
    },
    /// Leave a channel
    ChatLeave {
        channel: String,
    },
    /// List available channels
    ChatList {},
    /// Set channel secret mode
    ChatSecret {
        channel: String,
        secret: bool,
    },
    ChatSend {
        message: String,
        #[serde(default, skip_serializing_if = "is_normal_action")]
        action: ChatAction,
        channel: String,
    },
    ChatTopicUpdate {
        topic: String,
        channel: String,
    },
    /// Request list of active connections (admin/connection_monitor permission)
    ConnectionMonitor,
    FileCopy {
        /// Source path of the file or directory to copy
        source_path: String,
        /// Destination directory to copy into
        destination_dir: String,
        /// If true, overwrite existing file at destination
        #[serde(default)]
        overwrite: bool,
        /// If true, source_path is relative to file root instead of user's area
        #[serde(default)]
        source_root: bool,
        /// If true, destination_dir is relative to file root instead of user's area
        #[serde(default)]
        destination_root: bool,
    },
    FileCreateDir {
        /// Parent directory path where the new directory should be created
        path: String,
        /// Name of the new directory to create
        name: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Raw file data for upload (port 7501 only, mirrors ServerMessage::FileData)
    FileData,
    FileDelete {
        /// Path to the file or empty directory to delete
        path: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Request a file download (port 7501 only)
    FileDownload {
        /// Path to download (file or directory)
        path: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Per-file hash sent after FileData (or alone if file was skipped).
    /// Contains the full BLAKE3 hash computed by the sender during streaming.
    FileHash {
        /// BLAKE3 hash of the complete file
        blake3: String,
    },
    /// Keepalive sent while computing BLAKE3 hash for a large file (port 7501 only)
    /// Receiver should reset idle timer but otherwise ignore this message.
    FileHashing {
        /// File being hashed (for logging/debugging)
        file: String,
    },
    FileInfo {
        /// Path to the file or directory to get info for
        path: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    FileList {
        path: String,
        /// If true, browse from file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
        /// If true, include hidden files (dotfiles) in the listing
        #[serde(default)]
        show_hidden: bool,
    },
    FileMove {
        /// Source path of the file or directory to move
        source_path: String,
        /// Destination directory to move into
        destination_dir: String,
        /// If true, overwrite existing file at destination
        #[serde(default)]
        overwrite: bool,
        /// If true, source_path is relative to file root instead of user's area
        #[serde(default)]
        source_root: bool,
        /// If true, destination_dir is relative to file root instead of user's area
        #[serde(default)]
        destination_root: bool,
    },
    /// Request a file index rebuild (admin command)
    FileReindex,
    FileRename {
        /// Current path of the file or directory to rename
        path: String,
        /// New name (just the filename, not full path)
        new_name: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Search files in the file area
    FileSearch {
        /// Search query (minimum 3 characters, literal match, case-insensitive)
        query: String,
        /// If true, search entire file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Client announces a file to upload (port 7501 only, mirrors ServerMessage::FileStart)
    FileStart {
        /// Relative path (e.g., "subdir/file.txt")
        path: String,
        /// File size in bytes
        size: u64,
    },
    /// Client response to FileStart - reports local file state for resume (downloads)
    FileStartResponse {
        /// Size of local file (0 if no local file exists)
        size: u64,
        /// BLAKE3 hash of local file (None if size is 0)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blake3: Option<String>,
    },
    /// Request a file upload (port 7501 only)
    FileUpload {
        /// Destination directory on server (e.g., "/Uploads")
        destination: String,
        /// Number of files to upload
        file_count: u64,
        /// Total size of all files in bytes
        total_size: u64,
        /// If true, destination is relative to file root instead of user's area
        #[serde(default)]
        root: bool,
    },
    /// Create a new account group
    GroupCreate {
        /// Group name
        name: String,
        /// Whether this group is for shared accounts only
        is_shared: bool,
        /// Permissions to assign to this group
        permissions: Vec<String>,
        /// Bandwidth weight for the scheduler. Defaults to 1 when omitted
        /// during defensive parsing, matching the schema default.
        #[serde(default = "default_group_weight")]
        bandwidth_weight: u16,
    },
    /// Delete an account group
    GroupDelete {
        /// Group ID to delete
        id: i64,
    },
    /// Request group details for editing
    GroupEdit {
        /// Group ID to edit
        id: i64,
    },
    /// Request list of all account groups
    ///
    /// Requires one of: `user_create`, `user_edit`, `group_create`, `group_edit`, or `group_delete`
    GroupList,
    /// Update an existing account group
    GroupUpdate {
        /// Group ID to update
        id: i64,
        /// New group name (if changing)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// New shared status (if changing)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_shared: Option<bool>,
        /// New permissions (if changing)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        /// New bandwidth weight (if changing)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bandwidth_weight: Option<u16>,
    },
    Handshake {
        version: String,
    },
    Login {
        username: String,
        password: String,
        features: Vec<String>,
        #[serde(default = "crate::default_locale")]
        locale: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    NewsCreate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    NewsDelete {
        id: i64,
    },
    NewsEdit {
        id: i64,
    },
    NewsList,
    NewsShow {
        id: i64,
    },
    NewsUpdate {
        id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    /// Keepalive ping (client sends periodically to prevent NAT timeout)
    Ping,
    ServerInfoUpdate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Public address for `nexus://` URI sharing (hostname/IP, no port)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        public_address: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_connections_per_ip: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_transfers_per_ip: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
        /// File reindex interval in minutes (0 to disable automatic reindexing)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_reindex_interval: Option<u32>,
        /// Persistent channels (space-separated, survive restart)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        persistent_channels: Option<String>,
        /// Auto-join channels (space-separated, joined on login)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auto_join_channels: Option<String>,
        /// Chat burst limit (max messages in a burst, 0 = no burst allowance)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chat_burst_limit: Option<u32>,
        /// Chat rate limit (messages per minute, 0 = disabled)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chat_rate_limit: Option<u32>,
        /// Minimum password strength level (0-4)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_password_strength: Option<u8>,
        /// Maximum outbound bandwidth in bytes/sec (0 = unlimited)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_outbound_rate: Option<u64>,
        /// WF2Q+ scheduler chunk size in bytes (1024..=65536)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheduler_chunk_size: Option<u32>,
    },
    /// Promote a tracker's `pending_fingerprint` (set after a Stage 1 TOFU
    /// mismatch) to its active `fingerprint`. Requires `tracker_edit`
    /// permission. Server rejects if the row has no `pending_fingerprint`,
    /// which naturally restricts this flow to Stage 1 mismatches — Stage 2
    /// (server-reported vs TLS-observed) mismatches are unrecoverable and
    /// never populate `pending_fingerprint`.
    TrackerAcceptFingerprint {
        id: i64,
    },
    /// Add a tracker the server should publish registration entries to.
    /// Requires `tracker_add` permission.
    TrackerAdd {
        /// Hostname or IP literal (validated as a public address).
        address: String,
        /// TCP port (typically 7510).
        port: u16,
        /// TOFU-pinned cert fingerprint in canonical form. `None` = unpinned;
        /// the tracker task will TOFU-pin on first successful connect.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fingerprint: Option<String>,
        /// Registration password. `None` or empty = open tracker.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        /// Admin-supplied label (used as the primary identifier in the UI).
        name: String,
        /// Whether the tracker task should actively maintain a connection.
        enabled: bool,
    },
    /// Fetch a single tracker's details for the edit form. Requires
    /// `tracker_edit` permission.
    TrackerEdit {
        id: i64,
    },
    /// List all configured trackers and their runtime status. Requires
    /// `tracker_list` permission.
    TrackerList,
    /// Remove a tracker from the server's tracker list. Requires
    /// `tracker_remove` permission.
    TrackerRemove {
        id: i64,
    },
    /// Partially update a tracker's configuration. Requires `tracker_edit`
    /// permission.
    TrackerUpdate {
        id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        address: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fingerprint: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
    },
    /// Add an IP to the trusted list (bypasses ban checks)
    TrustCreate {
        /// Target: nickname, IP address, or CIDR range
        target: String,
        /// Duration: "10m", "4h", "7d", "0" (permanent), or None (permanent)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<String>,
        /// Reason for the trust entry (admin notes)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Remove an IP from the trusted list
    TrustDelete {
        /// Target: nickname (removes all IPs with that annotation) or IP address
        target: String,
    },
    /// Request list of trusted IPs
    TrustList,
    /// Set away status for all sessions of this user
    UserAway {
        /// Optional status message (max 128 bytes)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Clear away status for all sessions of this user
    UserBack,
    UserBroadcast {
        message: String,
    },
    UserCreate {
        username: String,
        password: String,
        is_admin: bool,
        #[serde(default)]
        is_shared: bool,
        enabled: bool,
        permissions: Vec<String>,
        /// Optional group assignment
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<i64>,
        /// Permissions to explicitly revoke from group (only meaningful with a group)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revokes: Option<Vec<String>>,
        /// Explicit bandwidth weight override; `None` stores NULL (inherit).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bandwidth_weight: Option<u16>,
        /// `Some(true)` forces NULL storage (inherit) regardless of
        /// `bandwidth_weight`. Mirrors the `remove_group` flag pattern.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inherit_bandwidth_weight: Option<bool>,
    },
    UserDelete {
        id: i64,
    },
    UserEdit {
        id: i64,
    },
    UserInfo {
        nickname: String,
    },
    UserKick {
        nickname: String,
        /// Optional reason for the kick (shown to kicked user)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    UserList {
        #[serde(default)]
        all: bool,
    },
    UserMessage {
        to_nickname: String,
        message: String,
        #[serde(default, skip_serializing_if = "is_normal_action")]
        action: ChatAction,
    },
    /// Set status message without changing away status
    UserStatus {
        /// Status message (None to clear)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    UserUpdate {
        id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_password: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        /// Assign user to a group (by group ID)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<i64>,
        /// Remove user from their current group (takes precedence over group_id if both set)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remove_group: Option<bool>,
        /// Permissions to explicitly revoke from group (only meaningful with a group)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revokes: Option<Vec<String>>,
        /// New bandwidth weight (if changing). `None` leaves stored value
        /// alone; `Some(w)` sets an override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bandwidth_weight: Option<u16>,
        /// `Some(true)` clears the override to NULL (inherit from group);
        /// takes precedence over `bandwidth_weight` if both set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inherit_bandwidth_weight: Option<bool>,
    },
    /// Join voice chat for a channel or user message
    VoiceJoin {
        /// Target channel (e.g., "#general") or nickname for user message voice
        target: String,
    },
    /// Leave current voice session
    VoiceLeave,
}

/// Helper for skip_serializing_if on ChatAction
fn is_normal_action(action: &ChatAction) -> bool {
    matches!(action, ChatAction::Normal)
}

/// Information about an active connection (used in ConnectionMonitorResponse)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Display name (always populated; equals username for regular accounts)
    pub nickname: String,
    /// Account username
    pub username: String,
    /// Remote IP address
    pub ip: String,
    /// Remote port
    pub port: u16,
    /// Unix timestamp when session logged in
    pub login_time: i64,
    /// Whether the user is an admin
    pub is_admin: bool,
    /// Whether this is a shared account
    pub is_shared: bool,
}

/// Information about an active file transfer (used in ConnectionMonitorResponse)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferInfo {
    /// Display name (always populated; equals username for regular accounts)
    pub nickname: String,
    /// Account username
    pub username: String,
    /// Remote IP address
    pub ip: String,
    /// Remote port (distinguishes WebSocket 7503 vs TCP 7501)
    pub port: u16,
    /// Whether the user is an admin
    pub is_admin: bool,
    /// Whether this is a shared account
    pub is_shared: bool,
    /// Transfer direction.
    pub direction: TransferInfoDirection,
    /// Path being transferred
    pub path: String,
    /// Total size in bytes (0 if unknown)
    pub total_size: u64,
    /// Bytes transferred so far
    pub bytes_transferred: u64,
    /// Unix timestamp when transfer started
    pub started_at: i64,
}

/// Voice join bearer token.
///
/// Serialized as the underlying UUID for wire compatibility, but redacted in
/// `Debug` output because it authenticates UDP voice packets.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VoiceJoinToken(Uuid);

impl From<Uuid> for VoiceJoinToken {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<VoiceJoinToken> for Uuid {
    fn from(value: VoiceJoinToken) -> Self {
        value.0
    }
}

impl std::fmt::Debug for VoiceJoinToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::REDACTED)
    }
}

/// Server response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
// Test-only variant-name iteration: the limits-table fixture in
// `framing/limits.rs` asserts every variant has a MESSAGE_TYPE_LIMITS row.
#[cfg_attr(test, derive(strum::EnumDiscriminants))]
#[cfg_attr(test, strum_discriminants(derive(strum::EnumIter)))]
pub enum ServerMessage {
    /// Response to BanCreate request
    BanCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// IPs that were banned (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        ips: Option<Vec<String>>,
        /// Nickname if banned by nickname (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    /// Response to BanDelete request
    BanDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// IPs that were unbanned (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        ips: Option<Vec<String>>,
        /// Nickname if unbanned by nickname (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    /// Response to BanList request
    BanListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        bans: Option<Vec<BanInfo>>,
    },
    /// Response to ChatJoin request
    /// On success, includes full channel data. On error (including already-member), only error is set.
    ChatJoinResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Channel name (only on success)
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
        /// Channel topic (only on success)
        #[serde(skip_serializing_if = "Option::is_none")]
        topic: Option<String>,
        /// Who set the topic (only on success, if topic is set)
        #[serde(skip_serializing_if = "Option::is_none")]
        topic_set_by: Option<String>,
        /// Whether channel is secret (only on success)
        #[serde(skip_serializing_if = "Option::is_none")]
        secret: Option<bool>,
        /// Nicknames of current channel members (only on success)
        #[serde(skip_serializing_if = "Option::is_none")]
        members: Option<Vec<String>>,
        /// Nicknames currently in voice chat (only on success, only if requester has voice feature + voice_listen)
        #[serde(skip_serializing_if = "Option::is_none")]
        voiced: Option<Vec<String>>,
    },
    /// Response to ChatLeave request
    ChatLeaveResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    /// Response to ChatList request
    ChatListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        channels: Option<Vec<ChannelInfo>>,
    },
    ChatMessage {
        session_id: u32,
        nickname: String,
        #[serde(default)]
        is_admin: bool,
        #[serde(default)]
        is_shared: bool,
        message: String,
        #[serde(default, skip_serializing_if = "is_normal_action")]
        action: ChatAction,
        channel: String,
        /// Unix timestamp (seconds since epoch)
        #[serde(default)]
        timestamp: i64,
    },
    /// Response to ChatSecret request
    ChatSecretResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    ChatTopicUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Broadcast when channel properties change (topic, secret mode)
    /// Only changed fields are included
    ChatUpdated {
        channel: String,
        /// New topic (None = not changed, Some("") = cleared)
        #[serde(skip_serializing_if = "Option::is_none")]
        topic: Option<String>,
        /// Who set the topic
        #[serde(skip_serializing_if = "Option::is_none")]
        topic_set_by: Option<String>,
        /// New secret mode (None = not changed)
        #[serde(skip_serializing_if = "Option::is_none")]
        secret: Option<bool>,
        /// Who changed secret mode
        #[serde(skip_serializing_if = "Option::is_none")]
        secret_set_by: Option<String>,
    },
    /// Broadcast when a user joins a channel
    ChatUserJoined {
        channel: String,
        nickname: String,
        is_admin: bool,
        is_shared: bool,
    },
    /// Broadcast when a user leaves a channel
    ChatUserLeft {
        channel: String,
        nickname: String,
    },
    /// Broadcast to a channel's members when a member is renamed, so channel-scoped
    /// state (member list, voiced set) stays correct without depending on the
    /// `UserList`-gated `UserUpdated`. Sent to every member of the channel,
    /// including the renamed user.
    ChatUserRenamed {
        channel: String,
        old_nickname: String,
        new_nickname: String,
        /// Current admin status for the renamed user.
        is_admin: bool,
    },
    /// Response to ConnectionMonitor request
    ConnectionMonitorResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        connections: Option<Vec<ConnectionInfo>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        transfers: Option<Vec<TransferInfo>>,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(default)]
        disconnect: bool,
    },
    FileCopyResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error kind for client decision making
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    FileCreateDirResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Full path of the created directory (for client to navigate to)
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    /// Raw file data for download (transfer port only)
    FileData,
    FileDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Response to a FileDownload request (port 7501 only)
    FileDownloadResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error kind: "not_found", "permission", "invalid"
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
        /// Total size of all files in bytes
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
        /// Number of files to transfer
        #[serde(skip_serializing_if = "Option::is_none")]
        file_count: Option<u64>,
        /// Transfer ID for logging (8 hex chars)
        #[serde(skip_serializing_if = "Option::is_none")]
        transfer_id: Option<String>,
    },
    /// Per-file hash sent after FileData (or alone if file was skipped).
    /// Contains the full BLAKE3 hash computed by the sender during streaming.
    FileHash {
        /// BLAKE3 hash of the complete file
        blake3: String,
    },
    /// Keepalive sent while computing BLAKE3 hash for a large file (port 7501 only)
    /// Receiver should reset idle timer but otherwise ignore this message.
    FileHashing {
        /// File being hashed (for logging/debugging)
        file: String,
    },
    FileInfoResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        info: Option<FileInfoDetails>,
    },
    FileListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entries: Option<Vec<FileEntry>>,
        /// Whether the current directory allows uploads (for UI to enable "New Directory" button)
        #[serde(default)]
        can_upload: bool,
        /// Username that owns the `[NEXUS-DB-username]` folder enclosing the
        /// listed directory (or the listed directory itself, if it is the
        /// drop box). `None` when the listing is not inside a user drop box.
        ///
        /// The named user implicitly gains delete and rename permissions
        /// for files inside their own drop box, so the client composes this
        /// with its own account username to decide whether to show Delete
        /// and Rename UI on entries. The comparison uses the account
        /// username (not nickname) to match the server-side ownership check.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dropbox_owner: Option<String>,
    },
    FileMoveResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error kind for client decision making
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    /// Response to FileReindex request
    FileReindexResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    FileRenameResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Response to FileSearch request
    FileSearchResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Search results (max 100)
        #[serde(skip_serializing_if = "Option::is_none")]
        results: Option<Vec<FileSearchResult>>,
    },
    /// Server announces a file to transfer (download, transfer port only)
    FileStart {
        /// Relative path (e.g., "Games/app.zip")
        path: String,
        /// File size in bytes
        size: u64,
    },
    /// Server response to client FileStart - reports server file state for resume (uploads)
    FileStartResponse {
        /// Size of file on server (0 if no file exists)
        size: u64,
        /// BLAKE3 hash of server's partial file (None if size is 0)
        #[serde(skip_serializing_if = "Option::is_none")]
        blake3: Option<String>,
    },
    /// Response to a FileUpload request (port 7501 only)
    FileUploadResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error kind: "not_found", "permission", "invalid", "exists"
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
        /// Transfer ID for logging (8 hex chars)
        #[serde(skip_serializing_if = "Option::is_none")]
        transfer_id: Option<String>,
    },
    /// Response to GroupCreate request
    GroupCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Response to GroupDelete request
    GroupDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Response to GroupEdit request (fetch group for editing)
    GroupEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_shared: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        member_count: Option<u32>,
        /// Raw stored bandwidth weight (groups always have one in the DB).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bandwidth_weight: Option<u16>,
    },
    /// Response to GroupList request
    GroupListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        groups: Option<Vec<GroupInfo>>,
    },
    /// Response to GroupUpdate request
    GroupUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    HandshakeResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        /// Server certificate fingerprint (SHA-256, colon-separated).
        /// Always sent, including on failure responses, so the client can
        /// detect TLS interception before considering the handshake outcome.
        fingerprint: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    LoginResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        features: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<String>,
        /// Channels the user was auto-joined to on login
        #[serde(skip_serializing_if = "Option::is_none")]
        channels: Option<Vec<ChannelJoinInfo>>,
        /// Server-confirmed nickname (equals username for regular accounts)
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
        /// Group ID (display only; permissions already resolved)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<i64>,
        /// Group name (display only; permissions already resolved)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_name: Option<String>,
    },
    NewsCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
    },
    NewsEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        items: Option<Vec<NewsItem>>,
    },
    NewsShowResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsUpdated {
        action: NewsAction,
        id: i64,
    },
    NewsUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    PermissionsUpdated {
        is_admin: bool,
        permissions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
        /// Group ID (display only; permissions already resolved)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<i64>,
        /// Group name (display only; permissions already resolved)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_name: Option<String>,
    },
    /// Keepalive pong (server response to client Ping)
    Pong,
    ServerBroadcast {
        session_id: u32,
        username: String,
        message: String,
    },
    ServerInfoUpdated {
        server_info: ServerInfo,
    },
    ServerInfoUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Response to TrackerAcceptFingerprint request
    TrackerAcceptFingerprintResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Tracker's id (so the client can identify which row was affected)
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        /// Tracker's name (for the toast message)
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Response to TrackerAdd request
    TrackerAddResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// New tracker's id (for the toast message)
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        /// New tracker's name (for the toast message)
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Response to TrackerEdit request
    TrackerEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// The full tracker info (config + runtime status), populated on success.
        #[serde(skip_serializing_if = "Option::is_none")]
        tracker: Option<TrackerInfo>,
    },
    /// Response to TrackerList request
    TrackerListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// All configured trackers with their runtime status. `None` on
        /// error; `Some([])` on success means no trackers are configured yet.
        #[serde(skip_serializing_if = "Option::is_none")]
        trackers: Option<Vec<TrackerInfo>>,
    },
    /// Response to TrackerRemove request
    TrackerRemoveResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Removed tracker's name (for the toast message)
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Response to TrackerUpdate request
    TrackerUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Updated tracker's id (for the toast message)
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        /// Updated tracker's name (for the toast message)
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Server signals transfer completion (transfer port only)
    TransferComplete {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    /// Response to TrustCreate request
    TrustCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// IPs that were trusted (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        ips: Option<Vec<String>>,
        /// Nickname if trusted by nickname (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    /// Response to TrustDelete request
    TrustDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// IPs that were untrusted (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        ips: Option<Vec<String>>,
        /// Nickname if untrusted by nickname (for success message)
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    /// Response to TrustList request
    TrustListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entries: Option<Vec<TrustInfo>>,
    },
    /// Response to UserAway request
    UserAwayResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Response to UserBack request
    UserBackResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserBroadcastResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserConnected {
        user: UserInfo,
    },
    UserCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    UserDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    UserDisconnected {
        session_id: u32,
        nickname: String,
    },
    UserEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_shared: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        /// Group ID (if user belongs to a group)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<i64>,
        /// Group name (if user belongs to a group)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_name: Option<String>,
        /// Base permissions of the assigned group (for computing inherited vs override)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_permissions: Option<Vec<String>>,
        /// Permissions explicitly revoked from the group for this user
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revoked_permissions: Option<Vec<String>>,
        /// Available groups for the group dropdown
        #[serde(default, skip_serializing_if = "Option::is_none")]
        available_groups: Option<Vec<GroupInfo>>,
        /// Raw stored bandwidth weight: `None` = inheriting from group,
        /// `Some(w)` = individual override. Effective weight is derived by
        /// the form from this value plus `available_groups`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bandwidth_weight: Option<u16>,
    },
    UserInfoResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user: Option<UserInfoDetailed>,
    },
    UserKickResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    UserListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        users: Option<Vec<UserInfo>>,
    },
    UserMessage {
        from_nickname: String,
        from_admin: bool,
        #[serde(default)]
        from_shared: bool,
        to_nickname: String,
        message: String,
        #[serde(default, skip_serializing_if = "is_normal_action")]
        action: ChatAction,
        /// Unix timestamp (seconds since epoch)
        #[serde(default)]
        timestamp: i64,
    },
    UserMessageResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Recipient's away status (if away when message sent)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_away: Option<bool>,
        /// Recipient's status message (if any)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    /// Response to UserStatus request
    UserStatusResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserUpdated {
        previous_username: String,
        user: UserInfo,
    },
    UserUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    /// Response to VoiceJoin request
    VoiceJoinResponse {
        success: bool,
        /// Voice token for UDP authentication (only on success)
        #[serde(skip_serializing_if = "Option::is_none")]
        token: Option<VoiceJoinToken>,
        /// Target (same as requested, or the other user's nickname for user message voice)
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// Nicknames of users already in this voice session
        #[serde(skip_serializing_if = "Option::is_none")]
        participants: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Response to VoiceLeave request
    VoiceLeaveResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Notification that a user joined voice
    VoiceUserJoined {
        /// Nickname of the user who joined
        nickname: String,
        /// Target channel or the other user's nickname for user message voice
        target: String,
    },
    /// Notification that a user left voice
    VoiceUserLeft {
        /// Nickname of the user who left
        nickname: String,
        /// Target channel or the other user's nickname for user message voice
        target: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Public address advertised by the admin for sharing `nexus://` URIs.
    /// Hostname or IP only; port is implied by the server's BBS port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections_per_ip: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_transfers_per_ip: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Port for file transfers (typically 7501)
    pub transfer_port: u16,
    /// Port for WebSocket file transfers (typically 7503)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_websocket_port: Option<u16>,
    /// File reindex interval in minutes (0 = disabled)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated; visible to admins or chat+chat_join sessions)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_join_channels: Option<String>,
    /// Chat burst limit (max messages in a burst, 0 = no burst allowance)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_burst_limit: Option<u32>,
    /// Chat rate limit (messages per minute, 0 = flood protection disabled)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_rate_limit: Option<u32>,
    /// Minimum password strength level (0-4, admin only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_password_strength: Option<u8>,
    /// Server log level (read-only, not settable via ServerInfoUpdate)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    /// Maximum outbound bandwidth in bytes/sec (0 = unlimited)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_outbound_rate: Option<u64>,
    /// WF2Q+ scheduler chunk size in bytes (admin-only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_chunk_size: Option<u32>,
}

/// Channel info returned when joining a channel (in LoginResponse or ChatJoinResponse)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelJoinInfo {
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic_set_by: Option<String>,
    pub secret: bool,
    pub members: Vec<String>,
    /// Nicknames currently in voice chat (only if requester has voice feature + voice_listen)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voiced: Option<Vec<String>>,
}

/// Channel info for channel lists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub member_count: u32,
    pub secret: bool,
}

/// User info for lists. `nickname` is the display name (== username for regular accounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub nickname: String,
    pub login_time: i64,
    pub is_admin: bool,
    #[serde(default)]
    pub is_shared: bool,
    pub session_ids: Vec<u32>,
    pub locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(default)]
    pub is_away: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Group ID (if user belongs to a group)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    /// Group name (if user belongs to a group)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Resolved effective bandwidth weight (user override → admin default
    /// → group → system default).
    pub bandwidth_weight: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsAction {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub author_is_admin: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Information about an active IP ban
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanInfo {
    /// The banned IP address or CIDR range (e.g., "192.168.1.100" or "192.168.1.0/24")
    pub ip_address: String,
    /// Nickname annotation (if banned by nickname)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// Reason for the ban (admin notes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Username of the admin who created the ban
    pub created_by: String,
    /// Unix timestamp when the ban was created
    pub created_at: i64,
    /// Unix timestamp when the ban expires (None = permanent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// Information about a trusted IP entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustInfo {
    /// The trusted IP address or CIDR range (e.g., "192.168.1.100" or "192.168.1.0/24")
    pub ip_address: String,
    /// Nickname annotation (if trusted by nickname)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// Reason for the trust entry (admin notes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Username of the admin who created the trust entry
    pub created_by: String,
    /// Unix timestamp when the trust entry was created
    pub created_at: i64,
    /// Unix timestamp when the trust entry expires (None = permanent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// File search result entry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileSearchResult {
    /// Full path relative to user's root (e.g., "/Documents/report.pdf")
    pub path: String,
    /// Filename only (e.g., "report.pdf")
    pub name: String,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modified time as Unix timestamp
    pub modified: i64,
    /// True if this is a directory
    pub is_directory: bool,
}

/// File entry in a directory listing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileEntry {
    /// Filesystem name (including any suffix like ` [NEXUS-UL]`; client strips for display)
    pub name: String,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modified time as Unix timestamp
    pub modified: i64,
    /// Directory type (None = file, Some = directory with type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir_type: Option<FileEntryDirType>,
    /// True if uploads are allowed at this location
    pub can_upload: bool,
}

/// Detailed file/directory information returned by FileInfo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfoDetails {
    /// File or directory name
    pub name: String,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Creation time as Unix timestamp (None if filesystem doesn't support it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    /// Last modified time as Unix timestamp
    pub modified: i64,
    /// True if this is a directory
    pub is_directory: bool,
    /// True if this is a symbolic link
    pub is_symlink: bool,
    /// MIME type (None for directories)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Number of items inside (directories only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_count: Option<u64>,
}

/// Detailed user info. `nickname` is the display name (== username for regular accounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoDetailed {
    pub id: i64,
    pub username: String,
    pub nickname: String,
    pub login_time: i64,
    #[serde(default)]
    pub is_shared: bool,
    pub session_ids: Vec<u32>,
    pub features: Vec<String>,
    pub created_at: i64,
    pub locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub is_admin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addresses: Option<Vec<String>>,
    #[serde(default)]
    pub is_away: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Channels the user is currently in (secret channels only visible to admins)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<String>>,
    /// Group ID (if user belongs to a group)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    /// Group name (if user belongs to a group)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Resolved effective bandwidth weight (user override → admin default
    /// → group → system default).
    pub bandwidth_weight: u16,
}

/// Information about an account group
///
/// Used in `GroupListResponse`, `UserEditResponse.available_groups`, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    /// Unique group identifier
    pub id: i64,
    /// Group name
    pub name: String,
    /// Whether this group is for shared accounts only
    pub is_shared: bool,
    /// Number of users assigned to this group
    pub member_count: u32,
    /// Permissions granted by this group
    pub permissions: Vec<String>,
    /// Bandwidth weight for all members of this group (NOT NULL in DB).
    /// Missing fields deserialize as the schema default for defensive parsing.
    #[serde(default = "default_group_weight")]
    pub bandwidth_weight: u16,
}

/// Information about a tracker the server is configured to publish to.
///
/// Combines the durable DB row (id, address, port, fingerprint, password,
/// name, enabled, created_at, updated_at) with the runtime status of the
/// tracker task (connection state, last successful refresh, last error,
/// pending fingerprint observations, tracker-supplied refresh interval).
///
/// For disabled trackers (no running tracker task), the runtime fields
/// are at their default "not connected" values: `connected: false`, every
/// other runtime field `None`. That accurately reflects "no task running."
///
/// Used in `TrackerListResponse`, `TrackerEditResponse`, etc.
///
/// Note: derives `Clone`, `Serialize`, `Deserialize` but NOT `Debug` —
/// the manual `Debug` impl below redacts the registration password so
/// it never leaks into operator logs.
#[derive(Clone, Serialize, Deserialize)]
pub struct TrackerInfo {
    // ---- Configuration (from DB row) ----
    /// Unique tracker identifier (DB row id).
    pub id: i64,
    /// Hostname or IP literal.
    pub address: String,
    /// TCP port (typically 7510).
    pub port: u16,
    /// TOFU-pinned cert fingerprint in canonical form. `None` until the
    /// tracker task has TOFU-pinned on first successful connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Registration password. Echoed plaintext to admins (it's a shared
    /// invite-code-style secret, not a personal credential — admins may
    /// need to share it with collaborators registering with the same
    /// tracker). `None` if the tracker is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Admin-supplied label.
    pub name: String,
    /// Whether the tracker task should actively maintain a connection.
    pub enabled: bool,
    /// Unix epoch seconds when the row was created.
    pub created_at: i64,
    /// Unix epoch seconds when the row was last updated.
    pub updated_at: i64,

    // ---- Runtime status (from tracker manager; ephemeral) ----
    /// Whether the tracker task currently has a healthy registration
    /// with the tracker.
    pub connected: bool,
    /// Unix epoch seconds when the tracker task last successfully registered.
    /// `None` if it's never connected since this task started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<i64>,
    /// Unix epoch seconds when the tracker task last started a connection
    /// attempt, regardless of outcome. Stamped at the top of each
    /// cycle so the admin UI can show that the tracker task is making
    /// forward progress even when every recent attempt has failed.
    /// `None` until the first attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempted_at: Option<i64>,
    /// Most recent error message, locale-translated. `None` if there's
    /// been no error or the last attempt succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Most recent error kind in machine-readable form (e.g.
    /// "fingerprint_mismatch", "unauthorized", "rate_limited").
    /// Distinguishable from `last_error` so the client can decide
    /// whether to render a "needs your attention" UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_kind: Option<String>,
    /// Fingerprint the tracker task observed but the admin hasn't accepted
    /// yet (after a Stage 1 mismatch). `None` in normal operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_fingerprint: Option<String>,
    /// Tracker-supplied refresh interval in seconds, populated once
    /// the tracker task has successfully registered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_interval: Option<u32>,
}

impl std::fmt::Debug for TrackerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact the registration password — it's a shared secret that
        // never belongs in operator logs. The `Option` wrapper is
        // preserved on the redacted path so a reader can distinguish
        // `Some(crate::REDACTED)` (password set, hidden) from `None`
        // (open tracker, no password).
        let password = self.password.as_ref().map(|_| crate::REDACTED);
        f.debug_struct("TrackerInfo")
            .field("id", &self.id)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("fingerprint", &self.fingerprint)
            .field("password", &password)
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("connected", &self.connected)
            .field("last_connected_at", &self.last_connected_at)
            .field("last_attempted_at", &self.last_attempted_at)
            .field("last_error", &self.last_error)
            .field("last_error_kind", &self.last_error_kind)
            .field("pending_fingerprint", &self.pending_fingerprint)
            .field("refresh_interval", &self.refresh_interval)
            .finish()
    }
}

fn debug_prefix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }

    let mut end = 0;
    for (idx, ch) in value.char_indices() {
        let next = idx + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        end = next;
    }
    &value[..end]
}

fn redact_optional_secret(value: &Option<String>) -> Option<&'static str> {
    value.as_ref().map(|_| crate::REDACTED)
}

impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientMessage::BanCreate {
                target,
                duration,
                reason,
            } => f
                .debug_struct("BanCreate")
                .field("target", target)
                .field("duration", duration)
                .field("reason", reason)
                .finish(),
            ClientMessage::BanDelete { target } => {
                f.debug_struct("BanDelete").field("target", target).finish()
            }
            ClientMessage::BanList => f.debug_struct("BanList").finish(),
            ClientMessage::ChatJoin { channel } => f
                .debug_struct("ChatJoin")
                .field("channel", channel)
                .finish(),
            ClientMessage::ChatLeave { channel } => f
                .debug_struct("ChatLeave")
                .field("channel", channel)
                .finish(),
            ClientMessage::ChatList {} => f.debug_struct("ChatList").finish(),
            ClientMessage::ChatSecret { channel, secret } => f
                .debug_struct("ChatSecret")
                .field("channel", channel)
                .field("secret", secret)
                .finish(),
            ClientMessage::ChatSend {
                message,
                action,
                channel,
            } => f
                .debug_struct("ChatSend")
                .field("message", message)
                .field("action", action)
                .field("channel", channel)
                .finish(),
            ClientMessage::ChatTopicUpdate { topic, channel } => f
                .debug_struct("ChatTopicUpdate")
                .field("topic", topic)
                .field("channel", channel)
                .finish(),
            ClientMessage::ConnectionMonitor => f.debug_struct("ConnectionMonitor").finish(),
            ClientMessage::FileCopy {
                source_path,
                destination_dir,
                overwrite,
                source_root,
                destination_root,
            } => f
                .debug_struct("FileCopy")
                .field("source_path", source_path)
                .field("destination_dir", destination_dir)
                .field("overwrite", overwrite)
                .field("source_root", source_root)
                .field("destination_root", destination_root)
                .finish(),
            ClientMessage::FileCreateDir { path, name, root } => f
                .debug_struct("FileCreateDir")
                .field("path", path)
                .field("name", name)
                .field("root", root)
                .finish(),
            ClientMessage::FileData => f.debug_struct("FileData").finish(),
            ClientMessage::FileDelete { path, root } => f
                .debug_struct("FileDelete")
                .field("path", path)
                .field("root", root)
                .finish(),
            ClientMessage::FileDownload { path, root } => f
                .debug_struct("FileDownload")
                .field("path", path)
                .field("root", root)
                .finish(),
            ClientMessage::FileHashing { file } => {
                f.debug_struct("FileHashing").field("file", file).finish()
            }
            ClientMessage::FileHash { blake3 } => {
                f.debug_struct("FileHash").field("blake3", blake3).finish()
            }
            ClientMessage::FileInfo { path, root } => f
                .debug_struct("FileInfo")
                .field("path", path)
                .field("root", root)
                .finish(),
            ClientMessage::FileList {
                path,
                root,
                show_hidden,
            } => f
                .debug_struct("FileList")
                .field("path", path)
                .field("root", root)
                .field("show_hidden", show_hidden)
                .finish(),
            ClientMessage::FileMove {
                source_path,
                destination_dir,
                overwrite,
                source_root,
                destination_root,
            } => f
                .debug_struct("FileMove")
                .field("source_path", source_path)
                .field("destination_dir", destination_dir)
                .field("overwrite", overwrite)
                .field("source_root", source_root)
                .field("destination_root", destination_root)
                .finish(),
            ClientMessage::FileReindex => f.debug_struct("FileReindex").finish(),
            ClientMessage::FileRename {
                path,
                new_name,
                root,
            } => f
                .debug_struct("FileRename")
                .field("path", path)
                .field("new_name", new_name)
                .field("root", root)
                .finish(),
            ClientMessage::FileSearch { query, root } => f
                .debug_struct("FileSearch")
                .field("query", query)
                .field("root", root)
                .finish(),
            ClientMessage::FileStart { path, size } => f
                .debug_struct("FileStart")
                .field("path", path)
                .field("size", size)
                .finish(),
            ClientMessage::FileStartResponse { size, blake3 } => f
                .debug_struct("FileStartResponse")
                .field("size", size)
                .field("blake3", blake3)
                .finish(),
            ClientMessage::FileUpload {
                destination,
                file_count,
                total_size,
                root,
            } => f
                .debug_struct("FileUpload")
                .field("destination", destination)
                .field("file_count", file_count)
                .field("total_size", total_size)
                .field("root", root)
                .finish(),
            ClientMessage::GroupCreate {
                name,
                is_shared,
                permissions,
                bandwidth_weight,
            } => f
                .debug_struct("GroupCreate")
                .field("name", name)
                .field("is_shared", is_shared)
                .field("permissions", permissions)
                .field("bandwidth_weight", bandwidth_weight)
                .finish(),
            ClientMessage::GroupDelete { id } => {
                f.debug_struct("GroupDelete").field("id", id).finish()
            }
            ClientMessage::GroupEdit { id } => f.debug_struct("GroupEdit").field("id", id).finish(),
            ClientMessage::GroupList => f.debug_struct("GroupList").finish(),
            ClientMessage::GroupUpdate {
                id,
                name,
                is_shared,
                permissions,
                bandwidth_weight,
            } => f
                .debug_struct("GroupUpdate")
                .field("id", id)
                .field("name", name)
                .field("is_shared", is_shared)
                .field("permissions", permissions)
                .field("bandwidth_weight", bandwidth_weight)
                .finish(),
            ClientMessage::Handshake { version } => f
                .debug_struct("Handshake")
                .field("version", version)
                .finish(),
            ClientMessage::Login {
                username,
                password: _,
                features,
                locale,
                avatar,
                nickname,
            } => f
                .debug_struct("Login")
                .field("username", username)
                .field("password", &crate::REDACTED)
                .field("features", features)
                .field("locale", locale)
                .field(
                    "avatar",
                    &avatar.as_ref().map(|a| {
                        if a.len() > 50 {
                            format!("{}...<{} bytes>", debug_prefix(a, 50), a.len())
                        } else {
                            a.clone()
                        }
                    }),
                )
                .field("nickname", nickname)
                .finish(),
            ClientMessage::NewsCreate { body, image } => {
                let mut s = f.debug_struct("NewsCreate");
                s.field("body", body);
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", debug_prefix(img, 100), img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::NewsDelete { id } => {
                f.debug_struct("NewsDelete").field("id", id).finish()
            }
            ClientMessage::NewsEdit { id } => f.debug_struct("NewsEdit").field("id", id).finish(),
            ClientMessage::NewsList => f.debug_struct("NewsList").finish(),
            ClientMessage::NewsShow { id } => f.debug_struct("NewsShow").field("id", id).finish(),
            ClientMessage::NewsUpdate { id, body, image } => {
                let mut s = f.debug_struct("NewsUpdate");
                s.field("id", id).field("body", body);
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", debug_prefix(img, 100), img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::Ping => f.debug_struct("Ping").finish(),
            ClientMessage::ServerInfoUpdate {
                name,
                description,
                public_address,
                file_reindex_interval,
                max_connections_per_ip,
                max_transfers_per_ip,
                image,
                persistent_channels,
                auto_join_channels,
                chat_burst_limit,
                chat_rate_limit,
                min_password_strength,
                max_outbound_rate,
                scheduler_chunk_size,
            } => {
                let mut s = f.debug_struct("ServerInfoUpdate");
                s.field("name", name)
                    .field("description", description)
                    .field("public_address", public_address)
                    .field("max_connections_per_ip", max_connections_per_ip)
                    .field("max_transfers_per_ip", max_transfers_per_ip)
                    .field("file_reindex_interval", file_reindex_interval)
                    .field("persistent_channels", persistent_channels)
                    .field("auto_join_channels", auto_join_channels)
                    .field("chat_burst_limit", chat_burst_limit)
                    .field("chat_rate_limit", chat_rate_limit)
                    .field("min_password_strength", min_password_strength)
                    .field("max_outbound_rate", max_outbound_rate)
                    .field("scheduler_chunk_size", scheduler_chunk_size);
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", debug_prefix(img, 100), img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::TrackerAcceptFingerprint { id } => f
                .debug_struct("TrackerAcceptFingerprint")
                .field("id", id)
                .finish(),
            ClientMessage::TrackerAdd {
                address,
                port,
                fingerprint,
                password,
                name,
                enabled,
            } => f
                .debug_struct("TrackerAdd")
                .field("address", address)
                .field("port", port)
                .field("fingerprint", fingerprint)
                .field("password", &redact_optional_secret(password))
                .field("name", name)
                .field("enabled", enabled)
                .finish(),
            ClientMessage::TrackerEdit { id } => {
                f.debug_struct("TrackerEdit").field("id", id).finish()
            }
            ClientMessage::TrackerList => f.debug_struct("TrackerList").finish(),
            ClientMessage::TrackerRemove { id } => {
                f.debug_struct("TrackerRemove").field("id", id).finish()
            }
            ClientMessage::TrackerUpdate {
                id,
                address,
                port,
                fingerprint,
                password,
                name,
                enabled,
            } => f
                .debug_struct("TrackerUpdate")
                .field("id", id)
                .field("address", address)
                .field("port", port)
                .field("fingerprint", fingerprint)
                .field("password", &redact_optional_secret(password))
                .field("name", name)
                .field("enabled", enabled)
                .finish(),
            ClientMessage::TrustCreate {
                target,
                duration,
                reason,
            } => f
                .debug_struct("TrustCreate")
                .field("target", target)
                .field("duration", duration)
                .field("reason", reason)
                .finish(),
            ClientMessage::TrustDelete { target } => f
                .debug_struct("TrustDelete")
                .field("target", target)
                .finish(),
            ClientMessage::TrustList => f.debug_struct("TrustList").finish(),
            ClientMessage::UserAway { message } => f
                .debug_struct("UserAway")
                .field("message", message)
                .finish(),
            ClientMessage::UserBack => f.debug_struct("UserBack").finish(),
            ClientMessage::UserBroadcast { message } => f
                .debug_struct("UserBroadcast")
                .field("message", message)
                .finish(),
            ClientMessage::UserCreate {
                username,
                is_admin,
                is_shared,
                enabled,
                permissions,
                group_id,
                revokes,
                bandwidth_weight,
                inherit_bandwidth_weight,
                ..
            } => f
                .debug_struct("UserCreate")
                .field("username", username)
                .field("is_admin", is_admin)
                .field("is_shared", is_shared)
                .field("enabled", enabled)
                .field("permissions", permissions)
                .field("group_id", group_id)
                .field("revokes", revokes)
                .field("bandwidth_weight", bandwidth_weight)
                .field("inherit_bandwidth_weight", inherit_bandwidth_weight)
                .field("password", &crate::REDACTED)
                .finish(),
            ClientMessage::UserDelete { id } => {
                f.debug_struct("UserDelete").field("id", id).finish()
            }
            ClientMessage::UserEdit { id } => f.debug_struct("UserEdit").field("id", id).finish(),
            ClientMessage::UserInfo { nickname } => f
                .debug_struct("UserInfo")
                .field("nickname", nickname)
                .finish(),
            ClientMessage::UserKick { nickname, reason } => f
                .debug_struct("UserKick")
                .field("nickname", nickname)
                .field("reason", reason)
                .finish(),
            ClientMessage::UserList { all } => {
                f.debug_struct("UserList").field("all", all).finish()
            }
            ClientMessage::UserMessage {
                to_nickname,
                message,
                action,
            } => f
                .debug_struct("UserMessage")
                .field("to_nickname", to_nickname)
                .field("message", message)
                .field("action", action)
                .finish(),
            ClientMessage::UserStatus { status } => f
                .debug_struct("UserStatus")
                .field("status", status)
                .finish(),
            ClientMessage::UserUpdate {
                id,
                current_password: _,
                username,
                password: _,
                is_admin,
                enabled,
                permissions,
                group_id,
                remove_group,
                revokes,
                bandwidth_weight,
                inherit_bandwidth_weight,
            } => f
                .debug_struct("UserUpdate")
                .field("id", id)
                .field("current_password", &crate::REDACTED)
                .field("username", username)
                .field("password", &crate::REDACTED)
                .field("is_admin", is_admin)
                .field("enabled", enabled)
                .field("permissions", permissions)
                .field("group_id", group_id)
                .field("remove_group", remove_group)
                .field("revokes", revokes)
                .field("bandwidth_weight", bandwidth_weight)
                .field("inherit_bandwidth_weight", inherit_bandwidth_weight)
                .finish(),
            ClientMessage::VoiceJoin { target } => {
                f.debug_struct("VoiceJoin").field("target", target).finish()
            }
            ClientMessage::VoiceLeave => f.debug_struct("VoiceLeave").finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tracker_info(password: Option<String>) -> TrackerInfo {
        TrackerInfo {
            id: 1,
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: None,
            password,
            name: "Public".to_string(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
            connected: false,
            last_connected_at: None,
            last_attempted_at: None,
            last_error: None,
            last_error_kind: None,
            pending_fingerprint: None,
            refresh_interval: None,
        }
    }

    #[test]
    fn tracker_info_debug_redacts_password() {
        let info = make_tracker_info(Some("supersecret".to_string()));
        let dbg = format!("{info:?}");
        assert!(
            !dbg.contains("supersecret"),
            "raw password leaked into Debug output: {dbg}"
        );
        assert!(
            dbg.contains(crate::REDACTED),
            "expected redaction marker, got: {dbg}"
        );
    }

    #[test]
    fn tracker_info_debug_shows_none_password_verbatim() {
        let info = make_tracker_info(None);
        let dbg = format!("{info:?}");
        assert!(
            !dbg.contains(crate::REDACTED),
            "should not redact when no password is set: {dbg}"
        );
        assert!(
            dbg.contains("password: None"),
            "expected `password: None`, got: {dbg}"
        );
    }

    #[test]
    fn server_message_with_tracker_info_redacts_password() {
        // ServerMessage's derive(Debug) recurses into TrackerInfo's
        // manual impl, so password redaction holds end-to-end.
        let msg = ServerMessage::TrackerEditResponse {
            success: true,
            error: None,
            tracker: Some(make_tracker_info(Some("supersecret".to_string()))),
        };
        let dbg = format!("{msg:?}");
        assert!(
            !dbg.contains("supersecret"),
            "password leaked through ServerMessage::Debug: {dbg}"
        );
        assert!(dbg.contains(crate::REDACTED));
    }

    #[test]
    fn test_serialize_login() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec!["chat".to_string()],
            locale: "en".to_string(),
            avatar: None,
            nickname: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Login\""));
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"features\""));
        assert!(json.contains("\"locale\":\"en\""));
        assert!(!json.contains("\"avatar\""));
    }

    #[test]
    fn test_deserialize_login() {
        let json = r#"{"type":"Login","username":"alice","password":"secret","features":["chat"]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login {
                username,
                password,
                features,
                locale,
                avatar,
                nickname,
            } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "secret");
                assert_eq!(features, vec!["chat".to_string()]);
                assert_eq!(locale, "en");
                assert!(avatar.is_none());
                assert!(nickname.is_none());
            }
            _ => panic!("Expected Login message"),
        }
    }

    #[test]
    fn test_debug_redacts_password() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "super_secret_password".to_string(),
            features: vec!["chat".to_string()],
            locale: "en".to_string(),
            avatar: None,
            nickname: None,
        };
        let debug_output = format!("{:?}", msg);
        assert!(debug_output.contains("alice"));
        assert!(debug_output.contains("chat"));
        assert!(!debug_output.contains("super_secret_password"));
        assert!(debug_output.contains("REDACTED"));
    }

    #[test]
    fn test_debug_user_create_redacts_password() {
        let msg = ClientMessage::UserCreate {
            username: "alice".to_string(),
            password: "create_secret".to_string(),
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: vec!["chat_send".to_string()],
            group_id: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let debug_output = format!("{:?}", msg);
        assert!(!debug_output.contains("create_secret"));
        assert!(debug_output.contains("password"));
        assert!(debug_output.contains(crate::REDACTED));
    }

    #[test]
    fn test_debug_tracker_add_redacts_some_password_and_preserves_none() {
        let with_password = ClientMessage::TrackerAdd {
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: None,
            password: Some("tracker_secret".to_string()),
            name: "Tracker".to_string(),
            enabled: true,
        };
        let dbg = format!("{:?}", with_password);
        assert!(!dbg.contains("tracker_secret"));
        assert!(dbg.contains("password: Some(\"[REDACTED]\")"), "{dbg}");

        let without_password = ClientMessage::TrackerAdd {
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: None,
            password: None,
            name: "Tracker".to_string(),
            enabled: true,
        };
        let dbg = format!("{:?}", without_password);
        assert!(dbg.contains("password: None"), "{dbg}");
        assert!(!dbg.contains(crate::REDACTED));
    }

    #[test]
    fn test_debug_tracker_update_redacts_some_password_and_preserves_none() {
        let with_password = ClientMessage::TrackerUpdate {
            id: 7,
            address: None,
            port: None,
            fingerprint: None,
            password: Some("tracker_secret".to_string()),
            name: None,
            enabled: None,
        };
        let dbg = format!("{:?}", with_password);
        assert!(!dbg.contains("tracker_secret"));
        assert!(dbg.contains("password: Some(\"[REDACTED]\")"), "{dbg}");

        let without_password = ClientMessage::TrackerUpdate {
            id: 7,
            address: None,
            port: None,
            fingerprint: None,
            password: None,
            name: None,
            enabled: None,
        };
        let dbg = format!("{:?}", without_password);
        assert!(dbg.contains("password: None"), "{dbg}");
        assert!(!dbg.contains(crate::REDACTED));
    }

    #[test]
    fn test_debug_voice_join_response_redacts_token() {
        let token = Uuid::new_v4();
        let msg = ServerMessage::VoiceJoinResponse {
            success: true,
            token: Some(token.into()),
            target: Some("#general".to_string()),
            participants: Some(vec!["alice".to_string()]),
            error: None,
        };

        let dbg = format!("{msg:?}");
        assert!(!dbg.contains(&token.to_string()), "{dbg}");
        assert!(dbg.contains(crate::REDACTED), "{dbg}");

        let json = serde_json::to_string(&msg).expect("serialize voice join response");
        assert!(
            json.contains(&token.to_string()),
            "wire JSON must still carry the UUID token: {json}"
        );
    }

    #[test]
    fn test_serialize_login_response() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some(12345),
            user_id: None,
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string()]),
            features: Some(vec![
                "chat".to_string(),
                "files".to_string(),
                "news".to_string(),
                "voice".to_string(),
            ]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
            nickname: Some("testuser".to_string()),
            group_id: None,
            group_name: None,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"session_id\":12345"));
        assert!(json.contains("\"features\":[\"chat\",\"files\",\"news\",\"voice\"]"));
    }

    #[test]
    fn test_serialize_login_error() {
        let msg = ServerMessage::LoginResponse {
            success: false,
            session_id: None,
            user_id: None,
            is_admin: None,
            permissions: None,
            features: None,
            server_info: None,
            locale: None,
            channels: None,
            nickname: None,
            error: Some("Invalid credentials".to_string()),
            group_id: None,
            group_name: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_error_includes_disconnect_false() {
        let msg = ServerMessage::Error {
            message: "oops".to_string(),
            command: None,
            disconnect: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Error\""));
        assert!(json.contains("\"disconnect\":false"));
    }

    #[test]
    fn test_serialize_error_includes_disconnect_true() {
        let msg = ServerMessage::Error {
            message: "bye".to_string(),
            command: Some("UserKick".to_string()),
            disconnect: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Error\""));
        assert!(json.contains("\"command\":\"UserKick\""));
        assert!(json.contains("\"disconnect\":true"));
    }

    #[test]
    fn test_deserialize_error_defaults_disconnect_false() {
        let json = r#"{"type":"Error","message":"oops","command":null}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Error {
                message,
                command,
                disconnect,
            } => {
                assert_eq!(message, "oops");
                assert_eq!(command, None);
                assert!(!disconnect);
            }
            other => panic!("Expected Error, got {other:?}"),
        }
    }

    #[test]
    fn test_serialize_file_list_response_includes_can_upload_false() {
        let msg = ServerMessage::FileListResponse {
            success: true,
            error: None,
            path: Some("/".to_string()),
            entries: Some(vec![]),
            can_upload: false,
            dropbox_owner: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileListResponse\""));
        assert!(json.contains("\"can_upload\":false"));
    }

    #[test]
    fn test_serialize_login_response_admin() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some(99999),
            user_id: None,
            is_admin: Some(true),
            permissions: Some(vec![]),
            features: Some(vec!["chat".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
            nickname: Some("admin".to_string()),
            group_id: None,
            group_name: None,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"is_admin\":true"));
        assert!(json.contains("\"permissions\":[]"));
    }

    #[test]
    fn test_serialize_login_response_with_permissions() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some(67890),
            user_id: None,
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            features: Some(vec!["chat".to_string(), "news".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
            nickname: Some("regularuser".to_string()),
            group_id: None,
            group_name: None,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"is_admin\":false"));
        assert!(json.contains("\"user_list\""));
        assert!(json.contains("\"chat_send\""));
    }

    #[test]
    fn test_serialize_login_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec!["chat".to_string()],
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
            nickname: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_deserialize_login_with_avatar() {
        let json = r#"{"type":"Login","username":"alice","password":"secret","features":["chat"],"locale":"en","avatar":"data:image/png;base64,abc123"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login { avatar, .. } => {
                assert_eq!(avatar, Some("data:image/png;base64,abc123".to_string()));
            }
            _ => panic!("Expected Login message"),
        }
    }

    #[test]
    fn test_serialize_user_info_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let user_info = UserInfo {
            id: 1,
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_serialize_user_info_without_avatar() {
        let user_info = UserInfo {
            id: 1,
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(!json.contains("\"avatar\""));
    }

    #[test]
    fn test_news_item_author_is_optional() {
        let item = NewsItem {
            id: 1,
            body: Some("body".to_string()),
            image: None,
            author: None,
            author_is_admin: false,
            created_at: 1_705_314_600,
            updated_at: 1_705_314_600,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("\"author\":"));

        let decoded: NewsItem = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.author, None);
        assert!(!decoded.author_is_admin);
    }

    #[test]
    fn test_serialize_user_info_detailed_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let user_info = UserInfoDetailed {
            id: 1,
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_shared: false,
            session_ids: vec![1, 2],
            features: vec!["chat".to_string()],
            created_at: 1234567800,
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
            is_admin: false,
            addresses: None,
            is_away: false,
            status: None,
            channels: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_debug_login_truncates_large_avatar() {
        let large_avatar = format!("data:image/png;base64,{}", "A".repeat(1000));
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: Some(large_avatar.clone()),
            nickname: None,
        };
        let debug_output = format!("{:?}", msg);
        assert!(debug_output.contains("..."));
        assert!(debug_output.contains("bytes"));
        assert!(!debug_output.contains(&large_avatar));
    }

    #[test]
    fn test_debug_login_truncates_multibyte_avatar_safely() {
        let large_avatar = format!("data:image/png;base64,{}", "資料".repeat(100));
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: Some(large_avatar.clone()),
            nickname: None,
        };
        let debug_output = format!("{:?}", msg);
        assert!(debug_output.contains("..."));
        assert!(debug_output.contains("bytes"));
        assert!(!debug_output.contains(&large_avatar));
    }

    #[test]
    fn test_debug_news_image_truncates_multibyte_payload_safely() {
        let image = "資料".repeat(100);
        let msg = ClientMessage::NewsCreate {
            body: Some("news".to_string()),
            image: Some(image.clone()),
        };
        let debug_output = format!("{:?}", msg);
        assert!(debug_output.contains("..."));
        assert!(debug_output.contains("bytes"));
        assert!(!debug_output.contains(&image));
    }

    #[test]
    fn test_serialize_login_with_nickname() {
        let msg = ClientMessage::Login {
            username: "shared_acct".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Nick1".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_login_without_nickname() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("\"nickname\""));
    }

    #[test]
    fn test_deserialize_login_with_nickname() {
        let json = r#"{"type":"Login","username":"shared_acct","password":"secret","features":[],"locale":"en","nickname":"Nick1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login { nickname, .. } => {
                assert_eq!(nickname, Some("Nick1".to_string()));
            }
            _ => panic!("Expected Login message"),
        }
    }

    #[test]
    fn test_serialize_user_create_with_is_shared() {
        let msg = ClientMessage::UserCreate {
            username: "shared_acct".to_string(),
            password: "secret".to_string(),
            is_admin: false,
            is_shared: true,
            enabled: true,
            permissions: vec!["chat_send".to_string()],
            group_id: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserCreate\""));
        assert!(json.contains("\"is_shared\":true"));
        assert!(json.contains("\"is_admin\":false"));
    }

    #[test]
    fn test_deserialize_user_create_with_is_shared() {
        let json = r#"{"type":"UserCreate","username":"shared_acct","password":"secret","is_admin":false,"is_shared":true,"enabled":true,"permissions":["chat_send"]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserCreate { is_shared, .. } => {
                assert!(is_shared);
            }
            _ => panic!("Expected UserCreate message"),
        }
    }

    #[test]
    fn test_deserialize_user_create_defaults_is_shared_false() {
        let json = r#"{"type":"UserCreate","username":"alice","password":"secret","is_admin":false,"enabled":true,"permissions":[]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserCreate { is_shared, .. } => {
                assert!(!is_shared);
            }
            _ => panic!("Expected UserCreate message"),
        }
    }

    #[test]
    fn test_transfer_info_direction_serializes_as_legacy_string() {
        let info = TransferInfo {
            nickname: "alice".to_string(),
            username: "alice".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 7501,
            is_admin: false,
            is_shared: false,
            direction: TransferInfoDirection::Download,
            path: "/file.txt".to_string(),
            total_size: 10,
            bytes_transferred: 5,
            started_at: 123,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""direction":"download""#), "{json}");

        let parsed: TransferInfo = serde_json::from_str(
            r#"{"nickname":"alice","username":"alice","ip":"127.0.0.1","port":7501,"is_admin":false,"is_shared":false,"direction":"upload","path":"/file.txt","total_size":10,"bytes_transferred":5,"started_at":123}"#,
        )
        .unwrap();
        assert_eq!(parsed.direction, TransferInfoDirection::Upload);
    }

    #[test]
    fn test_file_entry_dir_type_serializes_as_legacy_string() {
        let entry = FileEntry {
            name: "Inbox [NEXUS-DB-alice]".to_string(),
            size: 0,
            modified: 123,
            dir_type: Some(FileEntryDirType::UserDropBox("alice".to_string())),
            can_upload: true,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""dir_type":"dropbox:alice""#), "{json}");

        let parsed: FileEntry = serde_json::from_str(
            r#"{"name":"Uploads [NEXUS-UL]","size":0,"modified":123,"dir_type":"upload","can_upload":true}"#,
        )
        .unwrap();
        assert_eq!(parsed.dir_type, Some(FileEntryDirType::Upload));

        let parsed: FileEntry = serde_json::from_str(
            r#"{"name":"Archive","size":0,"modified":123,"dir_type":"archive","can_upload":false}"#,
        )
        .unwrap();
        assert_eq!(
            parsed.dir_type,
            Some(FileEntryDirType::Other("archive".to_string()))
        );
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(json.contains(r#""dir_type":"archive""#), "{json}");
    }

    #[test]
    fn test_serialize_chat_message_with_is_admin_and_is_shared() {
        let msg = ServerMessage::ChatMessage {
            session_id: 1,
            nickname: "Nick1".to_string(),
            message: "Hello!".to_string(),
            is_admin: false,
            is_shared: true,
            action: ChatAction::Normal,
            channel: "#general".to_string(),
            timestamp: 1718234567,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ChatMessage\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_admin\":false"));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_chat_user_renamed_with_is_admin() {
        let msg = ServerMessage::ChatUserRenamed {
            channel: "#general".to_string(),
            old_nickname: "alice".to_string(),
            new_nickname: "alicia".to_string(),
            is_admin: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ChatUserRenamed\""));
        assert!(json.contains("\"old_nickname\":\"alice\""));
        assert!(json.contains("\"new_nickname\":\"alicia\""));
        assert!(json.contains("\"is_admin\":true"));
    }

    #[test]
    fn test_serialize_user_info_with_nickname_and_is_shared() {
        let user_info = UserInfo {
            id: 1,
            is_away: false,
            status: None,
            username: "shared_acct".to_string(),
            nickname: "Nick1".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: true,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"shared_acct\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_info_regular_user() {
        let user_info = UserInfo {
            id: 1,
            is_away: false,
            status: None,
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"nickname\":\"alice\""));
        assert!(json.contains("\"is_shared\":false"));
    }

    #[test]
    fn test_serialize_user_info_detailed_with_shared_fields() {
        let user_info = UserInfoDetailed {
            id: 1,
            is_away: false,
            status: None,
            username: "shared_acct".to_string(),
            nickname: "Nick1".to_string(),
            login_time: 1234567890,
            is_shared: true,
            session_ids: vec![1, 2],
            features: vec!["chat".to_string()],
            created_at: 1234567800,
            locale: "en".to_string(),
            avatar: None,
            is_admin: false,
            addresses: None,
            channels: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"shared_acct\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_edit_response_with_is_shared() {
        let msg = ServerMessage::UserEditResponse {
            success: true,
            error: None,
            id: None,
            username: Some("shared_acct".to_string()),
            is_admin: Some(false),
            is_shared: Some(true),
            enabled: Some(true),
            permissions: Some(vec!["chat_send".to_string()]),
            group_id: None,
            group_name: None,
            group_permissions: None,
            revoked_permissions: None,
            available_groups: None,
            bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserEditResponse\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_edit_response_without_is_shared() {
        let msg = ServerMessage::UserEditResponse {
            success: true,
            error: None,
            id: None,
            username: Some("alice".to_string()),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(true),
            permissions: Some(vec![]),
            group_id: None,
            group_name: None,
            group_permissions: None,
            revoked_permissions: None,
            available_groups: None,
            bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserEditResponse\""));
        assert!(json.contains("\"is_shared\":false"));
    }

    #[test]
    fn test_serialize_user_message_with_nicknames() {
        let msg = ServerMessage::UserMessage {
            from_nickname: "Nick1".to_string(),
            from_admin: false,
            from_shared: false,
            to_nickname: "alice".to_string(),
            message: "Hello!".to_string(),
            action: ChatAction::Normal,
            timestamp: 1718234567,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserMessage\""));
        assert!(json.contains("\"from_nickname\":\"Nick1\""));
        assert!(json.contains("\"to_nickname\":\"alice\""));
    }

    #[test]
    fn test_serialize_user_disconnected_with_nickname() {
        let msg = ServerMessage::UserDisconnected {
            session_id: 1,
            nickname: "Nick1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserDisconnected\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_user_kick_response_with_nickname() {
        let msg = ServerMessage::UserKickResponse {
            success: true,
            error: None,
            nickname: Some("Nick1".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserKickResponse\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_client_user_info_with_nickname() {
        let msg = ClientMessage::UserInfo {
            nickname: "Nick1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserInfo\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_client_user_kick_with_nickname() {
        let msg = ClientMessage::UserKick {
            nickname: "Nick1".to_string(),
            reason: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserKick\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(!json.contains("\"reason\"")); // None should be skipped
    }

    #[test]
    fn test_serialize_client_user_kick_with_reason() {
        let msg = ClientMessage::UserKick {
            nickname: "Nick1".to_string(),
            reason: Some("spamming".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserKick\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"reason\":\"spamming\""));
    }

    #[test]
    fn test_serialize_client_user_message_with_to_nickname() {
        let msg = ClientMessage::UserMessage {
            to_nickname: "Nick1".to_string(),
            message: "Hello!".to_string(),
            action: ChatAction::Normal,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserMessage\""));
        assert!(json.contains("\"to_nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_file_move() {
        let msg = ClientMessage::FileMove {
            source_path: "/docs/file.txt".to_string(),
            destination_dir: "/archive".to_string(),
            overwrite: false,
            source_root: false,
            destination_root: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileMove\""));
        assert!(json.contains("\"source_path\":\"/docs/file.txt\""));
        assert!(json.contains("\"destination_dir\":\"/archive\""));
        // Default false values should not be serialized (serde default)
    }

    #[test]
    fn test_serialize_file_move_with_flags() {
        let msg = ClientMessage::FileMove {
            source_path: "file.txt".to_string(),
            destination_dir: "dest".to_string(),
            overwrite: true,
            source_root: true,
            destination_root: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"overwrite\":true"));
        assert!(json.contains("\"source_root\":true"));
    }

    #[test]
    fn test_deserialize_file_move_defaults() {
        let json = r#"{"type":"FileMove","source_path":"a.txt","destination_dir":"b"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileMove {
                source_path,
                destination_dir,
                overwrite,
                source_root,
                destination_root,
            } => {
                assert_eq!(source_path, "a.txt");
                assert_eq!(destination_dir, "b");
                assert!(!overwrite);
                assert!(!source_root);
                assert!(!destination_root);
            }
            _ => panic!("Expected FileMove"),
        }
    }

    #[test]
    fn test_serialize_file_copy() {
        let msg = ClientMessage::FileCopy {
            source_path: "/docs/file.txt".to_string(),
            destination_dir: "/backup".to_string(),
            overwrite: false,
            source_root: false,
            destination_root: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileCopy\""));
        assert!(json.contains("\"source_path\":\"/docs/file.txt\""));
        assert!(json.contains("\"destination_dir\":\"/backup\""));
    }

    #[test]
    fn test_deserialize_file_copy_with_flags() {
        let json = r#"{"type":"FileCopy","source_path":"a.txt","destination_dir":"b","overwrite":true,"source_root":false,"destination_root":true}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileCopy {
                overwrite,
                source_root,
                destination_root,
                ..
            } => {
                assert!(overwrite);
                assert!(!source_root);
                assert!(destination_root);
            }
            _ => panic!("Expected FileCopy"),
        }
    }

    #[test]
    fn test_serialize_file_move_response_success() {
        let msg = ServerMessage::FileMoveResponse {
            success: true,
            error: None,
            error_kind: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileMoveResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(!json.contains("\"error\""));
        assert!(!json.contains("\"error_kind\""));
    }

    #[test]
    fn test_serialize_file_move_response_error() {
        let msg = ServerMessage::FileMoveResponse {
            success: false,
            error: Some("File not found".to_string()),
            error_kind: Some("not_found".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"File not found\""));
        assert!(json.contains("\"error_kind\":\"not_found\""));
    }

    #[test]
    fn test_serialize_file_copy_response_success() {
        let msg = ServerMessage::FileCopyResponse {
            success: true,
            error: None,
            error_kind: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileCopyResponse\""));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_serialize_file_copy_response_exists_error() {
        let msg = ServerMessage::FileCopyResponse {
            success: false,
            error: Some("File already exists".to_string()),
            error_kind: Some("exists".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"error_kind\":\"exists\""));
    }

    #[test]
    fn test_serialize_file_download() {
        let msg = ClientMessage::FileDownload {
            path: "/Games".to_string(),
            root: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileDownload\""));
        assert!(json.contains("\"path\":\"/Games\""));
        assert!(json.contains("\"root\":false"));
    }

    #[test]
    fn test_deserialize_file_download() {
        let json = r#"{"type":"FileDownload","path":"/Games","root":true}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileDownload { path, root } => {
                assert_eq!(path, "/Games");
                assert!(root);
            }
            _ => panic!("Expected FileDownload"),
        }
    }

    #[test]
    fn test_deserialize_file_download_defaults() {
        let json = r#"{"type":"FileDownload","path":"/Games"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileDownload { path, root } => {
                assert_eq!(path, "/Games");
                assert!(!root);
            }
            _ => panic!("Expected FileDownload"),
        }
    }

    #[test]
    fn test_serialize_file_download_response_success() {
        let msg = ServerMessage::FileDownloadResponse {
            success: true,
            error: None,
            error_kind: None,
            size: Some(1048576),
            file_count: Some(10),
            transfer_id: Some("ffeeddcc".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileDownloadResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"size\":1048576"));
        assert!(json.contains("\"file_count\":10"));
        assert!(json.contains("\"transfer_id\":\"ffeeddcc\""));
    }

    #[test]
    fn test_serialize_file_download_response_empty_dir() {
        let msg = ServerMessage::FileDownloadResponse {
            success: true,
            error: None,
            error_kind: None,
            size: Some(0),
            file_count: Some(0),
            transfer_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"size\":0"));
        assert!(json.contains("\"file_count\":0"));
        assert!(!json.contains("\"transfer_id\""));
    }

    #[test]
    fn test_serialize_file_download_response_error() {
        let msg = ServerMessage::FileDownloadResponse {
            success: false,
            error: Some("Path not found".to_string()),
            error_kind: Some("not_found".to_string()),
            size: None,
            file_count: None,
            transfer_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Path not found\""));
        assert!(json.contains("\"error_kind\":\"not_found\""));
    }

    #[test]
    fn test_serialize_file_start() {
        let msg = ServerMessage::FileStart {
            path: "Games/app.zip".to_string(),
            size: 1048576,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileStart\""));
        assert!(json.contains("\"path\":\"Games/app.zip\""));
        assert!(json.contains("\"size\":1048576"));
    }

    #[test]
    fn test_deserialize_file_start() {
        let json = r#"{"type":"FileStart","path":"Documents/readme.txt","size":256}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::FileStart { path, size } => {
                assert_eq!(path, "Documents/readme.txt");
                assert_eq!(size, 256);
            }
            _ => panic!("Expected FileStart"),
        }
    }

    #[test]
    fn test_serialize_file_start_response_with_hash() {
        let msg = ClientMessage::FileStartResponse {
            size: 524288,
            blake3: Some("abc123def456".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileStartResponse\""));
        assert!(json.contains("\"size\":524288"));
        assert!(json.contains("\"blake3\":\"abc123def456\""));
        assert!(!json.contains("\"sha256\""));
    }

    #[test]
    fn test_serialize_file_start_response_no_hash() {
        let msg = ClientMessage::FileStartResponse {
            size: 0,
            blake3: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileStartResponse\""));
        assert!(json.contains("\"size\":0"));
        assert!(!json.contains("\"blake3\""));
        assert!(!json.contains("\"sha256\""));
    }

    #[test]
    fn test_deserialize_file_start_response() {
        let json = r#"{"type":"FileStartResponse","size":1024,"blake3":"hash123"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileStartResponse { size, blake3 } => {
                assert_eq!(size, 1024);
                assert_eq!(blake3, Some("hash123".to_string()));
            }
            _ => panic!("Expected FileStartResponse"),
        }
    }

    #[test]
    fn test_deserialize_file_start_response_no_hash() {
        let json = r#"{"type":"FileStartResponse","size":0}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileStartResponse { size, blake3 } => {
                assert_eq!(size, 0);
                assert_eq!(blake3, None);
            }
            _ => panic!("Expected FileStartResponse"),
        }
    }

    #[test]
    fn test_serialize_transfer_complete_success() {
        let msg = ServerMessage::TransferComplete {
            success: true,
            error: None,
            error_kind: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"TransferComplete\""));
        assert!(json.contains("\"success\":true"));
        assert!(!json.contains("\"error\""));
        assert!(!json.contains("\"error_kind\""));
    }

    #[test]
    fn test_serialize_transfer_complete_error() {
        let msg = ServerMessage::TransferComplete {
            success: false,
            error: Some("File not found".to_string()),
            error_kind: Some("not_found".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"TransferComplete\""));
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"File not found\""));
        assert!(json.contains("\"error_kind\":\"not_found\""));
    }

    #[test]
    fn test_deserialize_transfer_complete() {
        let json = r#"{"type":"TransferComplete","success":false,"error":"IO error","error_kind":"io_error"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::TransferComplete {
                success,
                error,
                error_kind,
            } => {
                assert!(!success);
                assert_eq!(error, Some("IO error".to_string()));
                assert_eq!(error_kind, Some("io_error".to_string()));
            }
            _ => panic!("Expected TransferComplete"),
        }
    }

    #[test]
    fn test_deserialize_file_download_response() {
        let json = r#"{"type":"FileDownloadResponse","success":true,"size":1048576,"file_count":10,"transfer_id":"aabbccdd"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::FileDownloadResponse {
                success,
                error,
                error_kind,
                size,
                file_count,
                transfer_id,
            } => {
                assert!(success);
                assert_eq!(error, None);
                assert_eq!(error_kind, None);
                assert_eq!(size, Some(1048576));
                assert_eq!(file_count, Some(10));
                assert_eq!(transfer_id, Some("aabbccdd".to_string()));
            }
            _ => panic!("Expected FileDownloadResponse"),
        }
    }

    #[test]
    fn test_server_info_with_transfer_fields() {
        let info = ServerInfo {
            name: Some("Test Server".to_string()),
            description: None,
            public_address: None,
            version: Some("0.5.0".to_string()),
            max_connections_per_ip: Some(5),
            max_transfers_per_ip: Some(3),
            image: None,
            transfer_port: 7501,
            transfer_websocket_port: Some(7503),
            file_reindex_interval: Some(5),
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            max_outbound_rate: None,
            scheduler_chunk_size: None,
            log_level: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"max_transfers_per_ip\":3"));
        assert!(json.contains("\"transfer_port\":7501"));
        assert!(json.contains("\"transfer_websocket_port\":7503"));

        // Test deserialization
        let parsed: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_transfers_per_ip, Some(3));
        assert_eq!(parsed.transfer_port, 7501);
        assert_eq!(parsed.transfer_websocket_port, Some(7503));
    }

    #[test]
    fn test_server_info_without_optional_fields() {
        // Missing optional fields default to None; transfer_port is required.
        let json = r#"{"name":"Old Server","version":"0.4.0","transfer_port":7501}"#;
        let info: ServerInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, Some("Old Server".to_string()));
        assert_eq!(info.max_transfers_per_ip, None);
        assert_eq!(info.transfer_port, 7501);
        assert_eq!(info.transfer_websocket_port, None);
    }

    #[test]
    fn test_server_info_log_level() {
        let info = ServerInfo {
            log_level: Some("info".to_string()),
            transfer_port: 7501,
            ..Default::default()
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"log_level\":\"info\""));

        let parsed: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.log_level, Some("info".to_string()));
    }

    // =========================================================================
    // Upload Protocol Tests
    // =========================================================================

    #[test]
    fn test_serialize_file_upload() {
        let msg = ClientMessage::FileUpload {
            destination: "/Uploads".to_string(),
            file_count: 5,
            total_size: 1048576,
            root: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileUpload\""));
        assert!(json.contains("\"destination\":\"/Uploads\""));
        assert!(json.contains("\"file_count\":5"));
        assert!(json.contains("\"total_size\":1048576"));
        assert!(!json.contains("\"root\":true"));
    }

    #[test]
    fn test_deserialize_file_upload() {
        let json = r#"{"type":"FileUpload","destination":"/My Uploads","file_count":10,"total_size":2097152,"root":true}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileUpload {
                destination,
                file_count,
                total_size,
                root,
            } => {
                assert_eq!(destination, "/My Uploads");
                assert_eq!(file_count, 10);
                assert_eq!(total_size, 2097152);
                assert!(root);
            }
            _ => panic!("Expected FileUpload"),
        }
    }

    #[test]
    fn test_deserialize_file_upload_defaults() {
        // root should default to false
        let json =
            r#"{"type":"FileUpload","destination":"/Uploads","file_count":1,"total_size":100}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileUpload { root, .. } => {
                assert!(!root);
            }
            _ => panic!("Expected FileUpload"),
        }
    }

    #[test]
    fn test_serialize_file_upload_response_success() {
        let msg = ServerMessage::FileUploadResponse {
            success: true,
            error: None,
            error_kind: None,
            transfer_id: Some("aabb1122".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileUploadResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"transfer_id\":\"aabb1122\""));
        assert!(!json.contains("\"error\""));
        assert!(!json.contains("\"error_kind\""));
    }

    #[test]
    fn test_serialize_file_upload_response_error() {
        let msg = ServerMessage::FileUploadResponse {
            success: false,
            error: Some("Permission denied".to_string()),
            error_kind: Some("permission".to_string()),
            transfer_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Permission denied\""));
        assert!(json.contains("\"error_kind\":\"permission\""));
        assert!(!json.contains("\"transfer_id\""));
    }

    #[test]
    fn test_deserialize_file_upload_response() {
        let json = r#"{"type":"FileUploadResponse","success":true,"transfer_id":"ccdd3344"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::FileUploadResponse {
                success,
                error,
                error_kind,
                transfer_id,
            } => {
                assert!(success);
                assert_eq!(error, None);
                assert_eq!(error_kind, None);
                assert_eq!(transfer_id, Some("ccdd3344".to_string()));
            }
            _ => panic!("Expected FileUploadResponse"),
        }
    }

    // =========================================================================
    // File Search Protocol Tests
    // =========================================================================

    #[test]
    fn test_serialize_file_search() {
        let msg = ClientMessage::FileSearch {
            query: "readme".to_string(),
            root: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileSearch\""));
        assert!(json.contains("\"query\":\"readme\""));
        assert!(!json.contains("\"root\":true"));
    }

    #[test]
    fn test_serialize_file_search_with_root() {
        let msg = ClientMessage::FileSearch {
            query: "config".to_string(),
            root: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileSearch\""));
        assert!(json.contains("\"query\":\"config\""));
        assert!(json.contains("\"root\":true"));
    }

    #[test]
    fn test_deserialize_file_search() {
        let json = r#"{"type":"FileSearch","query":"test.txt","root":true}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileSearch { query, root } => {
                assert_eq!(query, "test.txt");
                assert!(root);
            }
            _ => panic!("Expected FileSearch"),
        }
    }

    #[test]
    fn test_deserialize_file_search_default_root() {
        // root should default to false if omitted
        let json = r#"{"type":"FileSearch","query":"docs"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileSearch { query, root } => {
                assert_eq!(query, "docs");
                assert!(!root);
            }
            _ => panic!("Expected FileSearch"),
        }
    }

    #[test]
    fn test_serialize_file_reindex() {
        let msg = ClientMessage::FileReindex;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"FileReindex"}"#);
    }

    #[test]
    fn test_deserialize_file_reindex() {
        let json = r#"{"type":"FileReindex"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::FileReindex));
    }

    #[test]
    fn test_serialize_file_search_response_success() {
        let msg = ServerMessage::FileSearchResponse {
            success: true,
            error: None,
            results: Some(vec![FileSearchResult {
                path: "/Documents/report.pdf".to_string(),
                name: "report.pdf".to_string(),
                size: 12345,
                modified: 1700000000,
                is_directory: false,
            }]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileSearchResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"path\":\"/Documents/report.pdf\""));
        assert!(json.contains("\"name\":\"report.pdf\""));
        assert!(json.contains("\"size\":12345"));
        assert!(json.contains("\"is_directory\":false"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_file_search_response_error() {
        let msg = ServerMessage::FileSearchResponse {
            success: false,
            error: Some("Query too short".to_string()),
            results: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Query too short\""));
        assert!(!json.contains("\"results\""));
    }

    #[test]
    fn test_deserialize_file_search_response() {
        let json = r#"{"type":"FileSearchResponse","success":true,"results":[{"path":"/test.txt","name":"test.txt","size":100,"modified":1700000000,"is_directory":false}]}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::FileSearchResponse {
                success,
                error,
                results,
            } => {
                assert!(success);
                assert_eq!(error, None);
                let results = results.unwrap();
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].path, "/test.txt");
                assert_eq!(results[0].name, "test.txt");
                assert_eq!(results[0].size, 100);
                assert!(!results[0].is_directory);
            }
            _ => panic!("Expected FileSearchResponse"),
        }
    }

    #[test]
    fn test_serialize_file_reindex_response_success() {
        let msg = ServerMessage::FileReindexResponse {
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileReindexResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_file_reindex_response_error() {
        let msg = ServerMessage::FileReindexResponse {
            success: false,
            error: Some("Reindex already in progress".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Reindex already in progress\""));
    }

    #[test]
    fn test_deserialize_file_reindex_response() {
        let json = r#"{"type":"FileReindexResponse","success":false,"error":"Permission denied"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::FileReindexResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some("Permission denied".to_string()));
            }
            _ => panic!("Expected FileReindexResponse"),
        }
    }

    // =========================================================================
    // Mirror Message Type Tests
    //
    // These tests verify that message types with the same name in ClientMessage
    // and ServerMessage are true mirrors - they serialize to identical JSON.
    // This is a critical protocol invariant.
    // =========================================================================

    /// Helper macro to test that mirrored message types serialize identically
    /// and deserialize from the same JSON to both enums.
    macro_rules! assert_mirror {
        ($name:literal, $client:expr, $server:expr) => {{
            let client_json = serde_json::to_string(&$client).unwrap();
            let server_json = serde_json::to_string(&$server).unwrap();
            assert_eq!(
                client_json, server_json,
                "{} must serialize identically in both enums",
                $name
            );

            // Verify the JSON deserializes back to both enum types
            let _: ClientMessage = serde_json::from_str(&client_json).unwrap();
            let _: ServerMessage = serde_json::from_str(&server_json).unwrap();
        }};
    }

    #[test]
    fn test_mirror_messages_serialize_identically() {
        // FileStart
        assert_mirror!(
            "FileStart",
            ClientMessage::FileStart {
                path: "test/file.txt".to_string(),
                size: 12345,
            },
            ServerMessage::FileStart {
                path: "test/file.txt".to_string(),
                size: 12345,
            }
        );

        // FileStartResponse with hash
        assert_mirror!(
            "FileStartResponse",
            ClientMessage::FileStartResponse {
                size: 5000,
                blake3: Some("hash123".to_string()),
            },
            ServerMessage::FileStartResponse {
                size: 5000,
                blake3: Some("hash123".to_string()),
            }
        );

        // FileStartResponse without hash
        assert_mirror!(
            "FileStartResponse (no hash)",
            ClientMessage::FileStartResponse {
                size: 0,
                blake3: None,
            },
            ServerMessage::FileStartResponse {
                size: 0,
                blake3: None,
            }
        );

        // FileData
        assert_mirror!("FileData", ClientMessage::FileData, ServerMessage::FileData);

        // FileHashing
        assert_mirror!(
            "FileHashing",
            ClientMessage::FileHashing {
                file: "large-archive.zip".to_string(),
            },
            ServerMessage::FileHashing {
                file: "large-archive.zip".to_string(),
            }
        );

        // FileHash
        assert_mirror!(
            "FileHash",
            ClientMessage::FileHash {
                blake3: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
                    .to_string(),
            },
            ServerMessage::FileHash {
                blake3: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
                    .to_string(),
            }
        );
    }

    #[test]
    fn test_user_message_shared_type_name_not_mirror() {
        // UserMessage is a SHARED TYPE NAME but NOT a true mirror.
        // Client sends (to_nickname, message), server broadcasts (from_nickname, from_admin, to_nickname, message).
        let client_json = serde_json::to_string(&ClientMessage::UserMessage {
            to_nickname: "alice".to_string(),
            message: "Hello!".to_string(),
            action: ChatAction::Normal,
        })
        .unwrap();
        let server_json = serde_json::to_string(&ServerMessage::UserMessage {
            from_nickname: "bob".to_string(),
            from_admin: false,
            from_shared: false,
            to_nickname: "alice".to_string(),
            message: "Hello!".to_string(),
            action: ChatAction::Normal,
            timestamp: 1718234567,
        })
        .unwrap();

        // Same type name, different structure
        assert!(client_json.contains("\"type\":\"UserMessage\""));
        assert!(server_json.contains("\"type\":\"UserMessage\""));
        assert!(!client_json.contains("from_nickname"));
        assert!(server_json.contains("from_nickname"));
    }

    // =========================================================================
    // Group protocol message tests
    // =========================================================================

    #[test]
    fn test_serialize_group_list() {
        let msg = ClientMessage::GroupList;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"GroupList"}"#);
    }

    #[test]
    fn test_serialize_group_create() {
        let msg = ClientMessage::GroupCreate {
            name: "Moderators".to_string(),
            is_shared: false,
            permissions: vec!["chat_send".to_string(), "user_kick".to_string()],
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupCreate\""));
        assert!(json.contains("\"name\":\"Moderators\""));
        assert!(json.contains("\"is_shared\":false"));
        assert!(json.contains("\"permissions\":[\"chat_send\",\"user_kick\"]"));
    }

    #[test]
    fn test_serialize_group_create_shared() {
        let msg = ClientMessage::GroupCreate {
            name: "SharedGuests".to_string(),
            is_shared: true,
            permissions: vec!["chat_send".to_string()],
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_group_edit() {
        let msg = ClientMessage::GroupEdit { id: 42 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupEdit\""));
        assert!(json.contains("\"id\":42"));
    }

    #[test]
    fn test_serialize_group_update() {
        let msg = ClientMessage::GroupUpdate {
            id: 1,
            name: Some("NewName".to_string()),
            is_shared: None,
            permissions: Some(vec!["chat_send".to_string()]),
            bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupUpdate\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"name\":\"NewName\""));
        assert!(!json.contains("\"is_shared\""));
        assert!(json.contains("\"permissions\":[\"chat_send\"]"));
    }

    #[test]
    fn test_serialize_group_update_minimal() {
        let msg = ClientMessage::GroupUpdate {
            id: 5,
            name: None,
            is_shared: None,
            permissions: None,
            bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":5"));
        assert!(!json.contains("\"name\""));
        assert!(!json.contains("\"is_shared\""));
        assert!(!json.contains("\"permissions\""));
    }

    #[test]
    fn test_serialize_group_delete() {
        let msg = ClientMessage::GroupDelete { id: 7 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupDelete\""));
        assert!(json.contains("\"id\":7"));
    }

    #[test]
    fn test_serialize_group_list_response_success() {
        let msg = ServerMessage::GroupListResponse {
            success: true,
            error: None,
            groups: Some(vec![GroupInfo {
                id: 1,
                name: "Admins".to_string(),
                is_shared: false,
                member_count: 3,
                permissions: vec!["user_kick".to_string(), "ban_create".to_string()],
                bandwidth_weight: 1,
            }]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupListResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"name\":\"Admins\""));
        assert!(json.contains("\"is_shared\":false"));
        assert!(json.contains("\"member_count\":3"));
        assert!(json.contains("\"permissions\":[\"user_kick\",\"ban_create\"]"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_group_list_response_error() {
        let msg = ServerMessage::GroupListResponse {
            success: false,
            error: Some("Permission denied".to_string()),
            groups: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Permission denied\""));
        assert!(!json.contains("\"groups\""));
    }

    #[test]
    fn test_serialize_group_create_response_success() {
        let msg = ServerMessage::GroupCreateResponse {
            success: true,
            error: None,
            id: Some(42),
            name: Some("Moderators".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupCreateResponse\""));
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"name\":\"Moderators\""));
    }

    #[test]
    fn test_serialize_group_edit_response_success() {
        let msg = ServerMessage::GroupEditResponse {
            success: true,
            error: None,
            id: Some(1),
            name: Some("Editors".to_string()),
            is_shared: Some(false),
            permissions: Some(vec!["news_edit".to_string()]),
            member_count: Some(5),
            bandwidth_weight: Some(1),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupEditResponse\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"name\":\"Editors\""));
        assert!(json.contains("\"is_shared\":false"));
        assert!(json.contains("\"member_count\":5"));
        assert!(json.contains("\"permissions\":[\"news_edit\"]"));
    }

    #[test]
    fn test_serialize_group_update_response_success() {
        let msg = ServerMessage::GroupUpdateResponse {
            success: true,
            error: None,
            id: Some(1),
            name: Some("Editors".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupUpdateResponse\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"name\":\"Editors\""));
    }

    #[test]
    fn test_serialize_group_delete_response_success() {
        let msg = ServerMessage::GroupDeleteResponse {
            success: true,
            error: None,
            id: Some(3),
            name: Some("OldGroup".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"GroupDeleteResponse\""));
        assert!(json.contains("\"id\":3"));
        assert!(json.contains("\"name\":\"OldGroup\""));
    }

    #[test]
    fn test_serialize_group_delete_response_error() {
        let msg = ServerMessage::GroupDeleteResponse {
            success: false,
            error: Some("Cannot delete group while users are assigned to it".to_string()),
            id: None,
            name: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("Cannot delete group"));
        assert!(!json.contains("\"id\""));
        assert!(!json.contains("\"name\""));
    }

    #[test]
    fn test_deserialize_group_create() {
        let json =
            r#"{"type":"GroupCreate","name":"Mods","is_shared":false,"permissions":["chat_send"]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::GroupCreate {
                name,
                is_shared,
                permissions,
                bandwidth_weight,
            } => {
                assert_eq!(name, "Mods");
                assert!(!is_shared);
                assert_eq!(permissions, vec!["chat_send"]);
                assert_eq!(bandwidth_weight, 1);
            }
            _ => panic!("Expected GroupCreate"),
        }
    }

    #[test]
    fn test_deserialize_group_list_response() {
        let json = r#"{"type":"GroupListResponse","success":true,"groups":[{"id":1,"name":"Staff","is_shared":false,"member_count":2,"permissions":["user_kick"]}]}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::GroupListResponse {
                success,
                groups,
                error,
            } => {
                assert!(success);
                assert!(error.is_none());
                let groups = groups.unwrap();
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].id, 1);
                assert_eq!(groups[0].name, "Staff");
                assert!(!groups[0].is_shared);
                assert_eq!(groups[0].member_count, 2);
                assert_eq!(groups[0].permissions, vec!["user_kick"]);
                // Missing bandwidth_weight deserializes to schema default 1.
                assert_eq!(groups[0].bandwidth_weight, 1);
            }
            _ => panic!("Expected GroupListResponse"),
        }
    }

    #[test]
    fn test_serialize_user_create_with_group() {
        let msg = ClientMessage::UserCreate {
            username: "alice".to_string(),
            password: "secret".to_string(),
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: vec!["chat_send".to_string()],
            group_id: Some(5),
            revokes: Some(vec!["file_upload".to_string()]),
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"group_id\":5"));
        assert!(json.contains("\"revokes\":[\"file_upload\"]"));
    }

    #[test]
    fn test_serialize_user_create_without_group() {
        let msg = ClientMessage::UserCreate {
            username: "bob".to_string(),
            password: "pass".to_string(),
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: vec![],
            group_id: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("\"group_id\""));
        assert!(!json.contains("\"revokes\""));
    }

    #[test]
    fn test_deserialize_user_create_without_group_defaults() {
        // Missing group fields default to None.
        let json = r#"{"type":"UserCreate","username":"alice","password":"pw","is_admin":false,"enabled":true,"permissions":[]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserCreate {
                group_id, revokes, ..
            } => {
                assert!(group_id.is_none());
                assert!(revokes.is_none());
            }
            _ => panic!("Expected UserCreate"),
        }
    }

    #[test]
    fn test_serialize_user_update_with_group() {
        let msg = ClientMessage::UserUpdate {
            id: 1,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(3),
            remove_group: None,
            revokes: Some(vec!["news_edit".to_string()]),
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"group_id\":3"));
        assert!(json.contains("\"revokes\":[\"news_edit\"]"));
        assert!(!json.contains("\"remove_group\""));
    }

    #[test]
    fn test_serialize_user_update_remove_group() {
        let msg = ClientMessage::UserUpdate {
            id: 1,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: Some(true),
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"remove_group\":true"));
        assert!(!json.contains("\"group_id\""));
    }

    #[test]
    fn test_deserialize_user_update_without_group_defaults() {
        // Missing group fields default to None.
        let json = r#"{"type":"UserUpdate","id":1}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserUpdate {
                group_id,
                remove_group,
                revokes,
                ..
            } => {
                assert!(group_id.is_none());
                assert!(remove_group.is_none());
                assert!(revokes.is_none());
            }
            _ => panic!("Expected UserUpdate"),
        }
    }

    #[test]
    fn test_serialize_login_response_with_group() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            error: None,
            session_id: Some(1),
            user_id: None,
            is_admin: Some(false),
            permissions: Some(vec!["chat_send".to_string()]),
            features: Some(vec!["chat".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
            nickname: Some("alice".to_string()),
            group_id: Some(2),
            group_name: Some("Editors".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"group_id\":2"));
        assert!(json.contains("\"group_name\":\"Editors\""));
    }

    #[test]
    fn test_deserialize_login_response_without_group_defaults() {
        // Missing group fields default to None.
        let json = r#"{"type":"LoginResponse","success":true,"session_id":1,"is_admin":false,"permissions":[],"locale":"en","nickname":"alice"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::LoginResponse {
                group_id,
                group_name,
                user_id,
                features,
                ..
            } => {
                assert!(group_id.is_none());
                assert!(group_name.is_none());
                assert!(user_id.is_none());
                assert!(features.is_none());
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[test]
    fn test_serialize_user_edit_response_with_group() {
        let msg = ServerMessage::UserEditResponse {
            success: true,
            error: None,
            id: Some(1),
            username: Some("alice".to_string()),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(true),
            permissions: Some(vec!["chat_send".to_string()]),
            group_id: Some(1),
            group_name: Some("Staff".to_string()),
            group_permissions: Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            revoked_permissions: Some(vec!["user_kick".to_string()]),
            available_groups: Some(vec![GroupInfo {
                id: 1,
                name: "Staff".to_string(),
                is_shared: false,
                member_count: 3,
                permissions: vec!["chat_send".to_string(), "user_kick".to_string()],
                bandwidth_weight: 1,
            }]),
            bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"group_id\":1"));
        assert!(json.contains("\"group_name\":\"Staff\""));
        assert!(json.contains("\"group_permissions\":[\"chat_send\",\"user_kick\"]"));
        assert!(json.contains("\"revoked_permissions\":[\"user_kick\"]"));
        assert!(json.contains("\"available_groups\":[{"));
    }

    #[test]
    fn test_deserialize_user_edit_response_without_group_defaults() {
        // Missing group fields default to None.
        let json = r#"{"type":"UserEditResponse","success":true,"username":"alice","is_admin":false,"is_shared":false,"enabled":true,"permissions":["chat_send"]}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::UserEditResponse {
                id,
                group_id,
                group_name,
                group_permissions,
                revoked_permissions,
                available_groups,
                ..
            } => {
                assert!(id.is_none());
                assert!(group_id.is_none());
                assert!(group_name.is_none());
                assert!(group_permissions.is_none());
                assert!(revoked_permissions.is_none());
                assert!(available_groups.is_none());
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[test]
    fn test_serialize_permissions_updated_with_group() {
        let msg = ServerMessage::PermissionsUpdated {
            is_admin: false,
            permissions: vec!["chat_send".to_string()],
            server_info: None,
            group_id: Some(5),
            group_name: Some("Mods".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"group_id\":5"));
        assert!(json.contains("\"group_name\":\"Mods\""));
    }

    #[test]
    fn test_deserialize_permissions_updated_without_group_defaults() {
        // Missing group fields default to None.
        let json = r#"{"type":"PermissionsUpdated","is_admin":false,"permissions":["chat_send"]}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::PermissionsUpdated {
                group_id,
                group_name,
                ..
            } => {
                assert!(group_id.is_none());
                assert!(group_name.is_none());
            }
            _ => panic!("Expected PermissionsUpdated"),
        }
    }

    #[test]
    fn test_serialize_user_info_with_group() {
        let user = UserInfo {
            id: 1,
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1718234567,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
            is_away: false,
            status: None,
            group_id: Some(3),
            group_name: Some("Editors".to_string()),
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("\"group_id\":3"));
        assert!(json.contains("\"group_name\":\"Editors\""));
    }

    #[test]
    fn test_deserialize_user_info_without_group_defaults() {
        let json = r#"{"id":1,"username":"alice","nickname":"alice","login_time":0,"is_admin":false,"session_ids":[],"locale":"en","bandwidth_weight":1}"#;
        let user: UserInfo = serde_json::from_str(json).unwrap();
        assert!(user.group_id.is_none());
        assert!(user.group_name.is_none());
    }

    #[test]
    fn test_user_info_bandwidth_weight_is_required() {
        let basic = r#"{"id":1,"username":"alice","nickname":"alice","login_time":0,"is_admin":false,"session_ids":[],"locale":"en"}"#;
        let detailed = r#"{"id":1,"username":"alice","nickname":"alice","login_time":0,"is_admin":false,"session_ids":[],"features":[],"created_at":0,"locale":"en"}"#;

        assert!(serde_json::from_str::<UserInfo>(basic).is_err());
        assert!(serde_json::from_str::<UserInfoDetailed>(detailed).is_err());
    }

    #[test]
    fn test_serialize_group_info() {
        let info = GroupInfo {
            id: 10,
            name: "PowerUsers".to_string(),
            is_shared: false,
            member_count: 7,
            permissions: vec!["file_upload".to_string(), "news_create".to_string()],
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":10"));
        assert!(json.contains("\"name\":\"PowerUsers\""));
        assert!(json.contains("\"is_shared\":false"));
        assert!(json.contains("\"member_count\":7"));
        assert!(json.contains("\"permissions\":[\"file_upload\",\"news_create\"]"));
    }

    #[test]
    fn test_deserialize_group_info() {
        let json = r#"{"id":5,"name":"Staff","is_shared":true,"member_count":0,"permissions":[]}"#;
        let info: GroupInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, 5);
        assert_eq!(info.name, "Staff");
        assert!(info.is_shared);
        assert_eq!(info.member_count, 0);
        assert!(info.permissions.is_empty());
    }

    #[test]
    fn test_serialize_user_delete_with_id() {
        let msg = ClientMessage::UserDelete { id: 42 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserDelete\""));
        assert!(json.contains("\"id\":42"));
        assert!(!json.contains("\"username\""));
    }

    #[test]
    fn test_serialize_user_edit_with_id() {
        let msg = ClientMessage::UserEdit { id: 99 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserEdit\""));
        assert!(json.contains("\"id\":99"));
        assert!(!json.contains("\"username\""));
    }

    #[test]
    fn test_serialize_user_update_with_id() {
        let msg = ClientMessage::UserUpdate {
            id: 7,
            current_password: None,
            username: Some("newname".to_string()),
            password: None,
            is_admin: Some(true),
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":7"));
        assert!(json.contains("\"username\":\"newname\""));
        assert!(json.contains("\"is_admin\":true"));
        assert!(!json.contains("\"requested_"));
    }

    #[test]
    fn test_serialize_user_create_response_with_id() {
        let msg = ServerMessage::UserCreateResponse {
            success: true,
            error: None,
            id: Some(10),
            username: Some("alice".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":10"));
        assert!(json.contains("\"username\":\"alice\""));
    }

    #[test]
    fn test_serialize_user_update_response_with_id() {
        let msg = ServerMessage::UserUpdateResponse {
            success: true,
            error: None,
            id: Some(7),
            username: Some("newname".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":7"));
        assert!(json.contains("\"username\":\"newname\""));
    }

    #[test]
    fn test_serialize_login_response_with_user_id() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            error: None,
            session_id: Some(1),
            user_id: Some(42),
            is_admin: Some(false),
            permissions: Some(vec![]),
            features: Some(vec!["chat".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
            nickname: Some("alice".to_string()),
            group_id: None,
            group_name: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"user_id\":42"));
    }

    #[test]
    fn test_user_info_has_id_field() {
        let user = UserInfo {
            id: 55,
            username: "test".to_string(),
            nickname: "test".to_string(),
            login_time: 0,
            is_admin: false,
            is_shared: false,
            session_ids: vec![],
            locale: "en".to_string(),
            avatar: None,
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("\"id\":55"));
    }

    #[test]
    fn test_user_info_detailed_has_id_field() {
        let user = UserInfoDetailed {
            id: 77,
            username: "test".to_string(),
            nickname: "test".to_string(),
            login_time: 0,
            is_shared: false,
            session_ids: vec![],
            features: vec![],
            created_at: 0,
            locale: "en".to_string(),
            avatar: None,
            is_admin: false,
            addresses: None,
            is_away: false,
            status: None,
            channels: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: 1,
        };
        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("\"id\":77"));
    }

    #[test]
    fn test_debug_user_update_redacts_passwords() {
        let msg = ClientMessage::UserUpdate {
            id: 1,
            current_password: Some("old_secret".to_string()),
            username: Some("newname".to_string()),
            password: Some("new_secret".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
        };
        let debug_output = format!("{:?}", msg);
        assert!(!debug_output.contains("old_secret"));
        assert!(!debug_output.contains("new_secret"));
        // Both `current_password` and `password` must appear in redacted
        // form — silently omitting `current_password` would hide the wire
        // shape from anyone reading the debug output.
        assert!(debug_output.contains("current_password"));
        assert!(debug_output.contains("password"));
        assert_eq!(debug_output.matches(crate::REDACTED).count(), 2);
        assert!(debug_output.contains("newname"));
        assert!(debug_output.contains("id"));
    }

    #[test]
    fn test_debug_user_delete_uses_id() {
        let msg = ClientMessage::UserDelete { id: 42 };
        let debug_output = format!("{:?}", msg);
        assert!(debug_output.contains("UserDelete"));
        assert!(debug_output.contains("42"));
    }

    #[test]
    fn test_debug_user_edit_uses_id() {
        let msg = ClientMessage::UserEdit { id: 99 };
        let debug_output = format!("{:?}", msg);
        assert!(debug_output.contains("UserEdit"));
        assert!(debug_output.contains("99"));
    }

    #[test]
    fn test_deserialize_user_create_omits_bandwidth_fields() {
        // Missing bandwidth fields default to no override / no forced inheritance.
        let json = r#"{"type":"UserCreate","username":"alice","password":"x","is_admin":false,"is_shared":false,"enabled":true,"permissions":[]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserCreate {
                bandwidth_weight,
                inherit_bandwidth_weight,
                ..
            } => {
                assert!(bandwidth_weight.is_none());
                assert!(inherit_bandwidth_weight.is_none());
            }
            _ => panic!("Expected UserCreate"),
        }
    }

    #[test]
    fn test_user_update_inherit_bandwidth_weight_roundtrip() {
        let msg = ClientMessage::UserUpdate {
            id: 1,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: Some(true),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"inherit_bandwidth_weight\":true"));
        // Field omitted when None (matches the remove_group precedent).
        assert!(!json.contains("\"bandwidth_weight\""));
        // Roundtrip back.
        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            ClientMessage::UserUpdate {
                inherit_bandwidth_weight,
                bandwidth_weight,
                ..
            } => {
                assert_eq!(inherit_bandwidth_weight, Some(true));
                assert!(bandwidth_weight.is_none());
            }
            _ => panic!("Expected UserUpdate"),
        }
    }
}
