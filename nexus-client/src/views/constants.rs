//! Shared constants used across multiple view files
//!
//! NOTE: UI text constants have been moved to locales/*.ftl for i18n support.
//! Use `crate::i18n::t("key")` to get localized strings.
//!
//! This file contains only non-localizable constants like permission names
//! that must match server-side values exactly.

// === Permission String Constants ===
// These must match the server-side permission names exactly (not translated)

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

/// Permission to view chat topic
pub(crate) const PERMISSION_CHAT_TOPIC: &str = "chat_topic";

/// Permission to edit chat topic
pub(crate) const PERMISSION_CHAT_TOPIC_EDIT: &str = "chat_topic_edit";
