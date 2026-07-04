//! Input validation functions
//!
//! Reusable validators for common input types. These validators are shared
//! between client and server - clients can use them for pre-validation,
//! servers use them for enforcement.

mod avatar;
mod ban_reason;
mod bandwidth_chunk_size;
mod bandwidth_weight;
mod blake3;
mod channel;
mod channel_list;
mod chat_topic;
mod connection_address;
mod data_uri;
mod dir_name;
mod duration;
mod error;
mod features;
mod file_path;
mod group_name;
#[cfg(feature = "image-decode")]
mod image_decode;
mod ip_rule_reason;
mod kick_reason;
mod locale;
mod message;
mod news_body;
mod news_image;
mod nickname;
mod password;
mod permissions;
mod public_address;
mod search_query;
mod server_description;
mod server_image;
mod server_name;
mod status;
mod target;
mod tracker_address;
mod tracker_name;
mod trust_reason;
mod username;
mod version;

pub use avatar::{AvatarError, MAX_AVATAR_DATA_URI_LENGTH, is_valid_svg, validate_avatar};
pub use ban_reason::{BanReasonError, MAX_BAN_REASON_LENGTH, validate_ban_reason};
pub use bandwidth_chunk_size::{
    BandwidthChunkSizeError, DEFAULT_BANDWIDTH_CHUNK_SIZE, MAX_BANDWIDTH_CHUNK_SIZE,
    MIN_BANDWIDTH_CHUNK_SIZE, validate_bandwidth_chunk_size,
};
pub use bandwidth_weight::{
    BandwidthWeightError, DEFAULT_ADMIN_BANDWIDTH_WEIGHT, DEFAULT_BANDWIDTH_WEIGHT,
    MIN_BANDWIDTH_WEIGHT, resolve_bandwidth_weight, validate_bandwidth_weight,
};
pub use blake3::{BLAKE3_HEX_LENGTH, Blake3Error, validate_blake3};
pub use channel::{
    CHANNEL_PREFIX, ChannelError, DEFAULT_CHANNEL, MAX_CHANNEL_LENGTH, MAX_CHANNELS_PER_USER,
    MIN_CHANNEL_LENGTH, is_channel_target, validate_channel,
};
pub use channel_list::{
    AutoJoinChannelsError, ChannelListError, MAX_AUTO_JOIN_CHANNELS_LENGTH,
    MAX_CHANNEL_LIST_LENGTH, MAX_PERSISTENT_CHANNELS_LENGTH, PersistentChannelsError,
    validate_auto_join_channels, validate_channel_list, validate_persistent_channels,
};
pub use chat_topic::{ChatTopicError, MAX_CHAT_TOPIC_LENGTH, validate_chat_topic};
pub use connection_address::{ConnectionAddressError, validate_connection_address};
pub use data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};
pub use dir_name::{DirNameError, MAX_DIR_NAME_LENGTH, validate_dir_name};
pub use duration::{DurationError, MAX_DURATION_LENGTH, validate_duration};
pub use error::{
    MAX_COMMAND_LENGTH, MAX_ERROR_KIND_LENGTH, MAX_ERROR_LENGTH, MAX_LOG_LEVEL_LENGTH,
    MAX_NEWS_ACTION_LENGTH, TRANSFER_ID_LENGTH,
};
pub use features::{FeaturesError, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT, validate_features};
pub use file_path::{FilePathError, MAX_FILE_PATH_LENGTH, validate_file_path};
pub use group_name::{GroupNameError, MAX_GROUP_NAME_LENGTH, validate_group_name};
#[cfg(feature = "image-decode")]
pub use image_decode::{
    ImageDecodeError, ImageDecodeLimits, ImageDecodeProfile, decode_image_data_uri_payload,
    image_crate_limits, validate_image_bytes_decode, validate_image_data_uri_decodes,
};
pub use ip_rule_reason::{IpRuleReasonError, MAX_IP_RULE_REASON_LENGTH, validate_ip_rule_reason};
pub use kick_reason::{KickReasonError, MAX_KICK_REASON_LENGTH, validate_kick_reason};
pub use locale::{LocaleError, MAX_LOCALE_LENGTH, validate_locale};
pub use message::{MAX_MESSAGE_LENGTH, MessageError, validate_message};
pub use news_body::{MAX_NEWS_BODY_LENGTH, NewsBodyError, validate_news_body};
pub use news_image::{MAX_NEWS_IMAGE_DATA_URI_LENGTH, NewsImageError, validate_news_image};
pub use nickname::{MAX_NICKNAME_LENGTH, NicknameError, validate_nickname};
pub use password::{
    MAX_PASSWORD_LENGTH, PasswordError, PasswordStrength, password_strength, validate_password,
    validate_password_input,
};
pub use permissions::{MAX_PERMISSION_LENGTH, PermissionsError, validate_permissions};
pub use public_address::{
    MAX_PUBLIC_ADDRESS_LENGTH, NormalizedAddress, PublicAddressError,
    validate_and_classify_public_address, validate_and_normalize_public_address,
    validate_public_address,
};
pub use search_query::{
    MAX_SEARCH_QUERY_LENGTH, MIN_PRIMARY_TERM_LENGTH, MIN_QUERY_LENGTH, MIN_TERM_LENGTH,
    SearchQueryError, extract_search_terms, validate_search_query,
};
pub use server_description::{
    MAX_SERVER_DESCRIPTION_LENGTH, ServerDescriptionError, validate_server_description,
};
pub use server_image::{MAX_SERVER_IMAGE_DATA_URI_LENGTH, ServerImageError, validate_server_image};
pub use server_name::{MAX_SERVER_NAME_LENGTH, ServerNameError, validate_server_name};
pub use status::{MAX_STATUS_LENGTH, StatusError, validate_status};
pub use target::{MAX_TARGET_LENGTH, TargetError, validate_target};
pub use tracker_address::{TrackerAddressError, validate_tracker_address};
pub use tracker_name::{MAX_TRACKER_NAME_LENGTH, TrackerNameError, validate_tracker_name};
pub use trust_reason::{MAX_TRUST_REASON_LENGTH, TrustReasonError, validate_trust_reason};
pub use username::{MAX_USERNAME_LENGTH, UsernameError, validate_username};
pub use version::{MAX_VERSION_LENGTH, VersionError, validate_version};
