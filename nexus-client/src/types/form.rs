//! Connection and user management form state

use std::sync::atomic::{AtomicU64, Ordering};

use crate::avatar::generate_identicon;
use crate::config::Config;
use crate::i18n::t;
use crate::image::{CachedImage, decode_data_uri_max_width, decode_data_uri_square};
use crate::style::{
    AVATAR_MAX_CACHE_SIZE, NEWS_IMAGE_MAX_CACHE_WIDTH, SERVER_IMAGE_MAX_CACHE_WIDTH,
};
use nexus_common::protocol::{NewsItem, UserInfo};
use nexus_common::{ALL_PERMISSIONS, DEFAULT_PORT};

// =============================================================================
// Password Change State
// =============================================================================

/// Password change form state (for User Info panel)
///
/// Tracks the form fields when a user is changing their own password.
#[derive(Debug, Clone)]
pub struct PasswordChangeState {
    /// Current password (required for verification)
    pub current_password: String,
    /// New password
    pub new_password: String,
    /// Confirm new password (must match new_password)
    pub confirm_password: String,
    /// Error message to display
    pub error: Option<String>,
    /// Panel to return to after cancel/success (e.g., UserInfo)
    pub return_to_panel: Option<super::ActivePanel>,
}

impl PasswordChangeState {
    /// Create a new empty password change state with a return panel
    pub fn new(return_to_panel: Option<super::ActivePanel>) -> Self {
        Self {
            current_password: String::new(),
            new_password: String::new(),
            confirm_password: String::new(),
            error: None,
            return_to_panel,
        }
    }
}

// =============================================================================
// News Management State
// =============================================================================

/// News management panel mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum NewsManagementMode {
    /// Showing list of all news items
    #[default]
    List,
    /// Creating a new news item
    Create,
    /// Editing an existing news item
    Edit {
        /// News item ID being edited
        id: i64,
    },
    /// Confirming deletion of a news item
    ConfirmDelete {
        /// News item ID to delete
        id: i64,
    },
}

/// News management panel state (per-connection)
///
/// Note: The body text is stored in `NexusApp.news_body_content` as a `text_editor::Content`
/// because it's not Clone. Only the image and error state are stored here.
#[derive(Clone)]
pub struct NewsManagementState {
    /// Current mode (list, create, edit, confirm delete)
    pub mode: NewsManagementMode,
    /// All news items (None = not loaded, Some(Ok) = loaded, Some(Err) = error)
    pub news_items: Option<Result<Vec<NewsItem>, String>>,
    /// Image data URI for form (used in both create and edit modes)
    pub form_image: String,
    /// Cached image for form preview
    pub cached_form_image: Option<CachedImage>,
    /// Error message for form (create or edit)
    pub form_error: Option<String>,
    /// Error message for list view
    pub list_error: Option<String>,
    /// Error message for delete confirmation dialog
    pub delete_error: Option<String>,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for NewsManagementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewsManagementState")
            .field("mode", &self.mode)
            .field("news_items", &self.news_items)
            .field("form_image", &format!("<{} bytes>", self.form_image.len()))
            .field(
                "cached_form_image",
                &self.cached_form_image.as_ref().map(|_| "<cached>"),
            )
            .field("form_error", &self.form_error)
            .field("list_error", &self.list_error)
            .finish()
    }
}

impl Default for NewsManagementState {
    fn default() -> Self {
        Self {
            mode: NewsManagementMode::List,
            news_items: None,
            form_image: String::new(),
            cached_form_image: None,
            form_error: None,
            list_error: None,
            delete_error: None,
        }
    }
}

impl NewsManagementState {
    /// Reset to list mode and clear all form state
    pub fn reset_to_list(&mut self) {
        self.mode = NewsManagementMode::List;
        self.clear_form();
        self.list_error = None;
    }

    /// Clear the form fields (used for both create and edit)
    pub fn clear_form(&mut self) {
        self.form_image.clear();
        self.cached_form_image = None;
        self.form_error = None;
    }

    /// Enter create mode
    pub fn enter_create_mode(&mut self) {
        self.clear_form();
        self.mode = NewsManagementMode::Create;
    }

    /// Enter edit mode for a news item (image pre-populated, body handled by text_editor)
    pub fn enter_edit_mode(&mut self, id: i64, image: Option<String>) {
        self.form_image = image.clone().unwrap_or_default();
        self.cached_form_image = if self.form_image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(&self.form_image, NEWS_IMAGE_MAX_CACHE_WIDTH)
        };
        self.form_error = None;

        self.mode = NewsManagementMode::Edit { id };
    }

    /// Enter confirm delete mode for a news item
    pub fn enter_confirm_delete_mode(&mut self, id: i64) {
        self.mode = NewsManagementMode::ConfirmDelete { id };
        self.delete_error = None;
    }
}

// =============================================================================
// Files Management State
// =============================================================================

/// Column to sort files by
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileSortColumn {
    /// Sort by name (default) - keeps dirs first, sorts within groups
    #[default]
    Name,
    /// Sort by size - full sort, mixes dirs and files
    Size,
    /// Sort by modified date - full sort, mixes dirs and files
    Modified,
}

/// Clipboard operation type (cut or copy)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOperation {
    /// Cut - file will be moved on paste
    Cut,
    /// Copy - file will be copied on paste
    Copy,
}

