//! Shared constants used across multiple view files
//!
//! NOTE: UI text constants have been moved to locales/*.ftl for i18n support.
//! Use `crate::i18n::t("key")` to get localized strings.
//!
//! This file contains only non-localizable constants like permission names
//! that must match server-side values exactly.

// =============================================================================
// Permission String Constants
// =============================================================================
// These must match the server-side permission names exactly (not translated)

// === Chat Permissions ===

/// Permission to send chat messages
pub(crate) const PERMISSION_CHAT_SEND: &str = "chat_send";

/// Permission to view chat topic
pub(crate) const PERMISSION_CHAT_TOPIC: &str = "chat_topic";

/// Permission to edit chat topic
pub(crate) const PERMISSION_CHAT_TOPIC_EDIT: &str = "chat_topic_edit";

// === User Permissions ===

/// Permission to view the user list
pub(crate) const PERMISSION_USER_LIST: &str = "user_list";

/// Permission to view user information
pub(crate) const PERMISSION_USER_INFO: &str = "user_info";

/// Permission to send user messages (private messages)
pub(crate) const PERMISSION_USER_MESSAGE: &str = "user_message";

/// Permission to broadcast messages to all users
pub(crate) const PERMISSION_USER_BROADCAST: &str = "user_broadcast";

/// Permission to kick users
pub(crate) const PERMISSION_USER_KICK: &str = "user_kick";

/// Permission to create new users
pub(crate) const PERMISSION_USER_CREATE: &str = "user_create";

/// Permission to edit user accounts
pub(crate) const PERMISSION_USER_EDIT: &str = "user_edit";

/// Permission to delete users
pub(crate) const PERMISSION_USER_DELETE: &str = "user_delete";

// === News Permissions ===

/// Permission to view news posts
pub(crate) const PERMISSION_NEWS_LIST: &str = "news_list";

/// Permission to create news posts
pub(crate) const PERMISSION_NEWS_CREATE: &str = "news_create";

/// Permission to edit any news post (without: only own posts)
pub(crate) const PERMISSION_NEWS_EDIT: &str = "news_edit";

/// Permission to delete any news post (without: only own posts)
pub(crate) const PERMISSION_NEWS_DELETE: &str = "news_delete";

// === File Permissions ===

/// Permission to browse files and directories
pub(crate) const PERMISSION_FILE_LIST: &str = "file_list";

/// Permission to browse from file area root (see all areas)
pub(crate) const PERMISSION_FILE_ROOT: &str = "file_root";

/// Permission to create directories anywhere in file area
pub(crate) const PERMISSION_FILE_CREATE_DIR: &str = "file_create_dir";

/// Permission to delete files and empty directories
pub(crate) const PERMISSION_FILE_DELETE: &str = "file_delete";

/// Permission to view detailed file/directory information
pub(crate) const PERMISSION_FILE_INFO: &str = "file_info";

/// Permission to rename files and directories
pub(crate) const PERMISSION_FILE_RENAME: &str = "file_rename";

/// Permission to move files and directories
pub(crate) const PERMISSION_FILE_MOVE: &str = "file_move";

/// Permission to copy files and directories
pub(crate) const PERMISSION_FILE_COPY: &str = "file_copy";

/// Permission to download files
pub(crate) const PERMISSION_FILE_DOWNLOAD: &str = "file_download";

/// Permission to upload files
pub(crate) const PERMISSION_FILE_UPLOAD: &str = "file_upload";

// === Ban Permissions ===

/// Permission to create/update bans
pub(crate) const PERMISSION_BAN_CREATE: &str = "ban_create";

/// Permission to remove bans
pub(crate) const PERMISSION_BAN_DELETE: &str = "ban_delete";

/// Permission to view ban list
pub(crate) const PERMISSION_BAN_LIST: &str = "ban_list";
