//! Shared UI constants used across multiple view files

// === Common Button Labels ===

/// "Cancel" button text - used in multiple dialogs
pub(crate) const BUTTON_CANCEL: &str = "Cancel";

/// "Send" button text - used in broadcast and chat
pub(crate) const BUTTON_SEND: &str = "Send";

/// "Delete" button text - used in bookmark and user management
pub(crate) const BUTTON_DELETE: &str = "Delete";

// === Common Input Placeholders ===

/// "Username" placeholder - used in connection and user management forms
pub(crate) const PLACEHOLDER_USERNAME: &str = "Username";

/// "Password" placeholder - used in connection and user management forms
pub(crate) const PLACEHOLDER_PASSWORD: &str = "Password";

/// "Port" placeholder - used in connection and bookmark forms
pub(crate) const PLACEHOLDER_PORT: &str = "Port";

// === Permission String Constants ===
// These must match the server-side permission names exactly

/// Permission to view the user list
pub(crate) const PERMISSION_USER_LIST: &str = "user_list";

/// Permission to view user information
pub(crate) const PERMISSION_USER_INFO: &str = "user_info";

/// Permission to send chat messages
pub(crate) const PERMISSION_CHAT_SEND: &str = "chat_send";

/// Permission to broadcast messages to all users
pub(crate) const PERMISSION_USER_BROADCAST: &str = "user_broadcast";

/// Permission to create new users
pub(crate) const PERMISSION_USER_CREATE: &str = "user_create";

/// Permission to delete users
pub(crate) const PERMISSION_USER_DELETE: &str = "user_delete";

/// Permission to edit user accounts
pub(crate) const PERMISSION_USER_EDIT: &str = "user_edit";

/// Permission to kick users
pub(crate) const PERMISSION_USER_KICK: &str = "user_kick";

/// Permission to send user messages (private messages)
pub(crate) const PERMISSION_USER_MESSAGE: &str = "user_message";