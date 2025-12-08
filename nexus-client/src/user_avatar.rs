//! Avatar handle types and utilities for user avatars
//!
//! This module provides:
//! - `CachedAvatar` - Cached image/SVG handle for stable rendering
//! - `generate_identicon()` - Generate identicon from username
//! - `decode_data_uri()` - Decode data URI to cached handle

use iced::Element;
use iced::widget::{image, svg};

// =============================================================================
// Types
// =============================================================================

/// Cached avatar for rendering (avoids re-decoding on every frame)
///
/// Iced's image handles use unique IDs internally, so creating a new handle
/// on every render causes flickering. This type caches the decoded handle
/// for stable rendering.
#[derive(Clone)]
pub enum CachedAvatar {
    /// Raster image (PNG, WebP)
    Image(image::Handle),
    /// SVG image
    Svg(svg::Handle),
}

impl CachedAvatar {
    /// Render this avatar as an Iced Element with the given size
    pub fn render<'a, Message: 'a>(&self, size: f32) -> Element<'a, Message> {
        match self {
            CachedAvatar::Image(handle) => image(handle.clone()).width(size).height(size).into(),
            CachedAvatar::Svg(handle) => svg(handle.clone()).width(size).height(size).into(),
        }
    }
}

// =============================================================================
// Public Functions
// =============================================================================

/// Generate an identicon for a given seed string (e.g., username)
///
/// Returns a cached avatar that can be used for rendering.
/// The identicon is deterministic - the same seed always produces
/// the same image.
///
/// # Panics
///
/// This function uses `expect()` internally because identicon generation
/// from a string seed is a deterministic operation that cannot fail under
/// normal circumstances. The underlying library creates a simple grid-based
/// PNG image from a hash of the input string - there are no I/O operations
/// or external dependencies that could cause failure.
pub fn generate_identicon(seed: &str) -> CachedAvatar {
    let identicon = identicon_rs::Identicon::new(seed);
    let png_data = identicon
        .export_png_data()
        .expect("Identicon PNG generation from string seed should not fail");
    CachedAvatar::Image(image::Handle::from_bytes(png_data))
}

/// Decode a data URI into a cached avatar
///
/// Supports:
/// - `data:image/png;base64,...`
/// - `data:image/webp;base64,...`
/// - `data:image/svg+xml;base64,...`
///
/// Returns `None` if:
/// - The data URI is malformed (missing `base64,` marker)
/// - Base64 decoding fails
/// - The decoded bytes don't match the expected image format signature
pub fn decode_data_uri(data_uri: &str) -> Option<CachedAvatar> {
    use base64::Engine;

    let base64_start = data_uri.find("base64,")?;
    let base64_data = &data_uri[base64_start + 7..];
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .ok()?;

    if data_uri.starts_with("data:image/svg+xml") {
        // Validate SVG: should start with '<' or '<?xml'
        // (may have leading whitespace or BOM)
        if !is_valid_svg(&bytes) {
            return None;
        }
        Some(CachedAvatar::Svg(svg::Handle::from_memory(bytes)))
    } else if data_uri.starts_with("data:image/png") {
        if !is_valid_png(&bytes) {
            return None;
        }
        Some(CachedAvatar::Image(image::Handle::from_bytes(bytes)))
    } else if data_uri.starts_with("data:image/webp") {
        if !is_valid_webp(&bytes) {
            return None;
        }
        Some(CachedAvatar::Image(image::Handle::from_bytes(bytes)))
    } else {
        // Unknown image type - try to detect from bytes
        if is_valid_png(&bytes) || is_valid_webp(&bytes) {
            Some(CachedAvatar::Image(image::Handle::from_bytes(bytes)))
        } else if is_valid_svg(&bytes) {
            Some(CachedAvatar::Svg(svg::Handle::from_memory(bytes)))
        } else {
            None
        }
    }
}

// =============================================================================
// Validation Helpers
// =============================================================================

/// Validate that image bytes match the expected format based on MIME type
///
/// Returns `true` if the bytes are valid for the given MIME type.
/// Supported MIME types: `image/png`, `image/webp`, `image/svg+xml`
///
/// Uses the `image` crate for PNG/WebP validation (magic byte detection).
/// SVG is validated by checking for XML-like content (starts with `<`).
pub fn validate_image_bytes(bytes: &[u8], mime_type: &str) -> bool {
    match mime_type {
        "image/png" => matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::Png)),
        "image/webp" => matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::WebP)),
        "image/svg+xml" => is_valid_svg(bytes),
        _ => false,
    }
}

/// Check if bytes represent a valid PNG file
fn is_valid_png(bytes: &[u8]) -> bool {
    matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::Png))
}

/// Check if bytes represent a valid WebP file
fn is_valid_webp(bytes: &[u8]) -> bool {
    matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::WebP))
}

