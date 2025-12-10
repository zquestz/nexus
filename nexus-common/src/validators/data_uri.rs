//! Data URI validation for images (avatars, server images).

/// Allowed MIME types for images
pub const ALLOWED_IMAGE_MIME_TYPES: &[&str] =
    &["image/png", "image/webp", "image/svg+xml", "image/jpeg"];

/// Data URI validation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataUriError {
    TooLarge,
    InvalidFormat,
    UnsupportedType,
}

/// Validate an image data URI against max length and allowed MIME types.
pub fn validate_image_data_uri(
    data_uri: &str,
    max_length: usize,
    allowed_types: &[&str],
) -> Result<(), DataUriError> {
    if data_uri.len() > max_length {
        return Err(DataUriError::TooLarge);
    }

    if !data_uri.starts_with("data:") {
        return Err(DataUriError::InvalidFormat);
    }

    let Some(base64_pos) = data_uri.find(";base64,") else {
        return Err(DataUriError::InvalidFormat);
    };

    let mime_type = &data_uri[5..base64_pos];
    if !allowed_types.contains(&mime_type) {
        return Err(DataUriError::UnsupportedType);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_LEN: usize = 1000;

    #[test]
    fn test_valid_types() {
        for uri in [
            "data:image/png;base64,iVBORw0KGgo=",
            "data:image/webp;base64,UklGRh4=",
            "data:image/svg+xml;base64,PHN2Zz4=",
            "data:image/jpeg;base64,/9j/4AAQ",
            "data:image/png;base64,",
        ] {
            assert!(validate_image_data_uri(uri, MAX_LEN, ALLOWED_IMAGE_MIME_TYPES).is_ok());
        }
    }

    #[test]
    fn test_invalid_format() {
        for uri in [
            "",
            "data:",
            "not a data uri",
            "image/png;base64,abc",
            "data:image/png,abc",
        ] {
            assert_eq!(
                validate_image_data_uri(uri, MAX_LEN, ALLOWED_IMAGE_MIME_TYPES),
                Err(DataUriError::InvalidFormat)
            );
        }
    }

    #[test]
    fn test_unsupported_type() {
        for uri in [
            "data:image/gif;base64,R0lGODlh",
            "data:image/bmp;base64,Qk0=",
            "data:text/plain;base64,SGVsbG8=",
            "data:;base64,SGVsbG8=",
        ] {
            assert_eq!(
                validate_image_data_uri(uri, MAX_LEN, ALLOWED_IMAGE_MIME_TYPES),
                Err(DataUriError::UnsupportedType)
            );
        }
    }

    #[test]
    fn test_size_limit() {
        let prefix = "data:image/png;base64,";
        let at_limit = format!("{}{}", prefix, "A".repeat(MAX_LEN - prefix.len()));
        let over_limit = format!("{}{}", prefix, "A".repeat(MAX_LEN - prefix.len() + 1));

        assert!(validate_image_data_uri(&at_limit, MAX_LEN, ALLOWED_IMAGE_MIME_TYPES).is_ok());
        assert_eq!(
            validate_image_data_uri(&over_limit, MAX_LEN, ALLOWED_IMAGE_MIME_TYPES),
            Err(DataUriError::TooLarge)
        );
    }

    #[test]
    fn test_custom_allowed_types() {
        let types = &["image/png", "image/gif"];
        assert!(validate_image_data_uri("data:image/gif;base64,abc", MAX_LEN, types).is_ok());
        assert_eq!(
            validate_image_data_uri("data:image/jpeg;base64,abc", MAX_LEN, types),
            Err(DataUriError::UnsupportedType)
        );
    }
}
