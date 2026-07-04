//! Permission system for user authorization

use std::collections::HashSet;
use strum::AsRefStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum Permission {
    BanCreate,
    BanDelete,
    BanList,
    TrustCreate,
    TrustDelete,
    TrustList,
    ConnectionMonitor,
    UserList,
    UserInfo,
    ChatSend,
    ChatReceive,
    ChatJoin,
    ChatCreate,
    ChatList,
    ChatSecret,
    ChatTopic,
    ChatTopicEdit,
    /// Bypass chat rate limiting (flood protection).
    ChatUnlimited,
    UserBroadcast,
    UserCreate,
    UserDelete,
    UserEdit,
    UserKick,
    UserMessage,
    NewsList,
    NewsCreate,
    /// Edit any news post (without: only own posts).
    NewsEdit,
    /// Delete any news post (without: only own posts).
    NewsDelete,
    FileDownload,
    /// Upload files to upload/dropbox folders.
    FileUpload,
    /// Upload files to any directory, bypassing upload/dropbox folder restriction.
    FileUploadAnywhere,
    FileList,
    FileSearch,
    FileReindex,
    /// Browse entire file area from root.
    FileRoot,
    FileCreateDir,
    /// Delete files and empty directories.
    FileDelete,
    FileInfo,
    FileRename,
    FileMove,
    FileCopy,
    GroupCreate,
    GroupDelete,
    GroupEdit,
    TrackerAdd,
    /// Edit a tracker's configuration (also gates fetching detail for the edit form, and accepting a Stage 1 pending fingerprint).
    TrackerEdit,
    TrackerList,
    TrackerRemove,
    VoiceListen,
    VoiceTalk,
}

impl Permission {
    /// Convert permission to its snake_case database string (e.g. UserList → user_list).
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Parse a snake_case permission string; `None` if unrecognized.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ban_create" => Some(Permission::BanCreate),
            "ban_delete" => Some(Permission::BanDelete),
            "ban_list" => Some(Permission::BanList),
            "trust_create" => Some(Permission::TrustCreate),
            "trust_delete" => Some(Permission::TrustDelete),
            "trust_list" => Some(Permission::TrustList),
            "connection_monitor" => Some(Permission::ConnectionMonitor),
            "user_list" => Some(Permission::UserList),
            "user_info" => Some(Permission::UserInfo),
            "chat_send" => Some(Permission::ChatSend),
            "chat_create" => Some(Permission::ChatCreate),
            "chat_join" => Some(Permission::ChatJoin),
            "chat_list" => Some(Permission::ChatList),
            "chat_receive" => Some(Permission::ChatReceive),
            "chat_secret" => Some(Permission::ChatSecret),
            "chat_topic" => Some(Permission::ChatTopic),
            "chat_topic_edit" => Some(Permission::ChatTopicEdit),
            "chat_unlimited" => Some(Permission::ChatUnlimited),
            "file_copy" => Some(Permission::FileCopy),
            "file_create_dir" => Some(Permission::FileCreateDir),
            "file_delete" => Some(Permission::FileDelete),
            "file_download" => Some(Permission::FileDownload),
            "file_info" => Some(Permission::FileInfo),
            "file_list" => Some(Permission::FileList),
            "file_move" => Some(Permission::FileMove),
            "file_reindex" => Some(Permission::FileReindex),
            "file_rename" => Some(Permission::FileRename),
            "file_root" => Some(Permission::FileRoot),
            "file_search" => Some(Permission::FileSearch),
            "file_upload" => Some(Permission::FileUpload),
            "file_upload_anywhere" => Some(Permission::FileUploadAnywhere),
            "group_create" => Some(Permission::GroupCreate),
            "group_delete" => Some(Permission::GroupDelete),
            "group_edit" => Some(Permission::GroupEdit),
            "news_create" => Some(Permission::NewsCreate),
            "news_delete" => Some(Permission::NewsDelete),
            "news_edit" => Some(Permission::NewsEdit),
            "news_list" => Some(Permission::NewsList),
            "tracker_add" => Some(Permission::TrackerAdd),
            "tracker_edit" => Some(Permission::TrackerEdit),
            "tracker_list" => Some(Permission::TrackerList),
            "tracker_remove" => Some(Permission::TrackerRemove),
            "user_broadcast" => Some(Permission::UserBroadcast),
            "user_create" => Some(Permission::UserCreate),
            "user_delete" => Some(Permission::UserDelete),
            "user_edit" => Some(Permission::UserEdit),
            "user_kick" => Some(Permission::UserKick),
            "user_message" => Some(Permission::UserMessage),
            "voice_listen" => Some(Permission::VoiceListen),
            "voice_talk" => Some(Permission::VoiceTalk),
            _ => None,
        }
    }
}

