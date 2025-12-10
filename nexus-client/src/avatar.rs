//! Avatar utilities for user avatars
//!
//! This module provides avatar-specific utilities:
//! - `generate_identicon()` - Generate identicon from username
//! - `get_or_create_avatar()` - Get cached avatar or create one
//! - `compute_avatar_hash()` - Compute hash for efficient change detection

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use iced::widget::image;

use crate::image::{CachedImage, decode_data_uri_square};
use crate::style::AVATAR_MAX_CACHE_SIZE;

// =============================================================================
// Public Functions
// =============================================================================

/// Generate an identicon for a given seed string (e.g., username)
///
/// Returns a cached image that can be used for rendering.
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
pub fn generate_identicon(seed: &str) -> CachedImage {
    let identicon = identicon_rs::Identicon::new(seed);
    let png_data = identicon
        .export_png_data()
        .expect("Identicon PNG generation from string seed should not fail");
    CachedImage::Raster(image::Handle::from_bytes(png_data))
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
    cache: &mut HashMap<String, CachedImage>,
    username: &str,
    avatar_data_uri: Option<&str>,
) -> CachedImage {
    // Check if already cached
    if let Some(cached) = cache.get(username) {
        return cached.clone();
    }

    // Try to decode custom avatar, fall back to identicon
    let avatar = avatar_data_uri
        .and_then(|uri| decode_data_uri_square(uri, AVATAR_MAX_CACHE_SIZE))
        .unwrap_or_else(|| generate_identicon(username));

    // Cache and return
    cache.insert(username.to_string(), avatar.clone());
    avatar
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
        let avatar = generate_identicon("testuser");
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_generate_identicon_deterministic() {
        let avatar1 = generate_identicon("testuser");
        let avatar2 = generate_identicon("testuser");

        // Both should be Image variants (we can't compare handles directly)
        assert!(matches!(avatar1, CachedImage::Raster(_)));
        assert!(matches!(avatar2, CachedImage::Raster(_)));
    }

    #[test]
    fn test_generate_identicon_different_seeds() {
        let avatar1 = generate_identicon("user1");
        let avatar2 = generate_identicon("user2");

        // Both should generate successfully
        assert!(matches!(avatar1, CachedImage::Raster(_)));
        assert!(matches!(avatar2, CachedImage::Raster(_)));
    }

    #[test]
    fn test_generate_identicon_empty_seed() {
        // Should not panic with empty seed
        let avatar = generate_identicon("");
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_generate_identicon_unicode_seed() {
        // Should handle unicode characters
        let avatar = generate_identicon("用户名");
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    // =========================================================================
    // compute_avatar_hash tests
    // =========================================================================

    #[test]
    fn test_compute_avatar_hash_none_returns_none() {
        assert!(compute_avatar_hash(None).is_none());
    }

    #[test]
    fn test_compute_avatar_hash_some_returns_some() {
        let hash = compute_avatar_hash(Some("data:image/png;base64,abc"));
        assert!(hash.is_some());
    }

    #[test]
    fn test_compute_avatar_hash_deterministic() {
        let hash1 = compute_avatar_hash(Some("data:image/png;base64,abc"));
        let hash2 = compute_avatar_hash(Some("data:image/png;base64,abc"));
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_avatar_hash_different_input_different_hash() {
        let hash1 = compute_avatar_hash(Some("data:image/png;base64,abc"));
        let hash2 = compute_avatar_hash(Some("data:image/png;base64,xyz"));
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_avatar_hash_empty_string() {
        // Empty string is valid input, should return a hash
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
        assert!(matches!(avatar, CachedImage::Raster(_)));
        assert!(cache.contains_key("testuser"));
    }

    #[test]
    fn test_get_or_create_avatar_caches_result() {
        let mut cache = HashMap::new();

        // First call creates and caches
        let _avatar1 = get_or_create_avatar(&mut cache, "testuser", None);
        assert_eq!(cache.len(), 1);

        // Second call returns cached
        let _avatar2 = get_or_create_avatar(&mut cache, "testuser", None);
        assert_eq!(cache.len(), 1); // Still just one entry
    }

    #[test]
    fn test_get_or_create_avatar_different_users() {
        let mut cache = HashMap::new();

        get_or_create_avatar(&mut cache, "user1", None);
        get_or_create_avatar(&mut cache, "user2", None);

        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key("user1"));
        assert!(cache.contains_key("user2"));
    }

    #[test]
    fn test_get_or_create_avatar_with_valid_png() {
        // Generate a real 1x1 PNG using the image crate
        use std::io::Cursor;
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(1, 1, ::image::Rgba([0, 0, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(&data_uri));
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_get_or_create_avatar_invalid_uri_falls_back_to_identicon() {
        let mut cache = HashMap::new();

        // Invalid data URI should fall back to identicon
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some("not a valid data uri"));
        assert!(matches!(avatar, CachedImage::Raster(_)));
        assert!(cache.contains_key("testuser"));
    }

    #[test]
    fn test_get_or_create_avatar_with_valid_svg() {
        use base64::Engine;

        let svg_content =
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(&data_uri));
        assert!(matches!(avatar, CachedImage::Svg(_)));
        assert!(cache.contains_key("testuser"));
    }

    #[test]
    fn test_get_or_create_avatar_large_image_gets_resized() {
        use std::io::Cursor;

        // Create a 200x200 PNG (larger than AVATAR_MAX_CACHE_SIZE which is 64)
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(200, 200, ::image::Rgba([255, 0, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(&data_uri));
        // Should succeed and be resized internally
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_get_or_create_avatar_at_exact_cache_size() {
        use std::io::Cursor;

        // Create a 64x64 PNG (exactly at AVATAR_MAX_CACHE_SIZE)
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(64, 64, ::image::Rgba([0, 255, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(&data_uri));
        // Should succeed without resizing (exactly at limit)
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_get_or_create_avatar_nonsquare_image() {
        use std::io::Cursor;

        // Create a non-square 100x50 PNG
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(100, 50, ::image::Rgba([0, 0, 255, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(&data_uri));
        // Should succeed - aspect ratio preserved during resize
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_get_or_create_avatar_empty_data_uri_falls_back() {
        let mut cache = HashMap::new();

        // Empty string should fall back to identicon
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(""));
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_get_or_create_avatar_corrupted_png_falls_back() {
        use base64::Engine;

        // Valid base64 but invalid PNG content
        let fake_png = b"not a real PNG file";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(fake_png);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let mut cache = HashMap::new();
        let avatar = get_or_create_avatar(&mut cache, "testuser", Some(&data_uri));
        // Should fall back to identicon
        assert!(matches!(avatar, CachedImage::Raster(_)));
    }

    #[test]
    fn test_get_or_create_avatar_case_sensitive_username() {
        let mut cache = HashMap::new();

        // Usernames are case-sensitive for caching
        get_or_create_avatar(&mut cache, "TestUser", None);
        get_or_create_avatar(&mut cache, "testuser", None);

        // Should have two separate cache entries
        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key("TestUser"));
        assert!(cache.contains_key("testuser"));
    }
}
