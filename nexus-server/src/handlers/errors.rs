//! Error message constants for handlers

/// Error message when user is not logged in
pub const ERR_NOT_LOGGED_IN: &str = "Not logged in";

/// Error message for database errors
pub const ERR_DATABASE: &str = "Database error";

/// Error message for authentication errors
pub const ERR_AUTHENTICATION: &str = "Authentication error";

/// Error message for permission denied
pub const ERR_PERMISSION_DENIED: &str = "Permission denied";

/// Error message for invalid credentials
pub const ERR_INVALID_CREDENTIALS: &str = "Invalid username or password";

/// Error message when handshake is required
pub const ERR_HANDSHAKE_REQUIRED: &str = "Handshake required";

/// Error message when already logged in
pub const ERR_ALREADY_LOGGED_IN: &str = "Already logged in";

/// Error message when trying to delete last admin
pub const ERR_CANNOT_DELETE_LAST_ADMIN: &str = "Cannot delete the last admin";

/// Error message for failed user creation
pub const ERR_FAILED_TO_CREATE_USER: &str = "Failed to create user";

/// Error message when account has been deleted
pub const ERR_ACCOUNT_DELETED: &str = "Your account has been deleted";