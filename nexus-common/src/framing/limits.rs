//! Per-type payload limits for protocol messages

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::PERMISSIONS_COUNT;
use crate::validators::{
    BLAKE3_HEX_LENGTH, MAX_AUTO_JOIN_CHANNELS_LENGTH, MAX_AVATAR_DATA_URI_LENGTH,
    MAX_BAN_REASON_LENGTH, MAX_CHANNEL_LENGTH, MAX_CHANNELS_PER_USER, MAX_CHAT_TOPIC_LENGTH,
    MAX_COMMAND_LENGTH, MAX_DIR_NAME_LENGTH, MAX_DURATION_LENGTH, MAX_ERROR_KIND_LENGTH,
    MAX_ERROR_LENGTH, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT, MAX_FILE_PATH_LENGTH,
    MAX_GROUP_NAME_LENGTH, MAX_KICK_REASON_LENGTH, MAX_LOCALE_LENGTH, MAX_LOG_LEVEL_LENGTH,
    MAX_MESSAGE_LENGTH, MAX_NEWS_ACTION_LENGTH, MAX_NEWS_BODY_LENGTH,
    MAX_NEWS_IMAGE_DATA_URI_LENGTH, MAX_NICKNAME_LENGTH, MAX_PASSWORD_LENGTH,
    MAX_PERMISSION_LENGTH, MAX_PERSISTENT_CHANNELS_LENGTH, MAX_PUBLIC_ADDRESS_LENGTH,
    MAX_SEARCH_QUERY_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_IMAGE_DATA_URI_LENGTH,
    MAX_SERVER_NAME_LENGTH, MAX_STATUS_LENGTH, MAX_TARGET_LENGTH, MAX_TRACKER_NAME_LENGTH,
    MAX_TRUST_REASON_LENGTH, MAX_USERNAME_LENGTH, MAX_VERSION_LENGTH, TRANSFER_ID_LENGTH,
};

// =============================================================================
// JSON Size Helper Constants
// =============================================================================

/// Panic message: [`max_payload_for_type`] called with a message type
/// that isn't in `MESSAGE_TYPE_LIMITS`. Programmer-error: the framing
/// layer rejects unknown types in `read_frame()` before any code can
/// reach this lookup.
const ERR_UNKNOWN_MESSAGE_TYPE: &str =
    "unknown message types should be rejected before calling max_payload_for_type";

/// Maximum JSON representation of a boolean: `false` (5 chars)
const MAX_JSON_BOOL: usize = 5;

/// Maximum JSON representation of a u8: `255` (3 digits)
const MAX_JSON_U8: usize = 3;

/// Maximum JSON representation of a u16: `65535` (5 digits)
const MAX_JSON_U16: usize = 5;

/// Maximum JSON representation of a u32: `4294967295` (10 digits)
const MAX_JSON_U32: usize = 10;

/// Maximum JSON representation of an i64: `-9223372036854775808` (20 chars)
const MAX_JSON_I64: usize = 20;

/// Maximum JSON representation of a u64: `18446744073709551615` (20 digits)
const MAX_JSON_U64: usize = 20;

// =============================================================================
// JSON Size Helper Functions
// =============================================================================

/// Size of a JSON object with just the type field: `{"type":"TypeName"}`
///
/// # Example
/// ```text
/// json_type_base("ChatSend") = 10 + 8 = 18
/// // Produces: {"type":"ChatSend"}
/// ```
const fn json_type_base(type_name: &str) -> usize {
    // {"type":"TypeName"}
    // {"type":" = 9 chars, then type_name, then "} = 2 chars
    // 9 + len + 2 = 11 + len
    11 + type_name.len()
}

/// Size of a string field: `,"key":"value"`
///
/// Includes leading comma (assumes this follows the type field or another field).
///
/// # Example
/// ```text
/// json_string_field("message", 1024) = 7 + 1024 + 6 = 1037
/// // Produces: ,"message":"...1024 chars..."
/// ```
const fn json_string_field(key: &str, max_value_len: usize) -> usize {
    // ,"key":"value"
    // ^    ^^^     ^
    // 1 + len + 3 + max + 1 = len + max + 5... wait
    // , " key " : " value "
    // 1 + 1 + len + 1 + 1 + 1 + max + 1 = len + max + 6
    key.len() + max_value_len + 6
}

/// Size of a string field whose value is bounded by **character count**
/// rather than byte count (e.g. fields validated by `validate_message`,
/// `validate_server_name`, etc., which call `.chars().count()`).
///
/// Each Unicode scalar value can be up to 4 bytes of UTF-8, so the wire
/// budget for a `max_chars`-bounded string is `4 * max_chars` bytes plus
/// the JSON envelope overhead from `json_string_field`. Using this helper
/// at field-budget sites makes the chars-vs-bytes intent explicit and
/// keeps the framing layer from rejecting valid payloads from non-ASCII
/// users (a 1024-char Japanese message is up to 4096 bytes).
const fn json_string_chars_field(key: &str, max_chars: usize) -> usize {
    json_string_field(key, max_chars * 4)
}

/// Size of a boolean field: `,"key":false`
///
/// Uses `false` (5 chars) as worst case since it's longer than `true` (4 chars).
const fn json_bool_field(key: &str) -> usize {
    // ,"key":false
    // 1 + 1 + len + 1 + 1 + 5 = len + 9
    key.len() + 4 + MAX_JSON_BOOL
}

/// Size of a u8 field: `,"key":255`
const fn json_u8_field(key: &str) -> usize {
    // ,"key":255
    // 1 + 1 + len + 1 + 1 + 3 = len + 7
    key.len() + 4 + MAX_JSON_U8
}

/// Size of a u16 field: `,"key":65535`
const fn json_u16_field(key: &str) -> usize {
    // ,"key":65535
    // 1 + 1 + len + 1 + 1 + 5 = len + 9
    key.len() + 4 + MAX_JSON_U16
}

/// Size of a u32 field: `,"key":4294967295`
const fn json_u32_field(key: &str) -> usize {
    // ,"key":4294967295
    // 1 + 1 + len + 1 + 1 + 10 = len + 14
    key.len() + 4 + MAX_JSON_U32
}

/// Size of an i64 field: `,"key":-9223372036854775808`
const fn json_i64_field(key: &str) -> usize {
    // ,"key":-9223372036854775808
    // 1 + 1 + len + 1 + 1 + 20 = len + 24
    key.len() + 4 + MAX_JSON_I64
}

/// Size of a u64 field: `,"key":18446744073709551615`
const fn json_u64_field(key: &str) -> usize {
    // ,"key":18446744073709551615
    // 1 + 1 + len + 1 + 1 + 20 = len + 24
    key.len() + 4 + MAX_JSON_U64
}

/// Size of an enum field serialized as string: `,"key":"VariantName"`
///
/// Same as `json_string_field` but named for clarity when dealing with enums.
const fn json_enum_field(key: &str, max_variant_len: usize) -> usize {
    json_string_field(key, max_variant_len)
}

/// Size of an array field with string elements: `,"key":["elem1","elem2",...]`
///
/// # Arguments
/// * `key` - The field name
/// * `count` - Maximum number of elements
/// * `max_elem_len` - Maximum length of each string element
///
/// # Formula
/// - Field header: `,"key":` = key.len() + 4
/// - Array: `[` + elements + `]`
/// - Each element: `"value"` = max_elem_len + 2
/// - Commas between elements: count - 1 (but we add 1 per element for simplicity, then subtract 1)
///
/// Total: key.len() + 4 + 1 + count * (max_elem_len + 3) + 1 - 1 = key.len() + 5 + count * (max_elem_len + 3)
const fn json_string_array_field(key: &str, count: usize, max_elem_len: usize) -> usize {
    // ,"key":["elem","elem",...]
    // Empty array case: ,"key":[] = key.len() + 6
    if count == 0 {
        return key.len() + 6;
    }
    // Non-empty: key.len() + 4 (,"key":) + 2 ([]) + count * (max_elem_len + 2) + (count - 1) commas
    // = key.len() + 6 + count * (max_elem_len + 2) + count - 1
    // = key.len() + 5 + count * (max_elem_len + 3)
    key.len() + 5 + count * (max_elem_len + 3)
}

/// Size of a JSON array-of-objects field with `count` elements, each
/// up to `per_object_size` bytes (including the surrounding `{}` and
/// inner fields).
///
/// Use this when the array elements are JSON objects (e.g. `TrackerInfo`,
/// `ServerEntry`), not plain strings — `json_string_array_field` would
/// model them as `"…"` instead of `{…}`.
///
/// # Formula
///
/// - Field header: `,"key":` = key.len() + 4
/// - Array: `[` + elements + `]` = 2
/// - Each element: `per_object_size`
/// - Commas between elements: count - 1 (folded into the per-element
///   constant: `+ 1`, then subtract 1 at the end)
///
/// Empty array case: `,"key":[]` = key.len() + 6.
/// Non-empty: key.len() + 5 + count * (per_object_size + 1).
const fn json_object_array_field(key: &str, count: usize, per_object_size: usize) -> usize {
    if count == 0 {
        return key.len() + 6;
    }
    key.len() + 5 + count * (per_object_size + 1)
}

/// Size of an object field header: `,"key":{`
///
/// Does not include the closing `}`. Use with nested object calculations.
const fn json_object_field_start(key: &str) -> usize {
    // ,"key":{
    // 1 + 1 + len + 1 + 1 + 1 = len + 5
    key.len() + 5
}

/// Size of object/array closing character
const fn json_close() -> usize {
    1 // } or ]
}

/// Size of the first field in a nested object (no leading comma): `"key":"value"`
const fn json_first_string_field(key: &str, max_value_len: usize) -> usize {
    // "key":"value"
    // 1 + len + 1 + 1 + 1 + max + 1 = len + max + 5
    key.len() + max_value_len + 5
}

/// Char-counted variant of `json_first_string_field` — first field in
/// a nested object whose value is bounded by **character count**
/// rather than byte count. See `json_string_chars_field` for the
/// rationale (4× UTF-8 ceiling).
const fn json_first_string_chars_field(key: &str, max_chars: usize) -> usize {
    json_first_string_field(key, max_chars * 4)
}

/// Size of the first i64 field in a nested object (no leading comma): `"key":-9223372036854775808`
const fn json_first_i64_field(key: &str) -> usize {
    key.len() + 3 + MAX_JSON_I64
}

// =============================================================================
// Local field size constants (not in validators - specific to protocol limits)
// =============================================================================

/// Maximum ChatAction enum variant name length
/// Variants: "Normal" (6), "Me" (2) - max is 6
const MAX_ACTION_VARIANT: usize = 6;

/// Maximum number of members in a ChatJoinResponse
/// Typical channels have <50 members; this provides headroom
const MAX_CHANNEL_MEMBERS: usize = 50;

/// Maximum number of IPs returned in ban/trust responses
/// Typically 1-5 IPs when banning by nickname
const MAX_RESPONSE_IPS: usize = 8;

/// Maximum IPv6 address length (including scope)
const MAX_IP_LENGTH: usize = 45;

/// Maximum MIME type length (e.g., "application/octet-stream")
const MAX_MIME_TYPE: usize = 128;

/// Maximum number of session IDs per user (one per connection)
const MAX_SESSION_IDS: usize = 10;

/// Maximum number of addresses in UserInfoDetailed
const MAX_ADDRESSES: usize = 10;

/// Length of a colon-separated SHA-256 fingerprint: "AA:BB:CC:...:ZZ" (32×2 hex + 31 colons)
const SHA256_FINGERPRINT_LENGTH: usize = 95;

/// Maximum path length for FileCreateDirResponse
/// Path = parent (4096) + separator (1) + name (255) = 4352
/// JSON escaping could double quote chars in worst case, so ~9000 is safe
const MAX_CREATED_DIR_PATH: usize = 4352;

// =============================================================================
// Self-documenting message size calculations using JSON helpers
// =============================================================================

/// Login: {"type":"Login","username":"...32...","password":"...256...","features":["...32..."],"locale":"...16...","avatar":"...176000...","nickname":"...32..."}
const LOGIN_SIZE: usize = json_type_base("Login")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_string_array_field("features", MAX_FEATURES_COUNT, MAX_FEATURE_LENGTH)
    + json_string_field("locale", MAX_LOCALE_LENGTH)
    + json_string_field("avatar", MAX_AVATAR_DATA_URI_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

// -----------------------------------------------------------------------------
// Client messages - Chat
// -----------------------------------------------------------------------------

/// ChatSend: {"type":"ChatSend","message":"...1024...","action":"Normal","channel":"...32..."}
/// Note: action is skipped when Normal (default), but we calculate worst case
const CHAT_SEND_SIZE: usize = json_type_base("ChatSend")
    + json_string_chars_field("message", MAX_MESSAGE_LENGTH)
    + json_enum_field("action", MAX_ACTION_VARIANT)
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH);

/// ChatTopicUpdate: {"type":"ChatTopicUpdate","topic":"...256...","channel":"...32..."}
const CHAT_TOPIC_UPDATE_SIZE: usize = json_type_base("ChatTopicUpdate")
    + json_string_chars_field("topic", MAX_CHAT_TOPIC_LENGTH)
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH);

/// ChatJoin: {"type":"ChatJoin","channel":"...32..."}
const CHAT_JOIN_SIZE: usize =
    json_type_base("ChatJoin") + json_string_chars_field("channel", MAX_CHANNEL_LENGTH);

/// ChatLeave: {"type":"ChatLeave","channel":"...32..."}
const CHAT_LEAVE_SIZE: usize =
    json_type_base("ChatLeave") + json_string_chars_field("channel", MAX_CHANNEL_LENGTH);

/// ChatList: {"type":"ChatList"}
const CHAT_LIST_SIZE: usize = json_type_base("ChatList");

/// ChatSecret: {"type":"ChatSecret","channel":"...32...","secret":false}
const CHAT_SECRET_SIZE: usize = json_type_base("ChatSecret")
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_bool_field("secret");

// -----------------------------------------------------------------------------
// Client messages - Basic
// -----------------------------------------------------------------------------

/// Handshake: {"type":"Handshake","version":"...32..."}
const HANDSHAKE_SIZE: usize =
    json_type_base("Handshake") + json_string_field("version", MAX_VERSION_LENGTH);

/// UserBroadcast: {"type":"UserBroadcast","message":"...1024..."}
const USER_BROADCAST_SIZE: usize =
    json_type_base("UserBroadcast") + json_string_chars_field("message", MAX_MESSAGE_LENGTH);

