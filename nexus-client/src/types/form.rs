//! Connection and user management form state

use crate::avatar::generate_identicon;
use crate::config::Config;
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
