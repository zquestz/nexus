//! Per-type payload limits for protocol messages

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::PERMISSIONS_COUNT;

// =============================================================================
// Permission-dependent limit calculations
// =============================================================================

/// Overhead per permission in a JSON array: `"permission_name",` = max 35 bytes
/// (32 char max + 2 quotes + comma + potential space)
const PERMISSION_ARRAY_ENTRY_SIZE: usize = 35;

/// Calculate the size contribution of a max-sized permissions array
const fn permissions_array_size() -> usize {
    PERMISSIONS_COUNT * PERMISSION_ARRAY_ENTRY_SIZE
}

/// Base overhead for UserCreate message (without permissions array)
/// {"type":"UserCreate","username":"...32...","password":"...256...","is_admin":false,"is_shared":false,"enabled":true,"permissions":[]}
const USER_CREATE_BASE: usize = 403;

/// Base overhead for UserUpdate message (without permissions array)  
/// {"type":"UserUpdate","username":"...32...","new_username":"...32...","password":"...256...","is_admin":false,"enabled":true,"current_password":"...256...","permissions":[]}
const USER_UPDATE_BASE: usize = 758;

/// Base overhead for UserEditResponse message (without permissions array)
const USER_EDIT_RESPONSE_BASE: usize = 154;

/// Base overhead for LoginResponse message (without permissions array)
const LOGIN_RESPONSE_BASE: usize = 700909;

/// Base overhead for PermissionsUpdated message (without permissions array)
const PERMISSIONS_UPDATED_BASE: usize = 700847;

/// Maximum payload sizes for each message type
///
/// These limits are enforced after parsing the frame header but before reading
/// the payload, allowing early rejection of oversized messages.
///
/// Limits are set to exactly match the maximum possible serialized JSON size
/// based on validator constraints. Tests verify these values are correct.
///
/// A limit of `0` means "unlimited" (no per-type limit). This is used for
/// server-to-client messages where the client has already chosen to trust
/// the server. The global `MAX_PAYLOAD_LENGTH` sanity check still applies.
static MESSAGE_TYPE_LIMITS: LazyLock<HashMap<&'static str, u64>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Permission array size calculated from PERMISSIONS_COUNT
    let perm_size = permissions_array_size() as u64;

    // Client messages (limits match actual max size from validators)
    m.insert("ChatSend", 1056);
    m.insert("ChatTopicUpdate", 293);
    m.insert("Handshake", 65);
    m.insert("Login", 176991);
    m.insert("UserBroadcast", 1061);
    m.insert("UserCreate", USER_CREATE_BASE as u64 + perm_size);
    m.insert("UserDelete", 67);
    m.insert("UserEdit", 65);
    m.insert("UserInfo", 65);
    m.insert("UserKick", 65);
    m.insert("UserList", 31);
    m.insert("UserUpdate", USER_UPDATE_BASE as u64 + perm_size);
    m.insert("ServerInfoUpdate", 700421); // includes image field (700000 + overhead)

    // News client messages
    m.insert("NewsList", 19);
    m.insert("NewsShow", 32);
    m.insert("NewsCreate", 704150); // body (4096) + image (700000) + overhead
    m.insert("NewsEdit", 30);
    m.insert("NewsUpdate", 704170); // id + body (4096) + image (700000) + overhead
    m.insert("NewsDelete", 32);

    // File client messages
    m.insert("FileList", 4158); // path (4096) + root bool + show_hidden bool + overhead
    m.insert("FileCreateDir", 4433); // path (4096) + name (255) + root bool + overhead
    m.insert("FileDelete", 4138); // path (4096) + root bool + overhead
    m.insert("FileInfo", 4138); // path (4096) + root bool + overhead
    m.insert("FileRename", 4433); // path (4096) + new_name (255) + root bool + overhead
    m.insert("FileMove", 8316); // source_path (4096) + destination_dir (4096) + overwrite + source_root + destination_root + overhead
    m.insert("FileCopy", 8316); // source_path (4096) + destination_dir (4096) + overwrite + source_root + destination_root + overhead
    m.insert("FileDownload", 4142); // path (4096) + root bool + overhead

    // Server messages (limits match actual max size from validators)
    // ServerInfo now includes image field (up to 700000 chars), adding ~700011 bytes
    m.insert("ChatMessage", 1164);
    m.insert("ChatTopicUpdated", 340);
    m.insert("ChatTopicUpdateResponse", 573);
    m.insert("Error", 2154);
    m.insert("HandshakeResponse", 356);
    m.insert("LoginResponse", LOGIN_RESPONSE_BASE as u64 + perm_size);
    m.insert(
        "PermissionsUpdated",
        PERMISSIONS_UPDATED_BASE as u64 + perm_size,
    );
    m.insert("ServerBroadcast", 1133);
    m.insert("ServerInfoUpdated", 700483); // includes ServerInfo with image
    m.insert("ServerInfoUpdateResponse", 574);
    m.insert("UserConnected", 176359);
    m.insert("UserCreateResponse", 614);
    m.insert("UserDeleteResponse", 614);
    m.insert("UserDisconnected", 97);
    m.insert(
        "UserEditResponse",
        USER_EDIT_RESPONSE_BASE as u64 + perm_size,
    );
    m.insert("UserBroadcastResponse", 571);
    m.insert("UserInfoResponse", 177477);
    m.insert("UserKickResponse", 612);
    m.insert("UserListResponse", 0); // unlimited (server-trusted)
    m.insert("UserMessage", 1177); // shared type: server (1177) > client (1108)
    m.insert("UserMessageResponse", 569);
    m.insert("UserUpdated", 176412);
    m.insert("UserUpdateResponse", 614);

    // News server messages
    m.insert("NewsListResponse", 0); // unlimited (server-trusted, can have many items)
    m.insert("NewsShowResponse", 704500); // single NewsItem with body + image
    m.insert("NewsCreateResponse", 704500); // single NewsItem with body + image
    m.insert("NewsEditResponse", 704500); // single NewsItem with body + image
    m.insert("NewsUpdateResponse", 704500); // single NewsItem with body + image
    m.insert("NewsDeleteResponse", 100);
    m.insert("NewsUpdated", 50); // action enum + id

    // File server messages
    m.insert("FileListResponse", 0); // unlimited (server-trusted, can have many entries)
    // FileCreateDirResponse: path can be up to 4352 bytes (4096 path + 1 separator + 255 name)
    // JSON escaping doubles size for quote characters (worst case): 8704 bytes + ~60 overhead
    m.insert("FileCreateDirResponse", 9000);
    m.insert("FileDeleteResponse", 300); // success bool + error message + overhead
    // FileInfoResponse: name (4096) + size + created + modified + is_directory + is_symlink
    // + mime_type (~128) + item_count + error (~2048) + overhead
    m.insert("FileInfoResponse", 6500);
    m.insert("FileRenameResponse", 300); // success bool + error message + overhead
    m.insert("FileMoveResponse", 350); // success bool + error message + error_kind + overhead
    m.insert("FileCopyResponse", 350); // success bool + error message + error_kind + overhead
    m.insert("FileDownloadResponse", 2186); // success + error (2048) + error_kind (64) + overhead

    m
});

