//! Input validation functions
//!
//! Reusable validators for common input types. These validators are shared
//! between client and server - clients can use them for pre-validation,
//! servers use them for enforcement.

mod chattopic;
mod features;
mod locale;
mod message;
mod password;
mod permissions;
mod serverdescription;
mod servername;
mod username;
mod version;

pub use chattopic::{ChatTopicError, MAX_CHAT_TOPIC_LENGTH, validate_chat_topic};
pub use features::{FeaturesError, MAX_FEATURE_LENGTH, MAX_FEATURES_COUNT, validate_features};
pub use locale::{LocaleError, MAX_LOCALE_LENGTH, validate_locale};
pub use message::{MAX_MESSAGE_LENGTH, MessageError, validate_message};
pub use password::{MAX_PASSWORD_LENGTH, PasswordError, validate_password};
pub use permissions::{
    MAX_PERMISSION_LENGTH, MAX_PERMISSIONS_COUNT, PermissionsError, validate_permissions,
};
pub use serverdescription::MAX_SERVER_DESCRIPTION_LENGTH;
pub use servername::MAX_SERVER_NAME_LENGTH;
pub use username::{MAX_USERNAME_LENGTH, UsernameError, validate_username};
pub use version::{MAX_VERSION_LENGTH, VersionError, validate_version};