/// Item stored in clipboard for move/copy operations
#[derive(Debug, Clone)]
pub struct ClipboardItem {
    /// Full path to the file or directory
    pub path: String,
    /// Display name of the file or directory
    pub name: String,
    /// Cut or Copy operation
    pub operation: ClipboardOperation,
    /// Whether source was in root view mode when cut/copied
    pub root: bool,
}

/// Pending overwrite confirmation for move/copy operations
#[derive(Debug, Clone)]
pub struct PendingOverwrite {
    /// Source path of the file/directory
    pub source_path: String,
    /// Destination directory path
    pub destination_dir: String,
    /// Name of the file/directory (for display)
    pub name: String,
    /// True if this is a move operation, false if copy
    pub is_move: bool,
    /// Source root flag (from clipboard)
    pub source_root: bool,
    /// Destination root flag (from viewing_root at paste time)
    pub destination_root: bool,
}

// =============================================================================
// File Tab ID Generation
// =============================================================================

/// Global counter for generating unique tab IDs
static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a new unique tab ID
fn next_tab_id() -> TabId {
    NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed)
}

/// Unique identifier for a file browser tab
///
/// This ID is stable across tab reordering and is used to route
/// async responses back to the correct tab.
pub type TabId = u64;

/// A single file browser tab (per-tab state)
#[derive(Debug, Clone)]
pub struct FileTab {
    /// Unique identifier for this tab (stable across reordering)
    pub id: TabId,
    /// Current directory path (empty string means home/root)
    pub current_path: String,
    /// File entries in current directory (None = loading, Some = loaded)
    pub entries: Option<Vec<nexus_common::protocol::FileEntry>>,
    /// Error message for this tab
    pub error: Option<String>,
    /// Whether viewing from the file root (requires file_root permission)
    pub viewing_root: bool,
    /// Whether the current directory allows uploads (from FileListResponse)
    pub current_dir_can_upload: bool,
    /// Current sort column
    pub sort_column: FileSortColumn,
    /// Sort ascending (true) or descending (false)
    pub sort_ascending: bool,
    /// Cached sorted entries (updated when entries or sort settings change)
    pub sorted_entries: Option<Vec<nexus_common::protocol::FileEntry>>,
    /// Whether the "New Directory" dialog is open
    pub creating_directory: bool,
    /// New directory name input
    pub new_directory_name: String,
    /// New directory validation/creation error
    pub new_directory_error: Option<String>,
    /// Path pending deletion (for confirmation dialog)
    pub pending_delete: Option<String>,
    /// Error from delete operation (shown in delete dialog)
    pub delete_error: Option<String>,
    /// File/directory info to display (for info dialog)
    pub pending_info: Option<nexus_common::protocol::FileInfoDetails>,
    /// Path of file/directory being renamed (for rename dialog)
    pub pending_rename: Option<String>,
    /// New name input for rename dialog
    pub rename_name: String,
    /// Error message for rename dialog
    pub rename_error: Option<String>,
    /// Pending overwrite confirmation (when destination exists)
    pub pending_overwrite: Option<PendingOverwrite>,
}

impl Default for FileTab {
    fn default() -> Self {
        Self {
            id: next_tab_id(),
            current_path: String::new(),
            entries: None,
            error: None,
            viewing_root: false,
            current_dir_can_upload: false,
            sort_column: FileSortColumn::Name,
            sort_ascending: true,
            sorted_entries: None,
            creating_directory: false,
            new_directory_name: String::new(),
            new_directory_error: None,
            pending_delete: None,
            delete_error: None,
            pending_info: None,
            pending_rename: None,
            rename_name: String::new(),
            rename_error: None,
            pending_overwrite: None,
        }
    }
}

impl FileTab {
    /// Create a new tab copying another tab's location and sort settings
    ///
    /// The new tab will have a new unique ID, the same path, viewing_root,
    /// and sort settings, but entries will be loaded fresh (not copied).
    pub fn new_from_location(other: &FileTab) -> Self {
        Self {
            id: next_tab_id(),
            current_path: other.current_path.clone(),
            entries: None, // Will be loaded fresh
            error: None,
            viewing_root: other.viewing_root,
            current_dir_can_upload: false,
            sort_column: other.sort_column,
            sort_ascending: other.sort_ascending,
            sorted_entries: None,
            creating_directory: false,
            new_directory_name: String::new(),
            new_directory_error: None,
            pending_delete: None,
            delete_error: None,
            pending_info: None,
            pending_rename: None,
            rename_name: String::new(),
            rename_error: None,
            pending_overwrite: None,
        }
    }

    /// Get the tab display name (last path segment, or translated "Home"/"Root" for empty path)
    pub fn tab_name(&self) -> String {
        if self.current_path.is_empty() {
            if self.viewing_root {
                t("files-root")
            } else {
                t("files-home")
            }
        } else {
            // Get last path segment
            let path = self.current_path.trim_end_matches('/');
            let segment = if let Some(pos) = path.rfind('/') {
                &path[pos + 1..]
            } else {
                path
            };
            // Strip folder type suffixes for display
            FilesManagementState::display_name(segment)
        }
    }

    /// Navigate to a new path (preserves viewing_root state)
    pub fn navigate_to(&mut self, path: String) {
        self.current_path = path;
        self.entries = None;
        self.sorted_entries = None;
        self.error = None;
    }

