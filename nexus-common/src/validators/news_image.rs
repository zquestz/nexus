//! News image validation (512KB max, data URI format).
//!
//! Reuses the same validation as server images.

use super::data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};

/// Maximum length of news image data URI (512KB binary + base64 overhead + prefix).
/// Same as server image limit.
pub const MAX_NEWS_IMAGE_DATA_URI_LENGTH: usize = 700_000;

/// News image validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsImageError {
    TooLarge,
    InvalidFormat,
    UnsupportedType,
}

impl From<DataUriError> for NewsImageError {
    fn from(err: DataUriError) -> Self {
        match err {
            DataUriError::TooLarge => NewsImageError::TooLarge,
            DataUriError::InvalidFormat => NewsImageError::InvalidFormat,
            DataUriError::UnsupportedType => NewsImageError::UnsupportedType,
        }
    }
}

/// Validate a news image data URI.
///
/// # Examples
///
/// ```
/// use nexus_common::validators::{validate_news_image, NewsImageError};
///
/// assert!(validate_news_image("data:image/png;base64,iVBORw0KGgo=").is_ok());
/// assert_eq!(
///     validate_news_image("data:image/gif;base64,R0lGODlh"),
///     Err(NewsImageError::UnsupportedType)
/// );
/// ```
pub fn validate_news_image(image: &str) -> Result<(), NewsImageError> {
    validate_image_data_uri(
        image,
        MAX_NEWS_IMAGE_DATA_URI_LENGTH,
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
            assert!(validate_news_image(uri).is_ok());
        }
    }

    #[test]
    fn test_invalid_format() {
        for uri in ["", "data:", "not a uri", "data:image/png,abc"] {
            assert_eq!(validate_news_image(uri), Err(NewsImageError::InvalidFormat));
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
                validate_news_image(uri),
                Err(NewsImageError::UnsupportedType)
            );
        }
    }

    #[test]
    fn test_size_limit() {
        let prefix = "data:image/png;base64,";
        let at_limit = format!(
            "{}{}",
            prefix,
            "A".repeat(MAX_NEWS_IMAGE_DATA_URI_LENGTH - prefix.len())
        );
        let over_limit = format!("{}A", at_limit);

        assert!(validate_news_image(&at_limit).is_ok());
        assert_eq!(
            validate_news_image(&over_limit),
            Err(NewsImageError::TooLarge)
        );
    }
}
