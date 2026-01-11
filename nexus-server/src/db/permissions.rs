//! Permission system for user authorization

use std::collections::HashSet;
use strum::AsRefStr;

/// Permission types for user actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum Permission {
    /// Permission to create/update IP bans
    BanCreate,
    /// Permission to remove IP bans
    BanDelete,
    /// Permission to view list of active bans
    BanList,
    /// Permission to create/update trusted IPs
    TrustCreate,
    /// Permission to remove trusted IPs
    TrustDelete,
    /// Permission to view list of trusted IPs
    TrustList,
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
    /// Permission to download files
    FileDownload,
    /// Permission to upload files to upload/dropbox folders
    FileUpload,
    /// Permission to list files and directories
    FileList,
    /// Permission to search files
    FileSearch,
    /// Permission to trigger file index rebuild
    FileReindex,
    /// Permission to browse entire file area from root
    FileRoot,
    /// Permission to create directories anywhere in file area
    FileCreateDir,
    /// Permission to delete files and empty directories
    FileDelete,
    /// Permission to view detailed file/directory information
    FileInfo,
    /// Permission to rename files and directories
    FileRename,
    /// Permission to move files and directories
    FileMove,
    /// Permission to copy files and directories
    FileCopy,
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
            "ban_create" => Some(Permission::BanCreate),
            "ban_delete" => Some(Permission::BanDelete),
            "ban_list" => Some(Permission::BanList),
            "trust_create" => Some(Permission::TrustCreate),
            "trust_delete" => Some(Permission::TrustDelete),
            "trust_list" => Some(Permission::TrustList),
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
            "file_copy" => Some(Permission::FileCopy),
            "file_create_dir" => Some(Permission::FileCreateDir),
            "file_delete" => Some(Permission::FileDelete),
            "file_download" => Some(Permission::FileDownload),
            "file_info" => Some(Permission::FileInfo),
            "file_upload" => Some(Permission::FileUpload),
            "file_list" => Some(Permission::FileList),
            "file_search" => Some(Permission::FileSearch),
            "file_reindex" => Some(Permission::FileReindex),
            "file_move" => Some(Permission::FileMove),
            "file_rename" => Some(Permission::FileRename),
            "file_root" => Some(Permission::FileRoot),
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
    /// Production code typically uses `update_user()` from the database layer.
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
        assert_eq!(Permission::BanCreate.as_str(), "ban_create");
        assert_eq!(Permission::BanDelete.as_str(), "ban_delete");
        assert_eq!(Permission::BanList.as_str(), "ban_list");
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
        assert_eq!(Permission::FileCopy.as_str(), "file_copy");
        assert_eq!(Permission::FileCreateDir.as_str(), "file_create_dir");
        assert_eq!(Permission::FileDelete.as_str(), "file_delete");
        assert_eq!(Permission::FileDownload.as_str(), "file_download");
        assert_eq!(Permission::FileInfo.as_str(), "file_info");
        assert_eq!(Permission::FileList.as_str(), "file_list");
        assert_eq!(Permission::FileUpload.as_str(), "file_upload");
        assert_eq!(Permission::FileSearch.as_str(), "file_search");
        assert_eq!(Permission::FileReindex.as_str(), "file_reindex");
        assert_eq!(Permission::FileMove.as_str(), "file_move");
        assert_eq!(Permission::FileRename.as_str(), "file_rename");
        assert_eq!(Permission::FileRoot.as_str(), "file_root");
        assert_eq!(Permission::TrustCreate.as_str(), "trust_create");
        assert_eq!(Permission::TrustDelete.as_str(), "trust_delete");
        assert_eq!(Permission::TrustList.as_str(), "trust_list");
    }

    #[test]
    fn test_permission_parse_valid() {
        // Test parsing all valid permission strings
        assert_eq!(Permission::parse("ban_create"), Some(Permission::BanCreate));
        assert_eq!(Permission::parse("ban_delete"), Some(Permission::BanDelete));
        assert_eq!(Permission::parse("ban_list"), Some(Permission::BanList));
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
        assert_eq!(Permission::parse("file_copy"), Some(Permission::FileCopy));
        assert_eq!(
            Permission::parse("file_create_dir"),
            Some(Permission::FileCreateDir)
        );
        assert_eq!(
            Permission::parse("file_delete"),
            Some(Permission::FileDelete)
        );
        assert_eq!(
            Permission::parse("file_download"),
            Some(Permission::FileDownload)
        );
        assert_eq!(Permission::parse("file_info"), Some(Permission::FileInfo));
        assert_eq!(Permission::parse("file_list"), Some(Permission::FileList));
        assert_eq!(
            Permission::parse("file_upload"),
            Some(Permission::FileUpload)
        );
        assert_eq!(
            Permission::parse("file_search"),
            Some(Permission::FileSearch)
        );
        assert_eq!(
            Permission::parse("file_reindex"),
            Some(Permission::FileReindex)
        );
        assert_eq!(Permission::parse("file_move"), Some(Permission::FileMove));
        assert_eq!(
            Permission::parse("file_rename"),
            Some(Permission::FileRename)
        );
        assert_eq!(Permission::parse("file_root"), Some(Permission::FileRoot));
        assert_eq!(
            Permission::parse("trust_create"),
            Some(Permission::TrustCreate)
        );
        assert_eq!(
            Permission::parse("trust_delete"),
            Some(Permission::TrustDelete)
        );
        assert_eq!(Permission::parse("trust_list"), Some(Permission::TrustList));
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
            Permission::BanCreate,
            Permission::BanDelete,
            Permission::BanList,
            Permission::ChatReceive,
            Permission::ChatSend,
            Permission::ChatTopic,
            Permission::ChatTopicEdit,
            Permission::FileCopy,
            Permission::FileCreateDir,
            Permission::FileDelete,
            Permission::FileDownload,
            Permission::FileInfo,
            Permission::FileList,
            Permission::FileMove,
            Permission::FileUpload,
            Permission::FileSearch,
            Permission::FileReindex,
            Permission::FileRename,
            Permission::FileRoot,
            Permission::NewsCreate,
            Permission::NewsDelete,
            Permission::NewsEdit,
            Permission::NewsList,
            Permission::TrustCreate,
            Permission::TrustDelete,
            Permission::TrustList,
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
