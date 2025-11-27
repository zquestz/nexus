//! Permission system for user authorization

use std::collections::HashSet;
use strum::AsRefStr;

/// Permission types for user actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum Permission {
    /// Permission to use UserList command
    UserList,
    /// Permission to use UserInfo command
    UserInfo,
    /// Permission to use ChatSend command
    ChatSend,
    /// Permission to receive chat messages
    ChatReceive,
    /// Permission to see/receive chat topic
    ChatTopic,
    /// Permission to edit chat topic
    ChatTopicEdit,
    /// Permission to send broadcast messages
    UserBroadcast,
    /// Permission to create users
    UserCreate,
    /// Permission to delete users
    UserDelete,
    /// Permission to edit users
    UserEdit,
    /// Permission to kick/disconnect users
    UserKick,
    /// Permission to send messages to users
    UserMessage,
}

impl Permission {
    /// Convert permission to string for database storage.
    ///
    /// Uses strum's AsRefStr to automatically convert PascalCase enum variants
    /// to snake_case strings (UserList → user_list, ChatSend → chat_send).
    ///
    /// Returns `&str` with zero allocation and zero runtime cost.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Parse a permission string into a Permission enum variant.
    ///
    /// Accepts snake_case strings like "user_list", "chat_send", etc.
    ///
    /// Returns Some(Permission) if the string is valid, None otherwise.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user_list" => Some(Permission::UserList),
            "user_info" => Some(Permission::UserInfo),
            "chat_send" => Some(Permission::ChatSend),
            "chat_receive" => Some(Permission::ChatReceive),
            "chat_topic" => Some(Permission::ChatTopic),
            "chat_topic_edit" => Some(Permission::ChatTopicEdit),
            "user_broadcast" => Some(Permission::UserBroadcast),
            "user_create" => Some(Permission::UserCreate),
            "user_delete" => Some(Permission::UserDelete),
            "user_edit" => Some(Permission::UserEdit),
            "user_kick" => Some(Permission::UserKick),
            "user_message" => Some(Permission::UserMessage),
            _ => None,
        }
    }
}

/// A set of permissions for a user
#[derive(Debug, Clone)]
pub struct Permissions {
    pub(crate) permissions: HashSet<Permission>,
}

impl Permissions {
    /// Create a new empty permission set
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    /// Get all permissions as a vec
    /// Convert to vector of permissions
    pub fn to_vec(&self) -> Vec<Permission> {
        self.permissions.iter().copied().collect()
    }

    /// Add a permission to the set
    #[allow(dead_code)] // Used in integration tests
    pub fn add(&mut self, permission: Permission) {
        self.permissions.insert(permission);
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_snake_case_conversion() {
        // Test that strum correctly converts PascalCase to snake_case
        assert_eq!(Permission::UserList.as_str(), "user_list");
        assert_eq!(Permission::UserInfo.as_str(), "user_info");
        assert_eq!(Permission::ChatSend.as_str(), "chat_send");
        assert_eq!(Permission::ChatReceive.as_str(), "chat_receive");
        assert_eq!(Permission::UserBroadcast.as_str(), "user_broadcast");
        assert_eq!(Permission::UserCreate.as_str(), "user_create");
        assert_eq!(Permission::UserDelete.as_str(), "user_delete");
        assert_eq!(Permission::UserEdit.as_str(), "user_edit");
    }
}
