//! Nexus Common Library
//!
//! Shared types, protocols, and utilities for the Nexus BBS system.

pub mod framing;
pub mod io;
pub mod protocol;
pub mod validators;
pub mod version;

/// Version information for the Nexus protocol
pub const PROTOCOL_VERSION: &str = "0.5.0";

/// Default port for Nexus BBS connections
pub const DEFAULT_PORT: u16 = 7500;

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
    "user_broadcast",
    "user_create",
    "user_delete",
    "user_edit",
    "user_info",
    "user_kick",
    "user_list",
    "user_message",
];

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
        // Verify we have the expected number of permissions (12)
        assert_eq!(ALL_PERMISSIONS.len(), 12);
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
