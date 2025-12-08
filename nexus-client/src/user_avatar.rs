//! Avatar handle types and utilities for user avatars
//!
//! This module provides:
//! - `CachedAvatar` - Cached image/SVG handle for stable rendering
//! - `generate_identicon()` - Generate identicon from username
//! - `decode_data_uri()` - Decode data URI to cached handle
//! - `get_or_create_avatar()` - Get cached avatar or create one
//! - `compute_avatar_hash()` - Compute hash for efficient change detection

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use iced::Element;
use iced::widget::{image, svg};

use crate::style::AVATAR_MAX_CACHE_SIZE;

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
    /// Raster image (PNG, WebP, JPEG)
    Image(image::Handle),
    /// SVG image
    Svg(svg::Handle),
}

// Manual Debug implementation since Iced handles don't implement Debug
impl std::fmt::Debug for CachedAvatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CachedAvatar::Image(_) => f.debug_tuple("Image").field(&"<handle>").finish(),
            CachedAvatar::Svg(_) => f.debug_tuple("Svg").field(&"<handle>").finish(),
        }
    }
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

/// Compute a SHA-256 hash of an avatar data URI for efficient change detection
///
/// This allows storing a 32-byte hash instead of the full data URI (up to 176KB)
/// in the client's internal `UserInfo` struct. The hash is only used for comparing
/// whether an avatar has changed, not for rendering.
///
/// Uses SHA-256 for consistency with certificate fingerprinting elsewhere in the codebase.
///
/// Returns `None` for `None` input, allowing direct comparison of `Option<[u8; 32]>` values.
pub fn compute_avatar_hash(avatar_data_uri: Option<&str>) -> Option<[u8; 32]> {
    avatar_data_uri.map(|uri| Sha256::digest(uri.as_bytes()).into())
}

/// Get or create a cached avatar for a user
///
/// This function manages the avatar cache:
/// - If the user's avatar is already cached, returns a clone
/// - If the user has a custom avatar (data URI), decodes and caches it
/// - If decoding fails or no avatar, generates and caches an identicon
///
/// The cache key is the username (case-sensitive, matching server behavior).
pub fn get_or_create_avatar(
    cache: &mut HashMap<String, CachedAvatar>,
    username: &str,
    avatar_data_uri: Option<&str>,
) -> CachedAvatar {
    // Check if already cached
    if let Some(cached) = cache.get(username) {
        return cached.clone();
    }

    // Try to decode custom avatar, fall back to identicon
    let avatar = avatar_data_uri
        .and_then(decode_data_uri)
        .unwrap_or_else(|| generate_identicon(username));

    // Cache and return
    cache.insert(username.to_string(), avatar.clone());
    avatar
}

/// Decode a data URI into a cached avatar
///
/// Supports:
/// - `data:image/png;base64,...`
/// - `data:image/webp;base64,...`
/// - `data:image/jpeg;base64,...`
/// - `data:image/svg+xml;base64,...`
///
/// Raster images (PNG, WebP, JPEG) are resized to a maximum of 64x64 pixels
/// to save memory. SVGs are not resized (vector graphics scale without loss).
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
        // Quick sanity check: SVG should start with '<' (possibly after whitespace/BOM)
        if !is_valid_svg(&bytes) {
            return None;
        }
        Some(CachedAvatar::Svg(svg::Handle::from_memory(bytes)))
    } else {
        // For raster formats (PNG, WebP, JPEG), let the image crate validate during decode
        resize_and_cache_raster(&bytes)
    }
}

/// Resize a raster image to the max cache size and create a cached avatar
///
/// If the image is already smaller than the max size, it's used as-is.
/// The image is resized to fit within a 64x64 box while preserving aspect ratio,
/// using Lanczos3 filter for high quality downscaling.
///
/// Returns `None` if the bytes cannot be decoded as a valid image.
fn resize_and_cache_raster(bytes: &[u8]) -> Option<CachedAvatar> {
    use ::image::ImageReader;
    use ::image::imageops::FilterType;
    use std::io::Cursor;

    // Try to load and decode the image (validates format via image crate)
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;

    let (width, height) = (img.width(), img.height());
    let max_size = AVATAR_MAX_CACHE_SIZE;

    // Only resize if larger than max size
    // resize() preserves aspect ratio, fitting within the given bounds
    if width > max_size || height > max_size {
        let resized = img.resize(max_size, max_size, FilterType::Lanczos3);
        let mut png_bytes = Vec::new();
        resized
            .write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .ok()?;
        Some(CachedAvatar::Image(image::Handle::from_bytes(png_bytes)))
    } else {
        Some(CachedAvatar::Image(image::Handle::from_bytes(
            bytes.to_vec(),
        )))
    }
}