/// UserDelete: {"type":"UserDelete","id":i64}
const USER_DELETE_SIZE: usize = json_type_base("UserDelete") + json_i64_field("id");

/// UserEdit: {"type":"UserEdit","id":i64}
const USER_EDIT_SIZE: usize = json_type_base("UserEdit") + json_i64_field("id");

/// UserInfo: {"type":"UserInfo","nickname":"...64..."}
const USER_INFO_SIZE: usize =
    json_type_base("UserInfo") + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// UserKick: {"type":"UserKick","nickname":"...32...","reason":"...256..."}
const USER_KICK_SIZE: usize = json_type_base("UserKick")
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_string_chars_field("reason", MAX_KICK_REASON_LENGTH);

/// UserList: {"type":"UserList","all":false}
const USER_LIST_SIZE: usize = json_type_base("UserList") + json_bool_field("all");

/// UserAway: {"type":"UserAway","message":"...128..."}
const USER_AWAY_SIZE: usize =
    json_type_base("UserAway") + json_string_chars_field("message", MAX_STATUS_LENGTH);

/// UserBack: {"type":"UserBack"}
const USER_BACK_SIZE: usize = json_type_base("UserBack");

/// UserStatus: {"type":"UserStatus","status":"...128..."}
const USER_STATUS_SIZE: usize =
    json_type_base("UserStatus") + json_string_chars_field("status", MAX_STATUS_LENGTH);

// -----------------------------------------------------------------------------
// Client messages - Ban/Trust
// -----------------------------------------------------------------------------

/// BanCreate: {"type":"BanCreate","target":"...64...","duration":"...10...","reason":"...256..."}
const BAN_CREATE_SIZE: usize = json_type_base("BanCreate")
    + json_string_chars_field("target", MAX_TARGET_LENGTH)
    + json_string_field("duration", MAX_DURATION_LENGTH)
    + json_string_chars_field("reason", MAX_BAN_REASON_LENGTH);

/// TrustCreate: {"type":"TrustCreate","target":"...64...","duration":"...10...","reason":"...256..."}
const TRUST_CREATE_SIZE: usize = json_type_base("TrustCreate")
    + json_string_chars_field("target", MAX_TARGET_LENGTH)
    + json_string_field("duration", MAX_DURATION_LENGTH)
    + json_string_chars_field("reason", MAX_TRUST_REASON_LENGTH);

/// BanDelete: {"type":"BanDelete","target":"...64..."}
const BAN_DELETE_SIZE: usize =
    json_type_base("BanDelete") + json_string_chars_field("target", MAX_TARGET_LENGTH);

/// BanList: {"type":"BanList"}
const BAN_LIST_SIZE: usize = json_type_base("BanList");

/// TrustDelete: {"type":"TrustDelete","target":"...64..."}
const TRUST_DELETE_SIZE: usize =
    json_type_base("TrustDelete") + json_string_chars_field("target", MAX_TARGET_LENGTH);

/// TrustList: {"type":"TrustList"}
const TRUST_LIST_SIZE: usize = json_type_base("TrustList");

/// ConnectionMonitor: {"type":"ConnectionMonitor"}
const CONNECTION_MONITOR_SIZE: usize = json_type_base("ConnectionMonitor");

// -----------------------------------------------------------------------------
// Client messages - News
// -----------------------------------------------------------------------------

/// NewsList: {"type":"NewsList"}
const NEWS_LIST_SIZE: usize = json_type_base("NewsList");

/// NewsShow: {"type":"NewsShow","id":-9223372036854775808}
const NEWS_SHOW_SIZE: usize = json_type_base("NewsShow") + json_i64_field("id");

/// NewsCreate: {"type":"NewsCreate","body":"...4096...","image":"...700000..."}
const NEWS_CREATE_SIZE: usize = json_type_base("NewsCreate")
    + json_string_chars_field("body", MAX_NEWS_BODY_LENGTH)
    + json_string_field("image", MAX_NEWS_IMAGE_DATA_URI_LENGTH);

/// NewsEdit: {"type":"NewsEdit","id":-9223372036854775808}
const NEWS_EDIT_SIZE: usize = json_type_base("NewsEdit") + json_i64_field("id");

/// NewsUpdate: {"type":"NewsUpdate","id":-9223372036854775808,"body":"...4096...","image":"...700000..."}
const NEWS_UPDATE_SIZE: usize = json_type_base("NewsUpdate")
    + json_i64_field("id")
    + json_string_chars_field("body", MAX_NEWS_BODY_LENGTH)
    + json_string_field("image", MAX_NEWS_IMAGE_DATA_URI_LENGTH);

/// NewsDelete: {"type":"NewsDelete","id":-9223372036854775808}
const NEWS_DELETE_SIZE: usize = json_type_base("NewsDelete") + json_i64_field("id");

// -----------------------------------------------------------------------------
// Client messages - Files
// -----------------------------------------------------------------------------

/// FileList: {"type":"FileList","path":"...4096...","root":false,"show_hidden":false}
const FILE_LIST_SIZE: usize = json_type_base("FileList")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_bool_field("root")
    + json_bool_field("show_hidden");

/// FileCreateDir: {"type":"FileCreateDir","path":"...4096...","name":"...255...","root":false}
const FILE_CREATE_DIR_SIZE: usize = json_type_base("FileCreateDir")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_string_field("name", MAX_DIR_NAME_LENGTH)
    + json_bool_field("root");

/// FileDelete: {"type":"FileDelete","path":"...4096...","root":false}
const FILE_DELETE_SIZE: usize = json_type_base("FileDelete")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_bool_field("root");

/// FileInfo: {"type":"FileInfo","path":"...4096...","root":false}
const FILE_INFO_SIZE: usize = json_type_base("FileInfo")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_bool_field("root");

/// FileRename: {"type":"FileRename","path":"...4096...","new_name":"...255...","root":false}
const FILE_RENAME_SIZE: usize = json_type_base("FileRename")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_string_field("new_name", MAX_DIR_NAME_LENGTH)
    + json_bool_field("root");

/// FileMove: {"type":"FileMove","source_path":"...4096...","destination_dir":"...4096...","overwrite":false,"source_root":false,"destination_root":false}
const FILE_MOVE_SIZE: usize = json_type_base("FileMove")
    + json_string_field("source_path", MAX_FILE_PATH_LENGTH)
    + json_string_field("destination_dir", MAX_FILE_PATH_LENGTH)
    + json_bool_field("overwrite")
    + json_bool_field("source_root")
    + json_bool_field("destination_root");

/// FileCopy: {"type":"FileCopy","source_path":"...4096...","destination_dir":"...4096...","overwrite":false,"source_root":false,"destination_root":false}
const FILE_COPY_SIZE: usize = json_type_base("FileCopy")
    + json_string_field("source_path", MAX_FILE_PATH_LENGTH)
    + json_string_field("destination_dir", MAX_FILE_PATH_LENGTH)
    + json_bool_field("overwrite")
    + json_bool_field("source_root")
    + json_bool_field("destination_root");

/// FileDownload: {"type":"FileDownload","path":"...4096...","root":false}
const FILE_DOWNLOAD_SIZE: usize = json_type_base("FileDownload")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_bool_field("root");

/// FileUpload: {"type":"FileUpload","destination":"...4096...","file_count":18446744073709551615,"total_size":18446744073709551615,"root":false}
const FILE_UPLOAD_SIZE: usize = json_type_base("FileUpload")
    + json_string_field("destination", MAX_FILE_PATH_LENGTH)
    + json_u64_field("file_count")
    + json_u64_field("total_size")
    + json_bool_field("root");

/// FileSearch: {"type":"FileSearch","query":"...256...","root":false}
const FILE_SEARCH_SIZE: usize = json_type_base("FileSearch")
    + json_string_field("query", MAX_SEARCH_QUERY_LENGTH)
    + json_bool_field("root");

/// FileReindex: {"type":"FileReindex"}
const FILE_REINDEX_SIZE: usize = json_type_base("FileReindex");

// -----------------------------------------------------------------------------
// Voice client messages
// -----------------------------------------------------------------------------

/// VoiceJoin: {"type":"VoiceJoin","target":"...32..."}
/// Target is either "#channel" (max 32) or "nickname" (max 32)
const VOICE_JOIN_SIZE: usize =
    json_type_base("VoiceJoin") + json_string_chars_field("target", MAX_CHANNEL_LENGTH);

/// VoiceLeave: {"type":"VoiceLeave"}
const VOICE_LEAVE_SIZE: usize = json_type_base("VoiceLeave");

/// Ping: {"type":"Ping"}
const PING_SIZE: usize = json_type_base("Ping");

/// Pong: {"type":"Pong"}
const PONG_SIZE: usize = json_type_base("Pong");

// -----------------------------------------------------------------------------
// Transfer messages (shared between client and server)
// -----------------------------------------------------------------------------

/// FileStart: {"type":"FileStart","path":"...4096...","size":18446744073709551615}
const FILE_START_SIZE: usize = json_type_base("FileStart")
    + json_string_field("path", MAX_FILE_PATH_LENGTH)
    + json_u64_field("size");

/// FileStartResponse: {"type":"FileStartResponse","size":18446744073709551615,"blake3":"...64..."}
const FILE_START_RESPONSE_SIZE: usize = json_type_base("FileStartResponse")
    + json_u64_field("size")
    + json_string_field("blake3", BLAKE3_HEX_LENGTH);

/// FileHashing: {"type":"FileHashing","file":"...4096..."}
const FILE_HASHING_SIZE: usize =
    json_type_base("FileHashing") + json_string_field("file", MAX_FILE_PATH_LENGTH);

/// FileHash: {"type":"FileHash","blake3":"...64..."}
const FILE_HASH_SIZE: usize =
    json_type_base("FileHash") + json_string_field("blake3", BLAKE3_HEX_LENGTH);

/// TransferComplete: {"type":"TransferComplete","success":false,"error":"...2048...","error_kind":"...16..."}
const TRANSFER_COMPLETE_SIZE: usize = json_type_base("TransferComplete")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_field("error_kind", MAX_ERROR_KIND_LENGTH);

// -----------------------------------------------------------------------------
// Tracker messages (separate protocol)
// -----------------------------------------------------------------------------

/// TrackerServerRegister: full registration payload sent by servers to register or
/// refresh their entry in the tracker's listing.
const TRACKER_SERVER_REGISTER_SIZE: usize = json_type_base("TrackerServerRegister")
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_string_field("locale", MAX_LOCALE_LENGTH)
    + json_string_chars_field("name", MAX_SERVER_NAME_LENGTH)
    + json_string_chars_field("description", MAX_SERVER_DESCRIPTION_LENGTH)
    + json_string_field("address", MAX_PUBLIC_ADDRESS_LENGTH)
    + json_u16_field("port")
    + json_u16_field("websocket_port")
    + json_string_field("version", MAX_VERSION_LENGTH)
    + json_string_field("fingerprint", SHA256_FINGERPRINT_LENGTH)
    + json_u32_field("user_count")
    + json_bool_field("allows_guest");

/// TrackerServerList: client request for the current server listing.
const TRACKER_SERVER_LIST_SIZE: usize = json_type_base("TrackerServerList")
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_string_field("locale", MAX_LOCALE_LENGTH)
    + json_string_field("version", MAX_VERSION_LENGTH);

/// TrackerServerRegisterResponse: tracker's reply to TrackerServerRegister.
const TRACKER_SERVER_REGISTER_RESPONSE_SIZE: usize =
    json_type_base("TrackerServerRegisterResponse")
        + json_bool_field("success")
        + json_u32_field("refresh_interval")
        + json_string_field("error", MAX_ERROR_LENGTH)
        + json_string_field("error_kind", MAX_ERROR_KIND_LENGTH);

/// `TrackerServerListResponse` payload ceiling. The tracker is a
/// user-trusted originator (TLS+TOFU pinned), so the cap is purely a
/// defense-in-depth against a hostile or compromised tracker shipping
/// arbitrary-sized payloads to OOM the client. Per-entry worst case is
/// ~2KB at the field caps, so 16MB ≈ 8,000 entries — well above any
/// realistic tracker. Clients filter / sort large lists in the panel,
/// so the cap is the OOM ceiling, not a UX limit.
const TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE: usize = 16 * 1024 * 1024;

// -----------------------------------------------------------------------------
// Server messages - Voice
// -----------------------------------------------------------------------------

/// Maximum participants in a voice session (for sizing VoiceJoinResponse)
const MAX_VOICE_PARTICIPANTS: usize = 100;

/// UUID string length when serialized (e.g., "550e8400-e29b-41d4-a716-446655440000")
const UUID_STRING_LENGTH: usize = 36;

/// VoiceJoinResponse: {"type":"VoiceJoinResponse","success":false,"token":"...36...","target":"...32...","participants":["...64...",...100...],"error":"...2048..."}
const VOICE_JOIN_RESPONSE_SIZE: usize = json_type_base("VoiceJoinResponse")
    + json_bool_field("success")
    + json_string_field("token", UUID_STRING_LENGTH)
    + json_string_chars_field("target", MAX_CHANNEL_LENGTH)
    + json_string_array_field("participants", MAX_VOICE_PARTICIPANTS, MAX_NICKNAME_LENGTH)
    + json_string_field("error", MAX_ERROR_LENGTH);

