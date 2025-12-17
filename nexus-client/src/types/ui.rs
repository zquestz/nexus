//! UI state and widget identifier types

use iced::widget::Id;

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

/// Certificate fingerprint mismatch details (without connection)
///
/// Used as return type from fingerprint verification to avoid creating
/// dummy NetworkConnection objects.
#[derive(Debug, Clone)]
pub struct FingerprintMismatchDetails {
    /// Bookmark index with mismatched fingerprint
    pub bookmark_index: usize,
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
}

/// Certificate fingerprint mismatch information (with connection)
///
/// Used in the mismatch queue for user verification.
#[derive(Debug, Clone)]
pub struct FingerprintMismatch {
    /// Bookmark index with mismatched fingerprint
    pub bookmark_index: usize,
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
    /// The network connection to complete if user accepts
    pub connection: crate::types::NetworkConnection,
    /// Display name for the connection
    pub display_name: String,
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
            InputId::BroadcastMessage => "InputId::BroadcastMessage",
            InputId::ChatInput => "InputId::ChatInput",
            InputId::ChangePasswordCurrent => "InputId::ChangePasswordCurrent",
            InputId::ChangePasswordNew => "InputId::ChangePasswordNew",
            InputId::ChangePasswordConfirm => "InputId::ChangePasswordConfirm",
            InputId::NewsBody => "InputId::NewsBody",
        })
    }
}

/// Scrollable area IDs for scroll position control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollableId {
    /// Chat messages scrollable area
    ChatMessages,
}

impl From<ScrollableId> for Id {
    fn from(id: ScrollableId) -> Self {
        Id::new(match id {
            ScrollableId::ChatMessages => "ScrollableId::ChatMessages",
        })
    }
}
