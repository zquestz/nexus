//! UI state and widget identifier types

use iced::widget::Id;
use uuid::Uuid;

/// Which panel is currently active in the main content area
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivePanel {
    /// No panel active (show chat)
    #[default]
    None,
    /// About panel
    About,
    /// Broadcast panel
    Broadcast,
    /// Change Password panel
    ChangePassword,
    /// Connection Monitor panel (view active connections)
    ConnectionMonitor,
    /// Settings panel
    Settings,
    /// Server Info panel
    ServerInfo,
    /// User Info panel (triggered by info icon click)
    UserInfo,
    /// User Management panel (create, edit, delete users)
    UserManagement,
    /// News panel (view, create, edit, delete news posts)
    News,
    /// Files panel (browse, upload, download files)
    Files,
    /// Transfers panel (download/upload progress, global)
    Transfers,
}

/// UI visibility state for toggleable panels
/// Global UI state that persists across connection changes
#[derive(Debug, Clone)]
pub struct UiState {
    /// Show bookmarks sidebar
    pub show_bookmarks: bool,
    /// Show user list sidebar
    pub show_user_list: bool,
    /// App-wide active panel (Settings, About) - takes precedence over connection panels
    pub active_panel: ActivePanel,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_bookmarks: true,
            show_user_list: true,
            active_panel: ActivePanel::None,
        }
    }
}

/// What to do if the user accepts a fingerprint mismatch and we need to
/// retry the connection. Carries the original `ConnectionParams` so retry
/// uses the exact same intent (credentials, locale, proxy) the user picked.
///
/// The variant determines which `Message::*ConnectionResult` the retry
/// dispatches to, preserving variant-specific behavior (URI path navigation,
/// bookmark error routing, manual-form lifecycle).
#[derive(Debug, Clone)]
pub enum ReconnectAction {
    /// Retry a manual-form connect. Form data is preserved across the
    /// dialog because the form is only cleared on connection success.
    Manual {
        params: crate::network::types::ConnectionParams,
    },
    /// Retry a bookmark connect.
    Bookmark {
        params: crate::network::types::ConnectionParams,
        bookmark_id: Uuid,
        display_name: String,
    },
    /// Retry a `nexus://` URI connect, preserving the path-navigation intent.
    Uri {
        params: crate::network::types::ConnectionParams,
        display_name: String,
        path: Option<crate::uri::NexusPath>,
    },
}

/// Certificate fingerprint mismatch entry in the user-verification queue.
#[derive(Debug, Clone)]
pub struct FingerprintMismatch {
    /// Bookmark ID with mismatched fingerprint
    pub bookmark_id: Uuid,
    /// Expected fingerprint (stored)
    pub expected: String,
    /// Received fingerprint (new)
    pub received: String,
    /// Bookmark name for display
    pub bookmark_name: String,
    /// Server address (IP or hostname)
    pub server_address: String,
    /// Server port
    pub server_port: String,
    /// What to dispatch on accept.
    pub retry_action: ReconnectAction,
}

/// Text input IDs for focus management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputId {
    /// Connection form: Server name input
    ServerName,
    /// Connection form: Server address input
    ServerAddress,
    /// Connection form: Port input
    Port,
    /// Connection form: Username input
    Username,
    /// Connection form: Password input
    Password,
    /// Bookmark editor: Name input
    BookmarkName,
    /// Bookmark editor: Address input
    BookmarkAddress,
    /// Bookmark editor: Port input
    BookmarkPort,
    /// Bookmark editor: Username input
    BookmarkUsername,
    /// Bookmark editor: Password input
    BookmarkPassword,
    /// Admin panel: Username input
    AdminUsername,
    /// Admin panel: Password input
    AdminPassword,
    /// Edit user panel: New username input
    EditNewUsername,
    /// Edit user panel: New password input
    EditNewPassword,
    /// Server info edit: Name input
    EditServerInfoName,
    /// Server info edit: Description input
    EditServerInfoDescription,
    /// Server info edit: Public address input
    EditServerInfoPublicAddress,
    /// Server info edit: Auto-join channels input
    EditServerInfoAutoJoinChannels,
    /// Server info edit: Persistent channels input
    EditServerInfoPersistentChannels,
    /// Server info edit: Max connections per IP input
    EditServerInfoMaxConnections,
    /// Server info edit: Max transfers per IP input
    EditServerInfoMaxTransfers,
    /// Server info edit: File reindex interval input
    EditServerInfoFileReindexInterval,
    /// Broadcast panel: Message input
    BroadcastMessage,
    /// Chat: Message input
    ChatInput,
    /// Password change: Current password input
    ChangePasswordCurrent,
    /// Password change: New password input
    ChangePasswordNew,
    /// Password change: Confirm password input
    ChangePasswordConfirm,
    /// News panel: Body text editor
    NewsBody,
    /// Connection form: Nickname input
    Nickname,
    /// Bookmark editor: Nickname input
    BookmarkNickname,
    /// Settings panel: Nickname input
    SettingsNickname,
    /// Settings panel: Proxy address input
    ProxyAddress,
    /// Settings panel: Proxy port input
    ProxyPort,
    /// Settings panel: Proxy username input
    ProxyUsername,
    /// Settings panel: Proxy password input
    ProxyPassword,
    /// Files panel: New directory name input
    NewDirectoryName,
    /// Files panel: Rename name input
    RenameName,
    /// Files panel: Search input
    FileSearchInput,
    /// Tracker management add form: Name input
    AddTrackerName,
    /// Tracker management add form: Address input
    AddTrackerAddress,
    /// Tracker management add form: Port input
    AddTrackerPort,
    /// Tracker management add form: Fingerprint input
    AddTrackerFingerprint,
    /// Tracker management add form: Password input
    AddTrackerPassword,
    /// Tracker management edit form: Name input
    EditTrackerName,
    /// Tracker management edit form: Address input
    EditTrackerAddress,
    /// Tracker management edit form: Port input
    EditTrackerPort,
    /// Tracker management edit form: Fingerprint input
    EditTrackerFingerprint,
    /// Tracker management edit form: Password input
    EditTrackerPassword,
    /// Group management edit form: Name input
    EditGroupName,
}

