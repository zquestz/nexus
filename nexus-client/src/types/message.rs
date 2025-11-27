//! Message types for the Elm-style architecture

use crate::types::{NetworkConnection, ServerMessage};

/// Messages that drive the application state machine
#[derive(Debug, Clone)]
pub enum Message {
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
    /// Bookmark editor: Name field changed
    BookmarkNameChanged(String),
    /// Bookmark editor: Password field changed
    BookmarkPasswordChanged(String),
    /// Bookmark editor: Port field changed
    BookmarkPortChanged(String),
    /// Bookmark editor: Username field changed
    BookmarkUsernameChanged(String),
    /// Broadcast panel: Message input changed
    BroadcastMessageChanged(String),
    /// Bookmark editor: Cancel button pressed
    CancelBookmarkEdit,
    /// User edit panel: Cancel button pressed
    CancelEditUser,
    /// Connect to a bookmark by index
    ConnectToBookmark(usize),
    /// Connection form: Connect button pressed
    ConnectPressed,
    /// Network: Connection attempt completed
    ConnectionResult(Result<NetworkConnection, String>),
    /// Network: Bookmark connection attempt completed (with display name)
    BookmarkConnectionResult {
        result: Result<NetworkConnection, String>,
        bookmark_index: Option<usize>,
        display_name: String,
    },
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
    /// User edit panel: Edit button pressed (stage 1)
    EditUserPressed,
    /// User edit panel: Username field changed (stage 1)
    EditUsernameChanged(String),
    /// Keyboard or mouse event
    Event(iced::Event),
    /// Chat: Message input field changed
    ChatInputChanged(String),
    /// Network: Error occurred on connection
    NetworkError(usize, String),
    /// Connection form: Password field changed
    PasswordChanged(String),
    /// Connection form: Port field changed
    PortChanged(String),
    /// User list: User item clicked (expand/collapse)
    UserListItemClicked(String),
    /// User list: Info icon clicked on expanded user
    UserInfoIconClicked(String),
    /// User list: Message icon clicked on expanded user (private message - future)
    UserMessageIconClicked(String),
    /// User list: Kick icon clicked on expanded user (disconnect - future)
    UserKickIconClicked(String),
    /// Switch to a different chat tab
    SwitchChatTab(super::ChatTab),
    /// Close a user message tab
    CloseUserMessageTab(String),
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
    /// Bookmark list: Edit button pressed on bookmark
    ShowEditBookmark(usize),
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
    /// Toolbar: Show chat view
    ShowChatView,
    /// Toolbar: Toggle Edit User panel
    ToggleEditUser,
    /// Toolbar: Toggle light/dark theme
    ToggleTheme,
    /// Toolbar: Toggle user list sidebar
    ToggleUserList,
    /// User edit panel: Update button pressed (stage 2)
    UpdateUserPressed,
    /// Connection form: Username field changed
    UsernameChanged(String),
}
