//! Nexus Common Library
//!
//! Shared types, protocols, and utilities for the Nexus BBS system.

mod error_kind;
pub mod framing;
pub mod io;
pub mod protocol;
pub mod validators;
pub mod version;

pub use error_kind::FileErrorKind;

/// Version information for the Nexus protocol
pub const PROTOCOL_VERSION: &str = "0.5.0";

/// Default port for Nexus BBS connections
pub const DEFAULT_PORT: u16 = 7500;

/// Default port for file transfers
pub const DEFAULT_TRANSFER_PORT: u16 = 7501;

/// Default port as a string for form fields and display.
///
/// This is the string representation of [`DEFAULT_PORT`], provided as a constant
/// because Rust doesn't support const string formatting.
pub const DEFAULT_PORT_STR: &str = "7500";

/// All available permissions in the Nexus protocol.
///
/// These permission strings are used by both client and server to manage
/// user access control. The list is maintained in alphabetical order.
///
/// Permission meanings:
/// - `chat_receive`: Receive chat messages from #server
/// - `chat_send`: Send chat messages to #server
/// - `chat_topic`: View the server topic
/// - `chat_topic_edit`: Edit the server topic
/// - `file_copy`: Copy files and directories
/// - `file_create_dir`: Create directories anywhere in file area
/// - `file_delete`: Delete files and empty directories
/// - `file_download`: Download files from file area
/// - `file_info`: View detailed file/directory information
/// - `file_list`: Browse files and directories in user's area
/// - `file_move`: Move files and directories
/// - `file_rename`: Rename files and directories
/// - `file_root`: Browse entire file area from root (for admins/file managers)
/// - `news_create`: Create news posts
/// - `news_delete`: Delete any news post (without: only own posts)
/// - `news_edit`: Edit any news post (without: only own posts)
/// - `news_list`: View news posts
/// - `user_broadcast`: Send broadcast messages to all users
/// - `user_create`: Create new user accounts
/// - `user_delete`: Delete user accounts
/// - `user_edit`: Edit user accounts
/// - `user_info`: View detailed user information
/// - `user_kick`: Kick/disconnect users
/// - `user_list`: View the list of connected users
/// - `user_message`: Send private messages to users
pub const ALL_PERMISSIONS: &[&str] = &[
    "chat_receive",
    "chat_send",
    "chat_topic",
    "chat_topic_edit",
    "file_copy",
    "file_create_dir",
    "file_delete",
    "file_download",
    "file_info",
    "file_list",
    "file_move",
    "file_rename",
    "file_root",
    "news_create",
    "news_delete",
    "news_edit",
    "news_list",
    "user_broadcast",
    "user_create",
    "user_delete",
    "user_edit",
    "user_info",
    "user_kick",
    "user_list",
    "user_message",
];

/// Number of permissions in the system.
///
/// This is derived from `ALL_PERMISSIONS.len()` and provided as a const
/// for use in places that need the count without calling `.len()` repeatedly.
pub const PERMISSIONS_COUNT: usize = ALL_PERMISSIONS.len();

/// Permissions that can be granted to shared accounts.
///
/// Shared accounts are restricted to read-only operations and basic chat functionality.
/// They cannot perform any actions that modify database records (except sending messages).
///
/// Allowed permissions:
/// - `chat_receive`: Receive chat messages from #server
/// - `chat_send`: Send chat messages to #server
/// - `chat_topic`: View the server topic (but not edit)
/// - `file_download`: Download files from file area
/// - `file_info`: View detailed file/directory information
/// - `file_list`: Browse files and directories (read-only)
/// - `news_list`: View news posts (but not create/edit/delete)
/// - `user_info`: View detailed user information
/// - `user_list`: View the list of connected users
/// - `user_message`: Send private messages to users
pub const SHARED_ACCOUNT_PERMISSIONS: &[&str] = &[
    "chat_receive",
    "chat_send",
    "chat_topic",
    "file_download",
    "file_info",
    "file_list",
    "news_list",
    "user_info",
    "user_list",
    "user_message",
];

