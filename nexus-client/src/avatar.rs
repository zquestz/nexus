//! Avatar utilities for user avatars
//!
//! This module provides avatar-specific utilities:
//! - `generate_identicon()` - Generate identicon from username
//! - `get_or_create_avatar()` - Get cached avatar or create one
//! - `compute_avatar_hash()` - Compute hash for efficient change detection
//! - `apply_avatar_update()` - Apply a `UserUpdated` avatar delta (tri-state, re-keys on rename)
//! - `avatar_cache_key()` - Folded cache key for a nickname

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use iced::widget::image;
use nexus_common::names::fold_name;
use nexus_common::validators::ImageDecodeProfile;

use crate::constants::ERR_IDENTICON_GENERATION;
use crate::image::{CachedImage, decode_data_uri_square};
use crate::style::AVATAR_MAX_CACHE_SIZE;

// =============================================================================
// Public Functions
// =============================================================================

/// Generate an identicon for a given seed string (e.g., username or nickname)
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
    let png_data = identicon.export_png_data().expect(ERR_IDENTICON_GENERATION);
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
/// The cache key is the nickname folded for case-insensitive lookups.
/// This ensures consistent avatar display regardless of nickname casing from different
/// server responses (e.g., UserConnected vs UserInfoDetailed).
pub fn get_or_create_avatar(
    cache: &mut HashMap<String, CachedImage>,
    nickname: &str,
    avatar_data_uri: Option<&str>,
) -> CachedImage {
    // Fold for case-insensitive matching.
    let cache_key = fold_name(nickname);

    // Check if already cached
    if let Some(cached) = cache.get(&cache_key) {
        return cached.clone();
    }

    // Try to decode custom avatar, fall back to identicon
    // Use original nickname for identicon seed to preserve visual consistency
    let avatar = avatar_data_uri
        .and_then(|uri| {
            decode_data_uri_square(uri, AVATAR_MAX_CACHE_SIZE, ImageDecodeProfile::Avatar)
        })
        .unwrap_or_else(|| generate_identicon(nickname));

    // Cache and return (keyed by folded nickname).
    cache.insert(cache_key, avatar.clone());
    avatar
}

/// Get the cache key for a nickname (folded for case-insensitive lookups).
pub fn avatar_cache_key(nickname: &str) -> String {
    fold_name(nickname)
}

