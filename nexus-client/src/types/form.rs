//! Connection and user management form state

use super::DEFAULT_PORT;

/// State for user edit flow (two-stage process)
///
/// Stage 1: User enters username to edit
/// Stage 2: Form shows with current values, user can modify and submit
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
        /// Permissions (editable)
        permissions: Vec<(String, bool)>,
    },
}

/// State for connection form (connecting to new servers)
///
/// Temporary state for the connection dialog. Unlike bookmarks,
/// this state is not persisted and is cleared after connecting.
#[derive(Debug, Clone, Default)]
pub struct ConnectionFormState {
    pub server_name: String,
    pub server_address: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub error: Option<String>,
}

impl ConnectionFormState {
    /// Clear all form fields
    pub fn clear(&mut self) {
        self.server_name.clear();
        self.server_address.clear();
        self.port = DEFAULT_PORT.to_string();
        self.username.clear();
        self.password.clear();
    }
}

/// State for user management (create/delete/edit user forms)
///
/// Per-connection admin panel state. Each connection maintains its own
/// user management forms independently.
#[derive(Debug, Clone)]
pub struct UserManagementState {
    // Add User fields
    pub username: String,
    pub password: String,
    pub is_admin: bool,
    pub permissions: Vec<(String, bool)>,
    
    // Edit User state
    pub edit_state: UserEditState,
}

impl Default for UserManagementState {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            is_admin: false,
            permissions: vec![
                ("user_list".to_string(), false),
                ("user_info".to_string(), false),
                ("chat_send".to_string(), false),
                ("chat_receive".to_string(), false),
                ("user_broadcast".to_string(), false),
                ("user_create".to_string(), false),
                ("user_delete".to_string(), false),
                ("user_edit".to_string(), false),
            ],
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
    pub fn load_user_for_editing(
        &mut self,
        username: String,
        is_admin: bool,
        permissions: Vec<String>,
    ) {
        // Convert permissions Vec<String> to Vec<(String, bool)>
        let mut perm_map = vec![
            ("user_list".to_string(), false),
            ("user_info".to_string(), false),
            ("chat_send".to_string(), false),
            ("chat_receive".to_string(), false),
            ("user_broadcast".to_string(), false),
            ("user_create".to_string(), false),
            ("user_delete".to_string(), false),
            ("user_edit".to_string(), false),
        ];
        
        // Mark permissions that the user has
        for (perm_name, enabled) in &mut perm_map {
            *enabled = permissions.contains(perm_name);
        }
        
        self.edit_state = UserEditState::EditingUser {
            original_username: username.clone(),
            new_username: username,
            new_password: String::new(),
            is_admin,
            permissions: perm_map,
        };
    }
}
