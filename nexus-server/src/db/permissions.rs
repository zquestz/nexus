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
    /// Permission to delete users
    UserDelete,
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
}

/// Set of permissions for a user
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
    pub fn to_vec(&self) -> Vec<Permission> {
        self.permissions.iter().copied().collect()
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
        assert_eq!(Permission::UserDelete.as_str(), "user_delete");
    }
}