/// Check if bytes represent a valid SVG file
///
/// SVG files should start with '<' (possibly after whitespace or BOM).
/// We also accept '<?xml' declarations.
fn is_valid_svg(bytes: &[u8]) -> bool {
    // Skip UTF-8 BOM if present
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };

    // Skip leading whitespace
    let trimmed = bytes
        .iter()
        .skip_while(|&&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r')
        .copied()
        .collect::<Vec<_>>();

    // Should start with '<' (either '<svg' or '<?xml')
    trimmed.first() == Some(&b'<')
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // generate_identicon tests
    // =========================================================================

    #[test]
    fn test_generate_identicon_returns_image() {
        let handle = generate_identicon("testuser");
        assert!(matches!(handle, CachedAvatar::Image(_)));
    }

    #[test]
    fn test_generate_identicon_deterministic() {
        // Same seed should produce same result
        // We can't compare handles directly, but we can verify both succeed
        let handle1 = generate_identicon("alice");
        let handle2 = generate_identicon("alice");
        assert!(matches!(handle1, CachedAvatar::Image(_)));
        assert!(matches!(handle2, CachedAvatar::Image(_)));
    }

    #[test]
    fn test_generate_identicon_different_seeds() {
        // Different seeds should both succeed
        let handle1 = generate_identicon("alice");
        let handle2 = generate_identicon("bob");
        assert!(matches!(handle1, CachedAvatar::Image(_)));
        assert!(matches!(handle2, CachedAvatar::Image(_)));
    }

    #[test]
    fn test_generate_identicon_empty_seed() {
        // Empty string should still work
        let handle = generate_identicon("");
        assert!(matches!(handle, CachedAvatar::Image(_)));
    }

    #[test]
    fn test_generate_identicon_unicode_seed() {
        // Unicode characters should work
        let handle = generate_identicon("日本語ユーザー");
        assert!(matches!(handle, CachedAvatar::Image(_)));
    }

    // =========================================================================
    // decode_data_uri tests
    // =========================================================================

    #[test]
    fn test_decode_data_uri_valid_png() {
        use base64::Engine;

        // Create a minimal valid PNG (1x1 transparent pixel)
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
            0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, // IDAT chunk
            0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D,
            0xB4, 0x00, // IEND chunk
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let base64_data = base64::engine::general_purpose::STANDARD.encode(png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let result = decode_data_uri(&data_uri);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedAvatar::Image(_)));
    }

    #[test]
    fn test_decode_data_uri_valid_svg() {
        use base64::Engine;

        let svg_content =
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri(&data_uri);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedAvatar::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_svg_with_xml_declaration() {
        use base64::Engine;

        let svg_content = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri(&data_uri);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedAvatar::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_svg_with_whitespace() {
        use base64::Engine;

        let svg_content = "  \n  <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri(&data_uri);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedAvatar::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_invalid_base64() {
        let data_uri = "data:image/png;base64,not-valid-base64!!!";
        let result = decode_data_uri(data_uri);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_missing_base64_marker() {
        let data_uri = "data:image/png,somecontent";
        let result = decode_data_uri(data_uri);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_wrong_signature() {
        use base64::Engine;

        // Claim it's PNG but provide random bytes
        let fake_bytes = b"this is not a PNG file";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(fake_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let result = decode_data_uri(&data_uri);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_svg_invalid_content() {
        use base64::Engine;

        // Claim it's SVG but provide non-XML content
        let fake_bytes = b"this is not SVG content";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(fake_bytes);
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri(&data_uri);
        assert!(result.is_none());
    }

    // =========================================================================
    // validate_image_bytes tests
    // =========================================================================

    #[test]
    fn test_validate_image_bytes_png() {
        let valid_png = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert!(validate_image_bytes(valid_png, "image/png"));
        assert!(!validate_image_bytes(valid_png, "image/webp"));
        assert!(!validate_image_bytes(valid_png, "image/svg+xml"));

        let invalid_png = b"not a png";
        assert!(!validate_image_bytes(invalid_png, "image/png"));
    }

    #[test]
    fn test_validate_image_bytes_webp() {
        let valid_webp = b"RIFF\x00\x00\x00\x00WEBP";
        assert!(validate_image_bytes(valid_webp, "image/webp"));
        assert!(!validate_image_bytes(valid_webp, "image/png"));
        assert!(!validate_image_bytes(valid_webp, "image/svg+xml"));

        let invalid_webp = b"not a webp file";
        assert!(!validate_image_bytes(invalid_webp, "image/webp"));
    }

    #[test]
    fn test_validate_image_bytes_svg() {
        assert!(validate_image_bytes(b"<svg></svg>", "image/svg+xml"));
        assert!(validate_image_bytes(
            b"<?xml version=\"1.0\"?><svg></svg>",
            "image/svg+xml"
        ));
        assert!(validate_image_bytes(b"  <svg></svg>", "image/svg+xml"));
        // With BOM
        assert!(validate_image_bytes(
            &[0xEF, 0xBB, 0xBF, b'<', b's', b'v', b'g'],
            "image/svg+xml"
        ));

        assert!(!validate_image_bytes(b"not svg", "image/svg+xml"));
        assert!(!validate_image_bytes(b"", "image/svg+xml"));
    }

    #[test]
    fn test_validate_image_bytes_unknown_mime() {
        let valid_png = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert!(!validate_image_bytes(valid_png, "image/jpeg"));
        assert!(!validate_image_bytes(valid_png, "image/gif"));
        assert!(!validate_image_bytes(valid_png, "text/plain"));
    }
}
