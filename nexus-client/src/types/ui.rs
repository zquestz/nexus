//! UI state and widget identifier types

use iced::widget::{scrollable, text_input};

/// Which panel is currently active in the main content area
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivePanel {
    /// No panel active (show chat)
    #[default]
    None,
    /// About panel
    About,
    /// Add User panel
    AddUser,
    /// Edit User panel
    EditUser,
    /// Broadcast panel
    Broadcast,
    /// Settings panel
    Settings,
    /// Server Info panel
    ServerInfo,
}

/// UI visibility state for toggleable panels
#[derive(Debug, Clone)]
pub struct UiState {
    /// Show bookmarks sidebar
    pub show_bookmarks: bool,
    /// Show user list sidebar
    pub show_user_list: bool,
    /// Currently active panel in the main content area
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
    /// Edit user panel: Username input (stage 1)
    EditUsername,
    /// Edit user panel: New username input (stage 2)
    EditNewUsername,
    /// Edit user panel: New password input (stage 2)
    EditNewPassword,
    /// Broadcast panel: Message input
    BroadcastMessage,
    /// Chat: Message input
    ChatInput,
}

impl From<InputId> for text_input::Id {
    fn from(id: InputId) -> Self {
        text_input::Id::new(format!("{:?}", id))
    }
}

/// Scrollable area IDs for scroll position control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollableId {
    /// Chat messages scrollable area
    ChatMessages,
}

impl From<ScrollableId> for scrollable::Id {
    fn from(id: ScrollableId) -> Self {
        scrollable::Id::new(format!("{:?}", id))
    }
}
