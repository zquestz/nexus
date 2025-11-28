//! Error message constants for handlers

// Authentication & Session Errors
/// Error message when user is not logged in
pub const ERR_NOT_LOGGED_IN: &str = "Not logged in";

/// Error message for authentication errors
pub const ERR_AUTHENTICATION: &str = "Authentication error";

/// Error message for invalid credentials
pub const ERR_INVALID_CREDENTIALS: &str = "Invalid username or password";

/// Error message when handshake is required
pub const ERR_HANDSHAKE_REQUIRED: &str = "Handshake required";

/// Error message when already logged in
pub const ERR_ALREADY_LOGGED_IN: &str = "Already logged in";

/// Error message when handshake already completed
pub const ERR_HANDSHAKE_ALREADY_COMPLETED: &str = "Handshake already completed";

/// Error message when account has been deleted
pub const ERR_ACCOUNT_DELETED: &str = "Your account has been deleted";

/// Error message when account is disabled
pub const ERR_ACCOUNT_DISABLED: &str = "Account disabled";

// Permission & Access Errors
/// Error message for permission denied
pub const ERR_PERMISSION_DENIED: &str = "Permission denied";

// Database Errors
/// Error message for database errors
pub const ERR_DATABASE: &str = "Database error";

// User Management Errors
/// Error message when trying to delete last admin
pub const ERR_CANNOT_DELETE_LAST_ADMIN: &str = "Cannot delete the last admin";

/// Error message when trying to delete yourself
pub const ERR_CANNOT_DELETE_SELF: &str = "You cannot delete yourself";

/// Error message when trying to demote last admin
pub const ERR_CANNOT_DEMOTE_LAST_ADMIN: &str = "Cannot demote the last admin";

/// Error message when trying to edit yourself
pub const ERR_CANNOT_EDIT_SELF: &str = "You cannot edit yourself";

/// Error message for failed user creation
pub const ERR_FAILED_TO_CREATE_USER: &str = "Failed to create user";

/// Error message when username already exists
pub const ERR_USERNAME_EXISTS: &str = "Username already exists";

/// Error message when non-admin tries to create admin
pub const ERR_CANNOT_CREATE_ADMIN: &str = "Only admins can create admin users";

/// Error message when user to edit is not found
pub const ERR_USER_NOT_FOUND: &str = "User not found";

/// Error message when trying to kick yourself
pub const ERR_CANNOT_KICK_SELF: &str = "You cannot kick yourself";

/// Error message when trying to kick a user who is not online
pub const ERR_USER_NOT_ONLINE: &str = "User is not online";

/// Error message when trying to kick an admin
pub const ERR_CANNOT_KICK_ADMIN: &str = "Cannot kick admin users";

/// Error message when trying to disable the last admin
pub const ERR_CANNOT_DISABLE_LAST_ADMIN: &str = "Cannot disable the last admin";

// Chat Topic Errors
/// Error message when topic contains newlines
pub const ERR_TOPIC_CONTAINS_NEWLINES: &str = "Topic cannot contain newlines";

// Message Validation Errors
/// Error message when message is empty
pub const ERR_MESSAGE_EMPTY: &str = "Message cannot be empty";

// Helper functions to format dynamic error messages
// All format strings are defined inline to keep them in one place

/// Format broadcast message too long error
pub fn err_broadcast_too_long(max_length: usize) -> String {
    format!("Message too long (max {} characters)", max_length)
}

/// Format chat message too long error
pub fn err_chat_too_long(max_length: usize) -> String {
    format!("Message too long (max {} characters)", max_length)
}

/// Format topic too long error
pub fn err_topic_too_long(max_length: usize) -> String {
    format!("Topic cannot exceed {} characters", max_length)
}

/// Format version mismatch error
pub fn err_version_mismatch(server_version: &str, client_version: &str) -> String {
    format!(
        "Version mismatch: server uses {}, client uses {}",
        server_version, client_version
    )
}

/// Format kicked by user message
pub fn err_kicked_by(username: &str) -> String {
    format!("You have been kicked by {}", username)
}


