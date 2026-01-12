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
/// {"type":"UserCreate","username":"...32...","password":"...256...","is_admin":false,"is_shared":false,"enabled":false,"permissions":[]}
const USER_CREATE_BASE: usize = 404;

/// Base overhead for UserUpdate message (without permissions array)  
/// {"type":"UserUpdate","username":"...32...","new_username":"...32...","password":"...256...","is_admin":false,"enabled":false,"current_password":"...256...","permissions":[]}
const USER_UPDATE_BASE: usize = 760;

/// Base overhead for UserEditResponse message (without permissions array)
/// enabled:false adds 1 char vs true
const USER_EDIT_RESPONSE_BASE: usize = 156;

/// Base overhead for LoginResponse message (without permissions array)
/// Includes ServerInfo with transfer_port, max_transfers_per_ip, file_reindex_interval, persistent_channels, and auto_join_channels fields
/// Also includes channels array for auto-joined channels (up to ~10 channels with ~50 members each)
const LOGIN_RESPONSE_BASE: usize = 724000;

/// Base overhead for PermissionsUpdated message (without permissions array)
/// Includes ServerInfo with transfer_port, max_transfers_per_ip, file_reindex_interval, persistent_channels, and auto_join_channels fields
const PERMISSIONS_UPDATED_BASE: usize = 702011;

/// Apply 20% padding to a limit for safety margin
const fn pad_limit(base: u64) -> u64 {
    // Use integer math: multiply by 6 and divide by 5 equals 1.2x
    (base * 6) / 5
}