// =============================================================================
// Validation Helpers
// =============================================================================

/// Validate that image bytes match the expected format based on MIME type
///
/// Returns `true` if the bytes are valid for the given MIME type.
/// Supported MIME types: `image/png`, `image/webp`, `image/jpeg`, `image/svg+xml`
///
/// Uses the `image` crate for raster format validation (magic byte detection).
/// SVG is validated by checking for XML-like content (starts with `<`).
pub fn validate_image_bytes(bytes: &[u8], mime_type: &str) -> bool {
    match mime_type {
        "image/png" => matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::Png)),
        "image/webp" => matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::WebP)),
        "image/jpeg" => matches!(::image::guess_format(bytes), Ok(::image::ImageFormat::Jpeg)),
        "image/svg+xml" => is_valid_svg(bytes),
        _ => false,
    }
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

    // Find first non-whitespace byte and check if it's '<'
    bytes
        .iter()
        .find(|&&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r')
        .is_some_and(|&b| b == b'<')
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
    // compute_avatar_hash tests
    // =========================================================================

    #[test]
    fn test_compute_avatar_hash_none_returns_none() {
        assert_eq!(compute_avatar_hash(None), None);
    }

    #[test]
    fn test_compute_avatar_hash_some_returns_some() {
        let hash = compute_avatar_hash(Some("data:image/png;base64,iVBORw0KGgo="));
        assert!(hash.is_some());
    }

    #[test]
    fn test_compute_avatar_hash_deterministic() {
        let uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB";
        let hash1 = compute_avatar_hash(Some(uri));
        let hash2 = compute_avatar_hash(Some(uri));
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_avatar_hash_different_input_different_hash() {
        let hash1 = compute_avatar_hash(Some("data:image/png;base64,AAAA"));
        let hash2 = compute_avatar_hash(Some("data:image/png;base64,BBBB"));
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_avatar_hash_empty_string() {
        // Empty string should still produce a hash (different from None)
        let hash = compute_avatar_hash(Some(""));
        assert!(hash.is_some());
    }

    // =========================================================================
    // get_or_create_avatar tests
    // =========================================================================

    #[test]
    fn test_get_or_create_avatar_no_avatar_generates_identicon() {
        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", None);
        assert!(matches!(avatar, CachedAvatar::Image(_)));
        assert!(cache.contains_key("testuser"));
    }

    #[test]
    fn test_get_or_create_avatar_caches_result() {
        let mut cache = HashMap::new();

        // First call creates and caches
        let _avatar1 = get_or_create_avatar(&mut cache, "alice", None);
        assert_eq!(cache.len(), 1);

        // Second call returns cached (doesn't add new entry)
        let _avatar2 = get_or_create_avatar(&mut cache, "alice", None);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_get_or_create_avatar_different_users() {
        let mut cache = HashMap::new();

        get_or_create_avatar(&mut cache, "alice", None);
        get_or_create_avatar(&mut cache, "bob", None);

        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key("alice"));
        assert!(cache.contains_key("bob"));
    }

    #[test]
    fn test_get_or_create_avatar_with_valid_png() {
        use base64::Engine;

        let mut cache = HashMap::new();

        // Create a minimal valid PNG
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

        let avatar = get_or_create_avatar(&mut cache, "alice", Some(&data_uri));
        assert!(matches!(avatar, CachedAvatar::Image(_)));
    }

    #[test]
    fn test_get_or_create_avatar_invalid_uri_falls_back_to_identicon() {
        let mut cache = HashMap::new();

        // Invalid data URI should fall back to identicon
        let avatar = get_or_create_avatar(&mut cache, "alice", Some("not-a-valid-data-uri"));
        assert!(matches!(avatar, CachedAvatar::Image(_)));
        assert!(cache.contains_key("alice"));
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
    // is_valid_svg tests (our own logic, not delegated to image crate)
    // =========================================================================

    #[test]
    fn test_is_valid_svg_basic() {
        assert!(is_valid_svg(b"<svg></svg>"));
        assert!(is_valid_svg(b"<?xml version=\"1.0\"?><svg></svg>"));
    }

    #[test]
    fn test_is_valid_svg_with_whitespace() {
        assert!(is_valid_svg(b"  <svg></svg>"));
        assert!(is_valid_svg(b"\n\t<svg></svg>"));
    }

    #[test]
    fn test_is_valid_svg_with_bom() {
        assert!(is_valid_svg(&[0xEF, 0xBB, 0xBF, b'<', b's', b'v', b'g']));
    }

    #[test]
    fn test_is_valid_svg_invalid() {
        assert!(!is_valid_svg(b"not svg"));
        assert!(!is_valid_svg(b""));
    }
}
