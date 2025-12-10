//! Input validation functions
//!
//! Reusable validators for common input types. These validators are shared
//! between client and server - clients can use them for pre-validation,
//! servers use them for enforcement.

mod avatar;
mod chat_topic;
mod data_uri;
mod features;
mod locale;
mod message;
mod password;
mod permissions;
mod server_description;
mod server_image;
mod server_name;
mod username;
mod version;

pub use avatar::{AvatarError, MAX_AVATAR_DATA_URI_LENGTH, validate_avatar};
pub use chat_topic::{ChatTopicError, MAX_CHAT_TOPIC_LENGTH, validate_chat_topic};
pub use data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};
pub use features::{FeaturesError, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT, validate_features};
pub use locale::{LocaleError, MAX_LOCALE_LENGTH, validate_locale};
pub use message::{MAX_MESSAGE_LENGTH, MessageError, validate_message};
pub use password::{MAX_PASSWORD_LENGTH, PasswordError, validate_password};
pub use permissions::{
    MAX_PERMISSION_LENGTH, MAX_PERMISSIONS_COUNT, PermissionsError, validate_permissions,
};
pub use server_description::{
    MAX_SERVER_DESCRIPTION_LENGTH, ServerDescriptionError, validate_server_description,
};
pub use server_image::{MAX_SERVER_IMAGE_DATA_URI_LENGTH, ServerImageError, validate_server_image};
pub use server_name::{MAX_SERVER_NAME_LENGTH, ServerNameError, validate_server_name};
pub use username::{MAX_USERNAME_LENGTH, UsernameError, validate_username};
pub use version::{MAX_VERSION_LENGTH, VersionError, validate_version};
