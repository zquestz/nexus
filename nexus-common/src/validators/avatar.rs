//! Avatar validation (128KB max, data URI format).

use super::data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};

/// Maximum length of avatar data URI (128KB binary + base64 overhead + prefix).
pub const MAX_AVATAR_DATA_URI_LENGTH: usize = 176_000;

/// Avatar validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarError {
    TooLarge,
    InvalidFormat,
    UnsupportedType,
}

impl From<DataUriError> for AvatarError {
    fn from(err: DataUriError) -> Self {
        match err {
            DataUriError::TooLarge => AvatarError::TooLarge,
            DataUriError::InvalidFormat => AvatarError::InvalidFormat,
            DataUriError::UnsupportedType => AvatarError::UnsupportedType,
        }
    }
}

/// Validate an avatar data URI.
///
/// # Examples
///
/// ```
/// use nexus_common::validators::{validate_avatar, AvatarError};
///
/// assert!(validate_avatar("data:image/png;base64,iVBORw0KGgo=").is_ok());
/// assert_eq!(
///     validate_avatar("data:image/gif;base64,R0lGODlh"),
///     Err(AvatarError::UnsupportedType)
/// );
/// ```
pub fn validate_avatar(avatar: &str) -> Result<(), AvatarError> {
    validate_image_data_uri(avatar, MAX_AVATAR_DATA_URI_LENGTH, ALLOWED_IMAGE_MIME_TYPES)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_types() {
        for uri in [
            "data:image/png;base64,iVBORw0KGgo=",
            "data:image/webp;base64,UklGRh4=",
            "data:image/svg+xml;base64,PHN2Zz4=",
            "data:image/jpeg;base64,/9j/4AAQ",
            "data:image/png;base64,",
        ] {
            assert!(validate_avatar(uri).is_ok());
        }
    }

    #[test]
    fn test_invalid_format() {
        for uri in ["", "data:", "not a uri", "data:image/png,abc"] {
            assert_eq!(validate_avatar(uri), Err(AvatarError::InvalidFormat));
        }
    }

    #[test]
    fn test_unsupported_type() {
        for uri in [
            "data:image/gif;base64,abc",
            "data:image/bmp;base64,abc",
            "data:text/plain;base64,abc",
        ] {
            assert_eq!(validate_avatar(uri), Err(AvatarError::UnsupportedType));
        }
    }

    #[test]
    fn test_size_limit() {
        let prefix = "data:image/png;base64,";
        let at_limit = format!(
            "{}{}",
            prefix,
            "A".repeat(MAX_AVATAR_DATA_URI_LENGTH - prefix.len())
        );
        let over_limit = format!("{}A", at_limit);

        assert!(validate_avatar(&at_limit).is_ok());
        assert_eq!(validate_avatar(&over_limit), Err(AvatarError::TooLarge));
    }
}
