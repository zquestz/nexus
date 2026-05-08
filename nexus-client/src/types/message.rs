//! Message types for the Elm-style architecture

use iced::Theme;
use iced::widget::{markdown, text_editor};
use iced_toasts::ToastId;
use uuid::Uuid;

use nexus_common::framing::MessageId;
use nexus_common::protocol::FileSearchResult;
use nexus_common::validators::PasswordStrength;
use nexus_common::voice::VoiceQuality;

use super::panel::{
    FileSortColumn, GroupManagementSortColumn, SettingsTab, TabId, TrackerBrowserSortColumn,
    TrackerManagementSortColumn, UserManagementSortColumn, UserManagementTab,
};
use super::{ChatTab, NetworkConnection, ServerMessage};
use crate::config::audio::{PttMode, PttReleaseDelay};
use crate::config::events::{EventType, NotificationContent, SoundChoice};
use crate::image::ImagePickerError;
use crate::transfers::TransferEvent;
use crate::uri::NexusUri;
use crate::voice::audio::AudioDevice;
use crate::voice::manager::VoiceEvent;
use crate::voice::ptt::PttState;
use global_hotkey::GlobalHotKeyEvent;

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
    /// Network: Bookmark connection attempt completed (with display name).
    ///
    /// `params` carries the original connection parameters so that on a
    /// `FingerprintMismatch` the accept-and-retry path can be replayed
    /// faithfully (same credentials, locale, proxy, etc.).
    BookmarkConnectionResult {
        result: Result<NetworkConnection, crate::network::types::ConnectError>,
        params: crate::network::types::ConnectionParams,
        bookmark_id: Uuid,
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
    /// Bookmark editor: Optional fingerprint pin field changed
    BookmarkFingerprintChanged(String),
    /// Broadcast: Message input changed
    BroadcastMessageChanged(String),
    /// Group management: Cancel button pressed (return to list)
    CancelGroupManagement,
    /// User management: Cancel button pressed (return to list or close panel)
    CancelUserManagement,
    /// Bookmark editor: Cancel button pressed
    CancelBookmarkEdit,
    /// Bookmark editor: Cancel delete confirmation
    CancelDeleteBookmark,
    /// Broadcast panel: Cancel button pressed
    CancelBroadcast,
    /// Server info edit: Cancel button pressed (exit edit mode)
    CancelEditServerInfo,

    /// Fingerprint mismatch: Cancel button pressed (reject new certificate)
    CancelFingerprintMismatch,
    /// Fingerprint interception: OK button pressed (dismiss dialog)
    DismissFingerprintInterception,
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
    /// Password change: Tab pressed, move to next field
    ChangePasswordTabPressed,
    /// Password change: result of `find_focused` query — carries the
    /// id of the actually-focused widget for the resolver to advance.
    ChangePasswordTabResolved(iced::widget::Id),
    /// Bookmark Add/Edit: Tab pressed, move to next field
    BookmarkEditTabPressed,
    /// Bookmark Add/Edit: resolved focused id from `find_focused`.
    BookmarkEditTabResolved(iced::widget::Id),
    /// Connection form: Tab pressed, move to next field
    ConnectionFormTabPressed,
    /// Connection form: resolved focused id from `find_focused`.
    ConnectionFormTabResolved(iced::widget::Id),
    /// User management create: Tab pressed, move to next field
    UserManagementCreateTabPressed,
    /// User management create: resolved focused id.
    UserManagementCreateTabResolved(iced::widget::Id),
    /// User management edit: Tab pressed, move to next field
    UserManagementEditTabPressed,
    /// User management edit: resolved focused id.
    UserManagementEditTabResolved(iced::widget::Id),
    /// BBS-admin tracker Add form: Tab pressed, move to next field
    AddTrackerTabPressed,
    /// BBS-admin tracker Add form: resolved focused id.
    AddTrackerTabResolved(iced::widget::Id),
    /// BBS-admin tracker Edit form: Tab pressed, move to next field
    EditTrackerTabPressed,
    /// BBS-admin tracker Edit form: resolved focused id.
    EditTrackerTabResolved(iced::widget::Id),
    /// Server info edit: Tab pressed, move to next field
    ServerInfoEditTabPressed,
    /// Server info edit: resolved focused id.
    ServerInfoEditTabResolved(iced::widget::Id),
    /// Settings panel: Tab pressed, move to next field
    SettingsTabPressed,
    /// Settings panel: resolved focused id.
    SettingsTabResolved(iced::widget::Id),
    /// Group management create: Tab pressed
    GroupManagementCreateTabPressed,
    /// Group management create: resolved focused id.
    GroupManagementCreateTabResolved(iced::widget::Id),
    /// Group management edit: Tab pressed
    GroupManagementEditTabPressed,
    /// Group management edit: resolved focused id.
    GroupManagementEditTabResolved(iced::widget::Id),
    /// Chat: Message input field changed
    ChatInputChanged(String),
    /// Chat: result of the iced `find_focused` query for Tab. The
    /// resolved handler decides between nickname tab-completion (when
    /// chat input is already focused) and refocusing the chat input
    /// (when focus has wandered to a non-input widget).
    ChatTabResolved(iced::widget::Id),
    /// Chat scrollable: scroll position changed
    ChatScrolled(iced::widget::scrollable::Viewport),
    /// Close a channel tab (sends ChatLeave to server)
    CloseChannelTab(String),
    /// Close a user message tab
    CloseUserMessageTab(String),
    /// Connection form: Connect button pressed
    ConnectPressed,
    /// Connection form: Cancel button (or Escape) pressed. Restores
    /// `active_panel` to `connect_origin` if set, clears the form
    /// fields, and clears origin. No-op when origin is `None`
    /// (default-state form has no return target).
    ConnectionFormCancel,
    /// Open the global Connection form (server-list "+" entrypoint or
    /// any future summon path that doesn't pre-fill from a tracker).
    /// Captures the current active panel as `connect_origin` so Cancel
    /// returns there.
    OpenConnectionForm,
    /// Open the global Connection form pre-filled from a tracker
    /// discovery row (Name-cell click or context-menu Connect).
    /// Origin is set to `TrackerBrowser` so Cancel returns to the
    /// discovery panel. Only the four tracker-supplied fields are
    /// passed; username / password / nickname / `add_bookmark` are
    /// reset by the handler so a prior session can't leak in.
    OpenConnectFromTracker {
        name: String,
        address: String,
        port: u16,
        fingerprint: String,
    },
    /// Connect to a bookmark by ID
    ConnectToBookmark(Uuid),
    /// Network: Connection attempt completed (manual form connect).
    ///
    /// `params` carries the original connection parameters so that on a
    /// `FingerprintMismatch` the accept-and-retry path can be replayed
    /// faithfully.
    ConnectionResult {
        result: Result<NetworkConnection, crate::network::types::ConnectError>,
        params: crate::network::types::ConnectionParams,
    },
    /// Bookmark editor: Confirm delete after confirmation modal
    ConfirmDeleteBookmark(Uuid),
    /// Bookmark editor: Delete button pressed (shows confirmation modal)
    DeleteBookmark,
    /// Disconnect from server by connection_id
    DisconnectFromServer(usize),

    /// Server info edit: Chat burst limit changed
    EditServerInfoChatBurstLimitChanged(u32),
    /// Server info edit: Chat rate limit changed
    EditServerInfoChatRateLimitChanged(u32),
    /// Server info edit: Description field changed
    EditServerInfoDescriptionChanged(String),
    /// Server info edit: Image loaded from file picker (data URI or error)
    EditServerInfoImageLoaded(Result<String, ImagePickerError>),
    /// Server info edit: Max connections per IP field changed
    EditServerInfoMaxConnectionsChanged(u32),
    /// Server info edit: Max transfers per IP field changed
    EditServerInfoMaxTransfersChanged(u32),
    /// Server info edit: File reindex interval field changed
    EditServerInfoFileReindexIntervalChanged(u32),
    /// Server info edit: Name field changed
    EditServerInfoNameChanged(String),
    /// Server info edit: Persistent channels field changed
    EditServerInfoPersistentChannelsChanged(String),
    /// Server info edit: Public address field changed
    EditServerInfoPublicAddressChanged(String),
    /// Server info edit: Auto-join channels field changed
    EditServerInfoAutoJoinChannelsChanged(String),
    /// Server info edit: Min password strength changed
    EditServerInfoMinPasswordStrengthChanged(PasswordStrength),
    /// Server info edit: Edit button pressed (enter edit mode)
    EditServerInfoPressed,
    /// Server info display: Tab changed (tabs shown based on available data)
    ServerInfoTabChanged(super::ServerInfoTab),
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
    /// Network: Message received from server (connection_id, message_id, message, receive_timestamp)
    /// The timestamp is Some(Instant) for Pong messages (for accurate ping measurement), None otherwise
    ServerMessageReceived(usize, MessageId, ServerMessage, Option<std::time::Instant>),
    /// Connection form: Server name field changed
    ServerNameChanged(String),
    /// Connection form: Nickname field changed
    NicknameChanged(String),
    /// Connection form: Optional fingerprint pin field changed
    FingerprintChanged(String),
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
    /// Settings panel: Max scrollback lines changed
    MaxScrollbackChanged(usize),
    /// Settings panel: Chat history retention selected from picker
    ChatHistoryRetentionSelected(crate::config::settings::ChatHistoryRetention),
    /// Auto-away: periodic timer tick (every 30s when auto-away enabled)
    AutoAwayTick,
    /// Settings panel: Auto-away timeout selected from picker
    AutoAwayTimeoutSelected(crate::config::settings::AutoAwayTimeout),
    /// Settings panel: Auto-away message changed
    AutoAwayMessageChanged(String),
    /// Settings panel: Clear avatar button pressed
    ClearAvatarPressed,
    /// Settings panel: Connection notifications checkbox toggled
    ConnectionNotificationsToggled(bool),
    /// Settings panel: Channel join/leave notifications checkbox toggled
    ChannelNotificationsToggled(bool),
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
    /// Settings panel (Events tab): Event type selected from picker
    EventTypeSelected(EventType),
    /// Settings panel (Events tab): Global notifications toggle
    ToggleNotificationsEnabled(bool),
    /// Settings panel (Events tab): Show notification checkbox toggled
    EventShowNotificationToggled(bool),
    /// Settings panel (Events tab): Notification content level selected
    EventNotificationContentSelected(NotificationContent),
    /// Settings panel (Events tab): Test notification button pressed
    TestNotification,
    /// Settings panel (Events tab): Show toast checkbox toggled
    EventShowToastToggled(bool),
    /// Settings panel (Events tab): Toast content level selected
    EventToastContentSelected(NotificationContent),
    /// Settings panel (Events tab): Test toast button pressed
    TestToast,
    /// Settings panel (Events tab): Global sound toggle
    ToggleSoundEnabled(bool),
    /// Settings panel (Events tab): Sound volume slider changed
    SoundVolumeChanged(f32),
    /// Settings panel (Events tab): Play sound checkbox toggled
    EventPlaySoundToggled(bool),
    /// Settings panel (Events tab): Sound selected from picker
    EventSoundSelected(SoundChoice),
    /// Settings panel (Events tab): Always play sound checkbox toggled
    EventAlwaysPlaySoundToggled(bool),
    /// Settings panel (Events tab): Test sound button pressed
    TestSound,
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
    /// Toolbar: Toggle Transfers panel
    ToggleTransfers,
    /// Transfers panel: Close button pressed
    CloseTransfers,
    /// Toolbar: Toggle tracker discovery (Trackers) panel
    ToggleTrackerBrowser,
    /// Tracker browser: unconditionally close the panel (Escape on the
    /// list mode). Distinct from `ToggleTrackerBrowser` which no-ops
    /// when the panel is already active — matching the toolbar-button
    /// idiom but wrong for keyboard cancel.
    CloseTrackerBrowser,
    /// Tracker browser: dropdown selection changed
    TrackerBrowserSelectTracker(Uuid),
    /// Tracker browser: search-row input changed
    TrackerBrowserSearchInputChanged(String),
    /// Tracker browser: sortable table column header clicked
    TrackerBrowserSortChanged(TrackerBrowserSortColumn),
    /// Tracker browser: [+] toolbar button — open Add form
    TrackerBrowserShowAdd,
    /// Tracker browser: Edit toolbar button — open Edit form for the
    /// currently-selected tracker
    TrackerBrowserShowEdit,
    /// Tracker browser: Remove toolbar button — open ConfirmRemove
    /// modal for the currently-selected tracker
    TrackerBrowserShowRemove,
    /// Tracker browser: Cancel from any sub-mode (Add / Edit /
    /// ConfirmRemove). Resets to the list view.
    TrackerBrowserCancelMode,
    /// Tracker browser: Add form name field changed
    TrackerBrowserAddNameChanged(String),
    /// Tracker browser: Add form address field changed
    TrackerBrowserAddAddressChanged(String),
    /// Tracker browser: Add form port field changed
    TrackerBrowserAddPortChanged(u16),
    /// Tracker browser: Add form password field changed
    TrackerBrowserAddPasswordChanged(String),
    /// Tracker browser: Add form fingerprint field changed
    TrackerBrowserAddFingerprintChanged(String),
    /// Tracker browser: Add form submit (Save button or Enter on a
    /// completed form). Validates, persists, returns to list.
    TrackerBrowserAddSubmit,
    /// Tracker browser: Add form validate-then-submit fallback.
    /// Dispatched by Enter when the form is incomplete; surfaces a
    /// localized error in the form-level banner.
    TrackerBrowserValidateAdd,
    /// Tracker browser: Edit form name field changed
    TrackerBrowserEditNameChanged(String),
    /// Tracker browser: Edit form address field changed
    TrackerBrowserEditAddressChanged(String),
    /// Tracker browser: Edit form port field changed
    TrackerBrowserEditPortChanged(u16),
    /// Tracker browser: Edit form password field changed
    TrackerBrowserEditPasswordChanged(String),
    /// Tracker browser: Edit form fingerprint field changed
    TrackerBrowserEditFingerprintChanged(String),
    /// Tracker browser: Edit form submit
    TrackerBrowserEditSubmit,
    /// Tracker browser: Edit form validate-then-submit fallback
    TrackerBrowserValidateEdit,
    /// Tracker browser: ConfirmRemove modal — confirm removal
    TrackerBrowserRemoveConfirm,
    /// Tracker browser: Tab pressed in the Add form. Advances focus
    /// through the text inputs. Port is skipped because
    /// `iced_aw::NumberInput` consumes Tab internally.
    TrackerBrowserAddTabPressed,
    /// Tracker browser: Tab pressed in the Edit form.
    TrackerBrowserEditTabPressed,
    /// Tracker browser: result of the iced `find_focused` operation
    /// dispatched by `TrackerBrowserAddTabPressed`. Carries the id
    /// of whichever widget was actually focused at Tab time, so the
    /// cycle advances from the user's real focus rather than a
    /// stale `focused_field` tracker.
    TrackerBrowserAddTabResolved(iced::widget::Id),
    /// Tracker browser: same as the Add variant, for the Edit form.
    TrackerBrowserEditTabResolved(iced::widget::Id),
    /// Tracker browser: Refresh toolbar button pressed. Dispatches a
    /// one-shot `query_tracker` for the currently selected tracker;
    /// disabled when no tracker is selected or a query is in flight.
    TrackerBrowserRefresh,
    /// Tracker browser: AcceptFingerprint dialog Accept button.
    /// Distinct from `Message::AcceptFingerprintConfirm` (BBS-admin
    /// trackers panel) per the discovery-side namespace decision —
    /// the two flows mutate different state and must not collide.
    TrackerBrowserAcceptFingerprintConfirm,
    /// Tracker browser: AcceptFingerprint dialog Cancel button.
    /// Returns to the list view and surfaces the original Stage 1
    /// mismatch error in the toolbar status row.
    TrackerBrowserAcceptFingerprintCancel,
    /// Tracker browser: row context-menu "Bookmark" item. Directly
    /// creates a `ServerBookmark` from the four tracker-supplied
    /// fields with blank username / password / nickname. Runs the
    /// existing `check_bookmark_dedup`; collisions and save failures
    /// surface as toasts (no form opens).
    TrackerBrowserBookmarkRow {
        name: String,
        address: String,
        port: u16,
        fingerprint: String,
    },
    /// Tracker browser: row context-menu "Copy URI" item. Builds
    /// `nexus://{address}:{port}` and copies it to the clipboard.
    /// Reuses the existing `toast-link-copied` toast.
    TrackerBrowserCopyRowUri { address: String, port: u16 },
    /// Tracker browser: result of a `query_tracker` task. The handler
    /// drops the message silently if the cache entry is no longer
    /// `is_fetching` (stale-result-from-prior-query policy).
    TrackerQueryResult {
        tracker_id: Uuid,
        result: Result<crate::network::TrackerQueryOk, crate::network::TrackerQueryError>,
    },
    /// Toolbar: Toggle Connection Monitor panel
    ToggleConnectionMonitor,
    /// Connection Monitor panel: Close button pressed
    CloseConnectionMonitor,
    /// Connection Monitor panel: Refresh button pressed
    RefreshConnectionMonitor,
    /// Connection Monitor panel: Response received from server
    ConnectionMonitorResponse {
        connection_id: usize,
        success: bool,
        error: Option<String>,
        connections: Option<Vec<nexus_common::protocol::ConnectionInfo>>,
        transfers: Option<Vec<nexus_common::protocol::TransferInfo>>,
    },
    /// Connection monitor: Show user info for the selected user
    ConnectionMonitorInfo(String),
    /// Connection monitor: Kick the selected user (opens disconnect dialog with Kick pre-selected)
    ConnectionMonitorKick(String),
    /// Connection monitor: Ban the selected user (opens disconnect dialog with Ban pre-selected)
    ConnectionMonitorBan(String),
    /// Connection monitor: Copy value to clipboard
    ConnectionMonitorCopy(String),
    /// Connection monitor: Sort by column
    ConnectionMonitorSortBy(crate::types::ConnectionMonitorSortColumn),
    /// Connection monitor: Tab selected
    ConnectionMonitorTabSelected(crate::types::ConnectionMonitorTab),
    /// Connection monitor: Sort transfers by column
    ConnectionMonitorTransferSortBy(crate::types::TransferSortColumn),
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
    /// User management: Edit button clicked on user in list (user_id, username)
    UserManagementEditClicked(i64, String),
    /// User management: Delete button clicked on user in list (user_id, username)
    UserManagementDeleteClicked(i64, String),
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
    /// User management: Sort column clicked
    UserManagementSortBy(UserManagementSortColumn),
    /// User management: Tab selected (Users or Groups)
    UserManagementTabSelected(UserManagementTab),
    /// User management: Create form - group dropdown changed
    UserManagementGroupSelected(Option<i64>),
    /// User management: Edit form - group dropdown changed
    UserManagementEditGroupSelected(Option<i64>),
    /// User management: Refresh users list (Users tab toolbar)
    UserManagementRefreshUsers,
    /// User management: Refresh groups list (Groups tab toolbar)
    UserManagementRefreshGroups,

    // Group management (Groups tab within User Management panel)
    /// Group management: Show create form
    GroupManagementShowCreate,
    /// Group management: Create form - name field changed
    GroupManagementNameChanged(String),
    /// Group management: Create form - is shared checkbox toggled
    GroupManagementIsSharedToggled(bool),
    /// Group management: Create form - permission checkbox toggled
    GroupManagementPermissionToggled(String, bool),
    /// Group management: Create button pressed
    GroupManagementCreatePressed,
    /// Group management: Edit button clicked on group in list (group_id, name)
    GroupManagementEditClicked(i64, String),
    /// Group management: Delete button clicked on group in list (group_id, name)
    GroupManagementDeleteClicked(i64, String),
    /// Group management: Confirm delete button pressed in modal
    GroupManagementConfirmDelete,
    /// Group management: Cancel delete (close modal)
    GroupManagementCancelDelete,
    /// Group management: Edit form - name field changed
    GroupManagementEditNameChanged(String),
    /// Group management: Edit form - is shared checkbox toggled
    GroupManagementEditIsSharedToggled(bool),
    /// Group management: Edit form - permission checkbox toggled
    GroupManagementEditPermissionToggled(String, bool),
    /// Group management: Sort column clicked
    GroupManagementSortBy(GroupManagementSortColumn),
    /// Group management: Update button pressed (in edit form)
    GroupManagementUpdatePressed,

    // ==================== Tracker Management (Trackers tab inside Server Info) ====================
    /// Tracker management: Show Add subview (toolbar [+] button)
    TrackerManagementShowAdd,
    /// Tracker management: Edit row clicked (refetches via TrackerEdit)
    TrackerManagementShowEdit(i64),
    /// Tracker management: Remove row clicked (opens confirm modal) (id, name)
    TrackerManagementShowRemove(i64, String),
    /// Tracker management: Accept Fingerprint clicked from row context menu
    TrackerManagementShowAcceptFingerprint(i64),
    /// Tracker management: Cancel button (close form / modal / subview)
    CancelTrackerManagement,
    /// Tracker management: Refresh tracker list (toolbar refresh button)
    TrackerManagementRefresh,
    /// Tracker management: Sort column clicked
    TrackerManagementSortChanged(TrackerManagementSortColumn),
    /// Add form: Name field changed
    AddTrackerNameChanged(String),
    /// Add form: Address field changed
    AddTrackerAddressChanged(String),
    /// Add form: Port field changed
    AddTrackerPortChanged(u16),
    /// Add form: Fingerprint field changed
    AddTrackerFingerprintChanged(String),
    /// Add form: Password field changed
    AddTrackerPasswordChanged(String),
    /// Add form: Enabled checkbox toggled
    AddTrackerEnabledToggled(bool),
    /// Add form: Submit button pressed
    AddTrackerSubmit,
    /// Add form: Validate (called on Enter when form incomplete)
    ValidateAddTracker,
    /// Edit form: Name field changed
    EditTrackerNameChanged(String),
    /// Edit form: Address field changed
    EditTrackerAddressChanged(String),
    /// Edit form: Port field changed
    EditTrackerPortChanged(u16),
    /// Edit form: Fingerprint field changed
    EditTrackerFingerprintChanged(String),
    /// Edit form: Password field changed
    EditTrackerPasswordChanged(String),
    /// Edit form: Enabled checkbox toggled
    EditTrackerEnabledToggled(bool),
    /// Edit form: Submit button pressed
    EditTrackerSubmit,
    /// Edit form: Validate (called on Enter when form incomplete)
    ValidateEditTracker,
    /// Remove confirm: Confirm button pressed
    RemoveTrackerConfirm,
    /// Remove confirm: Cancel button pressed
    RemoveTrackerCancel,
    /// Accept Fingerprint dialog: Confirm button pressed
    AcceptFingerprintConfirm,
    /// Accept Fingerprint dialog: Cancel button pressed
    AcceptFingerprintCancel,

    /// Server info edit: Update button pressed (save changes)
    UpdateServerInfoPressed,
    /// User list: Info icon clicked on expanded user (nickname)
    UserInfoIconClicked(String),
    /// User list: Disconnect icon clicked on expanded user (nickname)
    /// Opens the disconnect dialog with kick/ban options
    DisconnectIconClicked(String),
    /// Disconnect dialog: Action changed (kick or ban)
    DisconnectDialogActionChanged(crate::types::DisconnectAction),
    /// Disconnect dialog: Ban duration changed
    DisconnectDialogDurationChanged(crate::types::BanDuration),
    /// Disconnect dialog: Ban reason changed
    DisconnectDialogReasonChanged(String),
    /// Disconnect dialog: Cancel button pressed
    DisconnectDialogCancel,
    /// Disconnect dialog: Submit button pressed (kick or ban)
    DisconnectDialogSubmit,
    /// User list: User item clicked (expand/collapse) (nickname)
    UserListItemClicked(String),
    /// User list: Message icon clicked on expanded user (nickname)
    UserMessageIconClicked(String),
    /// Connection form: Username field changed
    UsernameChanged(String),
    /// Bookmark editor: Validate add/edit form (on Enter when form incomplete)
    ValidateBookmarkEdit,
    /// Broadcast: Validate broadcast form (on Enter when empty)
    ValidateBroadcast,
    /// Change password: Validate the form (on Enter when form incomplete)
    ValidateChangePassword,
    /// Connection form: Validate the form (on Enter when form incomplete)
    ValidateConnectionForm,
    /// Group management: Validate create group form (on Enter when form incomplete)
    ValidateGroupManagementCreate,
    /// Group management: Validate edit group form (on Enter when form incomplete)
    ValidateGroupManagementEdit,
    /// User management: Validate create user form (on Enter when form incomplete)
    ValidateUserManagementCreate,
    /// User management: Validate edit user form (on Enter when form incomplete)
    ValidateUserManagementEdit,
    /// Window: Close requested - query size and position
    WindowCloseRequested(iced::window::Id),
    /// Window: Gained focus
    WindowFocused,
    /// Window: Lost focus
    WindowUnfocused,
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
    /// Files: Share - copy nexus:// link for file path to clipboard
    FileShare(String),
    /// Server info: Copy the server's public `nexus://` URI to the clipboard
    CopyServerUri(String),
    /// Server info: Copy the server's certificate fingerprint to the clipboard
    CopyServerFingerprint(String),
    /// Files: Download file (from context menu)
    FileDownload(String),
    /// Files: Download directory (from context menu or toolbar)
    FileDownloadAll(String),
    /// Files: Upload file(s) to a path (opens file picker)
    FileUpload(String),
    /// Files: File picker was cancelled (no-op, keeps panel open)
    FileUploadCancelled,
    /// Files: File picker returned selected files for upload
    FileUploadSelected(String, Vec<std::path::PathBuf>),
    /// Files: File being dragged over window (drag-and-drop)
    FileDragHovered,
    /// Files: File dropped on window (drag-and-drop)
    FileDragDropped(std::path::PathBuf),
    /// Files: Drag left window (drag-and-drop cancelled)
    FileDragLeft,

    // ==================== File Search ====================
    /// Files: Search input text changed
    FileSearchInputChanged(String),
    /// Files: Search submitted (Enter pressed or 🔍 button clicked)
    FileSearchSubmit,
    /// Files: Search result clicked (opens new tab)
    FileSearchResultClicked(FileSearchResult),
    /// Files: Search result context menu - Download
    FileSearchResultDownload(FileSearchResult),
    /// Files: Search result context menu - Info
    FileSearchResultInfo(FileSearchResult),
    /// Files: Search result context menu - Open (same as click)
    FileSearchResultOpen(FileSearchResult),
    /// Files: Search results sort by column clicked
    FileSearchSortBy(FileSortColumn),

    // ==================== Files Settings ====================
    /// Settings panel: Browse download path button pressed
    BrowseDownloadPathPressed,
    /// Settings panel: Download path selected from folder picker
    DownloadPathSelected(Option<String>),
    /// Settings panel: Queue transfers checkbox toggled
    QueueTransfersToggled(bool),
    /// Settings panel: Download limit changed
    DownloadLimitChanged(u8),
    /// Settings panel: Upload limit changed
    UploadLimitChanged(u8),

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

    // ==================== Transfers ====================
    /// Transfer: Progress event from executor
    TransferProgress(TransferEvent),
    /// Transfer: Pause a transfer
    TransferPause(Uuid),
    /// Transfer: Resume a paused transfer
    TransferResume(Uuid),
    /// Transfer: Cancel a transfer
    TransferCancel(Uuid),
    /// Transfer: Remove a completed/failed transfer from the list
    TransferRemove(Uuid),
    /// Transfer: Open the folder containing a transfer's local path
    TransferOpenFolder(Uuid),
    /// Transfer: Clear all inactive (completed and failed) transfers
    TransferClearInactive,
    /// Transfer: Move a queued transfer up (higher priority)
    TransferMoveUp(Uuid),
    /// Transfer: Move a queued transfer down (lower priority)
    TransferMoveDown(Uuid),
    /// Transfer: Retry a failed transfer (re-queue)
    TransferRetry(Uuid),

    // ==================== Voice ====================
    /// Voice: Join voice for a channel or user message
    VoiceJoinPressed(String),
    /// Voice: Leave current voice session
    VoiceLeavePressed,
    /// Voice: Event from voice session (DTLS connected, speaking, etc.)
    VoiceSessionEvent(usize, VoiceEvent),
    /// Voice: PTT state changed (called internally from VoicePttEvent handler)
    #[allow(dead_code)] // Constructed indirectly via handle_voice_ptt_state_changed
    VoicePttStateChanged(PttState),
    /// Voice: Raw PTT hotkey event (forwarded from global hotkey subscription)
    VoicePttEvent(GlobalHotKeyEvent),
    /// Voice: PTT release delay timer expired (time to actually stop transmitting)
    /// Contains generation counter to detect if PTT was pressed again during delay
    VoicePttReleaseDelayExpired(u64),
    /// Voice: Mute a user (client-side, stops hearing them)
    VoiceUserMute(String),
    /// Voice: Unmute a user
    VoiceUserUnmute(String),
    /// Voice: Toggle deafen (mute all incoming voice audio)
    VoiceDeafenToggle,
    /// Voice: VU meter tick (triggers UI update when transmitting)
    VoiceMeterTick,

    // ==================== Audio Settings ====================
    /// Audio: Refresh device list (re-enumerate audio devices)
    AudioRefreshDevices,
    /// Audio: Output device selected
    AudioOutputDeviceSelected(AudioDevice),
    /// Audio: Input device selected
    AudioInputDeviceSelected(AudioDevice),
    /// Audio: Voice quality selected
    AudioQualitySelected(VoiceQuality),
    /// Audio: Enter PTT key capture mode
    AudioPttKeyCapture,
    /// Audio: PTT key captured
    #[allow(dead_code)] // Will be emitted by keyboard event handler
    AudioPttKeyCaptured(String),
    /// Audio: PTT mode selected
    AudioPttModeSelected(PttMode),
    /// Audio: PTT release delay selected
    AudioPttReleaseDelaySelected(PttReleaseDelay),
    /// Audio: Start microphone test
    AudioTestMicStart,
    /// Audio: Stop microphone test
    AudioTestMicStop,
    /// Audio: Microphone level update (0.0 - 1.0)
    #[allow(dead_code)] // Will be emitted by mic test subscription
    AudioMicLevel(f32),
    /// Audio: Microphone test error
    #[allow(dead_code)] // Will be emitted by mic test subscription
    AudioMicError(String),
    /// Audio: Change noise suppression level
    AudioNoiseSuppressionLevel(crate::config::audio::NoiseSuppressionLevel),
    /// Audio: Toggle echo cancellation
    AudioEchoCancellation(bool),
    /// Audio: Toggle automatic gain control
    AudioAgc(bool),
    /// Audio: Toggle transient suppression (keyboard/click noise reduction)
    AudioTransientSuppression(bool),
    /// Audio: Change microphone boost level
    AudioMicBoost(crate::config::audio::MicBoost),

    // ==================== Toasts ====================
    /// Toast: Dismiss a toast notification
    ToastDismiss(ToastId),
    /// Toast: Show a toast notification (triggered after async operations like clipboard write)
    ShowToast(String),

    // ==================== System Tray (Windows/Linux only) ====================
    /// Tray: Icon was clicked (toggle window visibility)
    #[cfg(not(target_os = "macos"))]
    TrayIconClicked,
    /// Tray: Show/Hide window menu item selected
    #[cfg(not(target_os = "macos"))]
    TrayMenuShowHide,
    /// Tray: Mute/Unmute menu item selected
    #[cfg(not(target_os = "macos"))]
    TrayMenuMute,
    /// Tray: Quit menu item selected
    #[cfg(not(target_os = "macos"))]
    TrayMenuQuit,
    /// Tray: Hide window (internal, carries window geometry and maximized state)
    #[cfg(not(target_os = "macos"))]
    TrayHideWindow {
        id: iced::window::Id,
        was_maximized: bool,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
    },
    /// Tray: Show window (internal, restores maximized state if needed)
    #[cfg(not(target_os = "macos"))]
    TrayShowWindow(iced::window::Id),
    /// Tray: Restore minimized window (not tray-hidden, just OS-minimized)
    #[cfg(not(target_os = "macos"))]
    TrayRestoreMinimized {
        id: iced::window::Id,
        maximized: bool,
    },
    /// Tray: Deferred window + widget focus after window show/restore.
    /// Runs on the next update cycle so the window is ready to accept focus.
    /// Winit's `focus_window()` checks cached visibility flags that may not
    /// yet reflect `set_visible(true)` from the same batch, causing it to
    /// silently skip `force_window_active()`. Deferring ensures the flags
    /// are up to date.
    #[cfg(not(target_os = "macos"))]
    TrayRestoreFocus(iced::window::Id),
    /// Settings: Show tray icon toggled
    #[cfg(not(target_os = "macos"))]
    ShowTrayIconToggled(bool),
    /// Settings: Minimize to tray toggled
    #[cfg(not(target_os = "macos"))]
    MinimizeToTrayToggled(bool),
    /// Settings: Hardware rendering toggled (requires restart)
    HardwareRenderingToggled(bool),
    /// Settings: GPU backend selected (requires restart)
    GpuBackendSelected(crate::config::settings::GpuBackend),
    /// Tray: Service has closed (Linux: D-Bus connection dropped; Windows: tray receiver died)
    #[cfg(not(target_os = "macos"))]
    TrayServiceClosed,

    // ==================== URI Scheme ====================
    /// URI: Handle a nexus:// URI (from startup arg or IPC)
    HandleNexusUri(NexusUri),
    /// URI: Received from another instance via IPC
    UriReceivedFromIpc(String),
    /// IPC: Another instance requested we take focus
    FocusRequested,
    /// URI: Connection attempt from URI completed (shows errors in console).
    ///
    /// `params` carries the original connection parameters so that on a
    /// `FingerprintMismatch` the accept-and-retry path replays the URI
    /// flow including path navigation.
    UriConnectionResult {
        result: Result<NetworkConnection, crate::network::types::ConnectError>,
        params: crate::network::types::ConnectionParams,
        /// Display name for the connection (host:port)
        display_name: String,
        /// Path intent to navigate to after connection succeeds
        path: Option<crate::uri::NexusPath>,
    },
}