    /// Navigate to home directory (preserves viewing_root state)
    pub fn navigate_home(&mut self) {
        self.current_path = String::new();
        self.entries = None;
        self.sorted_entries = None;
        self.error = None;
    }

    /// Toggle between root view and user area view
    pub fn toggle_root(&mut self) {
        self.viewing_root = !self.viewing_root;
        self.current_path = String::new();
        self.entries = None;
        self.sorted_entries = None;
        self.error = None;
        self.current_dir_can_upload = false;
    }

    /// Open the new directory dialog
    pub fn open_new_directory_dialog(&mut self) {
        self.creating_directory = true;
        self.new_directory_name = String::new();
        self.new_directory_error = None;
    }

    /// Close the new directory dialog
    pub fn close_new_directory_dialog(&mut self) {
        self.creating_directory = false;
        self.new_directory_name = String::new();
        self.new_directory_error = None;
    }

    /// Navigate up one directory level (preserves viewing_root state)
    pub fn navigate_up(&mut self) {
        if self.current_path.is_empty() || self.current_path == "/" {
            return;
        }

        // Remove trailing slash if present
        let path = self.current_path.trim_end_matches('/');

        // Find the last path separator
        if let Some(pos) = path.rfind('/') {
            self.current_path = path[..pos].to_string();
        } else {
            // No separator found, go to home
            self.current_path = String::new();
        }

        self.entries = None;
        self.sorted_entries = None;
        self.error = None;
    }

    /// Update the sorted entries cache based on current entries and sort settings
    pub fn update_sorted_entries(&mut self) {
        self.sorted_entries = self.entries.as_ref().map(|entries| {
            let mut sorted = entries.clone();
            match self.sort_column {
                FileSortColumn::Name => {
                    // Sort by name, keeping directories first
                    sorted.sort_by(|a, b| {
                        let a_is_dir = a.dir_type.is_some();
                        let b_is_dir = b.dir_type.is_some();

                        // Directories always come first
                        match (a_is_dir, b_is_dir) {
                            (true, false) => std::cmp::Ordering::Less,
                            (false, true) => std::cmp::Ordering::Greater,
                            _ => {
                                // Same type: sort by name (case-insensitive)
                                let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                                if self.sort_ascending {
                                    cmp
                                } else {
                                    cmp.reverse()
                                }
                            }
                        }
                    });
                }
                FileSortColumn::Size => {
                    // Full sort by size, mixes directories and files
                    sorted.sort_by(|a, b| {
                        let cmp = a.size.cmp(&b.size);
                        if self.sort_ascending {
                            cmp
                        } else {
                            cmp.reverse()
                        }
                    });
                }
                FileSortColumn::Modified => {
                    // Full sort by modified date, mixes directories and files
                    sorted.sort_by(|a, b| {
                        let cmp = a.modified.cmp(&b.modified);
                        if self.sort_ascending {
                            cmp
                        } else {
                            cmp.reverse()
                        }
                    });
                }
            }
            sorted
        });
    }
}

/// Files management panel state (per-connection)
///
/// Contains multiple file browser tabs and shared state like clipboard.
#[derive(Debug, Clone)]
pub struct FilesManagementState {
    /// File browser tabs
    pub tabs: Vec<FileTab>,
    /// Index of the active tab
    pub active_tab: usize,
    /// Clipboard for cut/copy operations (shared across all tabs)
    pub clipboard: Option<ClipboardItem>,
}

impl Default for FilesManagementState {
    fn default() -> Self {
        Self {
            tabs: vec![FileTab::default()],
            active_tab: 0,
            clipboard: None,
        }
    }
}

impl FilesManagementState {
    /// Get a reference to the active tab
    pub fn active_tab(&self) -> &FileTab {
        &self.tabs[self.active_tab]
    }

    /// Get a mutable reference to the active tab
    pub fn active_tab_mut(&mut self) -> &mut FileTab {
        &mut self.tabs[self.active_tab]
    }

    /// Get the ID of the active tab
    pub fn active_tab_id(&self) -> TabId {
        self.tabs[self.active_tab].id
    }

    /// Find a tab by its unique ID
    ///
    /// Returns None if the tab has been closed.
    pub fn tab_by_id(&self, id: TabId) -> Option<&FileTab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// Find a tab by its unique ID (mutable)
    ///
    /// Returns None if the tab has been closed.
    pub fn tab_by_id_mut(&mut self, id: TabId) -> Option<&mut FileTab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// Create a new tab cloned from the current active tab
    ///
    /// Returns the index of the new tab.
    pub fn new_tab(&mut self) -> usize {
        let new_tab = FileTab::new_from_location(self.active_tab());
        self.tabs.push(new_tab);
        let new_index = self.tabs.len() - 1;
        self.active_tab = new_index;
        new_index
    }