/// Check if a permission is allowed for shared accounts
///
/// Shared accounts have a restricted set of permissions. This function
/// returns `true` if the given permission string is in the allowed set.
///
/// # Arguments
///
/// * `permission` - The permission string to check (e.g., "chat_send")
///
/// # Returns
///
/// `true` if the permission is allowed for shared accounts, `false` otherwise.
pub fn is_shared_account_permission(permission: &str) -> bool {
    SHARED_ACCOUNT_PERMISSIONS.contains(&permission)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        // Verify protocol version is valid semver
        let version = version::protocol_version();
        // Verify round-trip
        assert_eq!(version.to_string(), PROTOCOL_VERSION);
    }

    #[test]
    fn test_default_port() {
        // Verify default port is the expected value
        assert_eq!(DEFAULT_PORT, 7500);
    }

    #[test]
    fn test_default_port_str_matches() {
        // Verify DEFAULT_PORT_STR matches DEFAULT_PORT
        assert_eq!(DEFAULT_PORT_STR, DEFAULT_PORT.to_string());
    }

    #[test]
    fn test_all_permissions_count() {
        // Verify we have the expected number of permissions (25)
        assert_eq!(ALL_PERMISSIONS.len(), 25);
    }

    #[test]
    fn test_shared_account_permissions_count() {
        // Verify we have the expected number of shared account permissions (10)
        assert_eq!(SHARED_ACCOUNT_PERMISSIONS.len(), 10);
    }

    #[test]
    fn test_is_shared_account_permission() {
        // Allowed permissions
        assert!(is_shared_account_permission("chat_receive"));
        assert!(is_shared_account_permission("chat_send"));
        assert!(is_shared_account_permission("chat_topic"));
        assert!(is_shared_account_permission("file_info"));
        assert!(is_shared_account_permission("file_list"));
        assert!(is_shared_account_permission("news_list"));
        assert!(is_shared_account_permission("user_info"));
        assert!(is_shared_account_permission("user_list"));
        assert!(is_shared_account_permission("user_message"));

        // Forbidden permissions
        assert!(!is_shared_account_permission("user_create"));
        assert!(!is_shared_account_permission("user_delete"));
        assert!(!is_shared_account_permission("user_edit"));
        assert!(!is_shared_account_permission("user_kick"));
        assert!(!is_shared_account_permission("user_broadcast"));
        assert!(!is_shared_account_permission("chat_topic_edit"));
        assert!(!is_shared_account_permission("news_create"));
        assert!(!is_shared_account_permission("news_edit"));
        assert!(!is_shared_account_permission("news_delete"));

        // Invalid permissions
        assert!(!is_shared_account_permission("invalid"));
        assert!(!is_shared_account_permission(""));
    }

    #[test]
    fn test_shared_account_permissions_sorted() {
        // Verify shared account permissions are in alphabetical order
        let mut sorted = SHARED_ACCOUNT_PERMISSIONS.to_vec();
        sorted.sort();
        assert_eq!(SHARED_ACCOUNT_PERMISSIONS, sorted.as_slice());
    }

    #[test]
    fn test_shared_account_permissions_subset() {
        // Verify all shared account permissions are valid permissions
        for perm in SHARED_ACCOUNT_PERMISSIONS {
            assert!(
                ALL_PERMISSIONS.contains(perm),
                "SHARED_ACCOUNT_PERMISSIONS contains '{}' which is not in ALL_PERMISSIONS",
                perm
            );
        }
    }

    #[test]
    fn test_all_permissions_sorted() {
        // Verify permissions are in alphabetical order
        let mut sorted = ALL_PERMISSIONS.to_vec();
        sorted.sort();
        assert_eq!(ALL_PERMISSIONS, sorted.as_slice());
    }

    #[test]
    fn test_all_permissions_no_duplicates() {
        // Verify no duplicate permissions
        let mut seen = std::collections::HashSet::new();
        for perm in ALL_PERMISSIONS {
            assert!(seen.insert(perm), "Duplicate permission: {}", perm);
        }
    }
}