impl From<InputId> for Id {
    fn from(id: InputId) -> Self {
        Id::new(match id {
            InputId::ServerName => "InputId::ServerName",
            InputId::ServerAddress => "InputId::ServerAddress",
            InputId::Port => "InputId::Port",
            InputId::Username => "InputId::Username",
            InputId::Password => "InputId::Password",
            InputId::BookmarkName => "InputId::BookmarkName",
            InputId::BookmarkAddress => "InputId::BookmarkAddress",
            InputId::BookmarkPort => "InputId::BookmarkPort",
            InputId::BookmarkUsername => "InputId::BookmarkUsername",
            InputId::BookmarkPassword => "InputId::BookmarkPassword",
            InputId::AdminUsername => "InputId::AdminUsername",
            InputId::AdminPassword => "InputId::AdminPassword",
            InputId::EditNewUsername => "InputId::EditNewUsername",
            InputId::EditNewPassword => "InputId::EditNewPassword",
            InputId::EditServerInfoName => "InputId::EditServerInfoName",
            InputId::EditServerInfoDescription => "InputId::EditServerInfoDescription",
            InputId::EditServerInfoPublicAddress => "InputId::EditServerInfoPublicAddress",
            InputId::EditServerInfoAutoJoinChannels => "InputId::EditServerInfoAutoJoinChannels",
            InputId::EditServerInfoPersistentChannels => {
                "InputId::EditServerInfoPersistentChannels"
            }
            InputId::EditServerInfoMaxConnections => "InputId::EditServerInfoMaxConnections",
            InputId::EditServerInfoMaxTransfers => "InputId::EditServerInfoMaxTransfers",
            InputId::EditServerInfoFileReindexInterval => {
                "InputId::EditServerInfoFileReindexInterval"
            }
            InputId::BroadcastMessage => "InputId::BroadcastMessage",
            InputId::ChatInput => "InputId::ChatInput",
            InputId::ChangePasswordCurrent => "InputId::ChangePasswordCurrent",
            InputId::ChangePasswordNew => "InputId::ChangePasswordNew",
            InputId::ChangePasswordConfirm => "InputId::ChangePasswordConfirm",
            InputId::NewsBody => "InputId::NewsBody",
            InputId::Nickname => "InputId::Nickname",
            InputId::BookmarkNickname => "InputId::BookmarkNickname",
            InputId::SettingsNickname => "InputId::SettingsNickname",
            InputId::ProxyAddress => "InputId::ProxyAddress",
            InputId::ProxyPort => "InputId::ProxyPort",
            InputId::ProxyUsername => "InputId::ProxyUsername",
            InputId::ProxyPassword => "InputId::ProxyPassword",
            InputId::NewDirectoryName => "InputId::NewDirectoryName",
            InputId::RenameName => "InputId::RenameName",
            InputId::FileSearchInput => "InputId::FileSearchInput",
            InputId::AddTrackerName => "InputId::AddTrackerName",
            InputId::AddTrackerAddress => "InputId::AddTrackerAddress",
            InputId::AddTrackerPort => "InputId::AddTrackerPort",
            InputId::AddTrackerFingerprint => "InputId::AddTrackerFingerprint",
            InputId::AddTrackerPassword => "InputId::AddTrackerPassword",
            InputId::EditTrackerName => "InputId::EditTrackerName",
            InputId::EditTrackerAddress => "InputId::EditTrackerAddress",
            InputId::EditTrackerPort => "InputId::EditTrackerPort",
            InputId::EditTrackerFingerprint => "InputId::EditTrackerFingerprint",
            InputId::EditTrackerPassword => "InputId::EditTrackerPassword",
            InputId::EditGroupName => "InputId::EditGroupName",
        })
    }
}

/// Scrollable area IDs for scroll position control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollableId {
    /// Chat messages scrollable area
    ChatMessages,
    /// Files panel content area
    FilesContent,
}

impl From<ScrollableId> for Id {
    fn from(id: ScrollableId) -> Self {
        Id::new(match id {
            ScrollableId::ChatMessages => "ScrollableId::ChatMessages",
            ScrollableId::FilesContent => "ScrollableId::FilesContent",
        })
    }
}
