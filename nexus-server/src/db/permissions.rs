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
    /// Permission to view news posts
    NewsList,
    /// Permission to create news posts
    NewsCreate,
    /// Permission to edit any news post (without: only own posts)
    NewsEdit,
    /// Permission to delete any news post (without: only own posts)
    NewsDelete,
    /// Permission to list files and directories
    FileList,
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
            "news_list" => Some(Permission::NewsList),
            "news_create" => Some(Permission::NewsCreate),
            "news_edit" => Some(Permission::NewsEdit),
            "news_delete" => Some(Permission::NewsDelete),
            "file_list" => Some(Permission::FileList),
            _ => None,
        }
    }
}

/// A set of permissions for a user
///
/// This struct wraps a `HashSet<Permission>` to provide an efficient way to
/// store and check user permissions. It provides methods to add, remove, and
/// query permissions.
///
/// # Usage
///
/// Create a new permission set with `new()`, add permissions with `add()`,
/// and convert to a vector with `to_vec()` for iteration or inspection.
#[derive(Debug, Clone)]
pub struct Permissions {
    pub(crate) permissions: HashSet<Permission>,
}

impl Permissions {
    /// Create a new empty permission set
    ///
    /// Returns a `Permissions` instance with no permissions. Permissions can
    /// be added using the `add()` method.
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    /// Convert the permission set to a vector
    ///
    /// Returns a vector containing all permissions in the set. The order is
    /// not guaranteed as it depends on the internal hash set implementation.
    ///
    /// # Returns
    ///
    /// A `Vec<Permission>` containing all permissions in the set.
    pub fn to_vec(&self) -> Vec<Permission> {
        self.permissions.iter().copied().collect()
    }

    /// Add a permission to the set
    ///
    /// If the permission already exists in the set, this is a no-op.
    ///
    /// # Arguments
    ///
    /// * `permission` - The permission to add to the set
    ///
    /// # Note
    ///
    /// This method is primarily used in tests to build permission sets.
    /// Production code typically uses `set_permissions()` from the database layer.
    #[cfg_attr(not(test), allow(dead_code))]
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
    use nexus_common::ALL_PERMISSIONS;

    #[test]
    fn test_permission_snake_case_conversion() {
        // Test that strum correctly converts PascalCase to snake_case
        assert_eq!(Permission::UserList.as_str(), "user_list");
        assert_eq!(Permission::UserInfo.as_str(), "user_info");
        assert_eq!(Permission::ChatSend.as_str(), "chat_send");
        assert_eq!(Permission::ChatReceive.as_str(), "chat_receive");
        assert_eq!(Permission::ChatTopic.as_str(), "chat_topic");
        assert_eq!(Permission::ChatTopicEdit.as_str(), "chat_topic_edit");
        assert_eq!(Permission::UserBroadcast.as_str(), "user_broadcast");
        assert_eq!(Permission::UserCreate.as_str(), "user_create");
        assert_eq!(Permission::UserDelete.as_str(), "user_delete");
        assert_eq!(Permission::UserEdit.as_str(), "user_edit");
        assert_eq!(Permission::UserKick.as_str(), "user_kick");
        assert_eq!(Permission::UserMessage.as_str(), "user_message");
        assert_eq!(Permission::NewsList.as_str(), "news_list");
        assert_eq!(Permission::NewsCreate.as_str(), "news_create");
        assert_eq!(Permission::NewsEdit.as_str(), "news_edit");
        assert_eq!(Permission::NewsDelete.as_str(), "news_delete");
        assert_eq!(Permission::FileList.as_str(), "file_list");
    }