/// VoiceLeaveResponse: {"type":"VoiceLeaveResponse","success":false,"error":"...2048..."}
const VOICE_LEAVE_RESPONSE_SIZE: usize = json_type_base("VoiceLeaveResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// VoiceUserJoined: {"type":"VoiceUserJoined","nickname":"...32...","target":"...32..."}
const VOICE_USER_JOINED_SIZE: usize = json_type_base("VoiceUserJoined")
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_string_chars_field("target", MAX_CHANNEL_LENGTH);

/// VoiceUserLeft: {"type":"VoiceUserLeft","nickname":"...32...","target":"...32..."}
const VOICE_USER_LEFT_SIZE: usize = json_type_base("VoiceUserLeft")
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_string_chars_field("target", MAX_CHANNEL_LENGTH);

// -----------------------------------------------------------------------------
// Server messages - Chat
// -----------------------------------------------------------------------------

/// ChatMessage: {"type":"ChatMessage","session_id":4294967295,"nickname":"...64...","is_admin":false,"is_shared":false,"message":"...1024...","action":"Normal","channel":"...32...","timestamp":-9223372036854775808}
const CHAT_MESSAGE_SIZE: usize = json_type_base("ChatMessage")
    + json_u32_field("session_id")
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_bool_field("is_admin")
    + json_bool_field("is_shared")
    + json_string_chars_field("message", MAX_MESSAGE_LENGTH)
    + json_enum_field("action", MAX_ACTION_VARIANT)
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_i64_field("timestamp");

/// ChatUpdated: {"type":"ChatUpdated","channel":"...32...","topic":"...256...","topic_set_by":"...64...","secret":false,"secret_set_by":"...64..."}
const CHAT_UPDATED_SIZE: usize = json_type_base("ChatUpdated")
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_chars_field("topic", MAX_CHAT_TOPIC_LENGTH)
    + json_string_chars_field("topic_set_by", MAX_NICKNAME_LENGTH)
    + json_bool_field("secret")
    + json_string_chars_field("secret_set_by", MAX_NICKNAME_LENGTH);

/// ChatUserJoined: {"type":"ChatUserJoined","channel":"...32...","nickname":"...64...","is_admin":false,"is_shared":false}
const CHAT_USER_JOINED_SIZE: usize = json_type_base("ChatUserJoined")
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_bool_field("is_admin")
    + json_bool_field("is_shared");

/// ChatUserLeft: {"type":"ChatUserLeft","channel":"...32...","nickname":"...64..."}
const CHAT_USER_LEFT_SIZE: usize = json_type_base("ChatUserLeft")
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// ChatUserRenamed: {"type":"ChatUserRenamed","channel":"...32...","old_nickname":"...64...","new_nickname":"...64...","is_admin":false}
const CHAT_USER_RENAMED_SIZE: usize = json_type_base("ChatUserRenamed")
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_chars_field("old_nickname", MAX_NICKNAME_LENGTH)
    + json_string_chars_field("new_nickname", MAX_NICKNAME_LENGTH)
    + json_bool_field("is_admin");

/// ChatSecretResponse: {"type":"ChatSecretResponse","success":false,"error":"...2048..."}
const CHAT_SECRET_RESPONSE_SIZE: usize = json_type_base("ChatSecretResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// ChatLeaveResponse: {"type":"ChatLeaveResponse","success":false,"channel":"...32...","error":"...2048..."}
const CHAT_LEAVE_RESPONSE_SIZE: usize = json_type_base("ChatLeaveResponse")
    + json_bool_field("success")
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_field("error", MAX_ERROR_LENGTH);

/// ChatJoinResponse: {"type":"ChatJoinResponse","success":false,"error":"...2048...","channel":"...32...","topic":"...256...","topic_set_by":"...32...","secret":false,"members":["...32..."],"voiced":["...32..."]}
const CHAT_JOIN_RESPONSE_SIZE: usize = json_type_base("ChatJoinResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_chars_field("topic", MAX_CHAT_TOPIC_LENGTH)
    + json_string_chars_field("topic_set_by", MAX_NICKNAME_LENGTH)
    + json_bool_field("secret")
    + json_string_array_field("members", MAX_CHANNEL_MEMBERS, MAX_NICKNAME_LENGTH)
    + json_string_array_field("voiced", MAX_CHANNEL_MEMBERS, MAX_NICKNAME_LENGTH);

// -----------------------------------------------------------------------------
// Server messages - Simple responses (success + error pattern)
// -----------------------------------------------------------------------------

/// ChatTopicUpdateResponse: {"type":"ChatTopicUpdateResponse","success":false,"error":"...2048..."}
const CHAT_TOPIC_UPDATE_RESPONSE_SIZE: usize = json_type_base("ChatTopicUpdateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// HandshakeResponse: {"type":"HandshakeResponse","success":false,"version":"...32...","fingerprint":"...95...","error":"...2048..."}
const HANDSHAKE_RESPONSE_SIZE: usize = json_type_base("HandshakeResponse")
    + json_bool_field("success")
    + json_string_field("version", MAX_VERSION_LENGTH)
    + json_string_field("fingerprint", SHA256_FINGERPRINT_LENGTH)
    + json_string_field("error", MAX_ERROR_LENGTH);

/// ServerInfoUpdateResponse: {"type":"ServerInfoUpdateResponse","success":false,"error":"...2048..."}
const SERVER_INFO_UPDATE_RESPONSE_SIZE: usize = json_type_base("ServerInfoUpdateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// UserBroadcastResponse: {"type":"UserBroadcastResponse","success":false,"error":"...2048..."}
const USER_BROADCAST_RESPONSE_SIZE: usize = json_type_base("UserBroadcastResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// UserCreateResponse: {"type":"UserCreateResponse","success":false,"error":"...2048...","id":i64,"username":"...32..."}
const USER_CREATE_RESPONSE_SIZE: usize = json_type_base("UserCreateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH);

/// UserDeleteResponse: {"type":"UserDeleteResponse","success":false,"error":"...2048...","username":"...32..."}
const USER_DELETE_RESPONSE_SIZE: usize = json_type_base("UserDeleteResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_chars_field("username", MAX_USERNAME_LENGTH);

/// UserUpdateResponse: {"type":"UserUpdateResponse","success":false,"error":"...2048...","id":i64,"username":"...32..."}
const USER_UPDATE_RESPONSE_SIZE: usize = json_type_base("UserUpdateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH);

/// UserKickResponse: {"type":"UserKickResponse","success":false,"error":"...2048...","nickname":"...32..."}
const USER_KICK_RESPONSE_SIZE: usize = json_type_base("UserKickResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// BanCreateResponse: {"type":"BanCreateResponse","success":false,"error":"...2048...","ips":["...45..."],"nickname":"...32..."}
const BAN_CREATE_RESPONSE_SIZE: usize = json_type_base("BanCreateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_array_field("ips", MAX_RESPONSE_IPS, MAX_IP_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// BanDeleteResponse: {"type":"BanDeleteResponse","success":false,"error":"...2048...","ips":["...45..."],"nickname":"...32..."}
const BAN_DELETE_RESPONSE_SIZE: usize = json_type_base("BanDeleteResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_array_field("ips", MAX_RESPONSE_IPS, MAX_IP_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// TrustCreateResponse: {"type":"TrustCreateResponse","success":false,"error":"...2048...","ips":["...45..."],"nickname":"...32..."}
const TRUST_CREATE_RESPONSE_SIZE: usize = json_type_base("TrustCreateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_array_field("ips", MAX_RESPONSE_IPS, MAX_IP_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// TrustDeleteResponse: {"type":"TrustDeleteResponse","success":false,"error":"...2048...","ips":["...45..."],"nickname":"...32..."}
const TRUST_DELETE_RESPONSE_SIZE: usize = json_type_base("TrustDeleteResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_array_field("ips", MAX_RESPONSE_IPS, MAX_IP_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// UserMessageResponse: {"type":"UserMessageResponse","success":false,"error":"...2048...","is_away":false,"status":"...128..."}
const USER_MESSAGE_RESPONSE_SIZE: usize = json_type_base("UserMessageResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_bool_field("is_away")
    + json_string_chars_field("status", MAX_STATUS_LENGTH);

/// Error: {"type":"Error","message":"...2048...","command":"...32...","disconnect":false}
const ERROR_SIZE: usize = json_type_base("Error")
    + json_string_field("message", MAX_ERROR_LENGTH)
    + json_string_field("command", MAX_COMMAND_LENGTH)
    + json_bool_field("disconnect");

/// ServerBroadcast: {"type":"ServerBroadcast","session_id":4294967295,"username":"...32...","message":"...1024..."}
const SERVER_BROADCAST_SIZE: usize = json_type_base("ServerBroadcast")
    + json_u32_field("session_id")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_string_chars_field("message", MAX_MESSAGE_LENGTH);

/// UserDisconnected: {"type":"UserDisconnected","session_id":4294967295,"nickname":"...64..."}
const USER_DISCONNECTED_SIZE: usize = json_type_base("UserDisconnected")
    + json_u32_field("session_id")
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH);

/// UserMessage (server): {"type":"UserMessage","from_nickname":"...64...","from_admin":false,"from_shared":false,"to_nickname":"...64...","message":"...1024...","action":"Normal","timestamp":-9223372036854775808}
const USER_MESSAGE_SIZE: usize = json_type_base("UserMessage")
    + json_string_chars_field("from_nickname", MAX_NICKNAME_LENGTH)
    + json_bool_field("from_admin")
    + json_bool_field("from_shared")
    + json_string_chars_field("to_nickname", MAX_NICKNAME_LENGTH)
    + json_string_chars_field("message", MAX_MESSAGE_LENGTH)
    + json_enum_field("action", MAX_ACTION_VARIANT)
    + json_i64_field("timestamp");

/// UserAwayResponse: {"type":"UserAwayResponse","success":false,"error":"...2048..."}
const USER_AWAY_RESPONSE_SIZE: usize = json_type_base("UserAwayResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// UserBackResponse: {"type":"UserBackResponse","success":false,"error":"...2048..."}
const USER_BACK_RESPONSE_SIZE: usize = json_type_base("UserBackResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// UserStatusResponse: {"type":"UserStatusResponse","success":false,"error":"...2048..."}
const USER_STATUS_RESPONSE_SIZE: usize = json_type_base("UserStatusResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// NewsDeleteResponse: {"type":"NewsDeleteResponse","success":false,"id":-9223372036854775808,"error":"...2048..."}
const NEWS_DELETE_RESPONSE_SIZE: usize = json_type_base("NewsDeleteResponse")
    + json_bool_field("success")
    + json_i64_field("id")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// NewsUpdated: {"type":"NewsUpdated","action":"Created","id":-9223372036854775808}
const NEWS_UPDATED_SIZE: usize = json_type_base("NewsUpdated")
    + json_enum_field("action", MAX_NEWS_ACTION_LENGTH)
    + json_i64_field("id");

/// FileDeleteResponse: {"type":"FileDeleteResponse","success":false,"error":"...2048..."}
const FILE_DELETE_RESPONSE_SIZE: usize = json_type_base("FileDeleteResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// FileRenameResponse: {"type":"FileRenameResponse","success":false,"error":"...2048..."}
const FILE_RENAME_RESPONSE_SIZE: usize = json_type_base("FileRenameResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// FileMoveResponse: {"type":"FileMoveResponse","success":false,"error":"...2048...","error_kind":"...16..."}
const FILE_MOVE_RESPONSE_SIZE: usize = json_type_base("FileMoveResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_field("error_kind", MAX_ERROR_KIND_LENGTH);

/// FileCopyResponse: {"type":"FileCopyResponse","success":false,"error":"...2048...","error_kind":"...16..."}
const FILE_COPY_RESPONSE_SIZE: usize = json_type_base("FileCopyResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_field("error_kind", MAX_ERROR_KIND_LENGTH);

/// FileDownloadResponse: {"type":"FileDownloadResponse","success":false,"error":"...2048...","error_kind":"...16...","size":18446744073709551615,"file_count":18446744073709551615,"transfer_id":"...8..."}
const FILE_DOWNLOAD_RESPONSE_SIZE: usize = json_type_base("FileDownloadResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_field("error_kind", MAX_ERROR_KIND_LENGTH)
    + json_u64_field("size")
    + json_u64_field("file_count")
    + json_string_field("transfer_id", TRANSFER_ID_LENGTH);

/// FileUploadResponse: {"type":"FileUploadResponse","success":false,"error":"...2048...","error_kind":"...16...","transfer_id":"...8..."}
const FILE_UPLOAD_RESPONSE_SIZE: usize = json_type_base("FileUploadResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_field("error_kind", MAX_ERROR_KIND_LENGTH)
    + json_string_field("transfer_id", TRANSFER_ID_LENGTH);

/// FileReindexResponse: {"type":"FileReindexResponse","success":false,"error":"...2048..."}
const FILE_REINDEX_RESPONSE_SIZE: usize = json_type_base("FileReindexResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH);

/// FileCreateDirResponse: {"type":"FileCreateDirResponse","success":false,"error":"...2048...","path":"...4352..."}
const FILE_CREATE_DIR_RESPONSE_SIZE: usize = json_type_base("FileCreateDirResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_field("path", MAX_CREATED_DIR_PATH);

/// UserInfo struct size (nested object in responses):
/// {"id":i64,"username":"...32...","nickname":"...32...","login_time":i64,"is_admin":false,"is_shared":false,"session_ids":[u32,...],"locale":"...16...","avatar":"...176000...","is_away":false,"status":"...128...","group_id":i64,"group_name":"...32..."}
const USER_INFO_STRUCT_SIZE: usize = json_first_i64_field("id")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_i64_field("login_time")
    + json_bool_field("is_admin")
    + json_bool_field("is_shared")
    + json_string_array_field("session_ids", MAX_SESSION_IDS, 10) // u32 as string ~10 digits
    + json_string_field("locale", MAX_LOCALE_LENGTH)
    + json_string_field("avatar", MAX_AVATAR_DATA_URI_LENGTH)
    + json_bool_field("is_away")
    + json_string_chars_field("status", MAX_STATUS_LENGTH)
    + json_i64_field("group_id")
    + json_string_chars_field("group_name", MAX_GROUP_NAME_LENGTH)
    + json_u16_field("bandwidth_weight")
    + 2; // {} braces

/// UserConnected: {"type":"UserConnected","user":{...}}
const USER_CONNECTED_SIZE: usize = json_type_base("UserConnected")
    + json_object_field_start("user")
    + USER_INFO_STRUCT_SIZE
    + json_close();

/// UserUpdated: {"type":"UserUpdated","previous_username":"...32...","user":{...}}
const USER_UPDATED_SIZE: usize = json_type_base("UserUpdated")
    + json_string_chars_field("previous_username", MAX_USERNAME_LENGTH)
    + json_object_field_start("user")
    + USER_INFO_STRUCT_SIZE
    + json_close();

/// UserInfoDetailed struct size (nested object in UserInfoResponse):
/// Has more fields than UserInfo: features, created_at, addresses, channels
const USER_INFO_DETAILED_SIZE: usize = json_first_i64_field("id")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_i64_field("login_time")
    + json_bool_field("is_shared")
    + json_string_array_field("session_ids", MAX_SESSION_IDS, 10)
    + json_string_array_field("features", MAX_FEATURES_COUNT, MAX_FEATURE_LENGTH)
    + json_i64_field("created_at")
    + json_string_field("locale", MAX_LOCALE_LENGTH)
    + json_string_field("avatar", MAX_AVATAR_DATA_URI_LENGTH)
    + json_bool_field("is_admin")
    + json_string_array_field("addresses", MAX_ADDRESSES, MAX_IP_LENGTH)
    + json_bool_field("is_away")
    + json_string_chars_field("status", MAX_STATUS_LENGTH)
    + json_string_array_field("channels", MAX_CHANNELS_PER_USER, MAX_CHANNEL_LENGTH)
    + json_i64_field("group_id")
    + json_string_chars_field("group_name", MAX_GROUP_NAME_LENGTH)
    + json_u16_field("bandwidth_weight")
    + 2; // {} braces

/// UserInfoResponse: {"type":"UserInfoResponse","success":false,"error":"...2048...","user":{...}}
const USER_INFO_RESPONSE_SIZE: usize = json_type_base("UserInfoResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_field_start("user")
    + USER_INFO_DETAILED_SIZE
    + json_close();

/// Maximum number of groups (reasonable limit for group list responses)
const MAX_GROUPS: usize = 50;

/// GroupInfo struct size (nested object in responses):
/// {"id":i64,"name":"...32...","is_shared":false,"member_count":u32,"permissions":["...32...",...],"bandwidth_weight":u16}
const GROUP_INFO_STRUCT_SIZE: usize = json_first_i64_field("id")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH)
    + json_bool_field("is_shared")
    + json_u32_field("member_count")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_u16_field("bandwidth_weight")
    + 2; // {} braces

/// ServerInfo struct size (nested object in responses):
/// {"name":"...64...","description":"...256...","public_address":"...253...","version":"...32...","max_connections_per_ip":u32,"max_transfers_per_ip":u32,"image":"...700000...","transfer_port":u16,"transfer_websocket_port":u16,"file_reindex_interval":u32,"persistent_channels":"...512...","auto_join_channels":"...512...","chat_burst_limit":u32,"chat_rate_limit":u32,"min_password_strength":u8,"log_level":"...5..."}
const SERVER_INFO_STRUCT_SIZE: usize =
    json_first_string_chars_field("name", MAX_SERVER_NAME_LENGTH)
        + json_string_chars_field("description", MAX_SERVER_DESCRIPTION_LENGTH)
        + json_string_field("public_address", MAX_PUBLIC_ADDRESS_LENGTH)
        + json_string_field("version", MAX_VERSION_LENGTH)
        + json_u32_field("max_connections_per_ip")
        + json_u32_field("max_transfers_per_ip")
        + json_string_field("image", MAX_SERVER_IMAGE_DATA_URI_LENGTH)
        + json_u16_field("transfer_port")
        + json_u16_field("transfer_websocket_port")
        + json_u32_field("file_reindex_interval")
        + json_string_field("persistent_channels", MAX_PERSISTENT_CHANNELS_LENGTH)
        + json_string_field("auto_join_channels", MAX_AUTO_JOIN_CHANNELS_LENGTH)
        + json_u32_field("chat_burst_limit")
        + json_u32_field("chat_rate_limit")
        + json_u8_field("min_password_strength")
        + json_string_field("log_level", MAX_LOG_LEVEL_LENGTH)
        + json_u64_field("max_outbound_rate")
        + json_u32_field("scheduler_chunk_size")
        + 2; // {} braces

/// ServerInfoUpdate: {"type":"ServerInfoUpdate","name":"...64...","description":"...256...","public_address":"...253...","max_connections_per_ip":u32,"max_transfers_per_ip":u32,"image":"...700000...","file_reindex_interval":u32,"persistent_channels":"...512...","auto_join_channels":"...512...","chat_burst_limit":u32,"chat_rate_limit":u32,"min_password_strength":u8}
const SERVER_INFO_UPDATE_SIZE: usize = json_type_base("ServerInfoUpdate")
    + json_string_chars_field("name", MAX_SERVER_NAME_LENGTH)
    + json_string_chars_field("description", MAX_SERVER_DESCRIPTION_LENGTH)
    + json_string_field("public_address", MAX_PUBLIC_ADDRESS_LENGTH)
    + json_u32_field("max_connections_per_ip")
    + json_u32_field("max_transfers_per_ip")
    + json_string_field("image", MAX_SERVER_IMAGE_DATA_URI_LENGTH)
    + json_u32_field("file_reindex_interval")
    + json_string_field("persistent_channels", MAX_PERSISTENT_CHANNELS_LENGTH)
    + json_string_field("auto_join_channels", MAX_AUTO_JOIN_CHANNELS_LENGTH)
    + json_u32_field("chat_burst_limit")
    + json_u32_field("chat_rate_limit")
    + json_u8_field("min_password_strength")
    + json_u64_field("max_outbound_rate")
    + json_u32_field("scheduler_chunk_size");

/// ServerInfoUpdated: {"type":"ServerInfoUpdated","server_info":{...}}
const SERVER_INFO_UPDATED_SIZE: usize = json_type_base("ServerInfoUpdated")
    + json_object_field_start("server_info")
    + SERVER_INFO_STRUCT_SIZE
    + json_close();

/// NewsItem nested object size:
/// {"id":i64,"body":"...4096...","image":"...700000...","author":"...32...","author_is_admin":false,"created_at":i64,"updated_at":i64}
/// `author` is optional at runtime, but counted here for the worst case.
const NEWS_ITEM_SIZE: usize = json_first_i64_field("id")
    + json_string_chars_field("body", MAX_NEWS_BODY_LENGTH)
    + json_string_field("image", MAX_NEWS_IMAGE_DATA_URI_LENGTH)
    + json_string_chars_field("author", MAX_NICKNAME_LENGTH)
    + json_bool_field("author_is_admin")
    + json_i64_field("created_at")
    + json_i64_field("updated_at")
    + 2; // {} braces

/// NewsShowResponse: {"type":"NewsShowResponse","success":false,"error":"...2048...","news":{...}}
const NEWS_SHOW_RESPONSE_SIZE: usize = json_type_base("NewsShowResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_field_start("news")
    + NEWS_ITEM_SIZE
    + json_close();

/// NewsCreateResponse: {"type":"NewsCreateResponse","success":false,"error":"...2048...","news":{...}}
const NEWS_CREATE_RESPONSE_SIZE: usize = json_type_base("NewsCreateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_field_start("news")
    + NEWS_ITEM_SIZE
    + json_close();

/// NewsEditResponse: {"type":"NewsEditResponse","success":false,"error":"...2048...","news":{...}}
const NEWS_EDIT_RESPONSE_SIZE: usize = json_type_base("NewsEditResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_field_start("news")
    + NEWS_ITEM_SIZE
    + json_close();

/// NewsUpdateResponse: {"type":"NewsUpdateResponse","success":false,"error":"...2048...","news":{...}}
const NEWS_UPDATE_RESPONSE_SIZE: usize = json_type_base("NewsUpdateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_field_start("news")
    + NEWS_ITEM_SIZE
    + json_close();

/// FileInfoDetails nested object size:
/// {"name":"...4096...","size":u64,"created":i64,"modified":i64,"is_directory":false,"is_symlink":false,"mime_type":"...128...","item_count":u64,"blake3":"...64..."}
const FILE_INFO_DETAILS_SIZE: usize = json_first_string_field("name", MAX_FILE_PATH_LENGTH)
    + json_u64_field("size")
    + json_i64_field("created")
    + json_i64_field("modified")
    + json_bool_field("is_directory")
    + json_bool_field("is_symlink")
    + json_string_field("mime_type", MAX_MIME_TYPE)
    + json_u64_field("item_count")
    + json_string_field("blake3", BLAKE3_HEX_LENGTH)
    + 2; // {} braces

/// FileInfoResponse: {"type":"FileInfoResponse","success":false,"error":"...2048...","info":{...}}
const FILE_INFO_RESPONSE_SIZE: usize = json_type_base("FileInfoResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_field_start("info")
    + FILE_INFO_DETAILS_SIZE
    + json_close();

// =============================================================================
// Permission-dependent limit calculations
// =============================================================================

// =============================================================================
// Permission-array message size calculations
// =============================================================================

/// UserCreate: {"type":"UserCreate","username":"...32...","password":"...256...","is_admin":false,"is_shared":false,"enabled":false,"permissions":["...32...",...],"group_id":i64,"revokes":["...32...",...],"bandwidth_weight":u16,"inherit_bandwidth_weight":false}
const USER_CREATE_SIZE: usize = json_type_base("UserCreate")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_bool_field("is_admin")
    + json_bool_field("is_shared")
    + json_bool_field("enabled")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_i64_field("group_id")
    + json_string_array_field("revokes", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_u16_field("bandwidth_weight")
    + json_bool_field("inherit_bandwidth_weight");

/// UserUpdate: {"type":"UserUpdate","id":i64,"current_password":"...256...","username":"...32...","password":"...256...","is_admin":false,"enabled":false,"permissions":["...32...",...],"group_id":i64,"remove_group":false,"revokes":["...32...",...],"bandwidth_weight":u16,"inherit_bandwidth_weight":false}
const USER_UPDATE_SIZE: usize = json_type_base("UserUpdate")
    + json_i64_field("id")
    + json_string_field("current_password", MAX_PASSWORD_LENGTH)
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_bool_field("is_admin")
    + json_bool_field("enabled")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_i64_field("group_id")
    + json_bool_field("remove_group")
    + json_string_array_field("revokes", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_u16_field("bandwidth_weight")
    + json_bool_field("inherit_bandwidth_weight");

/// UserEditResponse: {"type":"UserEditResponse","success":false,"error":"...2048...","id":i64,"username":"...32...","is_admin":false,"is_shared":false,"enabled":false,"permissions":["...32...",...],"group_id":i64,"group_name":"...32...","group_permissions":["...32...",...],"revoked_permissions":["...32...",...],"bandwidth_weight":u16,"available_groups":[{...},...]}
const USER_EDIT_RESPONSE_SIZE: usize = json_type_base("UserEditResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("username", MAX_USERNAME_LENGTH)
    + json_bool_field("is_admin")
    + json_bool_field("is_shared")
    + json_bool_field("enabled")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_i64_field("group_id")
    + json_string_chars_field("group_name", MAX_GROUP_NAME_LENGTH)
    + json_string_array_field(
        "group_permissions",
        PERMISSIONS_COUNT,
        MAX_PERMISSION_LENGTH,
    )
    + json_string_array_field(
        "revoked_permissions",
        PERMISSIONS_COUNT,
        MAX_PERMISSION_LENGTH,
    )
    + json_object_array_field("available_groups", MAX_GROUPS, GROUP_INFO_STRUCT_SIZE)
    + json_u16_field("bandwidth_weight");

/// ChannelJoinInfo nested object size (for LoginResponse channels array):
/// {"channel":"...50...","topic":"...256...","topic_set_by":"...32...","secret":false,"members":["...32...",...]}
const CHANNEL_JOIN_INFO_SIZE: usize = json_first_string_chars_field("channel", MAX_CHANNEL_LENGTH)
    + json_string_chars_field("topic", MAX_CHAT_TOPIC_LENGTH)
    + json_string_chars_field("topic_set_by", MAX_NICKNAME_LENGTH)
    + json_bool_field("secret")
    + json_string_array_field("members", MAX_CHANNEL_MEMBERS, MAX_NICKNAME_LENGTH)
    + json_string_array_field("voiced", MAX_CHANNEL_MEMBERS, MAX_NICKNAME_LENGTH)
    + 2; // {} braces

/// LoginResponse: {"type":"LoginResponse","success":false,"error":"...2048...","session_id":u32,"user_id":i64,"is_admin":false,"permissions":["...32...",...],"features":["...32...",...],"server_info":{...},"locale":"...16...","channels":[{...},...],"nickname":"...32...","group_id":i64,"group_name":"...32..."}
const LOGIN_RESPONSE_SIZE: usize = json_type_base("LoginResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_u32_field("session_id")
    + json_i64_field("user_id")
    + json_bool_field("is_admin")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_string_array_field("features", MAX_FEATURES_COUNT, MAX_FEATURE_LENGTH)
    + json_object_field_start("server_info")
    + SERVER_INFO_STRUCT_SIZE
    + json_close()
    + json_string_field("locale", MAX_LOCALE_LENGTH)
    + json_object_array_field(
        "channels",
        MAX_AUTO_JOIN_CHANNELS_LENGTH,
        CHANNEL_JOIN_INFO_SIZE,
    )
    + json_string_chars_field("nickname", MAX_NICKNAME_LENGTH)
    + json_i64_field("group_id")
    + json_string_chars_field("group_name", MAX_GROUP_NAME_LENGTH);

/// PermissionsUpdated: {"type":"PermissionsUpdated","is_admin":false,"permissions":["...32...",...],"server_info":{...},"group_id":i64,"group_name":"...32..."}
const PERMISSIONS_UPDATED_SIZE: usize = json_type_base("PermissionsUpdated")
    + json_bool_field("is_admin")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_object_field_start("server_info")
    + SERVER_INFO_STRUCT_SIZE
    + json_close()
    + json_i64_field("group_id")
    + json_string_chars_field("group_name", MAX_GROUP_NAME_LENGTH);

// -----------------------------------------------------------------------------
// Group client messages
// -----------------------------------------------------------------------------

/// GroupList: {"type":"GroupList"}
const GROUP_LIST_SIZE: usize = json_type_base("GroupList");

/// GroupCreate: {"type":"GroupCreate","name":"...32...","is_shared":false,"permissions":["...32...",...],"bandwidth_weight":u16}
const GROUP_CREATE_SIZE: usize = json_type_base("GroupCreate")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH)
    + json_bool_field("is_shared")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_u16_field("bandwidth_weight");

/// GroupEdit: {"type":"GroupEdit","id":i64}
const GROUP_EDIT_SIZE: usize = json_type_base("GroupEdit") + json_i64_field("id");

/// GroupUpdate: {"type":"GroupUpdate","id":i64,"name":"...32...","is_shared":false,"permissions":["...32...",...],"bandwidth_weight":u16}
const GROUP_UPDATE_SIZE: usize = json_type_base("GroupUpdate")
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH)
    + json_bool_field("is_shared")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_u16_field("bandwidth_weight");

/// GroupDelete: {"type":"GroupDelete","id":i64}
const GROUP_DELETE_SIZE: usize = json_type_base("GroupDelete") + json_i64_field("id");

// -----------------------------------------------------------------------------
// Tracker client messages
// -----------------------------------------------------------------------------

/// TrackerAcceptFingerprint: {"type":"TrackerAcceptFingerprint","id":i64}
const TRACKER_ACCEPT_FINGERPRINT_SIZE: usize =
    json_type_base("TrackerAcceptFingerprint") + json_i64_field("id");

/// TrackerAdd: {"type":"TrackerAdd","address":"...253...","port":u16,"fingerprint":"...95...","password":"...256...","name":"...64...","enabled":false}
const TRACKER_ADD_SIZE: usize = json_type_base("TrackerAdd")
    + json_string_field("address", MAX_PUBLIC_ADDRESS_LENGTH)
    + json_u16_field("port")
    + json_string_field("fingerprint", SHA256_FINGERPRINT_LENGTH)
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH)
    + json_bool_field("enabled");

/// TrackerEdit: {"type":"TrackerEdit","id":i64}
const TRACKER_EDIT_SIZE: usize = json_type_base("TrackerEdit") + json_i64_field("id");

/// TrackerList: {"type":"TrackerList"}
const TRACKER_LIST_SIZE: usize = json_type_base("TrackerList");

/// TrackerRemove: {"type":"TrackerRemove","id":i64}
const TRACKER_REMOVE_SIZE: usize = json_type_base("TrackerRemove") + json_i64_field("id");

/// TrackerUpdate: {"type":"TrackerUpdate","id":i64,"address":"...253...","port":u16,"fingerprint":"...95...","password":"...256...","name":"...64...","enabled":false}
const TRACKER_UPDATE_SIZE: usize = json_type_base("TrackerUpdate")
    + json_i64_field("id")
    + json_string_field("address", MAX_PUBLIC_ADDRESS_LENGTH)
    + json_u16_field("port")
    + json_string_field("fingerprint", SHA256_FINGERPRINT_LENGTH)
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH)
    + json_bool_field("enabled");

// -----------------------------------------------------------------------------
// Tracker server messages
// -----------------------------------------------------------------------------

/// One serialized TrackerInfo (config + runtime status). Bounds:
/// - id (i64), port (u16), enabled (bool), connected (bool)
/// - created_at, updated_at, last_connected_at, last_attempted_at (i64 each)
/// - refresh_interval (u32)
/// - address, name (bounded strings)
/// - password, fingerprint, pending_fingerprint (bounded strings)
/// - last_error (bounded by MAX_ERROR_LENGTH), last_error_kind (bounded by MAX_ERROR_KIND_LENGTH)
///
/// Plain nested struct: no `"type"` discriminator on the wire (only
/// the parent `TrackerListResponse` / `TrackerEditResponse` carries
/// the envelope `"type"`). Follows the same shape as `NEWS_ITEM_SIZE`
/// and `CHANNEL_JOIN_INFO_SIZE`: first field uses `json_first_*_field`
/// (no leading comma), subsequent fields carry the comma, trailing
/// `+ 2` accounts for `{` and `}`.
const TRACKER_INFO_SIZE: usize = json_first_i64_field("id")
    + json_string_field("address", MAX_PUBLIC_ADDRESS_LENGTH)
    + json_u16_field("port")
    + json_string_field("fingerprint", SHA256_FINGERPRINT_LENGTH)
    + json_string_field("password", MAX_PASSWORD_LENGTH)
    + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH)
    + json_bool_field("enabled")
    + json_i64_field("created_at")
    + json_i64_field("updated_at")
    + json_bool_field("connected")
    + json_i64_field("last_connected_at")
    + json_i64_field("last_attempted_at")
    + json_string_field("last_error", MAX_ERROR_LENGTH)
    + json_string_field("last_error_kind", MAX_ERROR_KIND_LENGTH)
    + json_string_field("pending_fingerprint", SHA256_FINGERPRINT_LENGTH)
    + json_u32_field("refresh_interval")
    + 2; // {} braces

/// Practical maximum number of trackers a single server will have
/// configured. 64 is well above any realistic deployment (most servers
/// publish to 1–5 trackers); the `MAX_ENTRIES_PER_IP` cap on the
/// tracker side and the operational pain of maintaining many
/// tracker tasks both bound this in practice.
pub const MAX_TRACKERS_PER_SERVER: usize = 64;

/// TrackerAcceptFingerprintResponse: {"type":"TrackerAcceptFingerprintResponse","success":false,"error":"...2048...","id":i64,"name":"...64..."}
const TRACKER_ACCEPT_FINGERPRINT_RESPONSE_SIZE: usize =
    json_type_base("TrackerAcceptFingerprintResponse")
        + json_bool_field("success")
        + json_string_field("error", MAX_ERROR_LENGTH)
        + json_i64_field("id")
        + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH);

/// TrackerAddResponse: {"type":"TrackerAddResponse","success":false,"error":"...2048...","id":i64,"name":"...64..."}
const TRACKER_ADD_RESPONSE_SIZE: usize = json_type_base("TrackerAddResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH);

/// TrackerEditResponse: {"type":"TrackerEditResponse","success":false,"error":"...2048...","tracker":TrackerInfo}
const TRACKER_EDIT_RESPONSE_SIZE: usize = json_type_base("TrackerEditResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + TRACKER_INFO_SIZE;

/// TrackerListResponse: {"type":"TrackerListResponse","success":false,"error":"...2048...","trackers":[TrackerInfo, ...]}
const TRACKER_LIST_RESPONSE_SIZE: usize = json_type_base("TrackerListResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_object_array_field("trackers", MAX_TRACKERS_PER_SERVER, TRACKER_INFO_SIZE);

/// TrackerRemoveResponse: {"type":"TrackerRemoveResponse","success":false,"error":"...2048...","name":"...64..."}
const TRACKER_REMOVE_RESPONSE_SIZE: usize = json_type_base("TrackerRemoveResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH);

/// TrackerUpdateResponse: {"type":"TrackerUpdateResponse","success":false,"error":"...2048...","id":i64,"name":"...64..."}
const TRACKER_UPDATE_RESPONSE_SIZE: usize = json_type_base("TrackerUpdateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_TRACKER_NAME_LENGTH);

// -----------------------------------------------------------------------------
// Group server messages
// -----------------------------------------------------------------------------

/// GroupCreateResponse: {"type":"GroupCreateResponse","success":false,"error":"...2048...","id":i64,"name":"...32..."}
const GROUP_CREATE_RESPONSE_SIZE: usize = json_type_base("GroupCreateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH);

/// GroupEditResponse: {"type":"GroupEditResponse","success":false,"error":"...2048...","id":i64,"name":"...32...","is_shared":false,"permissions":["...32...",...],"member_count":u32,"bandwidth_weight":u16}
const GROUP_EDIT_RESPONSE_SIZE: usize = json_type_base("GroupEditResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH)
    + json_bool_field("is_shared")
    + json_string_array_field("permissions", PERMISSIONS_COUNT, MAX_PERMISSION_LENGTH)
    + json_u32_field("member_count")
    + json_u16_field("bandwidth_weight");

/// GroupUpdateResponse: {"type":"GroupUpdateResponse","success":false,"error":"...2048...","id":i64,"name":"...32..."}
const GROUP_UPDATE_RESPONSE_SIZE: usize = json_type_base("GroupUpdateResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH);

/// GroupDeleteResponse: {"type":"GroupDeleteResponse","success":false,"error":"...2048...","id":i64,"name":"...32..."}
const GROUP_DELETE_RESPONSE_SIZE: usize = json_type_base("GroupDeleteResponse")
    + json_bool_field("success")
    + json_string_field("error", MAX_ERROR_LENGTH)
    + json_i64_field("id")
    + json_string_chars_field("name", MAX_GROUP_NAME_LENGTH);

// =============================================================================
// Limit padding
// =============================================================================

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

    // Client messages - Chat (self-documenting via const calculations)
    m.insert("ChatSend", pad_limit(CHAT_SEND_SIZE as u64));
    m.insert("ChatTopicUpdate", pad_limit(CHAT_TOPIC_UPDATE_SIZE as u64));
    m.insert("ChatJoin", pad_limit(CHAT_JOIN_SIZE as u64));
    m.insert("ChatLeave", pad_limit(CHAT_LEAVE_SIZE as u64));
    m.insert("ChatList", pad_limit(CHAT_LIST_SIZE as u64));
    m.insert("ChatSecret", pad_limit(CHAT_SECRET_SIZE as u64));

    // Client messages - Basic (self-documenting via const calculations)
    m.insert("Handshake", pad_limit(HANDSHAKE_SIZE as u64));
    m.insert("Login", pad_limit(LOGIN_SIZE as u64));
    m.insert("UserBroadcast", pad_limit(USER_BROADCAST_SIZE as u64));
    m.insert("UserCreate", pad_limit(USER_CREATE_SIZE as u64));
    m.insert("UserDelete", pad_limit(USER_DELETE_SIZE as u64));
    m.insert("UserEdit", pad_limit(USER_EDIT_SIZE as u64));
    m.insert("UserInfo", pad_limit(USER_INFO_SIZE as u64));
    m.insert("UserKick", pad_limit(USER_KICK_SIZE as u64));
    m.insert("UserList", pad_limit(USER_LIST_SIZE as u64));
    m.insert("UserUpdate", pad_limit(USER_UPDATE_SIZE as u64));
    m.insert("UserAway", pad_limit(USER_AWAY_SIZE as u64));
    m.insert("UserBack", pad_limit(USER_BACK_SIZE as u64));
    m.insert("UserStatus", pad_limit(USER_STATUS_SIZE as u64));
    m.insert(
        "ServerInfoUpdate",
        pad_limit(SERVER_INFO_UPDATE_SIZE as u64),
    );

    // Ban client messages (self-documenting via const calculations)
    m.insert("BanCreate", pad_limit(BAN_CREATE_SIZE as u64));
    m.insert("BanDelete", pad_limit(BAN_DELETE_SIZE as u64));
    m.insert("BanList", pad_limit(BAN_LIST_SIZE as u64));

    // Trust client messages (self-documenting via const calculations)
    m.insert("TrustCreate", pad_limit(TRUST_CREATE_SIZE as u64));
    m.insert("TrustDelete", pad_limit(TRUST_DELETE_SIZE as u64));
    m.insert("TrustList", pad_limit(TRUST_LIST_SIZE as u64));

    // Connection monitor client message
    m.insert(
        "ConnectionMonitor",
        pad_limit(CONNECTION_MONITOR_SIZE as u64),
    );

    // Group client messages
    m.insert("GroupList", pad_limit(GROUP_LIST_SIZE as u64));
    m.insert("GroupCreate", pad_limit(GROUP_CREATE_SIZE as u64));
    m.insert("GroupEdit", pad_limit(GROUP_EDIT_SIZE as u64));
    m.insert("GroupUpdate", pad_limit(GROUP_UPDATE_SIZE as u64));
    m.insert("GroupDelete", pad_limit(GROUP_DELETE_SIZE as u64));

    // Tracker client messages
    m.insert(
        "TrackerAcceptFingerprint",
        pad_limit(TRACKER_ACCEPT_FINGERPRINT_SIZE as u64),
    );
    m.insert("TrackerAdd", pad_limit(TRACKER_ADD_SIZE as u64));
    m.insert("TrackerEdit", pad_limit(TRACKER_EDIT_SIZE as u64));
    m.insert("TrackerList", pad_limit(TRACKER_LIST_SIZE as u64));
    m.insert("TrackerRemove", pad_limit(TRACKER_REMOVE_SIZE as u64));
    m.insert("TrackerUpdate", pad_limit(TRACKER_UPDATE_SIZE as u64));

    // News client messages (self-documenting via const calculations)
    m.insert("NewsList", pad_limit(NEWS_LIST_SIZE as u64));
    m.insert("NewsShow", pad_limit(NEWS_SHOW_SIZE as u64));
    m.insert("NewsCreate", pad_limit(NEWS_CREATE_SIZE as u64));
    m.insert("NewsEdit", pad_limit(NEWS_EDIT_SIZE as u64));
    m.insert("NewsUpdate", pad_limit(NEWS_UPDATE_SIZE as u64));
    m.insert("NewsDelete", pad_limit(NEWS_DELETE_SIZE as u64));

    // File client messages (self-documenting via const calculations)
    m.insert("FileList", pad_limit(FILE_LIST_SIZE as u64));
    m.insert("FileCreateDir", pad_limit(FILE_CREATE_DIR_SIZE as u64));
    m.insert("FileDelete", pad_limit(FILE_DELETE_SIZE as u64));
    m.insert("FileInfo", pad_limit(FILE_INFO_SIZE as u64));
    m.insert("FileRename", pad_limit(FILE_RENAME_SIZE as u64));
    m.insert("FileMove", pad_limit(FILE_MOVE_SIZE as u64));
    m.insert("FileCopy", pad_limit(FILE_COPY_SIZE as u64));
    m.insert("FileDownload", pad_limit(FILE_DOWNLOAD_SIZE as u64));
    m.insert("FileUpload", pad_limit(FILE_UPLOAD_SIZE as u64));
    m.insert("FileSearch", pad_limit(FILE_SEARCH_SIZE as u64));
    m.insert("FileReindex", pad_limit(FILE_REINDEX_SIZE as u64));

    // Voice client messages (self-documenting via const calculations)
    m.insert("VoiceJoin", pad_limit(VOICE_JOIN_SIZE as u64));
    m.insert("VoiceLeave", pad_limit(VOICE_LEAVE_SIZE as u64));

    // Keepalive messages
    m.insert("Ping", pad_limit(PING_SIZE as u64));
    m.insert("Pong", pad_limit(PONG_SIZE as u64));

    // Server messages - Chat (self-documenting via const calculations)
    m.insert("ChatMessage", pad_limit(CHAT_MESSAGE_SIZE as u64));
    m.insert("ChatUpdated", pad_limit(CHAT_UPDATED_SIZE as u64));
    m.insert(
        "ChatTopicUpdateResponse",
        pad_limit(CHAT_TOPIC_UPDATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "ChatJoinResponse",
        pad_limit(CHAT_JOIN_RESPONSE_SIZE as u64),
    );
    m.insert(
        "ChatLeaveResponse",
        pad_limit(CHAT_LEAVE_RESPONSE_SIZE as u64),
    );
    m.insert("ChatListResponse", 0); // unlimited (server-trusted, can have many channels)
    m.insert(
        "ChatSecretResponse",
        pad_limit(CHAT_SECRET_RESPONSE_SIZE as u64),
    );
    m.insert("ChatUserJoined", pad_limit(CHAT_USER_JOINED_SIZE as u64));
    m.insert("ChatUserLeft", pad_limit(CHAT_USER_LEFT_SIZE as u64));
    m.insert("ChatUserRenamed", pad_limit(CHAT_USER_RENAMED_SIZE as u64));
    m.insert("Error", pad_limit(ERROR_SIZE as u64));
    m.insert(
        "HandshakeResponse",
        pad_limit(HANDSHAKE_RESPONSE_SIZE as u64),
    );
    m.insert("LoginResponse", pad_limit(LOGIN_RESPONSE_SIZE as u64));
    m.insert(
        "PermissionsUpdated",
        pad_limit(PERMISSIONS_UPDATED_SIZE as u64),
    );
    m.insert("ServerBroadcast", pad_limit(SERVER_BROADCAST_SIZE as u64));
    m.insert(
        "ServerInfoUpdated",
        pad_limit(SERVER_INFO_UPDATED_SIZE as u64),
    );
    m.insert(
        "ServerInfoUpdateResponse",
        pad_limit(SERVER_INFO_UPDATE_RESPONSE_SIZE as u64),
    );
    m.insert("UserConnected", pad_limit(USER_CONNECTED_SIZE as u64));
    m.insert(
        "UserCreateResponse",
        pad_limit(USER_CREATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserDeleteResponse",
        pad_limit(USER_DELETE_RESPONSE_SIZE as u64),
    );
    m.insert("UserDisconnected", pad_limit(USER_DISCONNECTED_SIZE as u64));
    m.insert(
        "UserEditResponse",
        pad_limit(USER_EDIT_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserBroadcastResponse",
        pad_limit(USER_BROADCAST_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserInfoResponse",
        pad_limit(USER_INFO_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserKickResponse",
        pad_limit(USER_KICK_RESPONSE_SIZE as u64),
    );
    m.insert("UserListResponse", 0); // unlimited (server-trusted)
    m.insert("UserMessage", pad_limit(USER_MESSAGE_SIZE as u64));
    m.insert(
        "UserMessageResponse",
        pad_limit(USER_MESSAGE_RESPONSE_SIZE as u64),
    );
    m.insert("UserUpdated", pad_limit(USER_UPDATED_SIZE as u64));
    m.insert(
        "UserAwayResponse",
        pad_limit(USER_AWAY_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserBackResponse",
        pad_limit(USER_BACK_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserStatusResponse",
        pad_limit(USER_STATUS_RESPONSE_SIZE as u64),
    );
    m.insert(
        "UserUpdateResponse",
        pad_limit(USER_UPDATE_RESPONSE_SIZE as u64),
    );

    // Ban server messages (self-documenting via const calculations)
    m.insert(
        "BanCreateResponse",
        pad_limit(BAN_CREATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "BanDeleteResponse",
        pad_limit(BAN_DELETE_RESPONSE_SIZE as u64),
    );
    m.insert("BanListResponse", 0); // unlimited (server-trusted, can have many bans)

    // Trust server messages (self-documenting via const calculations)
    m.insert(
        "TrustCreateResponse",
        pad_limit(TRUST_CREATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrustDeleteResponse",
        pad_limit(TRUST_DELETE_RESPONSE_SIZE as u64),
    );
    m.insert("TrustListResponse", 0); // unlimited (server-trusted, can have many trusts)

    // Connection monitor server message
    m.insert("ConnectionMonitorResponse", 0); // unlimited (server-trusted, can have many connections)

    // Group server messages
    m.insert("GroupListResponse", 0); // unlimited (server-trusted, can have many groups)
    m.insert(
        "GroupCreateResponse",
        pad_limit(GROUP_CREATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "GroupEditResponse",
        pad_limit(GROUP_EDIT_RESPONSE_SIZE as u64),
    );
    m.insert(
        "GroupUpdateResponse",
        pad_limit(GROUP_UPDATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "GroupDeleteResponse",
        pad_limit(GROUP_DELETE_RESPONSE_SIZE as u64),
    );

    // Tracker server messages
    m.insert(
        "TrackerAcceptFingerprintResponse",
        pad_limit(TRACKER_ACCEPT_FINGERPRINT_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrackerAddResponse",
        pad_limit(TRACKER_ADD_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrackerEditResponse",
        pad_limit(TRACKER_EDIT_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrackerListResponse",
        pad_limit(TRACKER_LIST_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrackerRemoveResponse",
        pad_limit(TRACKER_REMOVE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrackerUpdateResponse",
        pad_limit(TRACKER_UPDATE_RESPONSE_SIZE as u64),
    );

    // News server messages (self-documenting via const calculations)
    m.insert("NewsListResponse", 0); // unlimited (server-trusted, can have many items)
    m.insert(
        "NewsShowResponse",
        pad_limit(NEWS_SHOW_RESPONSE_SIZE as u64),
    );
    m.insert(
        "NewsCreateResponse",
        pad_limit(NEWS_CREATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "NewsEditResponse",
        pad_limit(NEWS_EDIT_RESPONSE_SIZE as u64),
    );
    m.insert(
        "NewsUpdateResponse",
        pad_limit(NEWS_UPDATE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "NewsDeleteResponse",
        pad_limit(NEWS_DELETE_RESPONSE_SIZE as u64),
    );
    m.insert("NewsUpdated", pad_limit(NEWS_UPDATED_SIZE as u64));

    // File server messages
    m.insert("FileListResponse", 0); // unlimited (server-trusted, can have many entries)
    m.insert(
        "FileCreateDirResponse",
        pad_limit(FILE_CREATE_DIR_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileDeleteResponse",
        pad_limit(FILE_DELETE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileInfoResponse",
        pad_limit(FILE_INFO_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileRenameResponse",
        pad_limit(FILE_RENAME_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileMoveResponse",
        pad_limit(FILE_MOVE_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileCopyResponse",
        pad_limit(FILE_COPY_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileDownloadResponse",
        pad_limit(FILE_DOWNLOAD_RESPONSE_SIZE as u64),
    );
    m.insert(
        "FileUploadResponse",
        pad_limit(FILE_UPLOAD_RESPONSE_SIZE as u64),
    );
    m.insert("FileSearchResponse", 0); // unlimited (server-trusted)
    m.insert(
        "FileReindexResponse",
        pad_limit(FILE_REINDEX_RESPONSE_SIZE as u64),
    );

    // Voice server messages (self-documenting via const calculations)
    m.insert(
        "VoiceJoinResponse",
        pad_limit(VOICE_JOIN_RESPONSE_SIZE as u64),
    );
    m.insert(
        "VoiceLeaveResponse",
        pad_limit(VOICE_LEAVE_RESPONSE_SIZE as u64),
    );
    m.insert("VoiceUserJoined", pad_limit(VOICE_USER_JOINED_SIZE as u64));
    m.insert("VoiceUserLeft", pad_limit(VOICE_USER_LEFT_SIZE as u64));

    // Transfer messages (self-documenting via const calculations)
    m.insert("FileStart", pad_limit(FILE_START_SIZE as u64));
    m.insert(
        "FileStartResponse",
        pad_limit(FILE_START_RESPONSE_SIZE as u64),
    );
    m.insert("FileData", 0); // unlimited - streaming binary data
    m.insert("TransferComplete", pad_limit(TRANSFER_COMPLETE_SIZE as u64));
    m.insert("FileHashing", pad_limit(FILE_HASHING_SIZE as u64));
    m.insert("FileHash", pad_limit(FILE_HASH_SIZE as u64));

    // Tracker messages (separate protocol)
    m.insert(
        "TrackerServerRegister",
        pad_limit(TRACKER_SERVER_REGISTER_SIZE as u64),
    );
    m.insert(
        "TrackerServerList",
        pad_limit(TRACKER_SERVER_LIST_SIZE as u64),
    );
    m.insert(
        "TrackerServerRegisterResponse",
        pad_limit(TRACKER_SERVER_REGISTER_RESPONSE_SIZE as u64),
    );
    m.insert(
        "TrackerServerListResponse",
        TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE as u64,
    );

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
        .expect(ERR_UNKNOWN_MESSAGE_TYPE)
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
        ChannelJoinInfo, ChatAction, ClientMessage, GroupInfo, ServerInfo, ServerMessage, UserInfo,
        UserInfoDetailed,
    };
    use crate::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
    use crate::validators::{
        MAX_AVATAR_DATA_URI_LENGTH, MAX_BAN_REASON_LENGTH, MAX_CHANNEL_LENGTH,
        MAX_CHAT_TOPIC_LENGTH, MAX_ERROR_KIND_LENGTH, MAX_ERROR_LENGTH, MAX_FEATURE_LENGTH,
        MAX_FEATURES_COUNT, MAX_FILE_PATH_LENGTH, MAX_GROUP_NAME_LENGTH, MAX_LOCALE_LENGTH,
        MAX_LOG_LEVEL_LENGTH, MAX_MESSAGE_LENGTH, MAX_NICKNAME_LENGTH, MAX_PASSWORD_LENGTH,
        MAX_PERMISSION_LENGTH, MAX_PERSISTENT_CHANNELS_LENGTH, MAX_PUBLIC_ADDRESS_LENGTH,
        MAX_SEARCH_QUERY_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_IMAGE_DATA_URI_LENGTH,
        MAX_SERVER_NAME_LENGTH, MAX_STATUS_LENGTH, MAX_TRUST_REASON_LENGTH, MAX_USERNAME_LENGTH,
        MAX_VERSION_LENGTH,
    };

    /// Helper to get serialized JSON size of a message
    fn json_size<T: serde::Serialize>(msg: &T) -> usize {
        serde_json::to_string(msg).unwrap().len()
    }

    /// Helper to create a string of given byte length (ASCII fill).
    fn str_of_len(len: usize) -> String {
        "x".repeat(len)
    }

    /// Helper to create a string of `n` characters where each character
    /// occupies 4 bytes of UTF-8 (worst case for chars-counted fields).
    /// Used to stress-test that `json_string_chars_field`-budgeted
    /// payloads still fit when filled with multi-byte text.
    ///
    /// `🚀` (U+1F680) encodes as 4 UTF-8 bytes — the maximum any
    /// Unicode scalar value reaches.
    fn unicode_str_of_len(chars: usize) -> String {
        "\u{1F680}".repeat(chars)
    }

    // =========================================================================
    // JSON Helper Function Tests
    // =========================================================================

    #[test]
    fn test_json_type_base() {
        // Verify against actual serde_json output
        #[derive(serde::Serialize)]
        struct TestType {
            #[serde(rename = "type")]
            type_name: &'static str,
        }

        let msg = TestType { type_name: "Foo" };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"Foo"}"#);
        assert_eq!(json_type_base("Foo"), json.len());

        let msg = TestType {
            type_name: "ChatSend",
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json_type_base("ChatSend"), json.len());

        let msg = TestType {
            type_name: "UserMessage",
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json_type_base("UserMessage"), json.len());
    }

    #[test]
    fn test_json_string_field() {
        // Test that our calculation matches actual JSON
        // We test by building a complete JSON and subtracting the base
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            message: String,
        }

        let msg = TestMsg {
            type_name: "Test",
            message: "hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        // {"type":"Test","message":"hello"}
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(
            json_string_field("message", 5),
            field_size,
            "json={json}, base={base}, field_size={field_size}"
        );

        // Test with empty string
        let msg = TestMsg {
            type_name: "Test",
            message: String::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let field_size = json.len() - base;
        assert_eq!(json_string_field("message", 0), field_size);
    }

    #[test]
    fn test_json_bool_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            flag: bool,
        }

        // Test with false (worst case - 5 chars)
        let msg = TestMsg {
            type_name: "Test",
            flag: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(
            json_bool_field("flag"),
            field_size,
            "json={json}, expected field_size={field_size}"
        );

        // Test with true (4 chars) - should be less than our estimate
        let msg = TestMsg {
            type_name: "Test",
            flag: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let actual_field_size = json.len() - base;
        assert!(
            json_bool_field("flag") >= actual_field_size,
            "bool field estimate should cover true case"
        );
    }

    #[test]
    fn test_json_u16_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            port: u16,
        }

        // Test with max u16 value (65535 = 5 digits)
        let msg = TestMsg {
            type_name: "Test",
            port: 65535,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(json_u16_field("port"), field_size);

        // Smaller value should fit within estimate
        let msg = TestMsg {
            type_name: "Test",
            port: 1,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let actual_field_size = json.len() - base;
        assert!(json_u16_field("port") >= actual_field_size);
    }

    #[test]
    fn test_json_u32_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            count: u32,
        }

        // Test with max u32 value (4294967295 = 10 digits)
        let msg = TestMsg {
            type_name: "Test",
            count: 4294967295,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(json_u32_field("count"), field_size);
    }

    #[test]
    fn test_json_i64_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            timestamp: i64,
        }

        // Test with min i64 value (-9223372036854775808 = 20 chars including sign)
        let msg = TestMsg {
            type_name: "Test",
            timestamp: i64::MIN,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(json_i64_field("timestamp"), field_size);

        // Max i64 should fit (19 digits, no sign)
        let msg = TestMsg {
            type_name: "Test",
            timestamp: i64::MAX,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let actual_field_size = json.len() - base;
        assert!(json_i64_field("timestamp") >= actual_field_size);
    }

    #[test]
    fn test_json_u64_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            size: u64,
        }

        // Test with max u64 value (18446744073709551615 = 20 digits)
        let msg = TestMsg {
            type_name: "Test",
            size: u64::MAX,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(json_u64_field("size"), field_size);
    }

    #[test]
    fn test_json_string_array_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            items: Vec<String>,
        }

        // Test empty array
        let msg = TestMsg {
            type_name: "Test",
            items: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;
        assert_eq!(
            json_string_array_field("items", 0, 10),
            field_size,
            "empty array: json={json}"
        );

        // Test single element
        let msg = TestMsg {
            type_name: "Test",
            items: vec!["hello".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let field_size = json.len() - base;
        assert_eq!(
            json_string_array_field("items", 1, 5),
            field_size,
            "single element: json={json}"
        );

        // Test two elements
        let msg = TestMsg {
            type_name: "Test",
            items: vec!["hello".to_string(), "world".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let field_size = json.len() - base;
        assert_eq!(
            json_string_array_field("items", 2, 5),
            field_size,
            "two elements: json={json}"
        );

        // Test three elements with varying lengths (max should cover all)
        let msg = TestMsg {
            type_name: "Test",
            items: vec!["a".to_string(), "bb".to_string(), "ccc".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let field_size = json.len() - base;
        assert!(
            json_string_array_field("items", 3, 3) >= field_size,
            "three elements with max_len=3 should cover: json={json}, estimate={}, actual={}",
            json_string_array_field("items", 3, 3),
            field_size
        );
    }

    #[test]
    fn test_json_object_field() {
        #[derive(serde::Serialize)]
        struct Inner {
            name: String,
            count: u32,
        }

        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            info: Inner,
        }

        // Use max values to verify exact formula match
        let msg = TestMsg {
            type_name: "Test",
            info: Inner {
                name: str_of_len(10), // Use specific max length
                count: u32::MAX,      // 4294967295 (10 digits)
            },
        };
        let json = serde_json::to_string(&msg).unwrap();

        let base = json_type_base("Test");
        let actual_field_size = json.len() - base;

        // Calculate using our helpers (must match max values used above)
        let calculated = json_object_field_start("info")
            + json_first_string_field("name", 10)
            + json_u32_field("count")
            + json_close();

        assert_eq!(
            calculated, actual_field_size,
            "json={json}, calculated={calculated}, actual={actual_field_size}"
        );
    }

    #[test]
    fn test_json_first_fields() {
        // Test first field variants (no leading comma)
        #[derive(serde::Serialize)]
        struct TestObj {
            name: String,
        }

        let obj = TestObj {
            name: "test".to_string(),
        };
        let json = serde_json::to_string(&obj).unwrap();
        // {"name":"test"}
        // The inner content is: "name":"test" = 12 chars
        // Total is 14, so inner is 12
        assert_eq!(json, r#"{"name":"test"}"#);

        // json_first_string_field should give us the inner field size
        // "name":"test" = 1 + 4 + 1 + 1 + 1 + 4 + 1 = 13? Let me count: " n a m e " : " t e s t "
        // That's 13 characters. Our formula: key.len() + max_value_len + 5 = 4 + 4 + 5 = 13
        assert_eq!(json_first_string_field("name", 4), 13);

        // Verify by checking the full object
        // {} = 2, so inner should be 14 - 2 = 12... hmm
        // Actually {"name":"test"} is 15 chars, not 14
        // { " n a m e " : " t e s t " } = 1 + 13 + 1 = 15
        // So the field itself is 13 chars ✓
    }

    #[test]
    fn test_json_enum_field() {
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            action: &'static str,
        }

        let msg = TestMsg {
            type_name: "Test",
            action: "Normal",
        };
        let json = serde_json::to_string(&msg).unwrap();
        let base = json_type_base("Test");
        let field_size = json.len() - base;

        // json_enum_field is same as json_string_field
        assert_eq!(json_enum_field("action", 6), field_size);
        assert_eq!(json_string_field("action", 6), field_size);
    }

    #[test]
    fn test_multiple_fields_accumulate() {
        // Verify that adding multiple fields accumulates correctly
        #[derive(serde::Serialize)]
        struct TestMsg {
            #[serde(rename = "type")]
            type_name: &'static str,
            message: String,
            channel: String,
            flag: bool,
        }

        let msg = TestMsg {
            type_name: "Test",
            message: str_of_len(10),
            channel: str_of_len(5),
            flag: false,
        };
        let json = serde_json::to_string(&msg).unwrap();

        let calculated = json_type_base("Test")
            + json_string_field("message", 10)
            + json_string_field("channel", 5)
            + json_bool_field("flag");

        assert_eq!(
            calculated,
            json.len(),
            "json={json}, calculated={calculated}, actual={}",
            json.len()
        );
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

    // Message type counts for protocol completeness tests.
    // If you add a new message type to ClientMessage or ServerMessage:
    // 1. Add a payload limit to MESSAGE_TYPE_LIMITS
    // 2. Update the relevant count below
    //
    // Note: Some type names are shared between client and server enums
    // (UserMessage, FileStart, FileStartResponse, FileData, FileHashing, FileHash),
    // so they're only counted once in the HashMap.
    const CLIENT_MESSAGE_COUNT: usize = 64; // 6 News + 8 File + 6 Transfer + 3 Away/Status + 3 Ban + 3 Trust + 2 FileSearch + 4 Chat channel + 1 ConnectionMonitor + 5 Group + 6 Tracker + 2 Voice client messages + 1 Ping
    const SERVER_MESSAGE_COUNT: usize = 80; // 7 News + 9 File + 7 Transfer + 3 Away/Status + 3 Ban + 3 Trust + 2 FileSearch + 7 Chat channel + 1 ConnectionMonitor + 5 Group + 6 Tracker + 4 Voice server messages + 1 Pong
    const SHARED_MESSAGE_COUNT: usize = 6; // UserMessage, FileStart, FileStartResponse, FileData, FileHashing, FileHash

    // Tracker protocol message counts (separate protocol from BBS).
    // The `Error` message type name is shared with the BBS ServerMessage::Error
    // and is already counted in SERVER_MESSAGE_COUNT, so we only count the
    // 4 tracker-specific message types here.
    const TRACKER_CLIENT_MESSAGE_COUNT: usize = 2; // TrackerServerRegister, TrackerServerList
    const TRACKER_SERVER_MESSAGE_COUNT: usize = 2; // TrackerServerRegisterResponse, TrackerServerListResponse

    #[test]
    fn test_all_protocol_types_have_limits() {
        // The exhaustive match in io.rs (client_message_type/server_message_type)
        // will cause a compile error if you add a variant there, reminding you to
        // also add the limit here.
        const TOTAL_MESSAGE_COUNT: usize = CLIENT_MESSAGE_COUNT + SERVER_MESSAGE_COUNT
            - SHARED_MESSAGE_COUNT
            + TRACKER_CLIENT_MESSAGE_COUNT
            + TRACKER_SERVER_MESSAGE_COUNT;

        let known_types = known_message_types();
        assert_eq!(
            known_types.len(),
            TOTAL_MESSAGE_COUNT,
            "MESSAGE_TYPE_LIMITS has {} entries but expected {} ({}+{}-{}+{}+{}). \
             Did you add a new message type? Update the limit and the count here.",
            known_types.len(),
            TOTAL_MESSAGE_COUNT,
            CLIENT_MESSAGE_COUNT,
            SERVER_MESSAGE_COUNT,
            SHARED_MESSAGE_COUNT,
            TRACKER_CLIENT_MESSAGE_COUNT,
            TRACKER_SERVER_MESSAGE_COUNT,
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
            "FileHashing", // True mirror - identical fields (keepalive during hash)
            "FileHash",    // True mirror - identical fields (per-file hash after streaming)
        ];

        for type_name in &shared_type_names {
            assert!(
                is_known_message_type(type_name),
                "Shared type name '{}' must have a limit defined",
                type_name
            );
        }

        // Verify count matches SHARED_MESSAGE_COUNT
        assert_eq!(
            shared_type_names.len(),
            SHARED_MESSAGE_COUNT,
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
            group_id: Some(i64::MAX),
            revokes: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            bandwidth_weight: Some(u16::MAX),
            inherit_bandwidth_weight: Some(true),
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
        let msg = ClientMessage::UserDelete { id: i64::MAX };
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
        let msg = ClientMessage::UserEdit { id: i64::MAX };
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
            reason: Some(str_of_len(MAX_KICK_REASON_LENGTH)),
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
            id: i64::MAX,
            current_password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
            password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            is_admin: Some(false),
            enabled: Some(false),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            group_id: Some(i64::MAX),
            remove_group: Some(false),
            revokes: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            bandwidth_weight: Some(u16::MAX),
            inherit_bandwidth_weight: Some(true),
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
        // Max size: target (32 nickname) + duration (10) + reason (256) + overhead
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
            public_address: Some(str_of_len(MAX_PUBLIC_ADDRESS_LENGTH)),
            max_connections_per_ip: Some(u32::MAX),
            max_transfers_per_ip: Some(u32::MAX),
            image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
            file_reindex_interval: Some(u32::MAX),
            persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
            auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
            chat_burst_limit: Some(u32::MAX),
            chat_rate_limit: Some(u32::MAX),
            min_password_strength: Some(u8::MAX),
            max_outbound_rate: Some(u64::MAX),
            scheduler_chunk_size: Some(u32::MAX),
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
            timestamp: i64::MIN,
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
            voiced: Some(vec![str_of_len(MAX_NICKNAME_LENGTH); 50]),
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
            voiced: None,
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
            // Members array - MAX_CHANNEL_MEMBERS at max nickname length
            members: Some(vec![str_of_len(MAX_NICKNAME_LENGTH); MAX_CHANNEL_MEMBERS]),
            // Voiced array - same size as members for worst case
            voiced: Some(vec![str_of_len(MAX_NICKNAME_LENGTH); MAX_CHANNEL_MEMBERS]),
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
    fn test_limit_chat_user_renamed() {
        let msg = ServerMessage::ChatUserRenamed {
            channel: str_of_len(MAX_CHANNEL_LENGTH),
            old_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            new_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            is_admin: true,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatUserRenamed") as usize;
        assert!(
            size <= limit,
            "ChatUserRenamed size {} exceeds limit {}",
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
            disconnect: false,
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
            fingerprint: str_of_len(SHA256_FINGERPRINT_LENGTH),
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
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
            voiced: Some((0..50).map(|_| str_of_len(MAX_NICKNAME_LENGTH)).collect()),
        };
        let channels: Vec<ChannelJoinInfo> = (0..10).map(|_| channel_info.clone()).collect();

        let msg = ServerMessage::LoginResponse {
            success: false,
            error: None,
            session_id: Some(u32::MAX),
            user_id: Some(i64::MAX),
            is_admin: Some(false),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            features: Some(
                (0..MAX_FEATURES_COUNT)
                    .map(|_| str_of_len(MAX_FEATURE_LENGTH))
                    .collect(),
            ),
            server_info: Some(ServerInfo {
                name: Some(str_of_len(MAX_SERVER_NAME_LENGTH)),
                description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
                public_address: Some(str_of_len(MAX_PUBLIC_ADDRESS_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                max_transfers_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
                transfer_port: u16::MAX,
                transfer_websocket_port: Some(u16::MAX),
                file_reindex_interval: Some(u32::MAX),
                persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                chat_burst_limit: Some(u32::MAX),
                chat_rate_limit: Some(u32::MAX),
                min_password_strength: Some(u8::MAX),
                log_level: Some(str_of_len(MAX_LOG_LEVEL_LENGTH)),
                max_outbound_rate: Some(u64::MAX),
                scheduler_chunk_size: Some(u32::MAX),
            }),
            locale: Some(str_of_len(MAX_LOCALE_LENGTH)),
            channels: Some(channels),
            nickname: Some(str_of_len(MAX_NICKNAME_LENGTH)),
            group_id: Some(i64::MAX),
            group_name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
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
                public_address: Some(str_of_len(MAX_PUBLIC_ADDRESS_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                max_transfers_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
                transfer_port: u16::MAX,
                transfer_websocket_port: Some(u16::MAX),
                file_reindex_interval: Some(u32::MAX),
                persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                chat_burst_limit: Some(u32::MAX),
                chat_rate_limit: Some(u32::MAX),
                min_password_strength: Some(u8::MAX),
                log_level: Some(str_of_len(MAX_LOG_LEVEL_LENGTH)),
                max_outbound_rate: Some(u64::MAX),
                scheduler_chunk_size: Some(u32::MAX),
            }),
            group_id: Some(i64::MAX),
            group_name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
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
                public_address: Some(str_of_len(MAX_PUBLIC_ADDRESS_LENGTH)),
                version: Some(str_of_len(MAX_VERSION_LENGTH)),
                max_connections_per_ip: Some(u32::MAX),
                max_transfers_per_ip: Some(u32::MAX),
                image: Some(str_of_len(MAX_SERVER_IMAGE_DATA_URI_LENGTH)),
                transfer_port: u16::MAX,
                transfer_websocket_port: Some(u16::MAX),
                file_reindex_interval: Some(u32::MAX),
                persistent_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                auto_join_channels: Some(str_of_len(MAX_PERSISTENT_CHANNELS_LENGTH)),
                chat_burst_limit: Some(u32::MAX),
                chat_rate_limit: Some(u32::MAX),
                min_password_strength: Some(u8::MAX),
                log_level: Some(str_of_len(MAX_LOG_LEVEL_LENGTH)),
                max_outbound_rate: Some(u64::MAX),
                scheduler_chunk_size: Some(u32::MAX),
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
                id: i64::MAX,
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
                group_id: Some(i64::MAX),
                group_name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
                bandwidth_weight: u16::MAX,
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
            id: Some(i64::MAX),
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
            id: Some(i64::MAX),
            username: Some(str_of_len(MAX_USERNAME_LENGTH)),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(false),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            bandwidth_weight: Some(u16::MAX),
            group_id: Some(i64::MAX),
            group_name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
            group_permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            revoked_permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            available_groups: Some(
                (0..MAX_GROUPS)
                    .map(|_| GroupInfo {
                        id: i64::MAX,
                        name: str_of_len(MAX_GROUP_NAME_LENGTH),
                        is_shared: false,
                        member_count: u32::MAX,
                        permissions: (0..PERMISSIONS_COUNT)
                            .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                            .collect(),
                        bandwidth_weight: u16::MAX,
                    })
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
                id: i64::MAX,
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
                is_admin: false,
                addresses: Some(vec![str_of_len(45); 10]),
                is_away: false,
                status: Some(str_of_len(MAX_STATUS_LENGTH)),
                channels: Some((0..100).map(|_| str_of_len(MAX_CHANNEL_LENGTH)).collect()),
                group_id: Some(i64::MAX),
                group_name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
                bandwidth_weight: u16::MAX,
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
            from_shared: false,
            to_nickname: str_of_len(MAX_NICKNAME_LENGTH),
            message: str_of_len(MAX_MESSAGE_LENGTH),
            action: ChatAction::Normal,
            timestamp: i64::MIN,
        };
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
                id: i64::MAX,
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
                group_id: Some(i64::MAX),
                group_name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
                bandwidth_weight: u16::MAX,
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
            id: Some(i64::MAX),
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
        // Max size: target (32 nickname) + duration (10) + reason (256) + overhead
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
        // Max size: u64 + 64 char BLAKE3 + overhead
        let msg = ClientMessage::FileStartResponse {
            size: u64::MAX,
            blake3: Some(str_of_len(64)),
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

    #[test]
    fn test_limit_file_hash() {
        // FileHash message (per-file hash sent after streaming)
        // Test with ClientMessage variant
        let client_msg = ClientMessage::FileHash {
            blake3: str_of_len(64),
        };
        let client_size = json_size(&client_msg);
        let limit = max_payload_for_type("FileHash") as usize;
        assert!(
            client_size <= limit,
            "ClientMessage::FileHash size {} exceeds limit {}",
            client_size,
            limit
        );

        // Test with ServerMessage variant
        let server_msg = ServerMessage::FileHash {
            blake3: str_of_len(64),
        };
        let server_size = json_size(&server_msg);
        assert!(
            server_size <= limit,
            "ServerMessage::FileHash size {} exceeds limit {}",
            server_size,
            limit
        );
    }

    // =========================================================================
    // Group message size tests
    // =========================================================================

    #[test]
    fn test_limit_group_list() {
        let msg = ClientMessage::GroupList;
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupList") as usize;
        assert!(
            size <= limit,
            "GroupList size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_create() {
        let msg = ClientMessage::GroupCreate {
            name: str_of_len(MAX_GROUP_NAME_LENGTH),
            is_shared: false,
            permissions: (0..PERMISSIONS_COUNT)
                .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                .collect(),
            bandwidth_weight: u16::MAX,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupCreate") as usize;
        assert!(
            size <= limit,
            "GroupCreate size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_edit() {
        let msg = ClientMessage::GroupEdit { id: i64::MAX };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupEdit") as usize;
        assert!(
            size <= limit,
            "GroupEdit size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_update() {
        let msg = ClientMessage::GroupUpdate {
            id: i64::MAX,
            name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
            is_shared: Some(false),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            bandwidth_weight: Some(u16::MAX),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupUpdate") as usize;
        assert!(
            size <= limit,
            "GroupUpdate size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_delete() {
        let msg = ClientMessage::GroupDelete { id: i64::MAX };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupDelete") as usize;
        assert!(
            size <= limit,
            "GroupDelete size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_list_response_unlimited() {
        // GroupListResponse has unlimited payload (like UserListResponse)
        assert_eq!(max_payload_for_type("GroupListResponse"), 0);
    }

    #[test]
    fn test_limit_group_create_response() {
        let msg = ServerMessage::GroupCreateResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupCreateResponse") as usize;
        assert!(
            size <= limit,
            "GroupCreateResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_edit_response() {
        let msg = ServerMessage::GroupEditResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
            is_shared: Some(false),
            permissions: Some(
                (0..PERMISSIONS_COUNT)
                    .map(|_| str_of_len(MAX_PERMISSION_LENGTH))
                    .collect(),
            ),
            member_count: Some(u32::MAX),
            bandwidth_weight: Some(u16::MAX),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupEditResponse") as usize;
        assert!(
            size <= limit,
            "GroupEditResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_update_response() {
        let msg = ServerMessage::GroupUpdateResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupUpdateResponse") as usize;
        assert!(
            size <= limit,
            "GroupUpdateResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_group_delete_response() {
        let msg = ServerMessage::GroupDeleteResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_GROUP_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("GroupDeleteResponse") as usize;
        assert!(
            size <= limit,
            "GroupDeleteResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    // =========================================================================
    // Tracker protocol message size tests
    // =========================================================================

    #[test]
    fn test_limit_tracker_server_register() {
        let msg = TrackerClientMessage::TrackerServerRegister {
            password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            locale: str_of_len(MAX_LOCALE_LENGTH),
            name: str_of_len(MAX_SERVER_NAME_LENGTH),
            description: Some(str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
            address: Some(str_of_len(MAX_PUBLIC_ADDRESS_LENGTH)),
            port: u16::MAX,
            websocket_port: Some(u16::MAX),
            version: str_of_len(MAX_VERSION_LENGTH),
            fingerprint: str_of_len(SHA256_FINGERPRINT_LENGTH),
            user_count: u32::MAX,
            allows_guest: false,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerServerRegister") as usize;
        assert!(
            size <= limit,
            "TrackerServerRegister size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_server_list() {
        let msg = TrackerClientMessage::TrackerServerList {
            password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            locale: str_of_len(MAX_LOCALE_LENGTH),
            version: str_of_len(MAX_VERSION_LENGTH),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerServerList") as usize;
        assert!(
            size <= limit,
            "TrackerServerList size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_server_register_response() {
        let msg = TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            refresh_interval: Some(u32::MAX),
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            error_kind: Some(str_of_len(MAX_ERROR_KIND_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerServerRegisterResponse") as usize;
        assert!(
            size <= limit,
            "TrackerServerRegisterResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_server_list_response_capped_at_16mib() {
        // TrackerServerListResponse cap: 16 MiB defense-in-depth ceiling
        // (see TRACKER_SERVER_LIST_RESPONSE_MAX_SIZE in this module).
        // Per-entry worst case is ~2 KiB at field caps, so the cap
        // accommodates ~8,000 entries — well above any realistic
        // tracker. Clients filter / sort large lists locally.
        assert_eq!(
            max_payload_for_type("TrackerServerListResponse"),
            16 * 1024 * 1024,
        );
    }

    // =========================================================================
    // BBS-side tracker admin response size tests
    //
    // Catch a future field addition / size-constant bump that would push the
    // worst-case wire shape past the 20% pad on `pad_limit`. Each test builds
    // a TrackerInfo (or response wrapping it) with every field at its maximum
    // and asserts the JSON size is within the per-message-type budget.
    // =========================================================================

    /// Build a worst-case `TrackerInfo` (every Option populated, every
    /// String at its max length, every numeric at its max value).
    fn worst_case_tracker_info() -> crate::protocol::TrackerInfo {
        crate::protocol::TrackerInfo {
            id: i64::MAX,
            address: str_of_len(MAX_PUBLIC_ADDRESS_LENGTH),
            port: u16::MAX,
            fingerprint: Some(str_of_len(SHA256_FINGERPRINT_LENGTH)),
            password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            name: str_of_len(MAX_TRACKER_NAME_LENGTH),
            enabled: true,
            created_at: i64::MAX,
            updated_at: i64::MAX,
            connected: true,
            last_connected_at: Some(i64::MAX),
            last_attempted_at: Some(i64::MAX),
            last_error: Some(str_of_len(MAX_ERROR_LENGTH)),
            last_error_kind: Some(str_of_len(MAX_ERROR_KIND_LENGTH)),
            pending_fingerprint: Some(str_of_len(SHA256_FINGERPRINT_LENGTH)),
            refresh_interval: Some(u32::MAX),
        }
    }

    #[test]
    fn test_limit_tracker_list() {
        let msg = ClientMessage::TrackerList;
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerList") as usize;
        assert!(
            size <= limit,
            "TrackerList size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_edit() {
        let msg = ClientMessage::TrackerEdit { id: i64::MAX };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerEdit") as usize;
        assert!(
            size <= limit,
            "TrackerEdit size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_add() {
        let msg = ClientMessage::TrackerAdd {
            address: str_of_len(MAX_PUBLIC_ADDRESS_LENGTH),
            port: u16::MAX,
            fingerprint: Some(str_of_len(SHA256_FINGERPRINT_LENGTH)),
            password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            name: str_of_len(MAX_TRACKER_NAME_LENGTH),
            enabled: false,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerAdd") as usize;
        assert!(
            size <= limit,
            "TrackerAdd size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_update() {
        let msg = ClientMessage::TrackerUpdate {
            id: i64::MAX,
            address: Some(str_of_len(MAX_PUBLIC_ADDRESS_LENGTH)),
            port: Some(u16::MAX),
            fingerprint: Some(str_of_len(SHA256_FINGERPRINT_LENGTH)),
            password: Some(str_of_len(MAX_PASSWORD_LENGTH)),
            name: Some(str_of_len(MAX_TRACKER_NAME_LENGTH)),
            enabled: Some(false),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerUpdate") as usize;
        assert!(
            size <= limit,
            "TrackerUpdate size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_remove() {
        let msg = ClientMessage::TrackerRemove { id: i64::MAX };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerRemove") as usize;
        assert!(
            size <= limit,
            "TrackerRemove size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_accept_fingerprint() {
        let msg = ClientMessage::TrackerAcceptFingerprint { id: i64::MAX };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerAcceptFingerprint") as usize;
        assert!(
            size <= limit,
            "TrackerAcceptFingerprint size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_list_response() {
        let msg = ServerMessage::TrackerListResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            trackers: Some(
                (0..MAX_TRACKERS_PER_SERVER)
                    .map(|_| worst_case_tracker_info())
                    .collect(),
            ),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerListResponse") as usize;
        assert!(
            size <= limit,
            "TrackerListResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_edit_response() {
        let msg = ServerMessage::TrackerEditResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            tracker: Some(worst_case_tracker_info()),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerEditResponse") as usize;
        assert!(
            size <= limit,
            "TrackerEditResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_add_response() {
        let msg = ServerMessage::TrackerAddResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_TRACKER_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerAddResponse") as usize;
        assert!(
            size <= limit,
            "TrackerAddResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_update_response() {
        let msg = ServerMessage::TrackerUpdateResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_TRACKER_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerUpdateResponse") as usize;
        assert!(
            size <= limit,
            "TrackerUpdateResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_accept_fingerprint_response() {
        let msg = ServerMessage::TrackerAcceptFingerprintResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            id: Some(i64::MAX),
            name: Some(str_of_len(MAX_TRACKER_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerAcceptFingerprintResponse") as usize;
        assert!(
            size <= limit,
            "TrackerAcceptFingerprintResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    #[test]
    fn test_limit_tracker_remove_response() {
        let msg = ServerMessage::TrackerRemoveResponse {
            success: false,
            error: Some(str_of_len(MAX_ERROR_LENGTH)),
            name: Some(str_of_len(MAX_TRACKER_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerRemoveResponse") as usize;
        assert!(
            size <= limit,
            "TrackerRemoveResponse size {} exceeds limit {}",
            size,
            limit
        );
    }

    // =========================================================================
    // UTF-8 stress tests: chars-counted fields filled with 4-byte chars
    // =========================================================================
    //
    // A field validated by `.chars().count()` can carry up to 4 bytes per
    // character on the wire (worst-case UTF-8). The framing budget for
    // these fields must use `json_string_chars_field`, not
    // `json_string_field`, or a CJK / emoji payload at the validator cap
    // will exceed the per-message limit. These tests fill char-counted
    // fields with `\u{1F680}` (4-byte rocket emoji) and assert the
    // payload still fits.
    //
    // The earlier sweep that switched 98 call sites would silently miss a
    // single `json_string_field(..., MAX_X_LENGTH)` left over for a
    // chars-counted constant; the ASCII-only `str_of_len` fixtures don't
    // catch that. These tests do.

    #[test]
    fn test_limit_chat_send_utf8_stress() {
        let msg = ClientMessage::ChatSend {
            message: unicode_str_of_len(MAX_MESSAGE_LENGTH),
            action: ChatAction::Normal,
            channel: format!("#{}", unicode_str_of_len(MAX_CHANNEL_LENGTH - 1)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ChatSend") as usize;
        assert!(
            size <= limit,
            "ChatSend (UTF-8 stress) size {size} exceeds limit {limit}"
        );
    }

    #[test]
    fn test_limit_server_info_update_utf8_stress() {
        let msg = ClientMessage::ServerInfoUpdate {
            name: Some(unicode_str_of_len(MAX_SERVER_NAME_LENGTH)),
            description: Some(unicode_str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
            public_address: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            min_password_strength: None,
            max_outbound_rate: None,
            scheduler_chunk_size: None,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("ServerInfoUpdate") as usize;
        assert!(
            size <= limit,
            "ServerInfoUpdate (UTF-8 stress) size {size} exceeds limit {limit}"
        );
    }

    #[test]
    fn test_limit_user_kick_utf8_stress() {
        let msg = ClientMessage::UserKick {
            nickname: unicode_str_of_len(MAX_NICKNAME_LENGTH),
            reason: Some(unicode_str_of_len(MAX_KICK_REASON_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("UserKick") as usize;
        assert!(
            size <= limit,
            "UserKick (UTF-8 stress) size {size} exceeds limit {limit}"
        );
    }

    #[test]
    fn test_limit_ban_create_utf8_stress() {
        let msg = ClientMessage::BanCreate {
            target: unicode_str_of_len(MAX_TARGET_LENGTH),
            duration: Some(str_of_len(MAX_DURATION_LENGTH)),
            reason: Some(unicode_str_of_len(MAX_BAN_REASON_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("BanCreate") as usize;
        assert!(
            size <= limit,
            "BanCreate (UTF-8 stress) size {size} exceeds limit {limit}"
        );
    }

    #[test]
    fn test_limit_tracker_server_register_utf8_stress() {
        let msg = TrackerClientMessage::TrackerServerRegister {
            password: None,
            locale: "en".to_string(),
            name: unicode_str_of_len(MAX_SERVER_NAME_LENGTH),
            description: Some(unicode_str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
            address: None,
            port: u16::MAX,
            websocket_port: Some(u16::MAX),
            version: "0.9.0".to_string(),
            fingerprint: str_of_len(SHA256_FINGERPRINT_LENGTH),
            user_count: u32::MAX,
            allows_guest: false,
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("TrackerServerRegister") as usize;
        assert!(
            size <= limit,
            "TrackerServerRegister (UTF-8 stress) size {size} exceeds limit {limit}"
        );
    }

    #[test]
    fn test_limit_login_response_utf8_stress() {
        // Regression test for the SERVER_INFO_STRUCT_SIZE bug: server
        // name in the embedded ServerInfo used `json_first_string_field`
        // (byte budget) for a chars-counted constant. A multi-byte server
        // name at the cap blew the budget.
        let msg = ServerMessage::LoginResponse {
            success: true,
            error: None,
            session_id: Some(1),
            user_id: Some(1),
            is_admin: Some(false),
            permissions: Some(vec![]),
            features: Some(vec![unicode_str_of_len(MAX_FEATURE_LENGTH)]),
            server_info: Some(ServerInfo {
                name: Some(unicode_str_of_len(MAX_SERVER_NAME_LENGTH)),
                description: Some(unicode_str_of_len(MAX_SERVER_DESCRIPTION_LENGTH)),
                public_address: None,
                version: Some("0.9.0".to_string()),
                max_connections_per_ip: None,
                max_transfers_per_ip: None,
                image: None,
                transfer_port: 0,
                transfer_websocket_port: None,
                file_reindex_interval: None,
                persistent_channels: None,
                auto_join_channels: None,
                chat_burst_limit: None,
                chat_rate_limit: None,
                min_password_strength: None,
                log_level: None,
                max_outbound_rate: None,
                scheduler_chunk_size: None,
            }),
            locale: Some("en".to_string()),
            channels: Some(vec![]),
            nickname: Some(unicode_str_of_len(MAX_NICKNAME_LENGTH)),
            group_id: None,
            group_name: Some(unicode_str_of_len(MAX_GROUP_NAME_LENGTH)),
        };
        let size = json_size(&msg);
        let limit = max_payload_for_type("LoginResponse") as usize;
        assert!(
            size <= limit,
            "LoginResponse (UTF-8 stress) size {size} exceeds limit {limit}"
        );
    }
}