/// Maximum payload sizes for each message type
///
/// These limits are enforced after parsing the frame header but before reading
/// the payload, allowing early rejection of oversized messages.
///
/// Base limits match the maximum possible serialized JSON size based on
/// validator constraints, then 20% padding is added for safety margin.
/// Tests verify the base values fit within the padded limits.
///
/// A limit of `0` means "unlimited" (no per-type limit). This is used for
/// server-to-client messages where the client has already chosen to trust
/// the server. The global `MAX_PAYLOAD_LENGTH` sanity check still applies.
///
/// ## Shared Message Type Names
///
/// Some message type names exist in both `ClientMessage` and `ServerMessage`
/// (e.g., `FileStart`, `FileStartResponse`, `FileData`, `UserMessage`). These
/// are **intentional mirrors** - same structure, same payload limit, opposite
/// direction. The HashMap naturally enforces this constraint: one limit per
/// type name guarantees both enums use the same limit for shared types.
static MESSAGE_TYPE_LIMITS: LazyLock<HashMap<&'static str, u64>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Permission array size calculated from PERMISSIONS_COUNT
    let perm_size = permissions_array_size() as u64;

    // Client messages (base limits match actual max size, then padded 20%)
    m.insert("ChatSend", pad_limit(1101)); // Added channel field (32 + overhead)
    m.insert("ChatTopicUpdate", pad_limit(338)); // Added channel field (32 + overhead)
    m.insert("ChatJoin", pad_limit(64)); // channel (32) + overhead
    m.insert("ChatLeave", pad_limit(65)); // channel (32) + overhead
    m.insert("ChatList", pad_limit(19)); // {"type":"ChatList"} = 19 bytes
    m.insert("ChatSecret", pad_limit(81)); // channel (32) + secret bool + overhead
    m.insert("Handshake", pad_limit(65));
    m.insert("Login", pad_limit(176991));
    m.insert("UserBroadcast", pad_limit(1061));
    m.insert("UserCreate", pad_limit(USER_CREATE_BASE as u64 + perm_size));
    m.insert("UserDelete", pad_limit(67));
    m.insert("UserEdit", pad_limit(65));
    m.insert("UserInfo", pad_limit(65));
    m.insert("UserKick", pad_limit(2125)); // nickname (32) + reason (2048) + overhead
    m.insert("UserList", pad_limit(31));
    m.insert("UserUpdate", pad_limit(USER_UPDATE_BASE as u64 + perm_size));
    m.insert("UserAway", pad_limit(160)); // status (128) + overhead
    m.insert("UserBack", pad_limit(19));
    m.insert("UserStatus", pad_limit(161)); // status (128) + overhead
    m.insert("ServerInfoUpdate", pad_limit(701563)); // includes image field (700000 + overhead) + max_transfers_per_ip + file_reindex_interval + persistent_channels (512) + auto_join_channels (512)

    // Ban client messages
    m.insert("BanCreate", pad_limit(2400)); // target (32 nickname or 45 IP/hostname) + duration (10) + reason (2048) + overhead
    m.insert("BanDelete", pad_limit(80)); // target (32 nickname or 45 IP) + overhead
    m.insert("BanList", pad_limit(18));

    // Trust client messages
    m.insert("TrustCreate", pad_limit(2400)); // target (32 nickname or 45 IP/hostname) + duration (10) + reason (2048) + overhead
    m.insert("TrustDelete", pad_limit(80)); // target (32 nickname or 45 IP) + overhead
    m.insert("TrustList", pad_limit(20)); // {"type":"TrustList"} = 20 bytes

    // News client messages
    m.insert("NewsList", pad_limit(19));
    m.insert("NewsShow", pad_limit(32));
    m.insert("NewsCreate", pad_limit(704150)); // body (4096) + image (700000) + overhead
    m.insert("NewsEdit", pad_limit(30));
    m.insert("NewsUpdate", pad_limit(704170)); // id + body (4096) + image (700000) + overhead
    m.insert("NewsDelete", pad_limit(32));

    // File client messages
    m.insert("FileList", pad_limit(4158)); // path (4096) + root bool + show_hidden bool + overhead
    m.insert("FileCreateDir", pad_limit(4433)); // path (4096) + name (255) + root bool + overhead
    m.insert("FileDelete", pad_limit(4138)); // path (4096) + root bool + overhead
    m.insert("FileInfo", pad_limit(4138)); // path (4096) + root bool + overhead
    m.insert("FileRename", pad_limit(4433)); // path (4096) + new_name (255) + root bool + overhead
    m.insert("FileMove", pad_limit(8316)); // source_path (4096) + destination_dir (4096) + overwrite + source_root + destination_root + overhead
    m.insert("FileCopy", pad_limit(8316)); // source_path (4096) + destination_dir (4096) + overwrite + source_root + destination_root + overhead
    m.insert("FileDownload", pad_limit(4142)); // path (4096) + root bool + overhead
    m.insert("FileUpload", pad_limit(4180)); // destination (4096) + file_count (u64) + total_size (u64) + root + overhead
    m.insert("FileSearch", pad_limit(305)); // query (256 max) + root bool + overhead
    m.insert("FileReindex", pad_limit(22)); // just type name + overhead

    // Server messages (base limits match actual max size, then padded 20%)
    // ServerInfo now includes image field (up to 700000 bytes), adding ~700011 bytes
    m.insert("ChatMessage", pad_limit(1209)); // Added channel field (32 + overhead)
    m.insert("ChatUpdated", pad_limit(450)); // channel (32) + topic (256) + topic_set_by (32) + secret + secret_set_by (32) + overhead
    m.insert("ChatTopicUpdateResponse", pad_limit(573));
    m.insert("ChatJoinResponse", pad_limit(4350)); // success + error (2048) + channel (32) + topic (256) + topic_set_by (32) + secret + members array (~50 members) + overhead
    m.insert("ChatLeaveResponse", pad_limit(2200)); // channel (32) + error (2048) + overhead
    m.insert("ChatListResponse", 0); // unlimited (server-trusted, can have many channels)
    m.insert("ChatSecretResponse", pad_limit(2150)); // success + error (2048) + overhead
    m.insert("ChatUserJoined", pad_limit(151)); // channel (32) + nickname (32) + is_admin + is_shared + overhead
    m.insert("ChatUserLeft", pad_limit(114)); // channel (32) + nickname (32) + overhead
    m.insert("Error", pad_limit(2154));
    m.insert("HandshakeResponse", pad_limit(356));
    m.insert(
        "LoginResponse",
        pad_limit(LOGIN_RESPONSE_BASE as u64 + perm_size),
    );
    m.insert(
        "PermissionsUpdated",
        pad_limit(PERMISSIONS_UPDATED_BASE as u64 + perm_size),
    );
    m.insert("ServerBroadcast", pad_limit(1133));
    m.insert("ServerInfoUpdated", pad_limit(701647)); // includes ServerInfo with image + transfer_port + max_transfers_per_ip + file_reindex_interval + persistent_channels (512) + auto_join_channels (512)
    m.insert("ServerInfoUpdateResponse", pad_limit(574));
    m.insert("UserConnected", pad_limit(176515)); // includes is_away + status (128)
    m.insert("UserCreateResponse", pad_limit(614));
    m.insert("UserDeleteResponse", pad_limit(614));
    m.insert("UserDisconnected", pad_limit(97));
    m.insert(
        "UserEditResponse",
        pad_limit(USER_EDIT_RESPONSE_BASE as u64 + perm_size),
    );
    m.insert("UserBroadcastResponse", pad_limit(571));
    m.insert("UserInfoResponse", pad_limit(181634)); // includes is_away + status (128) + channels (100 * ~36)
    m.insert("UserKickResponse", pad_limit(612));
    m.insert("UserListResponse", 0); // unlimited (server-trusted)
    m.insert("UserMessage", pad_limit(1178)); // shared type: server (1178) > client (1108)
    m.insert("UserMessageResponse", pad_limit(725)); // includes is_away + status (128)
    m.insert("UserUpdated", pad_limit(176568)); // includes is_away + status (128)
    m.insert("UserAwayResponse", pad_limit(2102)); // success + error (2048) + overhead
    m.insert("UserBackResponse", pad_limit(2102)); // success + error (2048) + overhead
    m.insert("UserStatusResponse", pad_limit(2104)); // success + error (2048) + overhead
    m.insert("UserUpdateResponse", pad_limit(614));

    // Ban server messages
    m.insert("BanCreateResponse", pad_limit(2500)); // success + error (2048) + ips array + nickname (32) + overhead
    m.insert("BanDeleteResponse", pad_limit(2500)); // success + error (2048) + ips array + nickname (32) + overhead
    m.insert("BanListResponse", 0); // unlimited (server-trusted, can have many bans)

    // Trust server messages
    m.insert("TrustCreateResponse", pad_limit(2500)); // success + error (2048) + ips array + nickname (32) + overhead
    m.insert("TrustDeleteResponse", pad_limit(2500)); // success + error (2048) + ips array + nickname (32) + overhead
    m.insert("TrustListResponse", 0); // unlimited (server-trusted, can have many trusts)

    // News server messages
    m.insert("NewsListResponse", 0); // unlimited (server-trusted, can have many items)
    m.insert("NewsShowResponse", pad_limit(704500)); // single NewsItem with body + image
    m.insert("NewsCreateResponse", pad_limit(704500)); // single NewsItem with body + image
    m.insert("NewsEditResponse", pad_limit(704500)); // single NewsItem with body + image
    m.insert("NewsUpdateResponse", pad_limit(704500)); // single NewsItem with body + image
    m.insert("NewsDeleteResponse", pad_limit(100));
    m.insert("NewsUpdated", pad_limit(50)); // action enum + id

    // File server messages
    m.insert("FileListResponse", 0); // unlimited (server-trusted, can have many entries)
    // FileCreateDirResponse: path can be up to 4352 bytes (4096 path + 1 separator + 255 name)
    // JSON escaping doubles size for quote characters (worst case): 8704 bytes + ~60 overhead
    m.insert("FileCreateDirResponse", pad_limit(9000));
    m.insert("FileDeleteResponse", pad_limit(300)); // success bool + error message + overhead
    // FileInfoResponse: name (4096) + size + created + modified + is_directory + is_symlink
    // + mime_type (~128) + item_count + error (~2048) + overhead
    m.insert("FileInfoResponse", pad_limit(6500));
    m.insert("FileRenameResponse", pad_limit(300)); // success bool + error message + overhead
    m.insert("FileMoveResponse", pad_limit(350)); // success bool + error message + error_kind + overhead
    m.insert("FileCopyResponse", pad_limit(350)); // success bool + error message + error_kind + overhead
    m.insert("FileDownloadResponse", pad_limit(2186)); // success + error (2048) + error_kind (64) + overhead
    m.insert("FileUploadResponse", pad_limit(2200)); // success + error (2048) + error_kind (64) + transfer_id + overhead
    m.insert("FileSearchResponse", 0); // unlimited (server-trusted, can have up to 100 results with long paths)
    m.insert("FileReindexResponse", pad_limit(2150)); // success + error (2048) + overhead

    // Transfer messages (shared type names, used in both directions)
    m.insert("FileStart", pad_limit(4235)); // path (4096) + size (u64 max 20 digits) + sha256 (64 hex) + overhead
    m.insert("FileStartResponse", pad_limit(135)); // size (u64 max 20 digits) + sha256 (64 hex) + overhead
    m.insert("FileData", 0); // unlimited - streaming binary data
    m.insert("TransferComplete", pad_limit(2200)); // success + error (2048) + error_kind (64) + overhead
    m.insert("FileHashing", pad_limit(4200)); // file name (4096) + overhead - keepalive during hash computation

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
        ChannelJoinInfo, ChatAction, ClientMessage, ServerInfo, ServerMessage, UserInfo,
        UserInfoDetailed,
    };
    use crate::validators::{
        MAX_AVATAR_DATA_URI_LENGTH, MAX_BAN_REASON_LENGTH, MAX_CHANNEL_LENGTH,
        MAX_CHAT_TOPIC_LENGTH, MAX_ERROR_LENGTH, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT,
        MAX_FILE_PATH_LENGTH, MAX_LOCALE_LENGTH, MAX_MESSAGE_LENGTH, MAX_NICKNAME_LENGTH,
        MAX_PASSWORD_LENGTH, MAX_PERMISSION_LENGTH, MAX_PERSISTENT_CHANNELS_LENGTH,
        MAX_SEARCH_QUERY_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_IMAGE_DATA_URI_LENGTH,
        MAX_SERVER_NAME_LENGTH, MAX_STATUS_LENGTH, MAX_TRUST_REASON_LENGTH, MAX_USERNAME_LENGTH,
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
        // Note: Some type names are shared between client and server enums
        // (UserMessage, FileStart, FileStartResponse, FileData, FileHashing), so they're only counted once in the HashMap.
        const CLIENT_MESSAGE_COUNT: usize = 48; // Added 6 News + 7 File + 6 Transfer + 3 Away/Status + 3 Ban + 3 Trust + 2 FileSearch + 4 Chat channel client messages
        const SERVER_MESSAGE_COUNT: usize = 61; // Added 7 News + 8 File + 7 Transfer + 3 Away/Status + 3 Ban + 3 Trust + 2 FileSearch + 6 Chat channel server messages
        const SHARED_MESSAGE_COUNT: usize = 5; // UserMessage, FileStart, FileStartResponse, FileData, FileHashing
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

    #[test]
    fn test_shared_message_type_names_have_limits() {
        // Message type names that exist in both ClientMessage and ServerMessage.
        // These share a single limit entry in the HashMap.
        //
        // Note: "UserMessage" has the same name but different fields (not a mirror).
        // The others (FileStart, FileStartResponse, FileData) are true mirrors
        // with identical structure in both enums.
        let shared_type_names = [
            "UserMessage", // Same name, different fields (client: to/message, server: from/to/message)
            "FileStart",   // True mirror - identical fields
            "FileStartResponse", // True mirror - identical fields
            "FileData",    // True mirror - no fields (raw bytes)
        ];

        for type_name in &shared_type_names {
            assert!(
                is_known_message_type(type_name),
                "Shared type name '{}' must have a limit defined",
                type_name
            );
        }

        // Verify count matches SHARED_MESSAGE_COUNT constant
        assert_eq!(
            shared_type_names.len(),
            4,
            "Update SHARED_MESSAGE_COUNT if shared type names change"
        );
    }

    // =========================================================================
    // Client message size tests - verify limits match actual max sizes
    // =========================================================================

    #[test]
    fn test_limit_chat_send() {
        let msg = ClientMessage::ChatSend {
            message: str_of_len(MAX_MESSAGE_LENGTH),
            action: ChatAction::Normal,
            channel: str_of_len(MAX_CHANNEL_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatSend") as usize,
            "{} size {} exceeds limit {}",
            "ChatSend",
            json_size(&msg),
            max_payload_for_type("ChatSend")
        );
    }

    #[test]
    fn test_limit_chat_topic_update() {
        let msg = ClientMessage::ChatTopicUpdate {
            topic: str_of_len(MAX_CHAT_TOPIC_LENGTH),
            channel: str_of_len(MAX_CHANNEL_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatTopicUpdate") as usize,
            "{} size {} exceeds limit {}",
            "ChatTopicUpdate",
            json_size(&msg),
            max_payload_for_type("ChatTopicUpdate")
        );
    }

    #[test]
    fn test_limit_chat_join() {
        let msg = ClientMessage::ChatJoin {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatJoin") as usize,
            "{} size {} exceeds limit {}",
            "ChatJoin",
            json_size(&msg),
            max_payload_for_type("ChatJoin")
        );
    }

    #[test]
    fn test_limit_chat_leave() {
        let msg = ClientMessage::ChatLeave {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatLeave") as usize,
            "{} size {} exceeds limit {}",
            "ChatLeave",
            json_size(&msg),
            max_payload_for_type("ChatLeave")
        );
    }

    #[test]
    fn test_limit_chat_list() {
        let msg = ClientMessage::ChatList {};
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatList") as usize,
            "{} size {} exceeds limit {}",
            "ChatList",
            json_size(&msg),
            max_payload_for_type("ChatList")
        );
    }

    #[test]
    fn test_limit_chat_secret() {
        let msg = ClientMessage::ChatSecret {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
            secret: false,
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatSecret") as usize,
            "{} size {} exceeds limit {}",
            "ChatSecret",
            json_size(&msg),
            max_payload_for_type("ChatSecret")
        );
    }

    #[test]
    fn test_limit_handshake() {
        let msg = ClientMessage::Handshake {
            version: str_of_len(MAX_VERSION_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("Handshake") as usize,
            "{} size {} exceeds limit {}",
            "Handshake",
            json_size(&msg),
            max_payload_for_type("Handshake")
        );
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
        assert!(
            json_size(&msg) <= max_payload_for_type("Login") as usize,
            "{} size {} exceeds limit {}",
            "Login",
            json_size(&msg),
            max_payload_for_type("Login")
        );
    }

    #[test]
    fn test_limit_user_broadcast() {
        let msg = ClientMessage::UserBroadcast {
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserBroadcast") as usize,
            "{} size {} exceeds limit {}",
            "UserBroadcast",
            json_size(&msg),
            max_payload_for_type("UserBroadcast")
        );
    }

    #[test]
    fn test_limit_user_create() {
        let msg = ClientMessage::UserCreate {
            username: str_of_len(MAX_USERNAME_LENGTH),
            password: str_of_len(MAX_PASSWORD_LENGTH),
            is_admin: false,
            is_shared: false,
            enabled: false,
            permissions: (0..PERMISSIONS_COUNT)
                .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                .collect(),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserCreate") as usize,
            "{} size {} exceeds limit {}",
            "UserCreate",
            json_size(&msg),
            max_payload_for_type("UserCreate")
        );
    }

    #[test]
    fn test_limit_user_delete() {
        let msg = ClientMessage::UserDelete {
            username: str_of_len(MAX_USERNAME_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserDelete") as usize,
            "{} size {} exceeds limit {}",
            "UserDelete",
            json_size(&msg),
            max_payload_for_type("UserDelete")
        );
    }

    #[test]
    fn test_limit_user_edit() {
        let msg = ClientMessage::UserEdit {
            username: str_of_len(MAX_USERNAME_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserEdit") as usize,
            "{} size {} exceeds limit {}",
            "UserEdit",
            json_size(&msg),
            max_payload_for_type("UserEdit")
        );
    }

    #[test]
    fn test_limit_user_info() {
        let msg = ClientMessage::UserInfo {
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserInfo") as usize,
            "{} size {} exceeds limit {}",
            "UserInfo",
            json_size(&msg),
            max_payload_for_type("UserInfo")
        );
    }

    #[test]
    fn test_limit_user_kick() {
        let msg = ClientMessage::UserKick {
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
            reason: Some(str_of_len(MAX_BAN_REASON_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserKick") as usize,
            "{} size {} exceeds limit {}",
            "UserKick",
            json_size(&msg),
            max_payload_for_type("UserKick")
        );
    }

    #[test]
    fn test_limit_user_list() {
        // Use all: false since "false" (5 chars) is longer than "true" (4 chars)
        let msg = ClientMessage::UserList { all: false };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserList") as usize,
            "{} size {} exceeds limit {}",
            "UserList",
            json_size(&msg),
            max_payload_for_type("UserList")
        );
    }

    #[test]
    fn test_limit_user_message_client() {
        let msg = ClientMessage::UserMessage {
            to_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
            action: ChatAction::Normal,
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
            requested_is_admin: Some(false),
            requested_enabled: Some(false),
            requested_permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserUpdate") as usize,
            "{} size {} exceeds limit {}",
            "UserUpdate",
            json_size(&msg),
            max_payload_for_type("UserUpdate")
        );
    }

    #[test]
    fn test_limit_user_away() {
        let msg = ClientMessage::UserAway {
            message: Some(str_of_len(MAX_STATUS_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserAway") as usize,
            "{} size {} exceeds limit {}",
            "UserAway",
            json_size(&msg),
            max_payload_for_type("UserAway")
        );
    }

    #[test]
    fn test_limit_user_back() {
        let msg = ClientMessage::UserBack;
        assert!(
            json_size(&msg) <= max_payload_for_type("UserBack") as usize,
            "{} size {} exceeds limit {}",
            "UserBack",
            json_size(&msg),
            max_payload_for_type("UserBack")
        );
    }

    #[test]
    fn test_limit_user_status() {
        let msg = ClientMessage::UserStatus {
            status: Some(str_of_len(MAX_STATUS_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserStatus") as usize,
            "{} size {} exceeds limit {}",
            "UserStatus",
            json_size(&msg),
            max_payload_for_type("UserStatus")
        );
    }

    #[test]
    fn test_limit_ban_create() {
        // Max size: target (32 nickname) + duration (10) + reason (2048) + overhead
        let msg = ClientMessage::BanCreate {
            target: str_of_len(MAX_NICKNAME_LENGTH),
            duration: Some(str_of_len(10)),
            reason: Some(str_of_len(MAX_BAN_REASON_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("BanCreate") as usize;
        assert!(
            size <= limit,
            "BanCreate size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_ban_delete() {
        // Max size: target (32 nickname or 45 IP) + overhead
        let msg = ClientMessage::BanDelete {
            target: str_of_len(45), // IPv6 max length
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("BanDelete") as usize;
        assert!(
            size <= limit,
            "BanDelete size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_ban_list() {
        let msg = ClientMessage::BanList;
        assert!(
            json_size(&msg) <= max_payload_for_type("BanList") as usize,
            "{} size {} exceeds limit {}",
            "BanList",
            json_size(&msg),
            max_payload_for_type("BanList")
        );
    }

    #[test]
    fn test_limit_server_info_update() {
        let msg = ClientMessage::ServerInfoUpdate {
            name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
            description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
            max_connections_per_ip: Some(u32::MAX),
            max_transfers_per_ip: Some(u32::MAX),
            image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
            file_reindex_interval: Some(u32::MAX),
            persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
            auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ServerInfoUpdate") as usize,
            "{} size {} exceeds limit {}",
            "ServerInfoUpdate",
            json_size(&msg),
            max_payload_for_type("ServerInfoUpdate")
        );
    }

    #[test]
    fn test_limit_file_list() {
        let msg = ClientMessage::FileList {
            path: str_of_len(MAX_FILE_PATH_LENGTH),
            root: false,
            show_hidden: false,
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("FileList") as usize,
            "{} size {} exceeds limit {}",
            "FileList",
            json_size(&msg),
            max_payload_for_type("FileList")
        );
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
            action: ChatAction::Normal,
            channel: str_of_len(MAX_CHANNEL_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatMessage") as usize,
            "{} size {} exceeds limit {}",
            "ChatMessage",
            json_size(&msg),
            max_payload_for_type("ChatMessage")
        );
    }

    #[test]
    fn test_limit_chat_updated() {
        // Test with all fields populated (max size)
        // Use false for bool since "false" (5 chars) > "true" (4 chars)
        let msg = ServerMessage::ChatUpdated {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
            topic: Some(str_of_len(MAX_CHAT_TOPIC_LENGTH)),
            topic_set_by: Some(str_of_len(MAX_NICKNAME_LENGTH)),
            secret: Some(false),
            secret_set_by: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatUpdated") as usize;
        assert!(
            size <= limit,
            "ChatUpdated size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_join_response() {
        let msg = ServerMessage::ChatJoinResponse {
            success: false,
            error: None,
            channel: Some(str_of_len(MAX_CHANNEL_LENGTH)),
            topic: Some(str_of_len(MAX_CHAT_TOPIC_LENGTH)),
            topic_set_by: Some(str_of_len(MAX_NICKNAME_LENGTH)),
            secret: Some(false),
            members: Some(vec![str_of_len(MAX_NICKNAME_LENGTH); 50]),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatJoinResponse") as usize;
        assert!(
            size <= limit,
            "ChatJoinResponse success size {} exceeds limit {}",
            size,
            limit
        );

        // Test error case
        let error_msg = ServerMessage::ChatJoinResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            channel: None,
            topic: None,
            topic_set_by: None,
            secret: None,
            members: None,
        };
        let error_size = json_size(&error_msg);
        assert!(
            error_size <= limit,
            "ChatJoinResponse error size {} exceeds limit {}",
            error_size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_join_response_with_many_members() {
        let msg = ServerMessage::ChatJoinResponse {
            success: false,
            error: None,
            channel: Some(str_of_len(MAX_CHANNEL_LENGTH)),
            topic: Some(str_of_len(MAX_CHAT_TOPIC_LENGTH)),
            topic_set_by: Some(str_of_len(MAX_NICKNAME_LENGTH)),
            secret: Some(false),
            // Members array - estimate ~100 members at max nickname length
            members: Some(vec![str_of_len(MAX_NICKNAME_LENGTH); 100]),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatJoinResponse") as usize;
        assert!(
            size <= limit,
            "ChatJoinResponse error size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_leave_response() {
        let msg = ServerMessage::ChatLeaveResponse {
            success: false,
            error: None,
            channel: Some(str_of_len(MAX_CHANNEL_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatLeaveResponse") as usize;
        assert!(
            size <= limit,
            "ChatLeaveResponse success size {} exceeds limit {}",
            size,
            limit
        );

        // Test error case
        let error_msg = ServerMessage::ChatLeaveResponse {
            success: false,
            error: Some(str_of_len(2048)),
            channel: None,
        };
        let error_size = json_size(&error_msg);
        assert!(
            error_size <= limit,
            "ChatLeaveResponse error size {} exceeds limit {}",
            error_size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_list_response() {
        // ChatListResponse has unlimited size (0) - just verify it's set that way
        assert_eq!(max_payload_for_type("ChatListResponse"), 0);
    }

    #[test]
    fn test_limit_chat_secret_response() {
        let msg = ServerMessage::ChatSecretResponse {
            success: false,
            error: None,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatSecretResponse") as usize;
        assert!(
            size <= limit,
            "ChatSecretResponse success size {} exceeds limit {}",
            size,
            limit
        );

        // Test error case
        let error_msg = ServerMessage::ChatSecretResponse {
            success: false,
            error: Some(str_of_len(2048)),
        };
        let error_size = json_size(&error_msg);
        assert!(
            error_size <= limit,
            "ChatSecretResponse error size {} exceeds limit {}",
            error_size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_user_joined() {
        let msg = ServerMessage::ChatUserJoined {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
            is_admin: false,
            is_shared: false,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatUserJoined") as usize;
        assert!(
            size <= limit,
            "ChatUserJoined size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_user_left() {
        let msg = ServerMessage::ChatUserLeft {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatUserLeft") as usize;
        assert!(
            size <= limit,
            "ChatUserLeft size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_chat_topic_update_response() {
        let msg = ServerMessage::ChatTopicUpdateResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ChatTopicUpdateResponse") as usize,
            "{} size {} exceeds limit {}",
            "ChatTopicUpdateResponse",
            json_size(&msg),
            max_payload_for_type("ChatTopicUpdateResponse")
        );
    }

    #[test]
    fn test_limit_error() {
        let msg = ServerMessage::Error {
            message: str_of_len(2048),
            command: Some(str_of_len(64)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("Error") as usize,
            "{} size {} exceeds limit {}",
            "Error",
            json_size(&msg),
            max_payload_for_type("Error")
        );
    }

    #[test]
    fn test_limit_handshake_response() {
        let msg = ServerMessage::HandshakeResponse {
            success: false,
            version: Some(str_of_len(MAX_VERSION_LENGTH)),
            error: Some(str_of_len(256)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("HandshakeResponse") as usize,
            "{} size {} exceeds limit {}",
            "HandshakeResponse",
            json_size(&msg),
            max_payload_for_type("HandshakeResponse")
        );
    }

    #[test]
    fn test_limit_login_response() {
        // Create channel join info for auto-joined channels (~10 channels with ~50 members each)
        let channel_info = ChannelJoinInfo {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
            topic: Some(str_of_len(MAX_CHAT_TOPIC_LENGTH)),
            topic_set_by: Some(str_of_len(MAX_NICKNAME_LENGTH)),
            secret: false,
            members: (0..50).map(|_| str_of_len(MAX_NICKNAME_LENGTH)).collect(),
        };
        let channels: Vec<ChannelJoinInfo> = (0..10).map(|_| channel_info.clone()).collect();

        let msg = ServerMessage::LoginResponse {
            success: false,
            error: None,
            session_id: Some(u32::MAX),
            is_admin: Some(false),
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
                max_transfers_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
                transfer_port: u16::MAX,
                file_reindex_interval: Some(u32::MAX),
                persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
            }),
            locale: Some(str_of_len(MAX_LOCALE_LENGTH)),
            channels: Some(channels),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("LoginResponse") as usize;
        assert!(
            size <= limit,
            "LoginResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_permissions_updated() {
        let msg = ServerMessage::PermissionsUpdated {
            is_admin: false,
            permissions: (0..PERMISSIONS_COUNT)
                .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                .collect(),
            server_info: Some(ServerInfo {
                name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
                description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                max_transfers_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
                transfer_port: u16::MAX,
                file_reindex_interval: Some(u32::MAX),
                persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
            }),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("PermissionsUpdated") as usize;
        assert!(
            size <= limit,
            "PermissionsUpdated size {} exceeds limit {}",
            size,
            limit
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
                max_transfers_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
                transfer_port: u16::MAX,
                file_reindex_interval: Some(u32::MAX),
                persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
            },
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ServerInfoUpdated") as usize,
            "{} size {} exceeds limit {}",
            "ServerInfoUpdated",
            json_size(&msg),
            max_payload_for_type("ServerInfoUpdated")
        );
    }

    #[test]
    fn test_limit_server_info_update_response() {
        let msg = ServerMessage::ServerInfoUpdateResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ServerInfoUpdateResponse") as usize,
            "{} size {} exceeds limit {}",
            "ServerInfoUpdateResponse",
            json_size(&msg),
            max_payload_for_type("ServerInfoUpdateResponse")
        );
    }

    #[test]
    fn test_limit_server_broadcast() {
        let msg = ServerMessage::ServerBroadcast {
            session_id: u32::MAX,
            username: str_of_len(MAX_USERNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("ServerBroadcast") as usize,
            "{} size {} exceeds limit {}",
            "ServerBroadcast",
            json_size(&msg),
            max_payload_for_type("ServerBroadcast")
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
                is_away: false,
                status: Some(str_of_len(MAX_STATUS_LENGTH)),
            },
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserConnected") as usize,
            "{} size {} exceeds limit {}",
            "UserConnected",
            json_size(&msg),
            max_payload_for_type("UserConnected")
        );
    }

    #[test]
    fn test_limit_user_create_response() {
        let msg = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(str_of_len(512)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserCreateResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserCreateResponse",
            json_size(&msg),
            max_payload_for_type("UserCreateResponse")
        );
    }

    #[test]
    fn test_limit_user_delete_response() {
        let msg = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(str_of_len(512)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserDeleteResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserDeleteResponse",
            json_size(&msg),
            max_payload_for_type("UserDeleteResponse")
        );
    }

    #[test]
    fn test_limit_user_disconnected() {
        let msg = ServerMessage::UserDisconnected {
            session_id: u32::MAX,
            nickname: str_of_len(MAX_NICKNAME_LENGTH),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserDisconnected") as usize,
            "{} size {} exceeds limit {}",
            "UserDisconnected",
            json_size(&msg),
            max_payload_for_type("UserDisconnected")
        );
    }

    #[test]
    fn test_limit_user_edit_response() {
        let msg = ServerMessage::UserEditResponse {
            success: false,
            error: None,
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(false),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserEditResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserEditResponse",
            json_size(&msg),
            max_payload_for_type("UserEditResponse")
        );
    }

    #[test]
    fn test_limit_user_broadcast_response() {
        let msg = ServerMessage::UserBroadcastResponse {
            success: false,
            error: Some(str_of_len(512)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserBroadcastResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserBroadcastResponse",
            json_size(&msg),
            max_payload_for_type("UserBroadcastResponse")
        );
    }

    #[test]
    fn test_limit_user_info_response() {
        let msg = ServerMessage::UserInfoResponse {
            success: false,
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
                is_away: false,
                status: Some(str_of_len(MAX_STATUS_LENGTH)),
                channels: Some((0..100).map(|_| str_of_len(MAX_CHANNEL_LENGTH)).collect()),
            }),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserInfoResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserInfoResponse",
            json_size(&msg),
            max_payload_for_type("UserInfoResponse")
        );
    }

    #[test]
    fn test_limit_user_kick_response() {
        let msg = ServerMessage::UserKickResponse {
            success: false,
            error: Some(str_of_len(512)),
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserKickResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserKickResponse",
            json_size(&msg),
            max_payload_for_type("UserKickResponse")
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
            from_admin: false,
            to_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
            action: ChatAction::Normal,
        };
        // Server variant defines the limit since it's larger
        assert!(
            json_size(&msg) <= max_payload_for_type("UserMessage") as usize,
            "{} size {} exceeds limit {}",
            "UserMessage",
            json_size(&msg),
            max_payload_for_type("UserMessage")
        );
    }

    #[test]
    fn test_limit_user_message_response() {
        let msg = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(str_of_len(512)),
            is_away: Some(false),
            status: Some(str_of_len(MAX_STATUS_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserMessageResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserMessageResponse",
            json_size(&msg),
            max_payload_for_type("UserMessageResponse")
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
                is_away: false,
                status: Some(str_of_len(MAX_STATUS_LENGTH)),
            },
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserUpdated") as usize,
            "{} size {} exceeds limit {}",
            "UserUpdated",
            json_size(&msg),
            max_payload_for_type("UserUpdated")
        );
    }

    #[test]
    fn test_limit_user_update_response() {
        let msg = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(str_of_len(512)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserUpdateResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserUpdateResponse",
            json_size(&msg),
            max_payload_for_type("UserUpdateResponse")
        );
    }

    #[test]
    fn test_limit_user_away_response() {
        let msg = ServerMessage::UserAwayResponse {
            success: false,
            error: Some(str_of_len(2048)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserAwayResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserAwayResponse",
            json_size(&msg),
            max_payload_for_type("UserAwayResponse")
        );
    }

    #[test]
    fn test_limit_user_back_response() {
        let msg = ServerMessage::UserBackResponse {
            success: false,
            error: Some(str_of_len(2048)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserBackResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserBackResponse",
            json_size(&msg),
            max_payload_for_type("UserBackResponse")
        );
    }

    #[test]
    fn test_limit_user_status_response() {
        let msg = ServerMessage::UserStatusResponse {
            success: false,
            error: Some(str_of_len(2048)),
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("UserStatusResponse") as usize,
            "{} size {} exceeds limit {}",
            "UserStatusResponse",
            json_size(&msg),
            max_payload_for_type("UserStatusResponse")
        );
    }

    #[test]
    fn test_limit_ban_create_response() {
        // Max size: success + error (2048) + ips array + nickname (32) + overhead
        let msg = ServerMessage::BanCreateResponse {
            success: false,
            error: Some(str_of_len(2048)),
            ips: Some(vec![str_of_len(45)]), // One IPv6 address
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("BanCreateResponse") as usize;
        assert!(
            size <= limit,
            "BanCreateResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_ban_delete_response() {
        // Max size: success + error (2048) + ips array + nickname (32) + overhead
        let msg = ServerMessage::BanDeleteResponse {
            success: false,
            error: Some(str_of_len(2048)),
            ips: Some(vec![str_of_len(45)]), // One IPv6 address
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("BanDeleteResponse") as usize;
        assert!(
            size <= limit,
            "BanDeleteResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_ban_list_response() {
        // BanListResponse is unlimited (0) since it can have many bans
        assert_eq!(max_payload_for_type("BanListResponse"), 0);
    }

    // =========================================================================
    // Trust message size tests
    // =========================================================================

    #[test]
    fn test_limit_trust_create() {
        // Max size: target (32 nickname) + duration (10) + reason (2048) + overhead
        let msg = ClientMessage::TrustCreate {
            target: str_of_len(MAX_NICKNAME_LENGTH),
            duration: Some(str_of_len(10)),
            reason: Some(str_of_len(MAX_TRUST_REASON_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrustCreate") as usize;
        assert!(
            size <= limit,
            "TrustCreate size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_trust_delete() {
        // Max size: target (32 nickname or 45 IP) + overhead
        let msg = ClientMessage::TrustDelete {
            target: str_of_len(45), // IPv6 max length
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrustDelete") as usize;
        assert!(
            size <= limit,
            "TrustDelete size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_trust_list() {
        let msg = ClientMessage::TrustList;
        assert!(
            json_size(&msg) <= max_payload_for_type("TrustList") as usize,
            "{} size {} exceeds limit {}",
            "TrustList",
            json_size(&msg),
            max_payload_for_type("TrustList")
        );
    }

    #[test]
    fn test_limit_trust_create_response() {
        // Max size: success + error (2048) + ips array + nickname (32) + overhead
        let msg = ServerMessage::TrustCreateResponse {
            success: false,
            error: Some(str_of_len(2048)),
            ips: Some(vec![str_of_len(45)]), // One IPv6 address
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrustCreateResponse") as usize;
        assert!(
            size <= limit,
            "TrustCreateResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_trust_delete_response() {
        // Max size: success + error (2048) + ips array + nickname (32) + overhead
        let msg = ServerMessage::TrustDeleteResponse {
            success: false,
            error: Some(str_of_len(2048)),
            ips: Some(vec![str_of_len(45)]), // One IPv6 address
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrustDeleteResponse") as usize;
        assert!(
            size <= limit,
            "TrustDeleteResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_trust_list_response() {
        // TrustListResponse is unlimited (0) since it can have many trusts
        assert_eq!(max_payload_for_type("TrustListResponse"), 0);
    }

    // =========================================================================
    // File search message size tests
    // =========================================================================

    #[test]
    fn test_limit_file_search() {
        let msg = ClientMessage::FileSearch {
            query: str_of_len(MAX_SEARCH_QUERY_LENGTH),
            root: false,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("FileSearch") as usize;
        assert!(
            size <= limit,
            "FileSearch size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_file_reindex() {
        let msg = ClientMessage::FileReindex;
        let size = json_size(&msg);
        let limit = max_payload_for_type("FileReindex") as usize;
        assert!(
            size <= limit,
            "FileReindex size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_file_search_response() {
        // FileSearchResponse is unlimited (0) since it can have many results with long paths
        assert_eq!(max_payload_for_type("FileSearchResponse"), 0);
    }

    #[test]
    fn test_limit_file_reindex_response() {
        let msg = ServerMessage::FileReindexResponse {
            success: false,
            error: Some(str_of_len(2048)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("FileReindexResponse") as usize;
        assert!(
            size <= limit,
            "FileReindexResponse size {} exceeds limit {}",
            size,
            limit
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
            size: None,
            file_count: None,
            transfer_id: None,
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("FileDownloadResponse") as usize,
            "{} size {} exceeds limit {}",
            "FileDownloadResponse",
            json_size(&msg),
            max_payload_for_type("FileDownloadResponse")
        );
    }

    #[test]
    fn test_limit_file_download() {
        let msg = ClientMessage::FileDownload {
            path: str_of_len(MAX_FILE_PATH_LENGTH),
            root: false,
        };
        assert!(
            json_size(&msg) <= max_payload_for_type("FileDownload") as usize,
            "{} size {} exceeds limit {}",
            "FileDownload",
            json_size(&msg),
            max_payload_for_type("FileDownload")
        );
    }

    #[test]
    fn test_limit_file_start_response() {
        // Max size: u64 + 64 char sha256 + overhead
        let msg = ClientMessage::FileStartResponse {
            size: u64::MAX,
            sha256: Some(str_of_len(64)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("FileStartResponse") as usize;
        assert!(
            size <= limit,
            "FileStartResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_file_start() {
        let msg = ServerMessage::FileStart {
            path: str_of_len(MAX_FILE_PATH_LENGTH),
            size: u64::MAX,
            sha256: str_of_len(64),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("FileStart") as usize;
        assert!(
            size <= limit,
            "FileStart size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_transfer_complete() {
        // Error case is larger than success case
        let msg = ServerMessage::TransferComplete {
            success: false,
            error: Some(str_of_len(2048)),
            error_kind: Some(str_of_len(64)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TransferComplete") as usize;
        assert!(
            size <= limit,
            "TransferComplete size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_file_data_unlimited() {
        // FileData has unlimited payload (streaming binary)
        assert_eq!(max_payload_for_type("FileData"), 0);
    }

    #[test]
    fn test_limit_file_hashing() {
        // FileHashing keepalive message (used during large file hash computation)
        // Test with ClientMessage variant
        let client_msg = ClientMessage::FileHashing {
            file: str_of_len(4096),
        };
        let client_size = json_size(&client_msg);
        let limit = max_payload_for_type("FileHashing") as usize;
        assert!(
            client_size <= limit,
            "ClientMessage::FileHashing size {} exceeds limit {}",
            client_size,
            limit
        );

        // Test with ServerMessage variant
        let server_msg = ServerMessage::FileHashing {
            file: str_of_len(4096),
        };
        let server_size = json_size(&server_msg);
        assert!(
            server_size <= limit,
            "ServerMessage::FileHashing size {} exceeds limit {}",
            server_size,
            limit
        );
    }
}
