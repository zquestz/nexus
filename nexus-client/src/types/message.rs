//! Message types for the Elm-style architecture

use iced::Theme;
use iced::widget::markdown;

use nexus_common::framing::MessageId;

use super::{ChatTab, NetworkConnection, ServerMessage};
use crate::image::ImagePickerError;

/// Messages that drive the application state machine
#[derive(Debug, Clone)]
pub enum Message {
    /// Fingerprint mismatch: Accept button pressed (update stored fingerprint)
    AcceptNewFingerprint,
    /// Connection form: Add bookmark checkbox toggled
    AddBookmarkToggled(bool),
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
    /// User management: Cancel button pressed (return to list or close panel)
    CancelUserManagement,
    /// Bookmark editor: Cancel button pressed
    CancelBookmarkEdit,
    /// Broadcast panel: Cancel button pressed
    CancelBroadcast,
    /// Server info edit: Cancel button pressed (exit edit mode)
    CancelEditServerInfo,

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
    /// Delete a bookmark by index
    DeleteBookmark(usize),
    /// Disconnect from server by connection_id
    DisconnectFromServer(usize),

    /// Server info edit: Description field changed
    EditServerInfoDescriptionChanged(String),
    /// Server info edit: Image loaded from file picker (data URI or error)
    EditServerInfoImageLoaded(Result<String, ImagePickerError>),
    /// Server info edit: Max connections per IP field changed
    EditServerInfoMaxConnectionsChanged(u32),
    /// Server info edit: Name field changed
    EditServerInfoNameChanged(String),
    /// Server info edit: Edit button pressed (enter edit mode)
    EditServerInfoPressed,
    /// Server info edit: Pick image button pressed
    PickServerImagePressed,
    /// Server info edit: Clear image button pressed
    ClearServerImagePressed,
    /// Keyboard or mouse event
    Event(iced::Event),
    /// Keyboard: Navigate to next chat tab (Ctrl+Tab)
    NextChatTab,
    /// Network: Error occurred on connection
    NetworkError(usize, String),
    /// Connection form: Password field changed
    PasswordChanged(String),
    /// Connection form: Port field changed
    PortChanged(String),
    /// Keyboard: Navigate to previous chat tab (Ctrl+Shift+Tab)
    PrevChatTab,
    /// Bookmark editor: Save button pressed
    SaveBookmark,
    /// Broadcast panel: Send button pressed
    SendBroadcastPressed,
    /// Chat: Send message button pressed
    SendMessagePressed,
    /// Connection form: Server address field changed
    ServerAddressChanged(String),
    /// Network: Message received from server (connection_id, message_id, message)
    ServerMessageReceived(usize, MessageId, ServerMessage),
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
    /// Toolbar: Toggle bookmarks sidebar
    ToggleBookmarks,
    /// Toolbar: Toggle Broadcast panel
    ToggleBroadcast,
    /// Toolbar: Toggle User Management panel
    ToggleUserManagement,
    /// Settings panel: Cancel button pressed (restore original settings)
    CancelSettings,
    /// Settings panel: Chat font size selected from picker
    ChatFontSizeSelected(u8),
    /// Settings panel: Clear avatar button pressed
    ClearAvatarPressed,
    /// Settings panel: Connection notifications checkbox toggled
    ConnectionNotificationsToggled(bool),
    /// Settings panel: Avatar loaded from file picker (data URI or error)
    AvatarLoaded(Result<String, ImagePickerError>),
    /// Settings panel: Pick avatar button pressed
    PickAvatarPressed,
    /// Settings panel: Save button pressed (persist to disk)
    SaveSettings,
    /// Settings panel: Show seconds in timestamps toggled
    ShowSecondsToggled(bool),
    /// Settings panel: Show timestamps checkbox toggled
    ShowTimestampsToggled(bool),
    /// Toolbar: Toggle Settings panel
    ToggleSettings,
    /// Settings panel: Theme selected from picker
    ThemeSelected(Theme),
    /// About panel: URL link clicked
    OpenUrl(markdown::Uri),
    /// About panel: Close button pressed
    CloseAbout,
    /// Server info panel: Close button pressed
    CloseServerInfo,
    /// User info panel: Close button pressed
    CloseUserInfo,
    /// Toolbar: Show About panel
    ShowAbout,
    /// Toolbar: Show Server Info panel
    ShowServerInfo,
    /// Settings panel: Use 24-hour time format toggled
    Use24HourTimeToggled(bool),
    /// Toolbar: Toggle user list sidebar
    ToggleUserList,
    /// User management: Create user form - username field changed
    UserManagementUsernameChanged(String),
    /// User management: Create user form - password field changed
    UserManagementPasswordChanged(String),
    /// User management: Create user form - is admin checkbox toggled
    UserManagementIsAdminToggled(bool),
    /// User management: Create user form - enabled checkbox toggled
    UserManagementEnabledToggled(bool),
    /// User management: Create user form - permission checkbox toggled
    UserManagementPermissionToggled(String, bool),
    /// User management: Create user button pressed
    UserManagementCreatePressed,
    /// User management: Edit button clicked on user in list
    UserManagementEditClicked(String),
    /// User management: Delete button clicked on user in list
    UserManagementDeleteClicked(String),
    /// User management: Confirm delete button pressed in modal
    UserManagementConfirmDelete,
    /// User management: Cancel delete (close modal)
    UserManagementCancelDelete,
    /// User management: Create new user button clicked (switch to create form)
    UserManagementShowCreate,
    /// User management: Edit form - new username field changed
    UserManagementEditUsernameChanged(String),
    /// User management: Edit form - new password field changed
    UserManagementEditPasswordChanged(String),
    /// User management: Edit form - is admin checkbox toggled
    UserManagementEditIsAdminToggled(bool),
    /// User management: Edit form - enabled checkbox toggled
    UserManagementEditEnabledToggled(bool),
    /// User management: Edit form - permission checkbox toggled
    UserManagementEditPermissionToggled(String, bool),
    /// User management: Update user button pressed (in edit form)
    UserManagementUpdatePressed,
    /// Server info edit: Update button pressed (save changes)
    UpdateServerInfoPressed,
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
    /// User management: Validate create user form (on Enter when form incomplete)
    ValidateUserManagementCreate,
    /// User management: Validate edit user form (on Enter when form incomplete)
    ValidateUserManagementEdit,
    /// Window: Close requested - query size and position
    WindowCloseRequested(iced::window::Id),
    /// Window: Save settings and close (internal - after querying size and position)
    WindowSaveAndClose {
        id: iced::window::Id,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
    },
}
