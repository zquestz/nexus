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

/// Action type for chat and private messages
///
/// Determines how a message is rendered:
/// - `Normal`: Standard message with brackets (e.g., `<alice> hello`)
/// - `Me`: Action format (e.g., `*** alice waves`)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatAction {
    #[default]
    Normal,
    Me,
}

fn default_locale() -> String {
    "en".to_string()
}

/// Client request messages
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
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
    Handshake {
        version: String,
    },
    Login {
        username: String,
        password: String,
        features: Vec<String>,
        #[serde(default = "default_locale")]
        locale: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
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
    },
    UserDelete {
        username: String,
    },
    UserEdit {
        username: String,
    },
    UserInfo {
        nickname: String,
    },
    UserKick {
        nickname: String,
        /// Optional reason for the kick (shown to kicked user)
        #[serde(skip_serializing_if = "Option::is_none")]
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
    UserUpdate {
        username: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_permissions: Option<Vec<String>>,
    },
    /// Set away status for all sessions of this user
    UserAway {
        /// Optional status message (max 128 bytes)
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Clear away status for all sessions of this user
    UserBack,
    /// Set status message without changing away status
    UserStatus {
        /// Status message (None to clear)
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    ServerInfoUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_connections_per_ip: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_transfers_per_ip: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
        /// File reindex interval in minutes (0 to disable automatic reindexing)
        #[serde(skip_serializing_if = "Option::is_none")]
        file_reindex_interval: Option<u32>,
        /// Persistent channels (space-separated, survive restart)
        #[serde(skip_serializing_if = "Option::is_none")]
        persistent_channels: Option<String>,
        /// Auto-join channels (space-separated, joined on login)
        #[serde(skip_serializing_if = "Option::is_none")]
        auto_join_channels: Option<String>,
    },
    NewsList,
    NewsShow {
        id: i64,
    },
    NewsCreate {
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    NewsEdit {
        id: i64,
    },
    NewsUpdate {
        id: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    NewsDelete {
        id: i64,
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
    FileCreateDir {
        /// Parent directory path where the new directory should be created
        path: String,
        /// Name of the new directory to create
        name: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    FileDelete {
        /// Path to the file or empty directory to delete
        path: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    FileInfo {
        /// Path to the file or directory to get info for
        path: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    FileRename {
        /// Current path of the file or directory to rename
        path: String,
        /// New name (just the filename, not full path)
        new_name: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
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
    /// Request a file download (port 7501 only)
    FileDownload {
        /// Path to download (file or directory)
        path: String,
        /// If true, path is relative to file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Client response to FileStart - reports local file state for resume (downloads)
    FileStartResponse {
        /// Size of local file (0 if no local file exists)
        size: u64,
        /// SHA-256 hash of local file (None if size is 0)
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
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
    /// Client announces a file to upload (port 7501 only, mirrors ServerMessage::FileStart)
    FileStart {
        /// Relative path (e.g., "subdir/file.txt")
        path: String,
        /// File size in bytes
        size: u64,
        /// SHA-256 hash of complete file
        sha256: String,
    },
    /// Raw file data for upload (port 7501 only, mirrors ServerMessage::FileData)
    FileData,
    /// Keepalive sent while computing SHA-256 hash for a large file (port 7501 only)
    /// Receiver should reset idle timer but otherwise ignore this message.
    FileHashing {
        /// File being hashed (for logging/debugging)
        file: String,
    },
    /// Create or update an IP ban
    BanCreate {
        /// Target: nickname, IP address, or hostname
        target: String,
        /// Duration: "10m", "4h", "7d", "0" (permanent), or None (permanent)
        #[serde(skip_serializing_if = "Option::is_none")]
        duration: Option<String>,
        /// Reason for the ban (admin notes)
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Delete an IP ban
    BanDelete {
        /// Target: nickname (removes all IPs with that annotation) or IP address
        target: String,
    },
    /// Request list of active bans
    BanList,
    /// Add an IP to the trusted list (bypasses ban checks)
    TrustCreate {
        /// Target: nickname, IP address, or CIDR range
        target: String,
        /// Duration: "10m", "4h", "7d", "0" (permanent), or None (permanent)
        #[serde(skip_serializing_if = "Option::is_none")]
        duration: Option<String>,
        /// Reason for the trust entry (admin notes)
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Remove an IP from the trusted list
    TrustDelete {
        /// Target: nickname (removes all IPs with that annotation) or IP address
        target: String,
    },
    /// Request list of trusted IPs
    TrustList,
    /// Search files in the file area
    FileSearch {
        /// Search query (minimum 3 characters, literal match, case-insensitive)
        query: String,
        /// If true, search entire file root instead of user's area (requires file_root permission)
        #[serde(default)]
        root: bool,
    },
    /// Request a file index rebuild (admin command)
    FileReindex,
}

/// Helper for skip_serializing_if on ChatAction
fn is_normal_action(action: &ChatAction) -> bool {
    matches!(action, ChatAction::Normal)
}

/// Server response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
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
    },
    ChatTopicUpdated {
        topic: String,
        nickname: String,
        channel: String,
    },
    ChatTopicUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
    /// Response to ChatSecret request
    ChatSecretResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    HandshakeResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    LoginResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<String>,
        /// Channels the user was auto-joined to on login
        #[serde(skip_serializing_if = "Option::is_none")]
        channels: Option<Vec<ChannelJoinInfo>>,
    },
    ServerBroadcast {
        session_id: u32,
        username: String,
        message: String,
    },
    UserConnected {
        user: UserInfo,
    },
    UserCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
    UserEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
    },
    UserDisconnected {
        session_id: u32,
        nickname: String,
    },
    PermissionsUpdated {
        is_admin: bool,
        permissions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
    },
    ServerInfoUpdated {
        server_info: ServerInfo,
    },
    ServerInfoUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserBroadcastResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
        to_nickname: String,
        message: String,
        #[serde(default, skip_serializing_if = "is_normal_action")]
        action: ChatAction,
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
    UserUpdated {
        previous_username: String,
        user: UserInfo,
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
    /// Response to UserStatus request
    UserStatusResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
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
    NewsCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsUpdateResponse {
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
    NewsUpdated {
        action: NewsAction,
        id: i64,
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
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        can_upload: bool,
    },
    FileCreateDirResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Full path of the created directory (for client to navigate to)
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    FileDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    FileInfoResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        info: Option<FileInfoDetails>,
    },
    FileRenameResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    FileMoveResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error kind for client decision making
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    FileCopyResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error kind for client decision making
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
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
    /// Server announces a file to transfer (download, transfer port only)
    FileStart {
        /// Relative path (e.g., "Games/app.zip")
        path: String,
        /// File size in bytes
        size: u64,
        /// SHA-256 hash of complete file
        sha256: String,
    },
    /// Raw file data for download (transfer port only)
    FileData,
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
    /// Server response to client FileStart - reports server file state for resume (uploads)
    FileStartResponse {
        /// Size of file on server (0 if no file exists)
        size: u64,
        /// SHA-256 hash of server's partial file (None if size is 0)
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
    /// Server signals transfer completion (transfer port only)
    TransferComplete {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    /// Keepalive sent while computing SHA-256 hash for a large file (port 7501 only)
    /// Receiver should reset idle timer but otherwise ignore this message.
    FileHashing {
        /// File being hashed (for logging/debugging)
        file: String,
    },
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
    /// Response to FileSearch request
    FileSearchResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Search results (max 100)
        #[serde(skip_serializing_if = "Option::is_none")]
        results: Option<Vec<FileSearchResult>>,
    },
    /// Response to FileReindex request
    FileReindexResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    /// File reindex interval in minutes (0 = disabled)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_join_channels: Option<String>,
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
    pub author: String,
    pub author_is_admin: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Filesystem name (including any suffix like ` [NEXUS-UL]`; client strips for display)
    pub name: String,
    /// File size in bytes (0 for directories)
    pub size: u64,
    /// Last modified time as Unix timestamp
    pub modified: i64,
    /// Directory type (None = file, Some = directory with type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir_type: Option<String>,
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
    /// SHA-256 hash of file contents (files only, None for directories)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Detailed user info. `nickname` is the display name (== username for regular accounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoDetailed {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addresses: Option<Vec<String>>,
    #[serde(default)]
    pub is_away: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
                .field("password", &"<REDACTED>")
                .field("features", features)
                .field("locale", locale)
                .field(
                    "avatar",
                    &avatar.as_ref().map(|a| {
                        if a.len() > 50 {
                            format!("{}...<{} bytes>", &a[..50], a.len())
                        } else {
                            a.clone()
                        }
                    }),
                )
                .field("nickname", nickname)
                .finish(),
            ClientMessage::UserBroadcast { message } => f
                .debug_struct("UserBroadcast")
                .field("message", message)
                .finish(),
            ClientMessage::UserCreate {
                username,
                is_admin,
                is_shared,
                permissions,
                ..
            } => f
                .debug_struct("UserCreate")
                .field("username", username)
                .field("is_admin", is_admin)
                .field("is_shared", is_shared)
                .field("permissions", permissions)
                .field("password", &"<REDACTED>")
                .finish(),
            ClientMessage::UserDelete { username } => f
                .debug_struct("UserDelete")
                .field("username", username)
                .finish(),
            ClientMessage::UserEdit { username } => f
                .debug_struct("UserEdit")
                .field("username", username)
                .finish(),
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
            ClientMessage::UserUpdate {
                username,
                current_password: _,
                requested_username,
                requested_password: _,
                requested_is_admin,
                requested_enabled,
                requested_permissions,
            } => f
                .debug_struct("UserUpdate")
                .field("username", username)
                .field("requested_username", requested_username)
                .field("requested_password", &"<REDACTED>")
                .field("requested_is_admin", requested_is_admin)
                .field("requested_enabled", requested_enabled)
                .field("requested_permissions", requested_permissions)
                .finish(),
            ClientMessage::UserAway { message } => f
                .debug_struct("UserAway")
                .field("message", message)
                .finish(),
            ClientMessage::UserBack => f.debug_struct("UserBack").finish(),
            ClientMessage::UserStatus { status } => f
                .debug_struct("UserStatus")
                .field("status", status)
                .finish(),
            ClientMessage::ServerInfoUpdate {
                name,
                description,
                file_reindex_interval,
                max_connections_per_ip,
                max_transfers_per_ip,
                image,
                persistent_channels,
                auto_join_channels,
            } => {
                let mut s = f.debug_struct("ServerInfoUpdate");
                s.field("name", name)
                    .field("description", description)
                    .field("max_connections_per_ip", max_connections_per_ip)
                    .field("max_transfers_per_ip", max_transfers_per_ip)
                    .field("file_reindex_interval", file_reindex_interval)
                    .field("persistent_channels", persistent_channels)
                    .field("auto_join_channels", auto_join_channels);
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", &img[..100], img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::NewsList => f.debug_struct("NewsList").finish(),
            ClientMessage::NewsShow { id } => f.debug_struct("NewsShow").field("id", id).finish(),
            ClientMessage::NewsCreate { body, image } => {
                let mut s = f.debug_struct("NewsCreate");
                s.field("body", body);
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", &img[..100], img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::NewsEdit { id } => f.debug_struct("NewsEdit").field("id", id).finish(),
            ClientMessage::NewsUpdate { id, body, image } => {
                let mut s = f.debug_struct("NewsUpdate");
                s.field("id", id).field("body", body);
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", &img[..100], img.len()),
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
            ClientMessage::FileCreateDir { path, name, root } => f
                .debug_struct("FileCreateDir")
                .field("path", path)
                .field("name", name)
                .field("root", root)
                .finish(),
            ClientMessage::FileDelete { path, root } => f
                .debug_struct("FileDelete")
                .field("path", path)
                .field("root", root)
                .finish(),
            ClientMessage::FileInfo { path, root } => f
                .debug_struct("FileInfo")
                .field("path", path)
                .field("root", root)
                .finish(),
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
            ClientMessage::FileDownload { path, root } => f
                .debug_struct("FileDownload")
                .field("path", path)
                .field("root", root)
                .finish(),
            ClientMessage::FileStartResponse { size, sha256 } => f
                .debug_struct("FileStartResponse")
                .field("size", size)
                .field("sha256", sha256)
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
            ClientMessage::FileStart { path, size, sha256 } => f
                .debug_struct("FileStart")
                .field("path", path)
                .field("size", size)
                .field("sha256", sha256)
                .finish(),
            ClientMessage::FileData => f.debug_struct("FileData").finish(),
            ClientMessage::FileHashing { file } => {
                f.debug_struct("FileHashing").field("file", file).finish()
            }
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
            ClientMessage::FileSearch { query, root } => f
                .debug_struct("FileSearch")
                .field("query", query)
                .field("root", root)
                .finish(),
            ClientMessage::FileReindex => f.debug_struct("FileReindex").finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_serialize_login_response() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some(12345),
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"session_id\":12345"));
    }

    #[test]
    fn test_serialize_login_error() {
        let msg = ServerMessage::LoginResponse {
            success: false,
            session_id: None,
            is_admin: None,
            permissions: None,
            server_info: None,
            locale: None,
            channels: None,
            error: Some("Invalid credentials".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_login_response_admin() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some(99999),
            is_admin: Some(true),
            permissions: Some(vec![]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
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
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            channels: None,
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
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_serialize_user_info_without_avatar() {
        let user_info = UserInfo {
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
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(!json.contains("\"avatar\""));
    }

    #[test]
    fn test_serialize_user_info_detailed_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let user_info = UserInfoDetailed {
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_shared: false,
            session_ids: vec![1, 2],
            features: vec!["chat".to_string()],
            created_at: 1234567800,
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
            is_admin: Some(false),
            addresses: None,
            is_away: false,
            status: None,
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
    fn test_serialize_chat_message_with_is_admin_and_is_shared() {
        let msg = ServerMessage::ChatMessage {
            session_id: 1,
            nickname: "Nick1".to_string(),
            message: "Hello!".to_string(),
            is_admin: false,
            is_shared: true,
            action: ChatAction::Normal,
            channel: "#general".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ChatMessage\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_admin\":false"));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_info_with_nickname_and_is_shared() {
        let user_info = UserInfo {
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
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"shared_acct\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_info_regular_user() {
        let user_info = UserInfo {
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
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"nickname\":\"alice\""));
        assert!(json.contains("\"is_shared\":false"));
    }

    #[test]
    fn test_serialize_user_info_detailed_with_shared_fields() {
        let user_info = UserInfoDetailed {
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
            is_admin: Some(false),
            addresses: None,
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
            username: Some("shared_acct".to_string()),
            is_admin: Some(false),
            is_shared: Some(true),
            enabled: Some(true),
            permissions: Some(vec!["chat_send".to_string()]),
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
            username: Some("alice".to_string()),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(true),
            permissions: Some(vec![]),
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
            to_nickname: "alice".to_string(),
            message: "Hello!".to_string(),
            action: ChatAction::Normal,
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
            sha256: "abc123def456".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileStart\""));
        assert!(json.contains("\"path\":\"Games/app.zip\""));
        assert!(json.contains("\"size\":1048576"));
        assert!(json.contains("\"sha256\":\"abc123def456\""));
    }

    #[test]
    fn test_deserialize_file_start() {
        let json = r#"{"type":"FileStart","path":"Documents/readme.txt","size":256,"sha256":"fedcba987654"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::FileStart { path, size, sha256 } => {
                assert_eq!(path, "Documents/readme.txt");
                assert_eq!(size, 256);
                assert_eq!(sha256, "fedcba987654");
            }
            _ => panic!("Expected FileStart"),
        }
    }

    #[test]
    fn test_serialize_file_start_response_with_hash() {
        let msg = ClientMessage::FileStartResponse {
            size: 524288,
            sha256: Some("abc123def456".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileStartResponse\""));
        assert!(json.contains("\"size\":524288"));
        assert!(json.contains("\"sha256\":\"abc123def456\""));
    }

    #[test]
    fn test_serialize_file_start_response_no_hash() {
        let msg = ClientMessage::FileStartResponse {
            size: 0,
            sha256: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"FileStartResponse\""));
        assert!(json.contains("\"size\":0"));
        assert!(!json.contains("\"sha256\""));
    }

    #[test]
    fn test_deserialize_file_start_response() {
        let json = r#"{"type":"FileStartResponse","size":1024,"sha256":"hash123"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileStartResponse { size, sha256 } => {
                assert_eq!(size, 1024);
                assert_eq!(sha256, Some("hash123".to_string()));
            }
            _ => panic!("Expected FileStartResponse"),
        }
    }

    #[test]
    fn test_deserialize_file_start_response_no_hash() {
        let json = r#"{"type":"FileStartResponse","size":0}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::FileStartResponse { size, sha256 } => {
                assert_eq!(size, 0);
                assert_eq!(sha256, None);
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
            version: Some("0.5.0".to_string()),
            max_connections_per_ip: Some(5),
            max_transfers_per_ip: Some(3),
            image: None,
            transfer_port: 7501,
            file_reindex_interval: Some(5),
            persistent_channels: None,
            auto_join_channels: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"max_transfers_per_ip\":3"));
        assert!(json.contains("\"transfer_port\":7501"));

        // Test deserialization
        let parsed: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_transfers_per_ip, Some(3));
        assert_eq!(parsed.transfer_port, 7501);
    }

    #[test]
    fn test_server_info_without_optional_fields() {
        // Ensure backward compatibility - missing optional fields default to None
        // transfer_port is required, so must be present
        let json = r#"{"name":"Old Server","version":"0.4.0","transfer_port":7501}"#;
        let info: ServerInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, Some("Old Server".to_string()));
        assert_eq!(info.max_transfers_per_ip, None);
        assert_eq!(info.transfer_port, 7501);
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
                sha256: "abcdef123456".to_string(),
            },
            ServerMessage::FileStart {
                path: "test/file.txt".to_string(),
                size: 12345,
                sha256: "abcdef123456".to_string(),
            }
        );

        // FileStartResponse with hash
        assert_mirror!(
            "FileStartResponse",
            ClientMessage::FileStartResponse {
                size: 5000,
                sha256: Some("hash123".to_string()),
            },
            ServerMessage::FileStartResponse {
                size: 5000,
                sha256: Some("hash123".to_string()),
            }
        );

        // FileStartResponse without hash
        assert_mirror!(
            "FileStartResponse (no hash)",
            ClientMessage::FileStartResponse {
                size: 0,
                sha256: None,
            },
            ServerMessage::FileStartResponse {
                size: 0,
                sha256: None,
            }
        );

        // FileData
        assert_mirror!("FileData", ClientMessage::FileData, ServerMessage::FileData);
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
            to_nickname: "alice".to_string(),
            message: "Hello!".to_string(),
            action: ChatAction::Normal,
        })
        .unwrap();

        // Same type name, different structure
        assert!(client_json.contains("\"type\":\"UserMessage\""));
        assert!(server_json.contains("\"type\":\"UserMessage\""));
        assert!(!client_json.contains("from_nickname"));
        assert!(server_json.contains("from_nickname"));
    }
}