/// Type of a per-user permission override row (`user_permissions.override_type`).
///
/// `Grant` adds a permission on top of the user's group; `Revoke` denies a
/// permission the group would otherwise provide. Stored as snake_case text
/// (`grant` / `revoke`) — the SQL constants in `db/sql.rs` bake the same
/// literals into their WHERE clauses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum OverrideType {
    Grant,
    Revoke,
}

impl OverrideType {
    /// Convert to its database string (`grant` / `revoke`).
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Parse a database override string; `None` if unrecognized.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grant" => Some(OverrideType::Grant),
            "revoke" => Some(OverrideType::Revoke),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Permissions {
    pub(crate) permissions: HashSet<Permission>,
}

impl Permissions {
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    pub fn to_vec(&self) -> Vec<Permission> {
        self.permissions.iter().copied().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Permission> {
        self.permissions.iter()
    }

    pub fn add(&mut self, permission: Permission) {
        self.permissions.insert(permission);
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&[Permission]> for Permissions {
    fn from(slice: &[Permission]) -> Self {
        Self {
            permissions: slice.iter().copied().collect(),
        }
    }
}

impl<const N: usize> From<&[Permission; N]> for Permissions {
    fn from(array: &[Permission; N]) -> Self {
        Self::from(array.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::ALL_PERMISSIONS;

    #[test]
    fn test_permission_snake_case_conversion() {
        assert_eq!(Permission::BanCreate.as_str(), "ban_create");
        assert_eq!(Permission::BanDelete.as_str(), "ban_delete");
        assert_eq!(Permission::BanList.as_str(), "ban_list");
        assert_eq!(Permission::UserList.as_str(), "user_list");
        assert_eq!(Permission::UserInfo.as_str(), "user_info");
        assert_eq!(Permission::ChatJoin.as_str(), "chat_join");
        assert_eq!(Permission::ChatCreate.as_str(), "chat_create");
        assert_eq!(Permission::ChatList.as_str(), "chat_list");
        assert_eq!(Permission::ChatSecret.as_str(), "chat_secret");
        assert_eq!(Permission::ChatSend.as_str(), "chat_send");
        assert_eq!(Permission::ChatReceive.as_str(), "chat_receive");
        assert_eq!(Permission::ChatTopic.as_str(), "chat_topic");
        assert_eq!(Permission::ChatTopicEdit.as_str(), "chat_topic_edit");
        assert_eq!(Permission::ChatUnlimited.as_str(), "chat_unlimited");
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
        assert_eq!(
            Permission::FileUploadAnywhere.as_str(),
            "file_upload_anywhere"
        );
        assert_eq!(Permission::FileSearch.as_str(), "file_search");
        assert_eq!(Permission::FileReindex.as_str(), "file_reindex");
        assert_eq!(Permission::FileMove.as_str(), "file_move");
        assert_eq!(Permission::FileRename.as_str(), "file_rename");
        assert_eq!(Permission::FileRoot.as_str(), "file_root");
        assert_eq!(Permission::ConnectionMonitor.as_str(), "connection_monitor");
        assert_eq!(Permission::GroupCreate.as_str(), "group_create");
        assert_eq!(Permission::GroupDelete.as_str(), "group_delete");
        assert_eq!(Permission::GroupEdit.as_str(), "group_edit");
        assert_eq!(Permission::TrackerAdd.as_str(), "tracker_add");
        assert_eq!(Permission::TrackerEdit.as_str(), "tracker_edit");
        assert_eq!(Permission::TrackerList.as_str(), "tracker_list");
        assert_eq!(Permission::TrackerRemove.as_str(), "tracker_remove");
        assert_eq!(Permission::TrustCreate.as_str(), "trust_create");
        assert_eq!(Permission::TrustDelete.as_str(), "trust_delete");
        assert_eq!(Permission::TrustList.as_str(), "trust_list");
        assert_eq!(Permission::VoiceListen.as_str(), "voice_listen");
        assert_eq!(Permission::VoiceTalk.as_str(), "voice_talk");
    }

    #[test]
    fn test_override_type_snake_case_conversion() {
        assert_eq!(OverrideType::Grant.as_str(), "grant");
        assert_eq!(OverrideType::Revoke.as_str(), "revoke");
    }

    #[test]
    fn test_override_type_parse() {
        assert_eq!(OverrideType::parse("grant"), Some(OverrideType::Grant));
        assert_eq!(OverrideType::parse("revoke"), Some(OverrideType::Revoke));
        assert_eq!(OverrideType::parse("deny"), None);
        assert_eq!(OverrideType::parse("GRANT"), None);
        assert_eq!(OverrideType::parse(""), None);
    }

    #[test]
    fn test_permission_parse_valid() {
        assert_eq!(Permission::parse("ban_create"), Some(Permission::BanCreate));
        assert_eq!(Permission::parse("ban_delete"), Some(Permission::BanDelete));
        assert_eq!(Permission::parse("ban_list"), Some(Permission::BanList));
        assert_eq!(Permission::parse("user_list"), Some(Permission::UserList));
        assert_eq!(Permission::parse("user_info"), Some(Permission::UserInfo));
        assert_eq!(Permission::parse("chat_join"), Some(Permission::ChatJoin));
        assert_eq!(Permission::parse("chat_list"), Some(Permission::ChatList));
        assert_eq!(
            Permission::parse("chat_secret"),
            Some(Permission::ChatSecret)
        );
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
            Permission::parse("chat_unlimited"),
            Some(Permission::ChatUnlimited)
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
            Permission::parse("file_upload_anywhere"),
            Some(Permission::FileUploadAnywhere)
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
            Permission::parse("tracker_add"),
            Some(Permission::TrackerAdd)
        );
        assert_eq!(
            Permission::parse("tracker_edit"),
            Some(Permission::TrackerEdit)
        );
        assert_eq!(
            Permission::parse("tracker_list"),
            Some(Permission::TrackerList)
        );
        assert_eq!(
            Permission::parse("tracker_remove"),
            Some(Permission::TrackerRemove)
        );
        assert_eq!(
            Permission::parse("trust_create"),
            Some(Permission::TrustCreate)
        );
        assert_eq!(
            Permission::parse("trust_delete"),
            Some(Permission::TrustDelete)
        );
        assert_eq!(Permission::parse("trust_list"), Some(Permission::TrustList));
        assert_eq!(
            Permission::parse("chat_create"),
            Some(Permission::ChatCreate)
        );
        assert_eq!(
            Permission::parse("connection_monitor"),
            Some(Permission::ConnectionMonitor)
        );
        assert_eq!(
            Permission::parse("group_create"),
            Some(Permission::GroupCreate)
        );
        assert_eq!(
            Permission::parse("group_delete"),
            Some(Permission::GroupDelete)
        );
        assert_eq!(Permission::parse("group_edit"), Some(Permission::GroupEdit));
        assert_eq!(
            Permission::parse("voice_listen"),
            Some(Permission::VoiceListen)
        );
        assert_eq!(Permission::parse("voice_talk"), Some(Permission::VoiceTalk));
    }

    #[test]
    fn test_permission_parse_invalid() {
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

        // Adding a duplicate does not increase the count.
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

        // Order is not guaranteed; check membership only.
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
            Permission::ChatCreate,
            Permission::ChatJoin,
            Permission::ChatList,
            Permission::ChatReceive,
            Permission::ChatSecret,
            Permission::ChatSend,
            Permission::ChatTopic,
            Permission::ChatTopicEdit,
            Permission::ChatUnlimited,
            Permission::ConnectionMonitor,
            Permission::FileCopy,
            Permission::FileCreateDir,
            Permission::FileDelete,
            Permission::FileDownload,
            Permission::FileInfo,
            Permission::FileList,
            Permission::FileMove,
            Permission::FileUpload,
            Permission::FileUploadAnywhere,
            Permission::FileSearch,
            Permission::FileReindex,
            Permission::FileRename,
            Permission::FileRoot,
            Permission::GroupCreate,
            Permission::GroupDelete,
            Permission::GroupEdit,
            Permission::NewsCreate,
            Permission::NewsDelete,
            Permission::NewsEdit,
            Permission::NewsList,
            Permission::TrackerAdd,
            Permission::TrackerEdit,
            Permission::TrackerList,
            Permission::TrackerRemove,
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
            Permission::VoiceListen,
            Permission::VoiceTalk,
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