/// Get the maximum payload size for a message type
///
/// # Panics
///
/// Panics if the message type is unknown. This should never happen in practice
/// because unknown types are rejected by `read_frame()` before this is called.
#[must_use]
pub fn max_payload_for_type(message_type: &str) -> u64 {
    MESSAGE_TYPE_LIMITS
        .get(message_type)
        .copied()
        .expect("unknown message types should be rejected before calling max_payload_for_type")
}

/// Check if a message type is known
#[must_use]
pub fn is_known_message_type(message_type: &str) -> bool {
    MESSAGE_TYPE_LIMITS.contains_key(message_type)
}

/// Get all known message type names
#[must_use]
pub fn known_message_types() -> Vec<&'static str> {
    MESSAGE_TYPE_LIMITS.keys().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        ChatInfo, ClientMessage, ServerInfo, ServerMessage, UserInfo, UserInfoDetailed,
    };
    use crate::validators::{
        MAX_AVATAR_DATA_URI_LENGTH, MAX_CHAT_TOPIC_LENGTH, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT,
        MAX_FILE_PATH_LENGTH, MAX_LOCALE_LENGTH, MAX_MESSAGE_LENGTH, MAX_NICKNAME_LENGTH,
        MAX_PASSWORD_LENGTH, MAX_PERMISSION_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH,
        MAX_SERVER_IMAGE_DATA_URI_LENGTH, MAX_SERVER_NAME_LENGTH, MAX_USERNAME_LENGTH,
        MAX_VERSION_LENGTH,
    };

    /// Helper to get serialized JSON size of a message
    fn json_size<T: serde::Serialize>(msg: &T) -> usize {
        serde_json::to_vec(msg).unwrap().len()
    }

    /// Helper to create a string of given length
    fn str_of_len(len: usize) -> String {
        "x".repeat(len)
    }

    #[test]
    #[should_panic(expected = "unknown message types should be rejected")]
    fn test_max_payload_for_type_unknown_panics() {
        let _ = max_payload_for_type("UnknownType");
    }

    #[test]
    fn test_is_known_message_type() {
        assert!(is_known_message_type("ChatSend"));
        assert!(is_known_message_type("Login"));
        assert!(!is_known_message_type("FakeMessage"));
    }

    #[test]
    fn test_all_protocol_types_have_limits() {
        // This test verifies that MESSAGE_TYPE_LIMITS has the expected number of entries.
        // If you add a new message type to ClientMessage or ServerMessage:
        // 1. Add a payload limit to MESSAGE_TYPE_LIMITS
        // 2. Update CLIENT_MESSAGE_COUNT or SERVER_MESSAGE_COUNT below
        //
        // The exhaustive match in io.rs (client_message_type/server_message_type)
        // will cause a compile error if you add a variant there, reminding you to
        // also add the limit here.
        //
        // Note: UserMessage is shared between client and server (same type name),
        // so it's only counted once in the HashMap.
        const CLIENT_MESSAGE_COUNT: usize = 28; // Added 6 News + 7 File + 1 Transfer client messages
        const SERVER_MESSAGE_COUNT: usize = 38; // Added 7 News + 7 File + 1 Transfer server messages
        const SHARED_MESSAGE_COUNT: usize = 1; // UserMessage
        const TOTAL_MESSAGE_COUNT: usize =
            CLIENT_MESSAGE_COUNT + SERVER_MESSAGE_COUNT - SHARED_MESSAGE_COUNT;

        let known_types = known_message_types();
        assert_eq!(
            known_types.len(),
            TOTAL_MESSAGE_COUNT,
            "MESSAGE_TYPE_LIMITS has {} entries but expected {} ({}+{}-{}). \
             Did you add a new message type? Update the limit and the count here.",
            known_types.len(),
            TOTAL_MESSAGE_COUNT,
            CLIENT_MESSAGE_COUNT,
            SERVER_MESSAGE_COUNT,
            SHARED_MESSAGE_COUNT
        );
    }

    // =========================================================================
    // Client message size tests - verify limits match actual max sizes
    // =========================================================================

    #[test]
    fn test_limit_chat_send() {
        let msg = ClientMessage::ChatSend {
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("ChatSend") as usize);
    }

    #[test]
    fn test_limit_chat_topic_update() {
        let msg = ClientMessage::ChatTopicUpdate {
            topic: str_of_len(MAX_CHAT_TOPIC_LENGTH),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ChatTopicUpdate") as usize
        );
    }

    #[test]
    fn test_limit_handshake() {
        let msg = ClientMessage::Handshake {
            version: str_of_len(MAX_VERSION_LENGTH),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("Handshake") as usize);
    }

    #[test]
    fn test_limit_login() {
        let msg = ClientMessage::Login {
            username: str_of_len(MAX_USERNAME_LENGTH),
            password: str_of_len(MAX_PASSWORD_LENGTH),
            features: (0..MAX_FEATURES_COUNT)
                .map(|_| str_of_len(MAX_FEATURE_LENGTH))
                .collect(),
            locale: str_of_len(MAX_LOCALE_LENGTH),
            avatar: Some(str_of_len(MAX_AVATAR_DATA_URI_LENGTH)),
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("Login") as usize);
    }

    #[test]
    fn test_limit_user_broadcast() {
        let msg = ClientMessage::UserBroadcast {
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserBroadcast") as usize
        );
    }

    #[test]
    fn test_limit_user_create() {
        let msg = ClientMessage::UserCreate {
            username: str_of_len(MAX_USERNAME_LENGTH),
            password: str_of_len(MAX_PASSWORD_LENGTH),
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: (0..PERMISSIONS_COUNT)
                .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                .collect(),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("UserCreate") as usize);
    }

    #[test]
    fn test_limit_user_delete() {
        let msg = ClientMessage::UserDelete {
            username: str_of_len(MAX_USERNAME_LENGTH),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("UserDelete") as usize);
    }

    #[test]
    fn test_limit_user_edit() {
        let msg = ClientMessage::UserEdit {
            username: str_of_len(MAX_USERNAME_LENGTH),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("UserEdit") as usize);
    }

    #[test]
    fn test_limit_user_info() {
        let msg = ClientMessage::UserInfo {
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("UserInfo") as usize);
    }

    #[test]
    fn test_limit_user_kick() {
        let msg = ClientMessage::UserKick {
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("UserKick") as usize);
    }

    #[test]
    fn test_limit_user_list() {
        // Use all: false since "false" (5 chars) is longer than "true" (4 chars)
        let msg = ClientMessage::UserList { all: false };
        assert_eq!(json_size(&msg), max_payload_for_type("UserList") as usize);
    }

    #[test]
    fn test_limit_user_message_client() {
        let msg = ClientMessage::UserMessage {
            to_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        // Client variant is smaller than server variant, so it fits within the limit
        assert!(json_size(&msg) <= max_payload_for_type("UserMessage") as usize);
    }

    #[test]
    fn test_limit_user_update() {
        let msg = ClientMessage::UserUpdate {
            username: str_of_len(MAX_USERNAME_LENGTH),
            current_password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            requested_username: Some(str_of_len(MAX_USERNAME_LENGTH)),
            requested_password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            requested_is_admin: Some(true),
            requested_enabled: Some(true),
            requested_permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("UserUpdate") as usize);
    }

    #[test]
    fn test_limit_server_info_update() {
        let msg = ClientMessage::ServerInfoUpdate {
            name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
            description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
            max_connections_per_ip: Some(u32::MAX),
            image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ServerInfoUpdate") as usize
        );
    }

    #[test]
    fn test_limit_file_list() {
        let msg = ClientMessage::FileList {
            path: str_of_len(MAX_FILE_PATH_LENGTH),
            root: false,
            show_hidden: false,
        };
        assert_eq!(json_size(&msg), max_payload_for_type("FileList") as usize);
    }

    // =========================================================================
    // Server message size tests - verify limits match actual max sizes
    // =========================================================================

    #[test]
    fn test_limit_chat_message() {
        let msg = ServerMessage::ChatMessage {
            session_id: u32::MAX,
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
            is_admin: false,
            is_shared: false,
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ChatMessage") as usize
        );
    }

    #[test]
    fn test_limit_chat_topic_updated() {
        let msg = ServerMessage::ChatTopicUpdated {
            topic: str_of_len(MAX_CHAT_TOPIC_LENGTH),
            username: str_of_len(MAX_USERNAME_LENGTH),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ChatTopicUpdated") as usize
        );
    }

    #[test]
    fn test_limit_chat_topic_update_response() {
        let msg = ServerMessage::ChatTopicUpdateResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ChatTopicUpdateResponse") as usize
        );
    }

    #[test]
    fn test_limit_error() {
        let msg = ServerMessage::Error {
            message: str_of_len(2048),
            command: Some(str_of_len(64)),
        };
        assert_eq!(json_size(&msg), max_payload_for_type("Error") as usize);
    }

    #[test]
    fn test_limit_handshake_response() {
        let msg = ServerMessage::HandshakeResponse {
            success: false,
            version: Some(str_of_len(MAX_VERSION_LENGTH)),
            error: Some(str_of_len(256)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("HandshakeResponse") as usize
        );
    }

    #[test]
    fn test_limit_login_response() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            error: None,
            session_id: Some(u32::MAX),
            is_admin: Some(true),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            server_info: Some(ServerInfo {
                name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
                description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
            }),
            chat_info: Some(ChatInfo {
                topic: str_of_len(MAX_CHAT_TOPIC_LENGTH),
                topic_set_by: str_of_len(MAX_USERNAME_LENGTH),
            }),
            locale: Some(str_of_len(MAX_LOCALE_LENGTH)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("LoginResponse") as usize
        );
    }

    #[test]
    fn test_limit_permissions_updated() {
        let msg = ServerMessage::PermissionsUpdated {
            is_admin: true,
            permissions: (0..PERMISSIONS_COUNT)
                .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                .collect(),
            server_info: Some(ServerInfo {
                name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
                description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
            }),
            chat_info: Some(ChatInfo {
                topic: str_of_len(MAX_CHAT_TOPIC_LENGTH),
                topic_set_by: str_of_len(MAX_USERNAME_LENGTH),
            }),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("PermissionsUpdated") as usize
        );
    }

    #[test]
    fn test_limit_server_info_updated() {
        let msg = ServerMessage::ServerInfoUpdated {
            server_info: ServerInfo {
                name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
                description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
            },
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ServerInfoUpdated") as usize
        );
    }

    #[test]
    fn test_limit_server_info_update_response() {
        let msg = ServerMessage::ServerInfoUpdateResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ServerInfoUpdateResponse") as usize
        );
    }

    #[test]
    fn test_limit_server_broadcast() {
        let msg = ServerMessage::ServerBroadcast {
            session_id: u32::MAX,
            username: str_of_len(MAX_USERNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("ServerBroadcast") as usize
        );
    }

    #[test]
    fn test_limit_user_connected() {
        let msg = ServerMessage::UserConnected {
            user: UserInfo {
                username: str_of_len(MAX_USERNAME_LENGTH),
                nickname: str_of_len(MAX_NICKNAME_LENGTH),
                login_time: i64::MAX,
                is_admin: false,
                is_shared: false,
                session_ids: vec![u32::MAX; 10],
                locale: str_of_len(MAX_LOCALE_LENGTH),
                avatar: Some(str_of_len(MAX_AVATAR_DATA_URI_LENGTH)),
            },
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserConnected") as usize
        );
    }

    #[test]
    fn test_limit_user_create_response() {
        let msg = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(str_of_len(512)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserCreateResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_delete_response() {
        let msg = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(str_of_len(512)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserDeleteResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_disconnected() {
        let msg = ServerMessage::UserDisconnected {
            session_id: u32::MAX,
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserDisconnected") as usize
        );
    }

    #[test]
    fn test_limit_user_edit_response() {
        let msg = ServerMessage::UserEditResponse {
            success: true,
            error: None,
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(true),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserEditResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_broadcast_response() {
        let msg = ServerMessage::UserBroadcastResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserBroadcastResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_info_response() {
        let msg = ServerMessage::UserInfoResponse {
            success: true,
            error: None,
            user: Some(UserInfoDetailed {
                username: str_of_len(MAX_USERNAME_LENGTH),
                nickname: str_of_len(MAX_NICKNAME_LENGTH),
                login_time: i64::MAX,
                is_shared: false,
                session_ids: vec![u32::MAX; 10],
                features: (0..MAX_FEATURES_COUNT)
                    .map(|_| str_of_len(MAX_FEATURE_LENGTH))
                    .collect(),
                created_at: i64::MAX,
                locale: str_of_len(MAX_LOCALE_LENGTH),
                avatar: Some(str_of_len(MAX_AVATAR_DATA_URI_LENGTH)),
                is_admin: Some(false),
                addresses: Some(vec![str_of_len(45); 10]),
            }),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserInfoResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_kick_response() {
        let msg = ServerMessage::UserKickResponse {
            success: false,
            error: Some(str_of_len(512)),
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserKickResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_list_response() {
        // UserListResponse has no per-type limit (0 = unlimited) since it comes
        // from the server which the client has chosen to trust. The global
        // MAX_PAYLOAD_LENGTH sanity check still applies.
        assert_eq!(max_payload_for_type("UserListResponse"), 0);
    }

    #[test]
    fn test_limit_user_message_server() {
        let msg = ServerMessage::UserMessage {
            from_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            from_admin: true,
            to_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        // Server variant defines the limit since it's larger
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserMessage") as usize
        );
    }

    #[test]
    fn test_limit_user_message_response() {
        let msg = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserMessageResponse") as usize
        );
    }

    #[test]
    fn test_limit_user_updated() {
        let msg = ServerMessage::UserUpdated {
            previous_username: str_of_len(MAX_USERNAME_LENGTH),
            user: UserInfo {
                username: str_of_len(MAX_USERNAME_LENGTH),
                nickname: str_of_len(MAX_NICKNAME_LENGTH),
                login_time: i64::MAX,
                is_admin: false,
                is_shared: false,
                session_ids: vec![u32::MAX; 10],
                locale: str_of_len(MAX_LOCALE_LENGTH),
                avatar: Some(str_of_len(MAX_AVATAR_DATA_URI_LENGTH)),
            },
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserUpdated") as usize
        );
    }

    #[test]
    fn test_limit_user_update_response() {
        let msg = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(str_of_len(512)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("UserUpdateResponse") as usize
        );
    }

    // =========================================================================
    // Transfer message size tests
    // =========================================================================

    #[test]
    fn test_limit_file_download_response() {
        // Error case is larger than success case
        let msg = ServerMessage::FileDownloadResponse {
            success: false,
            error: Some(str_of_len(2048)),
            error_kind: Some(str_of_len(64)),
            total_size: None,
            file_count: None,
            bytes_to_transfer: None,
            token: None,
            transfer_id: None,
            port: None,
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("FileDownloadResponse") as usize
        );
    }

    #[test]
    fn test_limit_file_download() {
        let msg = ClientMessage::FileDownload {
            path: str_of_len(MAX_FILE_PATH_LENGTH),
            root: false,
        };
        assert_eq!(
            json_size(&msg),
            max_payload_for_type("FileDownload") as usize
        );
    }
}
