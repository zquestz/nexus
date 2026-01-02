//! Message types for the Elm-style architecture

use iced::Theme;
use iced::widget::{markdown, text_editor};
use uuid::Uuid;

use nexus_common::framing::MessageId;

use super::form::{FileSortColumn, SettingsTab, TabId};
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
        bookmark_id: Option<Uuid>,
        display_name: String,
    },
    /// Bookmark editor: Name field changed
    BookmarkNameChanged(String),
    /// Bookmark editor: Password field changed
    BookmarkPasswordChanged(String),
    /// Bookmark editor: Port field changed
    BookmarkPortChanged(u16),
    /// Bookmark editor: Username field changed
    BookmarkUsernameChanged(String),
    /// Bookmark editor: Nickname field changed
    BookmarkNicknameChanged(String),
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
    /// Password change: Cancel button pressed (return to user info view)
    ChangePasswordCancelPressed,
    /// Password change: Confirm password field changed
    ChangePasswordConfirmChanged(String),
    /// Password change: Current password field changed
    ChangePasswordCurrentChanged(String),
    /// Password change: New password field changed
    ChangePasswordNewChanged(String),
    /// Password change: Change Password button pressed (enter password change mode)
    ChangePasswordPressed,
    /// Password change: Save button pressed (submit form)
    ChangePasswordSavePressed,
    /// Password change: Tab pressed, check focus and move to next field
    ChangePasswordTabPressed,
    /// Password change: Focus check result for Tab navigation (current, new, confirm)
    ChangePasswordFocusResult(bool, bool, bool),
    /// Bookmark edit: Tab pressed, check focus and move to next field
    BookmarkEditTabPressed,
    /// Bookmark edit: Focus check result for Tab navigation (name, address, port, username, password, nickname)
    BookmarkEditFocusResult(bool, bool, bool, bool, bool, bool),
    /// Connection form: Tab pressed, check focus and move to next field
    ConnectionFormTabPressed,
    /// Connection form: Focus check result for Tab navigation (name, address, port, username, password, nickname)
    ConnectionFormFocusResult(bool, bool, bool, bool, bool, bool),
    /// User management create: Tab pressed, check focus and move to next field
    UserManagementCreateTabPressed,
    /// User management create: Focus check result for Tab navigation (username, password)
    UserManagementCreateFocusResult(bool, bool),
    /// User management edit: Tab pressed, check focus and move to next field
    UserManagementEditTabPressed,
    /// User management edit: Focus check result for Tab navigation (username, password)
    UserManagementEditFocusResult(bool, bool),
    /// Server info edit: Tab pressed, check focus and move to next field
    ServerInfoEditTabPressed,
    /// Server info edit: Focus check result for Tab navigation (name, description)
    ServerInfoEditFocusResult(bool, bool),
    /// Settings panel: Tab pressed, check focus and move to next field
    SettingsTabPressed,
    /// Settings panel Network tab: Focus check result for Tab navigation (address, port, username, password)
    SettingsNetworkFocusResult(bool, bool, bool, bool),
    /// Chat: Message input field changed
    ChatInputChanged(String),
    /// Chat: Tab key pressed for nickname completion
    ChatTabComplete,
    /// Chat scrollable: scroll position changed
    ChatScrolled(iced::widget::scrollable::Viewport),
    /// Close a user message tab
    CloseUserMessageTab(String),
    /// Connection form: Connect button pressed
    ConnectPressed,
    /// Connect to a bookmark by ID
    ConnectToBookmark(Uuid),
    /// Network: Connection attempt completed
    ConnectionResult(Result<NetworkConnection, String>),
    /// Delete a bookmark by ID
    DeleteBookmark(Uuid),
    /// Disconnect from server by connection_id
    DisconnectFromServer(usize),

    /// Server info edit: Description field changed
    EditServerInfoDescriptionChanged(String),
    /// Server info edit: Image loaded from file picker (data URI or error)
    EditServerInfoImageLoaded(Result<String, ImagePickerError>),
    /// Server info edit: Max connections per IP field changed
    EditServerInfoMaxConnectionsChanged(u32),
    /// Server info edit: Max transfers per IP field changed
    EditServerInfoMaxTransfersChanged(u32),
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
    PortChanged(u16),
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
    /// Connection form: Nickname field changed
    NicknameChanged(String),
    /// Settings panel: Nickname field changed
    SettingsNicknameChanged(String),
    /// Bookmark list: Add Bookmark button pressed
    ShowAddBookmark,
    /// Toolbar: Show chat view
    ShowChatView,
    /// Bookmark list: Edit button pressed on bookmark
    ShowEditBookmark(Uuid),
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
    /// Settings panel: Tab selected
    SettingsTabSelected(SettingsTab),
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
    /// User management: Create user form - is shared account checkbox toggled
    UserManagementIsSharedToggled(bool),
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
    /// User list: Info icon clicked on expanded user (nickname)
    UserInfoIconClicked(String),
    /// User list: Kick icon clicked on expanded user (nickname)
    UserKickIconClicked(String),
    /// User list: User item clicked (expand/collapse) (nickname)
    UserListItemClicked(String),
    /// User list: Message icon clicked on expanded user (nickname)
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

    // ==================== News Management ====================
    /// Toolbar: Toggle News panel
    ToggleNews,
    /// News: Cancel button pressed (return to list or close panel)
    CancelNews,
    /// News: Create new post button clicked (switch to create form)
    NewsShowCreate,
    /// News: Edit button clicked on news item
    NewsEditClicked(i64),
    /// News: Delete button clicked on news item
    NewsDeleteClicked(i64),
    /// News: Confirm delete button pressed in modal
    NewsConfirmDelete,
    /// News: Cancel delete (close modal)
    NewsCancelDelete,
    /// News: Body editor action (used for both create and edit)
    NewsBodyAction(text_editor::Action),
    /// News: Pick image button pressed (create or edit)
    NewsPickImagePressed,
    /// News: Image loaded from file picker (create or edit)
    NewsImageLoaded(Result<String, ImagePickerError>),
    /// News: Clear image button pressed (create or edit)
    NewsClearImagePressed,
    /// News: Submit button pressed (create or edit)
    NewsSubmitPressed,

    // ==================== Files Panel ====================
    /// Toolbar: Toggle Files panel
    ToggleFiles,
    /// Files: Cancel button pressed (close panel)
    CancelFiles,
    /// Files: Navigate to a directory path
    FileNavigate(String),
    /// Files: Navigate up one directory level
    FileNavigateUp,
    /// Files: Navigate to home directory
    FileNavigateHome,
    /// Files: Refresh current directory listing
    FileRefresh,
    /// Files: Toggle between home and root view
    FileToggleRoot,
    /// Files: Toggle showing hidden files (dotfiles)
    FileToggleHidden,
    /// Files: New directory button clicked (open dialog)
    FileNewDirectoryClicked,
    /// Files: New directory name input changed
    FileNewDirectoryNameChanged(String),
    /// Files: New directory submit button pressed
    FileNewDirectorySubmit,
    /// Files: New directory cancel button pressed (close dialog)
    FileNewDirectoryCancel,
    /// Files: Delete clicked from context menu (path to delete)
    FileDeleteClicked(String),
    /// Files: Confirm delete button pressed in modal
    FileConfirmDelete,
    /// Files: Cancel delete (close modal)
    FileCancelDelete,
    /// Files: Info clicked from context menu (path to get info for)
    FileInfoClicked(String),
    /// Files: Close file info dialog
    CloseFileInfo,
    /// Files: Rename clicked from context menu (path to rename)
    FileRenameClicked(String),
    /// Files: Rename name input changed
    FileRenameNameChanged(String),
    /// Files: Rename submit button pressed
    FileRenameSubmit,
    /// Files: Rename cancel button pressed (close dialog)
    FileRenameCancel,
    /// Files: Cut clicked from context menu (path, name)
    FileCut(String, String),
    /// Files: Copy clicked from context menu (path, name)
    FileCopyToClipboard(String, String),
    /// Files: Paste to current directory
    FilePaste,
    /// Files: Paste into specific directory (from context menu on folder)
    FilePasteInto(String),
    /// Files: Clear clipboard (Escape key or context menu)
    FileClearClipboard,
    /// Files: Sort by column clicked
    FileSortBy(FileSortColumn),
    /// Files: Overwrite confirm button pressed in dialog
    FileOverwriteConfirm,
    /// Files: Overwrite cancel button pressed in dialog
    FileOverwriteCancel,
    /// Files: Create new tab (clones current tab's location/settings)
    FileTabNew,
    /// Files: Switch to tab by ID
    FileTabSwitch(TabId),
    /// Files: Close tab by ID
    FileTabClose(TabId),

    // ==================== Files Settings ====================
    /// Settings panel: Browse download path button pressed
    BrowseDownloadPathPressed,
    /// Settings panel: Download path selected from folder picker
    DownloadPathSelected(Option<String>),

    // ==================== Proxy Settings ====================
    /// Settings panel: Proxy enabled checkbox toggled
    ProxyEnabledToggled(bool),
    /// Settings panel: Proxy address field changed
    ProxyAddressChanged(String),
    /// Settings panel: Proxy port field changed
    ProxyPortChanged(u16),
    /// Settings panel: Proxy username field changed
    ProxyUsernameChanged(String),
    /// Settings panel: Proxy password field changed
    ProxyPasswordChanged(String),
}