/// Apply a `UserUpdated` avatar delta to the cache, honoring the wire rule:
/// `Some(non-empty)` = set this avatar, `Some("")` = clear (fall back to an
/// identicon), `None` = unchanged. On a rename (`old_nickname` != `new_nickname`)
/// the cache is re-keyed to the new nickname; in the unchanged (`None`) case a
/// *real* avatar is moved across (we no longer hold the data URI to re-decode it),
/// while an identicon is dropped so it regenerates from the new nickname.
/// `had_real_avatar` is whether the user had a decoded (non-identicon) avatar
/// before this update.
pub fn apply_avatar_update(
    cache: &mut HashMap<String, CachedImage>,
    old_nickname: &str,
    new_nickname: &str,
    avatar: Option<&str>,
    had_real_avatar: bool,
) {
    let old_key = avatar_cache_key(old_nickname);
    let new_key = avatar_cache_key(new_nickname);
    let renamed = old_key != new_key;

    match avatar {
        // Unchanged: keep the cached avatar. On a rename, move a real avatar to the
        // new key; an identicon is dropped so it regenerates from the new nickname.
        None => {
            if renamed {
                match cache.remove(&old_key) {
                    Some(image) if had_real_avatar => {
                        cache.insert(new_key, image);
                    }
                    _ => {}
                }
            } else if old_nickname != new_nickname && !had_real_avatar {
                // Case-only rename (same folded key, e.g. alice -> Alice): the
                // identicon is seeded from the exact nickname, so drop the stale one
                // and let the next render regenerate it from the new case.
                cache.remove(&new_key);
            }
        }
        // Cleared: drop any cached image so the view falls back to an identicon.
        Some("") => {
            cache.remove(&old_key);
            if renamed {
                cache.remove(&new_key);
            }
        }
        // Set: decode the new avatar under the new key.
        Some(data) => {
            if renamed {
                cache.remove(&old_key);
            }
            cache.remove(&new_key);
            get_or_create_avatar(cache, new_nickname, Some(data));
        }
    }
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
    fn test_get_or_create_avatar_case_insensitive_nickname() {
        let mut cache = HashMap::new();

        // Display names are case-insensitive for caching (normalized to lowercase)
        get_or_create_avatar(&mut cache, "TestUser", None);
        get_or_create_avatar(&mut cache, "testuser", None);

        // Should have one cache entry (both map to same lowercase key)
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key("testuser"));
    }

    // =========================================================================
    // apply_avatar_update tests (UserUpdated tri-state delta)
    // =========================================================================

    #[test]
    fn test_apply_avatar_update_none_same_nick_keeps_cache() {
        let mut cache = HashMap::new();
        cache.insert(avatar_cache_key("alice"), generate_identicon("alice"));

        // None = unchanged: the cache must be left exactly as-is.
        apply_avatar_update(&mut cache, "alice", "alice", None, true);

        assert!(cache.contains_key(&avatar_cache_key("alice")));
    }

    #[test]
    fn test_apply_avatar_update_none_rename_moves_real_avatar() {
        let mut cache = HashMap::new();
        cache.insert(avatar_cache_key("alice"), generate_identicon("seed"));

        // Rename with a real avatar and no new data URI: the decoded image must
        // move to the new key (it can't be re-decoded).
        apply_avatar_update(&mut cache, "alice", "alicia", None, true);

        assert!(!cache.contains_key(&avatar_cache_key("alice")));
        assert!(cache.contains_key(&avatar_cache_key("alicia")));
    }

    #[test]
    fn test_apply_avatar_update_none_rename_drops_identicon() {
        let mut cache = HashMap::new();
        cache.insert(avatar_cache_key("alice"), generate_identicon("alice"));

        // Rename of an identicon-only user: don't move it (the new nickname has a
        // different identicon); just drop the stale old key.
        apply_avatar_update(&mut cache, "alice", "alicia", None, false);

        assert!(!cache.contains_key(&avatar_cache_key("alice")));
        assert!(!cache.contains_key(&avatar_cache_key("alicia")));
    }

    #[test]
    fn test_apply_avatar_update_empty_clears() {
        let mut cache = HashMap::new();
        cache.insert(avatar_cache_key("alice"), generate_identicon("alice"));

        // "" = cleared: drop the entry so the view falls back to an identicon.
        apply_avatar_update(&mut cache, "alice", "alice", Some(""), true);

        assert!(!cache.contains_key(&avatar_cache_key("alice")));
    }

    #[test]
    fn test_apply_avatar_update_case_only_drops_identicon() {
        let mut cache = HashMap::new();
        // Identicon cached for "alice" (no real avatar), keyed by the folded name.
        get_or_create_avatar(&mut cache, "alice", None);
        assert!(cache.contains_key(&avatar_cache_key("alice")));

        // Case-only rename with no avatar change: the stale identicon (seeded from
        // the exact "alice") is dropped so it regenerates from "Alice".
        apply_avatar_update(&mut cache, "alice", "Alice", None, false);
        assert!(
            !cache.contains_key(&avatar_cache_key("Alice")),
            "case-only rename drops the name-seeded identicon"
        );

        // A real avatar is not name-seeded, so a case-only rename keeps it.
        let mut real = HashMap::new();
        real.insert(avatar_cache_key("bob"), generate_identicon("stand-in"));
        apply_avatar_update(&mut real, "bob", "Bob", None, true);
        assert!(
            real.contains_key(&avatar_cache_key("Bob")),
            "case-only rename keeps a real avatar"
        );
    }

    #[test]
    fn test_apply_avatar_update_data_repopulates() {
        let mut cache = HashMap::new();
        cache.insert(avatar_cache_key("alice"), generate_identicon("stale"));

        // Some(data) = set: the entry is invalidated and repopulated (decoded, or
        // identicon fallback when the URI is unusable — either way, present).
        apply_avatar_update(
            &mut cache,
            "alice",
            "alice",
            Some("data:image/png;base64,xxx"),
            false,
        );

        assert!(cache.contains_key(&avatar_cache_key("alice")));
    }
}