    /// Switch to a tab by its unique ID
    ///
    /// Returns true if the tab was found and switched to, false otherwise.
    pub fn switch_to_tab_by_id(&mut self, id: TabId) -> bool {
        if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
            self.active_tab = index;
            true
        } else {
            false
        }
    }

    /// Close a tab by its unique ID
    ///
    /// Returns true if the tab was closed, false if not found or it's the last tab.
    pub fn close_tab_by_id(&mut self, id: TabId) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
            self.tabs.remove(index);

            // Adjust active_tab if necessary
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > index {
                self.active_tab -= 1;
            }
            true
        } else {
            false
        }
    }

    /// Switch to the next tab (wraps around)
    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    /// Switch to the previous tab (wraps around)
    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            if self.active_tab == 0 {
                self.active_tab = self.tabs.len() - 1;
            } else {
                self.active_tab -= 1;
            }
        }
    }

    /// Get the display name for a file entry (strips folder type suffixes)
    pub fn display_name(name: &str) -> String {
        // Suffixes to strip (case-insensitive, with leading space)
        const SUFFIX_UPLOAD: &str = " [NEXUS-UL]";
        const SUFFIX_DROPBOX: &str = " [NEXUS-DB]";
        const SUFFIX_DROPBOX_PREFIX: &str = " [NEXUS-DB-";

        let name_upper = name.to_uppercase();

        // Check for user-specific dropbox suffix first (e.g., " [NEXUS-DB-alice]")
        if let Some(pos) = name_upper.rfind(SUFFIX_DROPBOX_PREFIX.to_uppercase().as_str())
            && name_upper.ends_with(']')
        {
            return name[..pos].to_string();
        }

        // Check for generic dropbox suffix
        if name_upper.ends_with(SUFFIX_DROPBOX.to_uppercase().as_str()) {
            let suffix_start = name.len() - SUFFIX_DROPBOX.len();
            return name[..suffix_start].to_string();
        }

        // Check for upload suffix
        if name_upper.ends_with(SUFFIX_UPLOAD.to_uppercase().as_str()) {
            let suffix_start = name.len() - SUFFIX_UPLOAD.len();
            return name[..suffix_start].to_string();
        }

        name.to_string()
    }
}

// =============================================================================
// User Management State
// =============================================================================

/// Default permissions for new users
///
/// These permissions are enabled by default when creating a new user:
/// - `chat_receive`: Receive chat messages
/// - `chat_send`: Send chat messages
/// - `chat_topic`: View chat topic
/// - `file_list`: Browse files and directories
/// - `news_list`: View news posts
/// - `user_info`: View user information
/// - `user_list`: View connected users list
/// - `user_message`: Send private messages
const DEFAULT_USER_PERMISSIONS: &[&str] = &[
    "chat_receive",
    "chat_send",
    "chat_topic",
    "file_info",
    "file_list",
    "news_list",
    "user_info",
    "user_list",
    "user_message",
];

/// User management panel mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UserManagementMode {
    /// Showing list of all users
    #[default]
    List,
    /// Creating a new user
    Create,
    /// Editing an existing user
    Edit {
        /// Original username (for the UserUpdate request)
        original_username: String,
        /// New username (editable field, pre-filled with original)
        new_username: String,
        /// New password (optional, empty = don't change)
        new_password: String,
        /// Is admin flag (editable)
        is_admin: bool,
        /// Is shared account flag (immutable - display only)
        is_shared: bool,
        /// Enabled flag (editable)
        enabled: bool,
        /// Permissions (editable)
        permissions: Vec<(String, bool)>,
    },
    /// Confirming deletion of a user
    ConfirmDelete {
        /// Username to delete
        username: String,
    },
}

/// Connection form state (not persisted)
#[derive(Debug, Clone)]
pub struct ConnectionFormState {
    /// Optional display name for connection
    pub server_name: String,
    /// Server address (IPv4 or IPv6)
    pub server_address: String,
    /// Server port number
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Nickname for shared account authentication
    pub nickname: String,
    /// Connection error message
    pub error: Option<String>,
    /// Whether a connection attempt is currently in progress
    pub is_connecting: bool,
    /// Whether to save this connection as a bookmark on successful connect
    pub add_bookmark: bool,
}

impl Default for ConnectionFormState {
    fn default() -> Self {
        Self {
            server_name: String::new(),
            server_address: String::new(),
            port: DEFAULT_PORT,
            username: String::new(),
            password: String::new(),
            nickname: String::new(),
            error: None,
            is_connecting: false,
            add_bookmark: false,
        }
    }
}

impl ConnectionFormState {
    /// Clear all form fields
    pub fn clear(&mut self) {
        self.server_name.clear();
        self.server_address.clear();
        self.port = DEFAULT_PORT;
        self.username.clear();
        self.password.clear();
        self.nickname.clear();
    }
}

/// User management panel state (per-connection)
#[derive(Debug, Clone)]
pub struct UserManagementState {
    /// Current mode (list, create, edit, confirm delete)
    pub mode: UserManagementMode,
    /// All users from database (None = not loaded, Some(Ok) = loaded, Some(Err) = error)
    pub all_users: Option<Result<Vec<UserInfo>, String>>,
    /// Panel to return to after edit (e.g., UserInfo if edit was triggered from there)
    pub return_to_panel: Option<super::ActivePanel>,
    /// Username for create user form
    pub username: String,
    /// Password for create user form
    pub password: String,
    /// Admin flag for create user form
    pub is_admin: bool,
    /// Shared account flag for create user form
    pub is_shared: bool,
    /// Enabled flag for create user form
    pub enabled: bool,
    /// Permissions for create user form
    pub permissions: Vec<(String, bool)>,
    /// Error message for create user form
    pub create_error: Option<String>,
    /// Error message for edit user form
    pub edit_error: Option<String>,
    /// Error message for list view (e.g., delete failed)
    pub list_error: Option<String>,
    /// Error message for delete confirmation dialog
    pub delete_error: Option<String>,
}

