//! Server image validation (512KB max, data URI format).

use super::data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};

/// Maximum length of server image data URI (512KB binary + base64 overhead + prefix).
pub const MAX_SERVER_IMAGE_DATA_URI_LENGTH: usize = 700_000;

/// Server image validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerImageError {
    TooLarge,
    InvalidFormat,
    UnsupportedType,
}

impl From<DataUriError> for ServerImageError {
    fn from(err: DataUriError) -> Self {
        match err {
            DataUriError::TooLarge => ServerImageError::TooLarge,
            DataUriError::InvalidFormat => ServerImageError::InvalidFormat,
            DataUriError::UnsupportedType => ServerImageError::UnsupportedType,
        }
    }
}

/// Validate a server image data URI.
///
/// # Examples
///
/// ```
/// use nexus_common::validators::{validate_server_image, ServerImageError};
///
/// assert!(validate_server_image("data:image/png;base64,iVBORw0KGgo=").is_ok());
/// assert_eq!(
///     validate_server_image("data:image/gif;base64,R0lGODlh"),
///     Err(ServerImageError::UnsupportedType)
/// );
/// ```
pub fn validate_server_image(image: &str) -> Result<(), ServerImageError> {
    validate_image_data_uri(
        image,
        MAX_SERVER_IMAGE_DATA_URI_LENGTH,
        ALLOWED_IMAGE_MIME_TYPES,
    )?;
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
            assert!(validate_server_image(uri).is_ok());
        }
    }

    #[test]
    fn test_invalid_format() {
        for uri in ["", "data:", "not a uri", "data:image/png,abc"] {
            assert_eq!(
                validate_server_image(uri),
                Err(ServerImageError::InvalidFormat)
            );
        }
    }

    #[test]
    fn test_unsupported_type() {
        for uri in [
            "data:image/gif;base64,abc",
            "data:image/bmp;base64,abc",
            "data:text/plain;base64,abc",
        ] {
            assert_eq!(
                validate_server_image(uri),
                Err(ServerImageError::UnsupportedType)
            );
        }
    }

    #[test]
    fn test_size_limit() {
        let prefix = "data:image/png;base64,";
        let at_limit = format!(
            "{}{}",
            prefix,
            "A".repeat(MAX_SERVER_IMAGE_DATA_URI_LENGTH - prefix.len())
        );
        let over_limit = format!("{}A", at_limit);

        assert!(validate_server_image(&at_limit).is_ok());
        assert_eq!(
            validate_server_image(&over_limit),
            Err(ServerImageError::TooLarge)
        );
    }
}
