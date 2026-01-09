//! Input validation functions
//!
//! Reusable validators for common input types. These validators are shared
//! between client and server - clients can use them for pre-validation,
//! servers use them for enforcement.

mod avatar;
mod ban_reason;
mod chat_topic;
mod data_uri;
mod dir_name;
mod features;
mod file_path;
mod locale;
mod message;
mod news_body;
mod news_image;
mod nickname;
mod password;
mod permissions;
mod server_description;
mod server_image;
mod server_name;
mod sha256;
mod status;
mod username;
mod version;

pub use avatar::{AvatarError, MAX_AVATAR_DATA_URI_LENGTH, validate_avatar};
pub use ban_reason::{BanReasonError, MAX_BAN_REASON_LENGTH, validate_ban_reason};
pub use chat_topic::{ChatTopicError, MAX_CHAT_TOPIC_LENGTH, validate_chat_topic};
pub use data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};
pub use dir_name::{DirNameError, MAX_DIR_NAME_LENGTH, validate_dir_name};
pub use features::{FeaturesError, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT, validate_features};
pub use file_path::{FilePathError, MAX_FILE_PATH_LENGTH, validate_file_path};
pub use locale::{LocaleError, MAX_LOCALE_LENGTH, validate_locale};
pub use message::{MAX_MESSAGE_LENGTH, MessageError, validate_message};
pub use news_body::{MAX_NEWS_BODY_LENGTH, NewsBodyError, validate_news_body};
pub use news_image::{MAX_NEWS_IMAGE_DATA_URI_LENGTH, NewsImageError, validate_news_image};
pub use nickname::{MAX_NICKNAME_LENGTH, NicknameError, validate_nickname};
pub use password::{
    MAX_PASSWORD_LENGTH, PasswordError, validate_password, validate_password_input,
};
pub use permissions::{MAX_PERMISSION_LENGTH, PermissionsError, validate_permissions};
pub use server_description::{
    MAX_SERVER_DESCRIPTION_LENGTH, ServerDescriptionError, validate_server_description,
};
pub use server_image::{MAX_SERVER_IMAGE_DATA_URI_LENGTH, ServerImageError, validate_server_image};
pub use server_name::{MAX_SERVER_NAME_LENGTH, ServerNameError, validate_server_name};
pub use sha256::{SHA256_HEX_LENGTH, Sha256Error, validate_sha256};
pub use status::{MAX_STATUS_LENGTH, StatusError, validate_status};
pub use username::{MAX_USERNAME_LENGTH, UsernameError, validate_username};
pub use version::{MAX_VERSION_LENGTH, VersionError, validate_version};