impl Default for UserManagementState {
    fn default() -> Self {
        Self {
            mode: UserManagementMode::List,
            all_users: None,
            return_to_panel: None,
            username: String::new(),
            password: String::new(),
            is_admin: false,
            is_shared: false,
            enabled: true, // Default to enabled
            permissions: ALL_PERMISSIONS
                .iter()
                .map(|s| (s.to_string(), DEFAULT_USER_PERMISSIONS.contains(s)))
                .collect(),
            create_error: None,
            edit_error: None,
            list_error: None,
            delete_error: None,
        }
    }
}

impl UserManagementState {
    /// Reset to list mode and clear all form state
    pub fn reset_to_list(&mut self) {
        self.mode = UserManagementMode::List;
        self.clear_create_form();
        self.edit_error = None;
        self.list_error = None;
        self.return_to_panel = None;
    }

    /// Clear the create user form fields
    pub fn clear_create_form(&mut self) {
        self.username.clear();
        self.password.clear();
        self.is_admin = false;
        self.is_shared = false;
        self.enabled = true; // Reset to default enabled
        for (perm_name, enabled) in &mut self.permissions {
            *enabled = DEFAULT_USER_PERMISSIONS.contains(&perm_name.as_str());
        }
        self.create_error = None;
    }

    /// Enter create mode
    pub fn enter_create_mode(&mut self) {
        self.clear_create_form();
        self.mode = UserManagementMode::Create;
    }

    /// Enter edit mode for a user (with pre-populated values from server)
    pub fn enter_edit_mode(
        &mut self,
        username: String,
        is_admin: bool,
        is_shared: bool,
        enabled: bool,
        permissions: Vec<String>,
    ) {
        // Convert permissions Vec<String> to Vec<(String, bool)>
        let mut perm_map: Vec<(String, bool)> = ALL_PERMISSIONS
            .iter()
            .map(|s| (s.to_string(), false))
            .collect();

        // Mark permissions that the user has
        for (perm_name, perm_enabled) in &mut perm_map {
            *perm_enabled = permissions.contains(perm_name);
        }

        self.mode = UserManagementMode::Edit {
            original_username: username.clone(),
            new_username: username,
            new_password: String::new(),
            is_admin,
            is_shared,
            enabled,
            permissions: perm_map,
        };
        self.edit_error = None;
    }

    /// Enter confirm delete mode for a user
    pub fn enter_confirm_delete_mode(&mut self, username: String) {
        self.mode = UserManagementMode::ConfirmDelete { username };
        self.delete_error = None;
    }
}

// =============================================================================
// Settings Form State
// =============================================================================

/// Settings panel tab identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// General settings (theme, avatar, nickname)
    #[default]
    General,
    /// Chat settings (font size, timestamps, notifications)
    Chat,
    /// Network settings (proxy configuration)
    Network,
    /// Files settings (download location)
    Files,
}

/// Settings panel form state
///
/// Stores a snapshot of the configuration when the settings panel is opened,
/// allowing the user to cancel and restore the original settings.
#[derive(Clone)]
pub struct SettingsFormState {
    /// Currently active settings tab
    pub active_tab: SettingsTab,
    /// Original config snapshot to restore on cancel
    pub original_config: Config,
    /// Error message to display (e.g., avatar load failure)
    pub error: Option<String>,
    /// Cached avatar for stable rendering (decoded from config.settings.avatar)
    pub cached_avatar: Option<CachedImage>,
    /// Default avatar for settings preview when no custom avatar is set
    pub default_avatar: CachedImage,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for SettingsFormState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsFormState")
            .field("original_config", &self.original_config)
            .field("error", &self.error)
            .field(
                "cached_avatar",
                &self.cached_avatar.as_ref().map(|_| "<cached>"),
            )
            .field("default_avatar", &"<cached>")
            .finish()
    }
}

// =============================================================================
// Server Info Edit State
// =============================================================================

/// Server info edit panel state
///
/// Stores the form values for editing server configuration.
/// Only admins can access this form.
#[derive(Clone)]
pub struct ServerInfoEditState {
    /// Server name (editable)
    pub name: String,
    /// Server description (editable)
    pub description: String,
    /// Max connections per IP (editable, uses NumberInput)
    pub max_connections_per_ip: Option<u32>,
    /// Server image data URI (editable, empty string means no image)
    pub image: String,
    /// Cached image for preview (decoded from image field)
    pub cached_image: Option<CachedImage>,
    /// Error message to display
    pub error: Option<String>,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for ServerInfoEditState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerInfoEditState")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("max_connections_per_ip", &self.max_connections_per_ip)
            .field("image", &format!("<{} bytes>", self.image.len()))
            .field(
                "cached_image",
                &self.cached_image.as_ref().map(|_| "<cached>"),
            )
            .field("error", &self.error)
            .finish()
    }
}

impl ServerInfoEditState {
    /// Create a new server info edit state with current values
    pub fn new(
        name: Option<&str>,
        description: Option<&str>,
        max_connections_per_ip: Option<u32>,
        image: &str,
    ) -> Self {
        // Decode image for preview
        let cached_image = if image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(image, SERVER_IMAGE_MAX_CACHE_WIDTH)
        };

