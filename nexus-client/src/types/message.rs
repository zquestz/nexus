//! Message types for the Elm-style architecture

use crate::types::{NetworkConnection, ServerMessage};

/// Messages that drive the application state machine
#[derive(Debug, Clone)]
pub enum Message {
    // === Connection Form ===
    ServerNameChanged(String),
    ServerAddressChanged(String),
    PortChanged(String),
    UsernameChanged(String),
    PasswordChanged(String),
    ConnectPressed,

    // === Server Bookmarks ===
    ShowAddBookmark,
    ShowEditBookmark(usize),
    CancelBookmarkEdit,
    BookmarkNameChanged(String),
    BookmarkAddressChanged(String),
    BookmarkPortChanged(String),
    BookmarkUsernameChanged(String),
    BookmarkPasswordChanged(String),
    BookmarkAutoConnectToggled(bool),
    SaveBookmark,
    DeleteBookmark(usize),

    // === Main App ===
    MessageInputChanged(String),
    SendMessagePressed,
    RequestUserInfo(u32),
    /// Disconnect from specific server by connection_id
    DisconnectFromServer(usize),

    // === Server Switching ===
    /// Switch view to connection by connection_id
    SwitchToConnection(usize),
    /// Connect to a bookmark by bookmark index
    ConnectToBookmark(usize),

    // === Admin Panel ===
    AdminUsernameChanged(String),
    AdminPasswordChanged(String),
    AdminIsAdminToggled(bool),
    AdminPermissionToggled(String, bool),
    CreateUserPressed,
    DeleteUserPressed(String),
    DeleteUsernameChanged(String),

    // === UI Toggles ===
    ToggleBookmarks,
    ToggleUserList,
    ToggleAddUser,
    ToggleDeleteUser,

    // === Keyboard Events ===
    TabPressed,
    Event(iced::Event),

    // === Network Events ===
    ConnectionResult(Result<NetworkConnection, String>),
    /// Server message received: (connection_id, message)
    ServerMessageReceived(usize, ServerMessage),
    /// Network error occurred: (connection_id, error)
    NetworkError(usize, String),
}
