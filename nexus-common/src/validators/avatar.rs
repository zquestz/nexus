//! Avatar validation (128KB max, data URI format).

use super::data_uri::{ALLOWED_IMAGE_MIME_TYPES, DataUriError, validate_image_data_uri};
#[cfg(feature = "image-decode")]
use super::{ImageDecodeError, ImageDecodeProfile, validate_image_data_uri_decodes};

/// Maximum length of avatar data URI (128KB binary + base64 overhead + prefix).
pub const MAX_AVATAR_DATA_URI_LENGTH: usize = 176_000;

/// Avatar validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarError {
    TooLarge,
    InvalidFormat,
    UnsupportedType,
    /// Passed shape/MIME/size checks but the base64 payload doesn't decode to a
    /// valid image. Only produced when the `image-decode` feature is enabled.
    Undecodable,
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

/// Validate an avatar data URI: shape, MIME allowlist, and size always; plus a
/// real decode (raster via `image`, SVG parsed via `usvg`) when the `image-decode`
/// feature is enabled, so a well-formed-but-undecodable avatar is rejected at the
/// boundary rather than reaching a client that can't render it.
///
/// # Examples
///
/// ```
/// use nexus_common::validators::{validate_avatar, AvatarError};
///
/// // A complete SVG document (also decodes under the `image-decode` feature).
/// let svg = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiPjwvc3ZnPg==";
/// assert!(validate_avatar(svg).is_ok());
/// assert_eq!(
///     validate_avatar("data:image/gif;base64,R0lGODlh"),
///     Err(AvatarError::UnsupportedType)
/// );
/// ```
pub fn validate_avatar(avatar: &str) -> Result<(), AvatarError> {
    validate_image_data_uri(avatar, MAX_AVATAR_DATA_URI_LENGTH, ALLOWED_IMAGE_MIME_TYPES)?;
    #[cfg(feature = "image-decode")]
    validate_avatar_decodes(avatar)?;
    Ok(())
}

/// Confirm the avatar payload actually decodes, so the client never receives a
/// well-formed-but-undecodable avatar (the shape check alone would accept e.g.
/// `data:image/png;base64,` with an empty payload). Raster formats get a real
/// decode via the `image` crate, and SVG a real parse via `usvg` — both the same
/// engines the client renders with, so the server accepts exactly what the client
/// can display. `validate_image_data_uri` has already confirmed the `;base64,`
/// marker and an allowed MIME type.
#[cfg(feature = "image-decode")]
fn validate_avatar_decodes(avatar: &str) -> Result<(), AvatarError> {
    validate_image_data_uri_decodes(avatar, ImageDecodeProfile::Avatar).map_err(|err| match err {
        ImageDecodeError::InvalidFormat => AvatarError::InvalidFormat,
        ImageDecodeError::Undecodable => AvatarError::Undecodable,
    })
}

/// Whether bytes look like an SVG document — starts with `<`, allowing a leading
/// UTF-8 BOM, surrounding whitespace, and `<?xml` declarations. Shared with the
/// client's render path so both sides agree on what counts as a valid SVG.
pub fn is_valid_svg(bytes: &[u8]) -> bool {
    // Skip a UTF-8 BOM if present.
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };

    bytes
        .iter()
        .find(|&&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r')
        .is_some_and(|&b| b == b'<')
}

#[cfg(test)]
mod tests {
    use super::*;

    // A complete SVG document, valid with or without the `image-decode` feature
    // (shape-only accepts it; usvg parses it). `<svg>` alone would pass shape-only
    // but fail the usvg parse, so the fixture must be a real document.
    const VALID_SVG: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiPjwvc3ZnPg==";

    #[test]
    fn test_valid_svg_accepted() {
        assert!(validate_avatar(VALID_SVG).is_ok());
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
    fn test_size_checked_before_decode() {
        // Size is rejected before any decode, so this holds regardless of feature.
        let prefix = "data:image/png;base64,";
        let over_limit = format!("{}{}", prefix, "A".repeat(MAX_AVATAR_DATA_URI_LENGTH));
        assert_eq!(validate_avatar(&over_limit), Err(AvatarError::TooLarge));
    }

    #[test]
    fn test_is_valid_svg() {
        assert!(is_valid_svg(b"<svg></svg>"));
        assert!(is_valid_svg(b"  \n\t <svg/>"));
        assert!(is_valid_svg(b"<?xml version=\"1.0\"?><svg/>"));
        assert!(is_valid_svg(&[0xEF, 0xBB, 0xBF, b'<', b's'])); // leading BOM
        assert!(!is_valid_svg(b"not svg"));
        assert!(!is_valid_svg(b""));
    }

    #[cfg(feature = "image-decode")]
    mod decode {
        use super::*;

        fn real_png_data_uri(width: u32, height: u32) -> String {
            use base64::Engine;
            use std::io::Cursor;
            let mut png = Vec::new();
            image::RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 255]))
                .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .expect("encode test PNG");
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&png)
            )
        }

        #[test]
        fn test_real_raster_decodes() {
            assert!(validate_avatar(&real_png_data_uri(1, 1)).is_ok());
        }

        #[test]
        fn test_real_svg_decodes() {
            use base64::Engine;

            // A complete SVG document parses.
            assert!(validate_avatar(VALID_SVG).is_ok());

            // Text must not fail the parse even with no fonts available (the server
            // runs an empty fontdb) — font availability is a render concern, not a
            // validity one.
            let text_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text x="0" y="5">hi</text></svg>"#;
            let uri = format!(
                "data:image/svg+xml;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(text_svg)
            );
            assert!(
                validate_avatar(&uri).is_ok(),
                "text SVG should validate with an empty fontdb"
            );
        }

        #[test]
        fn test_rejects_well_formed_but_undecodable() {
            for uri in [
                "data:image/png;base64,",                  // empty payload
                "data:image/png;base64,iVBORw0KGgo=",      // truncated PNG
                "data:image/jpeg;base64,bm90IGFuIGltYWdl", // "not an image"
                "data:image/svg+xml;base64,bm90IHN2Zw==",  // "not svg" — not XML
                "data:image/svg+xml;base64,PHN2Zz4=",      // "<svg>" — incomplete, usvg rejects
            ] {
                assert_eq!(
                    validate_avatar(uri),
                    Err(AvatarError::Undecodable),
                    "should reject: {uri}"
                );
            }
        }

        #[test]
        fn test_rejects_raster_over_avatar_dimensions() {
            assert_eq!(
                validate_avatar(&real_png_data_uri(513, 1)),
                Err(AvatarError::Undecodable)
            );
            assert_eq!(
                validate_avatar(&real_png_data_uri(1, 513)),
                Err(AvatarError::Undecodable)
            );
        }
    }

    #[cfg(not(feature = "image-decode"))]
    #[test]
    fn test_shape_only_accepts_undecodable_without_feature() {
        // Without decode validation, shape-valid-but-undecodable URIs are accepted.
        for uri in [
            "data:image/png;base64,",
            "data:image/png;base64,iVBORw0KGgo=",
            "data:image/webp;base64,UklGRh4=",
            "data:image/jpeg;base64,/9j/4AAQ",
            "data:image/svg+xml;base64,PHN2Zz4=", // bare "<svg>": usvg rejects, shape-only accepts
        ] {
            assert!(validate_avatar(uri).is_ok());
        }
    }
}