        Self {
            name: name.unwrap_or("").to_string(),
            description: description.unwrap_or("").to_string(),
            max_connections_per_ip,
            image: image.to_string(),
            cached_image,
            error: None,
        }
    }

    /// Check if the form has any changes compared to original values
    pub fn has_changes(
        &self,
        original_name: Option<&str>,
        original_description: Option<&str>,
        original_max_connections: Option<u32>,
        original_image: &str,
    ) -> bool {
        let name_changed = self.name != original_name.unwrap_or("");
        let desc_changed = self.description != original_description.unwrap_or("");
        let max_conn_changed = self.max_connections_per_ip != original_max_connections;
        let image_changed = self.image != original_image;
        name_changed || desc_changed || max_conn_changed || image_changed
    }
}

impl SettingsFormState {
    /// Create a new settings form state with a snapshot of the current config
    ///
    /// The `last_tab` parameter restores the previously selected tab when reopening the panel.
    pub fn new(config: &Config, last_tab: SettingsTab) -> Self {
        // Decode avatar from config if present
        let cached_avatar = config
            .settings
            .avatar
            .as_ref()
            .and_then(|data_uri| decode_data_uri_square(data_uri, AVATAR_MAX_CACHE_SIZE));
        // Generate default avatar for settings preview
        let default_avatar = generate_identicon("default");

        Self {
            active_tab: last_tab,
            original_config: config.clone(),
            error: None,
            cached_avatar,
            default_avatar,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // FileTab Tests
    // =========================================================================

    #[test]
    fn test_file_tab_navigate_to() {
        let mut tab = FileTab::default();

        tab.navigate_to("Documents/Photos".to_string());

        assert_eq!(tab.current_path, "Documents/Photos");
        assert!(tab.entries.is_none());
        assert!(tab.error.is_none());
    }

    #[test]
    fn test_file_tab_navigate_to_preserves_viewing_root() {
        let mut tab = FileTab {
            current_path: String::new(),
            entries: Some(vec![]),
            error: None,
            viewing_root: true,
            ..Default::default()
        };

        tab.navigate_to("shared/Documents".to_string());

        assert_eq!(tab.current_path, "shared/Documents");
        assert!(tab.viewing_root); // Should be preserved
    }

    #[test]
    fn test_file_tab_navigate_home() {
        let mut tab = FileTab {
            current_path: "Documents/Photos".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.navigate_home();

        assert!(tab.current_path.is_empty());
        assert!(tab.entries.is_none());
        assert!(tab.error.is_none());
        assert!(!tab.viewing_root);
    }

    #[test]
    fn test_file_tab_navigate_home_preserves_viewing_root() {
        let mut tab = FileTab {
            current_path: "shared/Documents".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: true,
            ..Default::default()
        };

        tab.navigate_home();

        assert!(tab.current_path.is_empty());
        assert!(tab.viewing_root); // Should be preserved
    }

    #[test]
    fn test_file_tab_toggle_root_from_user_area() {
        let mut tab = FileTab {
            current_path: "Documents/Photos".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.toggle_root();

        assert!(tab.current_path.is_empty()); // Path reset
        assert!(tab.entries.is_none());
        assert!(tab.viewing_root); // Now viewing root
    }

    #[test]
    fn test_file_tab_toggle_root_from_root() {
        let mut tab = FileTab {
            current_path: "shared/Documents".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: true,
            ..Default::default()
        };

        tab.toggle_root();

        assert!(tab.current_path.is_empty()); // Path reset
        assert!(tab.entries.is_none());
        assert!(!tab.viewing_root); // Now viewing user area
    }

    #[test]
    fn test_file_tab_navigate_up_preserves_viewing_root() {
        let mut tab = FileTab {
            current_path: "shared/Documents".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: true,
            ..Default::default()
        };

        tab.navigate_up();

        assert_eq!(tab.current_path, "shared");
        assert!(tab.viewing_root); // Should be preserved
    }

    #[test]
    fn test_file_tab_navigate_up_from_nested() {
        let mut tab = FileTab {
            current_path: "Documents/Photos/2024".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.navigate_up();

        assert_eq!(tab.current_path, "Documents/Photos");
        assert!(tab.entries.is_none());
    }

    #[test]
    fn test_file_tab_navigate_up_from_single_level() {
        let mut tab = FileTab {
            current_path: "Documents".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.navigate_up();

        assert!(tab.current_path.is_empty());
    }

    #[test]
    fn test_file_tab_navigate_up_from_root() {
        let mut tab = FileTab {
            current_path: String::new(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.navigate_up();

        assert!(tab.current_path.is_empty());
        // Entries should still be there since we didn't navigate
        assert!(tab.entries.is_some());
    }

    #[test]
    fn test_file_tab_navigate_up_from_slash() {
        let mut tab = FileTab {
            current_path: "/".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.navigate_up();

        // Should not change when at root
        assert_eq!(tab.current_path, "/");
    }

    #[test]
    fn test_file_tab_navigate_up_with_trailing_slash() {
        let mut tab = FileTab {
            current_path: "Documents/Photos/".to_string(),
            entries: Some(vec![]),
            error: None,
            viewing_root: false,
            ..Default::default()
        };

        tab.navigate_up();

        assert_eq!(tab.current_path, "Documents");
    }

    #[test]
    fn test_file_tab_open_close_new_directory_dialog() {
        let mut tab = FileTab::default();

        tab.open_new_directory_dialog();
        assert!(tab.creating_directory);
        assert!(tab.new_directory_name.is_empty());
        assert!(tab.new_directory_error.is_none());

        // Simulate user typing a name and getting an error
        tab.new_directory_name = "test".to_string();
        tab.new_directory_error = Some("test error".to_string());

        // Close dialog should reset everything
        tab.close_new_directory_dialog();
        assert!(!tab.creating_directory);
        assert!(tab.new_directory_name.is_empty());
        assert!(tab.new_directory_error.is_none());
    }

    #[test]
    fn test_file_tab_close_new_directory_dialog_clears_state() {
        let mut tab = FileTab {
            id: next_tab_id(),
            current_path: String::new(),
            entries: None,
            error: None,
            viewing_root: false,
            current_dir_can_upload: false,
            sort_column: FileSortColumn::Name,
            sort_ascending: true,
            sorted_entries: None,
            creating_directory: true,
            new_directory_name: "My Folder".to_string(),
            new_directory_error: Some("Name already exists".to_string()),
            pending_delete: None,
            delete_error: None,
            pending_info: None,
            pending_rename: None,
            rename_name: String::new(),
            rename_error: None,
            pending_overwrite: None,
        };

        tab.close_new_directory_dialog();

        assert!(!tab.creating_directory);
        assert!(tab.new_directory_name.is_empty());
        assert!(tab.new_directory_error.is_none());
    }

    // =========================================================================
    // FilesManagementState Tab Tests
    // =========================================================================

    #[test]
    fn test_files_management_default_has_one_tab() {
        let state = FilesManagementState::default();
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active_tab, 0);
    }

    #[test]
    fn test_files_management_new_tab() {
        let mut state = FilesManagementState::default();
        state.active_tab_mut().current_path = "Documents".to_string();
        state.active_tab_mut().viewing_root = true;

        let new_index = state.new_tab();

        assert_eq!(new_index, 1);
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active_tab, 1); // Switches to new tab
        assert_eq!(state.active_tab().current_path, "Documents"); // Cloned path
        assert!(state.active_tab().viewing_root); // Cloned setting
        assert!(state.active_tab().entries.is_none()); // Fresh entries
    }

    #[test]
    fn test_files_management_close_tab_by_id() {
        let mut state = FilesManagementState::default();
        let tab0_id = state.active_tab().id;
        state.new_tab(); // Now have 2 tabs, active is 1
        let tab1_id = state.active_tab().id;
        state.new_tab(); // Now have 3 tabs, active is 2
        let tab2_id = state.active_tab().id;

        // Close middle tab by ID
        assert!(state.close_tab_by_id(tab1_id));
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active_tab, 1); // Adjusted down

        // Close last tab by ID
        assert!(state.close_tab_by_id(tab2_id));
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active_tab, 0);

        // Can't close last tab
        assert!(!state.close_tab_by_id(tab0_id));
        assert_eq!(state.tabs.len(), 1);

        // Can't close non-existent tab
        assert!(!state.close_tab_by_id(99999));
    }

    #[test]
    fn test_files_management_close_active_tab_by_id() {
        let mut state = FilesManagementState::default();

        // Set up 3 tabs with different paths
        let tab0_id = state.active_tab().id;
        state.active_tab_mut().current_path = "tab0".to_string();
        state.new_tab();
        let tab1_id = state.active_tab().id;
        state.active_tab_mut().current_path = "tab1".to_string();
        state.new_tab();
        state.active_tab_mut().current_path = "tab2".to_string();

        // Now: tabs = [tab0, tab1, tab2], active = 2

        // Switch to tab1 and close it (the active tab)
        assert!(state.switch_to_tab_by_id(tab1_id));
        assert_eq!(state.active_tab, 1);
        assert!(state.close_tab_by_id(tab1_id)); // Close tab1 (active)

        // After closing: tabs = [tab0, tab2], active should point to what was tab2 (now at index 1)
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active_tab, 1);
        assert_eq!(state.active_tab().current_path, "tab2");

        // Close the last tab (active) - which is tab2, leaving only tab0
        let tab2_id = state.active_tab().id;
        assert!(state.close_tab_by_id(tab2_id));
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active_tab, 0);
        assert_eq!(state.active_tab().current_path, "tab0");

        // Verify tab0_id is still valid and matches the remaining tab
        assert!(state.tab_by_id(tab0_id).is_some());
        assert_eq!(state.active_tab().id, tab0_id);
    }

    #[test]
    fn test_files_management_tab_by_id() {
        let mut state = FilesManagementState::default();

        // Get the first tab's ID
        let first_tab_id = state.active_tab().id;

        // Create a second tab
        state.new_tab();
        let second_tab_id = state.active_tab().id;

        // IDs should be different
        assert_ne!(first_tab_id, second_tab_id);

        // Look up tabs by ID
        assert!(state.tab_by_id(first_tab_id).is_some());
        assert!(state.tab_by_id(second_tab_id).is_some());
        assert!(state.tab_by_id(99999).is_none()); // Non-existent ID

        // Mutable lookup works too
        state.tab_by_id_mut(first_tab_id).unwrap().current_path = "modified".to_string();
        assert_eq!(
            state.tab_by_id(first_tab_id).unwrap().current_path,
            "modified"
        );

        // Close the first tab by ID
        assert!(state.close_tab_by_id(first_tab_id));

        // First tab ID should no longer be found
        assert!(state.tab_by_id(first_tab_id).is_none());
        // Second tab ID should still work
        assert!(state.tab_by_id(second_tab_id).is_some());
    }

    #[test]
    fn test_files_management_tab_ids_are_unique() {
        let mut state = FilesManagementState::default();

        // Create several tabs and collect their IDs
        let mut ids = vec![state.active_tab().id];
        for _ in 0..5 {
            state.new_tab();
            ids.push(state.active_tab().id);
        }

        // All IDs should be unique
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }

    #[test]
    fn test_files_management_switch_to_tab_by_id() {
        let mut state = FilesManagementState::default();
        let tab0_id = state.active_tab().id;
        state.new_tab();
        state.new_tab();
        let tab2_id = state.active_tab().id;

        assert!(state.switch_to_tab_by_id(tab0_id));
        assert_eq!(state.active_tab, 0);

        assert!(state.switch_to_tab_by_id(tab2_id));
        assert_eq!(state.active_tab, 2);

        // Invalid ID returns false and doesn't change active tab
        assert!(!state.switch_to_tab_by_id(99999));
        assert_eq!(state.active_tab, 2);
    }

    #[test]
    fn test_files_management_next_prev_tab() {
        let mut state = FilesManagementState::default();
        let tab0_id = state.active_tab().id;
        state.new_tab();
        state.new_tab();
        state.switch_to_tab_by_id(tab0_id);

        state.next_tab();
        assert_eq!(state.active_tab, 1);

        state.next_tab();
        assert_eq!(state.active_tab, 2);

        state.next_tab(); // Wraps around
        assert_eq!(state.active_tab, 0);

        state.prev_tab(); // Wraps back
        assert_eq!(state.active_tab, 2);

        state.prev_tab();
        assert_eq!(state.active_tab, 1);
    }

    #[test]
    fn test_file_tab_tab_name_empty_path() {
        let tab = FileTab::default();
        assert_eq!(tab.tab_name(), "Home");

        let tab_root = FileTab {
            viewing_root: true,
            ..Default::default()
        };
        assert_eq!(tab_root.tab_name(), "Root");
    }

    #[test]
    fn test_file_tab_tab_name_with_path() {
        let mut tab = FileTab {
            current_path: "Documents".to_string(),
            ..Default::default()
        };
        assert_eq!(tab.tab_name(), "Documents");

        tab.current_path = "Documents/Photos".to_string();
        assert_eq!(tab.tab_name(), "Photos");

        // Trailing slash is trimmed
        tab.current_path = "Music/Albums/Jazz/".to_string();
        assert_eq!(tab.tab_name(), "Jazz");
    }

    #[test]
    fn test_file_tab_tab_name_strips_suffix() {
        let mut tab = FileTab {
            current_path: "Uploads [NEXUS-UL]".to_string(),
            ..Default::default()
        };
        assert_eq!(tab.tab_name(), "Uploads");

        tab.current_path = "Shared/Dropbox [NEXUS-DB]".to_string();
        assert_eq!(tab.tab_name(), "Dropbox");
    }

    // =========================================================================
    // display_name Tests
    // =========================================================================

    #[test]
    fn test_display_name_no_suffix() {
        assert_eq!(
            FilesManagementState::display_name("My Documents"),
            "My Documents"
        );
    }

    #[test]
    fn test_display_name_upload_suffix() {
        assert_eq!(
            FilesManagementState::display_name("Uploads [NEXUS-UL]"),
            "Uploads"
        );
    }

    #[test]
    fn test_display_name_upload_suffix_lowercase() {
        assert_eq!(
            FilesManagementState::display_name("Uploads [nexus-ul]"),
            "Uploads"
        );
    }

    #[test]
    fn test_display_name_upload_suffix_mixed_case() {
        assert_eq!(
            FilesManagementState::display_name("Uploads [Nexus-UL]"),
            "Uploads"
        );
    }

    #[test]
    fn test_display_name_dropbox_suffix() {
        assert_eq!(
            FilesManagementState::display_name("Admin Inbox [NEXUS-DB]"),
            "Admin Inbox"
        );
    }

    #[test]
    fn test_display_name_dropbox_suffix_lowercase() {
        assert_eq!(
            FilesManagementState::display_name("Admin Inbox [nexus-db]"),
            "Admin Inbox"
        );
    }

    #[test]
    fn test_display_name_user_dropbox_suffix() {
        assert_eq!(
            FilesManagementState::display_name("For Alice [NEXUS-DB-alice]"),
            "For Alice"
        );
    }

    #[test]
    fn test_display_name_user_dropbox_suffix_mixed_case() {
        assert_eq!(
            FilesManagementState::display_name("For Alice [Nexus-DB-Alice]"),
            "For Alice"
        );
    }

    #[test]
    fn test_display_name_partial_suffix_not_stripped() {
        // Missing space before bracket - should not be stripped
        assert_eq!(
            FilesManagementState::display_name("Uploads[NEXUS-UL]"),
            "Uploads[NEXUS-UL]"
        );
    }

    #[test]
    fn test_display_name_suffix_in_middle_not_stripped() {
        // Suffix in middle, not at end - should not be stripped
        assert_eq!(
            FilesManagementState::display_name("Files [NEXUS-UL] backup"),
            "Files [NEXUS-UL] backup"
        );
    }
}
