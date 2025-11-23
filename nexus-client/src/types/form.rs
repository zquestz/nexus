//! Connection and user management form state

use super::DEFAULT_PORT;

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

/// State for user management (create/delete user forms)
///
/// Per-connection admin panel state. Each connection maintains its own
/// user management forms independently.
#[derive(Debug, Clone)]
pub struct UserManagementState {
    pub username: String,
    pub password: String,
    pub is_admin: bool,
    pub permissions: Vec<(String, bool)>,
    pub delete_username: String,
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
                ("user_create".to_string(), false),
                ("user_delete".to_string(), false),
            ],
            delete_username: String::new(),
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

    /// Clear the delete user form fields
    pub fn clear_delete_user(&mut self) {
        self.delete_username.clear();
    }
}
