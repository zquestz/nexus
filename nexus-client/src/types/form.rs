//! Connection and user management form state

use crate::config::Config;

/// Default permissions for new users
///
/// These permissions are enabled by default when creating a new user:
/// - `chat_receive`: Receive chat messages
/// - `chat_send`: Send chat messages
/// - `chat_topic`: View chat topic
/// - `user_info`: View user information
/// - `user_list`: View connected users list
/// - `user_message`: Send private messages
const DEFAULT_USER_PERMISSIONS: &[&str] = &[
    "chat_receive",
    "chat_send",
    "chat_topic",
    "user_info",
    "user_list",
    "user_message",
];
use crate::avatar::generate_identicon;
use crate::image::{CachedImage, decode_data_uri_max_width, decode_data_uri_square};
use crate::style::{AVATAR_MAX_CACHE_SIZE, SERVER_IMAGE_MAX_CACHE_WIDTH};
use nexus_common::{ALL_PERMISSIONS, DEFAULT_PORT_STR};

/// User edit flow state (two-stage process)
#[derive(Debug, Clone, PartialEq)]
pub enum UserEditState {
    /// Not editing anyone
    None,
    /// Stage 1: Selecting which user to edit (username input only)
    SelectingUser { username: String },
    /// Stage 2: Editing user details (full form with current values)
    EditingUser {
        /// Original username (for the UserUpdate request)
        original_username: String,
        /// New username (editable field, pre-filled with original)
        new_username: String,
        /// New password (optional, empty = don't change)
        new_password: String,
        /// Is admin flag (editable)
        is_admin: bool,
        /// Enabled flag (editable)
        enabled: bool,
        /// Permissions (editable)
        permissions: Vec<(String, bool)>,
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
    pub port: String,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
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
            port: DEFAULT_PORT_STR.to_string(),
            username: String::new(),
            password: String::new(),
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
        self.port = DEFAULT_PORT_STR.to_string();
        self.username.clear();
        self.password.clear();
    }
}

/// User management panel state (per-connection)
#[derive(Debug, Clone)]
pub struct UserManagementState {
    /// Username for add user form
    pub username: String,
    /// Password for add user form
    pub password: String,
    /// Admin flag for add user form
    pub is_admin: bool,
    /// Enabled flag for add user form
    pub enabled: bool,
    /// Permissions for add user form
    pub permissions: Vec<(String, bool)>,
    /// Current edit user state
    pub edit_state: UserEditState,
    /// Error message for create user form
    pub create_error: Option<String>,
    /// Error message for edit user form
    pub edit_error: Option<String>,
}

impl Default for UserManagementState {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            is_admin: false,
            enabled: true, // Default to enabled
            permissions: ALL_PERMISSIONS
                .iter()
                .map(|s| (s.to_string(), DEFAULT_USER_PERMISSIONS.contains(s)))
                .collect(),
            edit_state: UserEditState::None,
            create_error: None,
            edit_error: None,
        }
    }
}

impl UserManagementState {
    /// Clear the add user form fields
    pub fn clear_add_user(&mut self) {
        self.username.clear();
        self.password.clear();
        self.is_admin = false;
        self.enabled = true; // Reset to default enabled
        for (perm_name, enabled) in &mut self.permissions {
            *enabled = DEFAULT_USER_PERMISSIONS.contains(&perm_name.as_str());
        }
        self.create_error = None;
    }

    /// Clear the edit user state
    pub fn clear_edit_user(&mut self) {
        self.edit_state = UserEditState::None;
        self.edit_error = None;
    }

    /// Start editing a user (stage 1: enter username)
    ///
    /// If `username` is provided, pre-fills the username field.
    pub fn start_editing(&mut self, username: Option<String>) {
        self.edit_state = UserEditState::SelectingUser {
            username: username.unwrap_or_default(),
        };
    }

    /// Load a user for editing (stage 2: full form with current values from server)
    pub fn load_user_for_editing(
        &mut self,
        username: String,
        is_admin: bool,
        enabled: bool,
        permissions: Vec<String>,
    ) {
        // Convert permissions Vec<String> to Vec<(String, bool)>
        let mut perm_map: Vec<(String, bool)> = ALL_PERMISSIONS
            .iter()
            .map(|s| (s.to_string(), false))
            .collect();

        // Mark permissions that the user has
        for (perm_name, enabled) in &mut perm_map {
            *enabled = permissions.contains(perm_name);
        }

        self.edit_state = UserEditState::EditingUser {
            original_username: username.clone(),
            new_username: username,
            new_password: String::new(),
            is_admin,
            enabled,
            permissions: perm_map,
        };
    }
}

// =============================================================================
// Settings Form State
// =============================================================================

/// Settings panel form state
///
/// Stores a snapshot of the configuration when the settings panel is opened,
/// allowing the user to cancel and restore the original settings.
#[derive(Clone)]
pub struct SettingsFormState {
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
    pub fn new(config: &Config) -> Self {
        // Decode avatar from config if present
        let cached_avatar = config
            .settings
            .avatar
            .as_ref()
            .and_then(|data_uri| decode_data_uri_square(data_uri, AVATAR_MAX_CACHE_SIZE));
        // Generate default avatar for settings preview
        let default_avatar = generate_identicon("default");

        Self {
            original_config: config.clone(),
            error: None,
            cached_avatar,
            default_avatar,
        }
    }
}
