//! Message types for the Elm-style architecture

use iced::Theme;

use super::{ChatTab, NetworkConnection, ServerMessage};

/// Messages that drive the application state machine
#[derive(Debug, Clone)]
pub enum Message {
    /// Fingerprint mismatch: Accept button pressed (update stored fingerprint)
    AcceptNewFingerprint,
    /// Connection form: Add bookmark checkbox toggled
    AddBookmarkToggled(bool),
    /// Admin panel: Enabled checkbox toggled
    AdminEnabledToggled(bool),
    /// Admin panel: Is Admin checkbox toggled
    AdminIsAdminToggled(bool),
    /// Admin panel: Password field changed
    AdminPasswordChanged(String),
    /// Admin panel: Permission checkbox toggled
    AdminPermissionToggled(String, bool),
    /// Admin panel: Username field changed
    AdminUsernameChanged(String),
    /// Bookmark editor: Address field changed
    BookmarkAddressChanged(String),
    /// Bookmark editor: Auto-connect checkbox toggled
    BookmarkAutoConnectToggled(bool),
    /// Network: Bookmark connection attempt completed (with display name)
    BookmarkConnectionResult {
        result: Result<NetworkConnection, String>,
        bookmark_index: Option<usize>,
        display_name: String,
    },
    /// Bookmark editor: Name field changed
    BookmarkNameChanged(String),
    /// Bookmark editor: Password field changed
    BookmarkPasswordChanged(String),
    /// Bookmark editor: Port field changed
    BookmarkPortChanged(String),
    /// Bookmark editor: Username field changed
    BookmarkUsernameChanged(String),
    /// Broadcast: Message input changed
    BroadcastMessageChanged(String),
    /// User add panel: Cancel button pressed
    CancelAddUser,
    /// Bookmark editor: Cancel button pressed
    CancelBookmarkEdit,
    /// Broadcast panel: Cancel button pressed
    CancelBroadcast,
    /// User edit panel: Cancel button pressed
    CancelEditUser,
    /// Fingerprint mismatch: Cancel button pressed (reject new certificate)
    CancelFingerprintMismatch,
    /// Chat: Message input field changed
    ChatInputChanged(String),
    /// Chat scrollable: scroll position changed
    ChatScrolled(iced::widget::scrollable::Viewport),
    /// Close a user message tab
    CloseUserMessageTab(String),
    /// Connection form: Connect button pressed
    ConnectPressed,
    /// Connect to a bookmark by index
    ConnectToBookmark(usize),
    /// Network: Connection attempt completed
    ConnectionResult(Result<NetworkConnection, String>),
    /// Admin panel: Create User button pressed
    CreateUserPressed,
    /// Delete a bookmark by index
    DeleteBookmark(usize),
    /// Admin panel: Delete User button pressed
    DeleteUserPressed(String),
    /// Disconnect from server by connection_id
    DisconnectFromServer(usize),
    /// User edit panel: Enabled checkbox toggled
    EditEnabledToggled(bool),
    /// User edit panel: Is Admin checkbox toggled
    EditIsAdminToggled(bool),
    /// User edit panel: New password field changed
    EditNewPasswordChanged(String),
    /// User edit panel: New username field changed
    EditNewUsernameChanged(String),
    /// User edit panel: Permission checkbox toggled
    EditPermissionToggled(String, bool),
    /// User edit panel: Username field changed (stage 1)
    EditUsernameChanged(String),
    /// User edit panel: Edit button pressed (stage 1)
    EditUserPressed,
    /// Keyboard or mouse event
    Event(iced::Event),
    /// Network: Error occurred on connection
    NetworkError(usize, String),
    /// Connection form: Password field changed
    PasswordChanged(String),
    /// Connection form: Port field changed
    PortChanged(String),
    /// Bookmark editor: Save button pressed
    SaveBookmark,
    /// Broadcast panel: Send button pressed
    SendBroadcastPressed,
    /// Chat: Send message button pressed
    SendMessagePressed,
    /// Connection form: Server address field changed
    ServerAddressChanged(String),
    /// Network: Message received from server
    ServerMessageReceived(usize, ServerMessage),
    /// Connection form: Server name field changed
    ServerNameChanged(String),
    /// Bookmark list: Add Bookmark button pressed
    ShowAddBookmark,
    /// Toolbar: Show chat view
    ShowChatView,
    /// Bookmark list: Edit button pressed on bookmark
    ShowEditBookmark(usize),
    /// Switch to a different chat tab
    SwitchChatTab(ChatTab),
    /// Switch active view to connection by connection_id
    SwitchToConnection(usize),
    /// Keyboard: Tab key pressed
    TabPressed,
    /// Toolbar: Toggle Add User panel
    ToggleAddUser,
    /// Toolbar: Toggle bookmarks sidebar
    ToggleBookmarks,
    /// Toolbar: Toggle Broadcast panel
    ToggleBroadcast,
    /// Toolbar: Toggle Edit User panel
    ToggleEditUser,
    /// Settings panel: Cancel button pressed (restore original settings)
    CancelSettings,
    /// Settings panel: Connection notifications checkbox toggled
    ConnectionNotificationsToggled(bool),
    /// Settings panel: Save button pressed (persist to disk)
    SaveSettings,
    /// Toolbar: Toggle Settings panel
    ToggleSettings,
    /// Settings panel: Theme selected from picker
    ThemeSelected(Theme),
    /// Toolbar: Toggle user list sidebar
    ToggleUserList,
    /// User edit panel: Update button pressed (stage 2)
    UpdateUserPressed,
    /// User list: Info icon clicked on expanded user
    UserInfoIconClicked(String),
    /// User list: Kick icon clicked on expanded user (disconnect - future)
    UserKickIconClicked(String),
    /// User list: User item clicked (expand/collapse)
    UserListItemClicked(String),
    /// User list: Message icon clicked on expanded user (private message - future)
    UserMessageIconClicked(String),
    /// Connection form: Username field changed
    UsernameChanged(String),
    /// Broadcast: Validate broadcast form (on Enter when empty)
    ValidateBroadcast,
    /// Admin panel: Validate create user form (on Enter when form incomplete)
    ValidateCreateUser,
    /// User edit panel: Validate edit user form (on Enter when form incomplete)
    ValidateEditUser,
}