    #[test]
    fn test_permission_parse_valid() {
        // Test parsing all valid permission strings
        assert_eq!(Permission::parse("user_list"), Some(Permission::UserList));
        assert_eq!(Permission::parse("user_info"), Some(Permission::UserInfo));
        assert_eq!(Permission::parse("chat_send"), Some(Permission::ChatSend));
        assert_eq!(
            Permission::parse("chat_receive"),
            Some(Permission::ChatReceive)
        );
        assert_eq!(Permission::parse("chat_topic"), Some(Permission::ChatTopic));
        assert_eq!(
            Permission::parse("chat_topic_edit"),
            Some(Permission::ChatTopicEdit)
        );
        assert_eq!(
            Permission::parse("user_broadcast"),
            Some(Permission::UserBroadcast)
        );
        assert_eq!(
            Permission::parse("user_create"),
            Some(Permission::UserCreate)
        );
        assert_eq!(
            Permission::parse("user_delete"),
            Some(Permission::UserDelete)
        );
        assert_eq!(Permission::parse("user_edit"), Some(Permission::UserEdit));
        assert_eq!(Permission::parse("user_kick"), Some(Permission::UserKick));
        assert_eq!(
            Permission::parse("user_message"),
            Some(Permission::UserMessage)
        );
        assert_eq!(Permission::parse("news_list"), Some(Permission::NewsList));
        assert_eq!(
            Permission::parse("news_create"),
            Some(Permission::NewsCreate)
        );
        assert_eq!(Permission::parse("news_edit"), Some(Permission::NewsEdit));
        assert_eq!(
            Permission::parse("news_delete"),
            Some(Permission::NewsDelete)
        );
        assert_eq!(Permission::parse("file_list"), Some(Permission::FileList));
    }

    #[test]
    fn test_permission_parse_invalid() {
        // Test that invalid strings return None
        assert_eq!(Permission::parse("invalid"), None);
        assert_eq!(Permission::parse(""), None);
        assert_eq!(Permission::parse("UserList"), None); // Wrong case
        assert_eq!(Permission::parse("user_lists"), None); // Typo
        assert_eq!(Permission::parse("admin"), None);
    }

    #[test]
    fn test_permissions_new() {
        let perms = Permissions::new();
        assert_eq!(perms.to_vec().len(), 0);
    }

    #[test]
    fn test_permissions_default() {
        let perms = Permissions::default();
        assert_eq!(perms.to_vec().len(), 0);
    }

    #[test]
    fn test_permissions_add() {
        let mut perms = Permissions::new();

        perms.add(Permission::UserList);
        assert_eq!(perms.to_vec().len(), 1);

        perms.add(Permission::ChatSend);
        assert_eq!(perms.to_vec().len(), 2);

        // Adding duplicate should not increase count
        perms.add(Permission::UserList);
        assert_eq!(perms.to_vec().len(), 2);
    }

    #[test]
    fn test_permissions_to_vec() {
        let mut perms = Permissions::new();
        perms.add(Permission::UserList);
        perms.add(Permission::ChatSend);
        perms.add(Permission::UserInfo);

        let vec = perms.to_vec();
        assert_eq!(vec.len(), 3);

        // Check that all permissions are present (order doesn't matter)
        assert!(vec.contains(&Permission::UserList));
        assert!(vec.contains(&Permission::ChatSend));
        assert!(vec.contains(&Permission::UserInfo));
    }

    #[test]
    fn test_permission_enum_matches_all_permissions() {
        // Verify that every permission in ALL_PERMISSIONS can be parsed
        for perm_str in ALL_PERMISSIONS {
            assert!(
                Permission::parse(perm_str).is_some(),
                "ALL_PERMISSIONS contains '{}' but Permission::parse() doesn't recognize it",
                perm_str
            );
        }

        // Verify that every Permission variant is in ALL_PERMISSIONS
        let all_variants = [
            Permission::ChatReceive,
            Permission::ChatSend,
            Permission::ChatTopic,
            Permission::ChatTopicEdit,
            Permission::FileList,
            Permission::NewsCreate,
            Permission::NewsDelete,
            Permission::NewsEdit,
            Permission::NewsList,
            Permission::UserBroadcast,
            Permission::UserCreate,
            Permission::UserDelete,
            Permission::UserEdit,
            Permission::UserInfo,
            Permission::UserKick,
            Permission::UserList,
            Permission::UserMessage,
        ];

        for variant in all_variants {
            assert!(
                ALL_PERMISSIONS.contains(&variant.as_str()),
                "Permission::{:?} (as '{}') is not in ALL_PERMISSIONS",
                variant,
                variant.as_str()
            );
        }

        // Verify counts match
        assert_eq!(
            all_variants.len(),
            ALL_PERMISSIONS.len(),
            "Permission enum variant count doesn't match ALL_PERMISSIONS length"
        );
    }
}
