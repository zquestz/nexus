//! Avatar validation
//!
//! Validates user avatar data URIs for the protocol. Avatars are transmitted
//! as base64-encoded data URIs with a maximum size limit.

/// Maximum length of avatar data URI string
///
/// This accommodates:
/// - 128KB binary data (MAX_AVATAR_SIZE from client)
/// - Base64 encoding overhead (~4/3 ratio): ceil(128 * 1024 * 4/3) ≈ 175,019 chars
/// - Data URI prefix (max): "data:image/svg+xml;base64," = 26 chars
/// - Rounded up for safety margin
pub const MAX_AVATAR_DATA_URI_LENGTH: usize = 176_000;

/// Allowed MIME types for avatars
const ALLOWED_MIME_TYPES: &[&str] = &["image/png", "image/webp", "image/svg+xml"];

/// Errors that can occur when validating an avatar
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarError {
    /// Avatar data URI exceeds maximum length
    TooLarge,
    /// Avatar data URI has invalid format (missing data: prefix or base64 marker)
    InvalidFormat,
    /// Avatar has unsupported MIME type
    UnsupportedType,
}

/// Validate an avatar data URI
///
/// Checks:
/// - Length does not exceed `MAX_AVATAR_DATA_URI_LENGTH`
/// - Starts with `data:` prefix
/// - Contains a supported MIME type (image/png, image/webp, image/svg+xml)
/// - Contains `;base64,` marker
///
/// # Arguments
///
/// * `avatar` - The avatar data URI string to validate
///
/// # Returns
///
/// * `Ok(())` if the avatar is valid
/// * `Err(AvatarError)` describing the validation failure
///
/// # Examples
///
/// ```
/// use nexus_common::validators::{validate_avatar, AvatarError};
///
/// // Valid PNG avatar
/// assert!(validate_avatar("data:image/png;base64,iVBORw0KGgo=").is_ok());
///
/// // Valid WebP avatar
/// assert!(validate_avatar("data:image/webp;base64,UklGR...").is_ok());
///
/// // Valid SVG avatar
/// assert!(validate_avatar("data:image/svg+xml;base64,PHN2Zz4=").is_ok());
///
/// // Invalid: unsupported type
/// assert_eq!(
///     validate_avatar("data:image/gif;base64,R0lGODlh"),
///     Err(AvatarError::UnsupportedType)
/// );
///
/// // Invalid: missing base64 marker
/// assert_eq!(
///     validate_avatar("data:image/png,raw-data"),
///     Err(AvatarError::InvalidFormat)
/// );
/// ```
pub fn validate_avatar(avatar: &str) -> Result<(), AvatarError> {
    // Check length first (cheapest check)
    if avatar.len() > MAX_AVATAR_DATA_URI_LENGTH {
        return Err(AvatarError::TooLarge);
    }

    // Must start with data: prefix
    if !avatar.starts_with("data:") {
        return Err(AvatarError::InvalidFormat);
    }

    // Must contain ;base64, marker
    let Some(base64_marker_pos) = avatar.find(";base64,") else {
        return Err(AvatarError::InvalidFormat);
    };

    // Extract MIME type (between "data:" and ";base64,")
    let mime_type = &avatar[5..base64_marker_pos];

    // Check MIME type is allowed
    if !ALLOWED_MIME_TYPES.contains(&mime_type) {
        return Err(AvatarError::UnsupportedType);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Valid avatars
    // =========================================================================

    #[test]
    fn test_valid_png_avatar() {
        let avatar = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        assert!(validate_avatar(avatar).is_ok());
    }

    #[test]
    fn test_valid_webp_avatar() {
        let avatar = "data:image/webp;base64,UklGRh4AAABXRUJQVlA4TBEAAAAvAAAAAAfQ//73v/+BiOh/AAA=";
        assert!(validate_avatar(avatar).is_ok());
    }

    #[test]
    fn test_valid_svg_avatar() {
        let avatar = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjwvc3ZnPg==";
        assert!(validate_avatar(avatar).is_ok());
    }

    #[test]
    fn test_valid_minimal_avatar() {
        // Minimal valid data URI
        let avatar = "data:image/png;base64,";
        assert!(validate_avatar(avatar).is_ok());
    }

    // =========================================================================
    // Invalid format
    // =========================================================================

    #[test]
    fn test_invalid_missing_data_prefix() {
        let avatar = "image/png;base64,iVBORw0KGgo=";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::InvalidFormat));
    }

    #[test]
    fn test_invalid_missing_base64_marker() {
        let avatar = "data:image/png,iVBORw0KGgo=";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::InvalidFormat));
    }

    #[test]
    fn test_invalid_empty_string() {
        assert_eq!(validate_avatar(""), Err(AvatarError::InvalidFormat));
    }

    #[test]
    fn test_invalid_just_data_prefix() {
        assert_eq!(validate_avatar("data:"), Err(AvatarError::InvalidFormat));
    }

    #[test]
    fn test_invalid_random_string() {
        assert_eq!(
            validate_avatar("not a data uri at all"),
            Err(AvatarError::InvalidFormat)
        );
    }

    // =========================================================================
    // Unsupported types
    // =========================================================================

    #[test]
    fn test_unsupported_gif() {
        let avatar =
            "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::UnsupportedType));
    }

    #[test]
    fn test_unsupported_jpeg() {
        let avatar = "data:image/jpeg;base64,/9j/4AAQSkZJRg==";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::UnsupportedType));
    }

    #[test]
    fn test_unsupported_bmp() {
        let avatar = "data:image/bmp;base64,Qk0=";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::UnsupportedType));
    }

    #[test]
    fn test_unsupported_text_plain() {
        let avatar = "data:text/plain;base64,SGVsbG8=";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::UnsupportedType));
    }

    #[test]
    fn test_unsupported_empty_mime_type() {
        let avatar = "data:;base64,SGVsbG8=";
        assert_eq!(validate_avatar(avatar), Err(AvatarError::UnsupportedType));
    }

    // =========================================================================
    // Size limits
    // =========================================================================

    #[test]
    fn test_avatar_at_max_length() {
        // Create an avatar exactly at the limit
        let prefix = "data:image/png;base64,";
        let padding_len = MAX_AVATAR_DATA_URI_LENGTH - prefix.len();
        let padding = "A".repeat(padding_len);
        let avatar = format!("{}{}", prefix, padding);

        assert_eq!(avatar.len(), MAX_AVATAR_DATA_URI_LENGTH);
        assert!(validate_avatar(&avatar).is_ok());
    }

    #[test]
    fn test_avatar_exceeds_max_length() {
        // Create an avatar one byte over the limit
        let prefix = "data:image/png;base64,";
        let padding_len = MAX_AVATAR_DATA_URI_LENGTH - prefix.len() + 1;
        let padding = "A".repeat(padding_len);
        let avatar = format!("{}{}", prefix, padding);

        assert_eq!(avatar.len(), MAX_AVATAR_DATA_URI_LENGTH + 1);
        assert_eq!(validate_avatar(&avatar), Err(AvatarError::TooLarge));
    }

    #[test]
    fn test_max_avatar_data_uri_length_constant() {
        // Verify the constant is set correctly
        // 128KB binary = 131072 bytes
        // Base64 encoding: ceil(131072 * 4/3) = 174763
        // Plus prefix "data:image/svg+xml;base64," = 26 chars
        // Total: ~174789, rounded up to 176000 for safety
        assert_eq!(MAX_AVATAR_DATA_URI_LENGTH, 176_000);
    }
}
