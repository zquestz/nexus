//! Connection and user management form state

use super::DEFAULT_PORT;

/// Complete list of all available permissions
const ALL_PERMISSIONS: &[&str] = &[
    "chat_receive",
    "chat_send",
    "chat_topic",
    "chat_topic_edit",
    "user_broadcast",
    "user_create",
    "user_delete",
    "user_edit",
    "user_info",
    "user_kick",
    "user_list",
    "user_message",
];

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
#[derive(Debug, Clone, Default)]
pub struct ConnectionFormState {
    /// Optional display name for connection
    pub server_name: String,
    /// Server IPv6 address
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
}

impl ConnectionFormState {
    /// Create new form with default values
    pub fn new() -> Self {
        Self {
            port: DEFAULT_PORT.to_string(),
            ..Default::default()
        }
    }

    /// Clear all form fields
    pub fn clear(&mut self) {
        self.server_name.clear();
        self.server_address.clear();
        self.port = DEFAULT_PORT.to_string();
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
                .map(|s| (s.to_string(), false))
                .collect(),
            edit_state: UserEditState::None,
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
        for (_, enabled) in &mut self.permissions {
            *enabled = false;
        }
    }

    /// Clear the edit user state
    pub fn clear_edit_user(&mut self) {
        self.edit_state = UserEditState::None;
    }

    /// Start editing a user (stage 1: enter username)
    pub fn start_editing(&mut self) {
        self.edit_state = UserEditState::SelectingUser {
            username: String::new(),
        };
    }

    /// Move to stage 2 of editing with user details from server
    /// Load a user for editing (stage 2)
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
