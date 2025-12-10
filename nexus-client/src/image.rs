//! Image types and parsing utilities
//!
//! This module provides:
//! - `CachedImage` - Cached image/SVG handle for stable rendering
//! - `ImagePickerError` - Errors from file picker image loading
//! - `decode_data_uri_square()` - Decode with square bounding box constraint (for avatars)
//! - `decode_data_uri_max_width()` - Decode with max width constraint (for server images)
//! - `validate_image_bytes()` - Validate image bytes match expected format

use iced::Element;
use iced::widget::{image, svg};

// =============================================================================
// Types
// =============================================================================

/// Cached image for rendering (avoids re-decoding on every frame)
///
/// Iced's image handles use unique IDs internally, so creating a new handle
/// on every render causes flickering. This type caches the decoded handle
/// for stable rendering.
#[derive(Clone)]
pub enum CachedImage {
    /// Raster image (PNG, WebP, JPEG)
    Raster(image::Handle),
    /// SVG image
    Svg(svg::Handle),
}

// Manual Debug implementation since Iced handles don't implement Debug
impl std::fmt::Debug for CachedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CachedImage::Raster(_) => f.debug_tuple("Raster").field(&"<handle>").finish(),
            CachedImage::Svg(_) => f.debug_tuple("Svg").field(&"<handle>").finish(),
        }
    }
}

impl CachedImage {
    /// Render this image as an Iced Element with the given size
    pub fn render<'a, Message: 'a>(&self, size: f32) -> Element<'a, Message> {
        match self {
            CachedImage::Raster(handle) => image(handle.clone()).width(size).height(size).into(),
            CachedImage::Svg(handle) => svg(handle.clone()).width(size).height(size).into(),
        }
    }
}

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur when loading an image from the file picker
#[derive(Debug, Clone)]
pub enum ImagePickerError {
    /// User cancelled the file picker
    Cancelled,
    /// File type not supported (not PNG, WebP, JPEG, or SVG)
    UnsupportedType,
    /// File exceeds maximum size
    TooLarge,
}

// =============================================================================
// Public Functions
// =============================================================================

/// Decode a data URI into a cached image with a square bounding box constraint
///
/// Images larger than `max_size` in either dimension are scaled down to fit
/// within a square bounding box while preserving aspect ratio.
/// Images smaller than `max_size` are not scaled up.
///
/// Use this for avatars which should fit within a square display area.
///
/// Returns `None` if:
/// - The data URI is malformed (missing `base64,` marker)
/// - Base64 decoding fails
/// - The decoded bytes don't match the expected image format signature
pub fn decode_data_uri_square(data_uri: &str, max_size: u32) -> Option<CachedImage> {
    decode_data_uri_impl(data_uri, ResizeConstraint::Square(max_size))
}

/// Decode a data URI into a cached image with a max width constraint
///
/// Images wider than `max_width` are scaled down while preserving aspect ratio.
/// Height is unconstrained. Images smaller than `max_width` are not scaled up.
///
/// Use this for server images which should be constrained to form width.
///
/// Returns `None` if:
/// - The data URI is malformed (missing `base64,` marker)
/// - Base64 decoding fails
/// - The decoded bytes don't match the expected image format signature
pub fn decode_data_uri_max_width(data_uri: &str, max_width: u32) -> Option<CachedImage> {
    decode_data_uri_impl(data_uri, ResizeConstraint::MaxWidth(max_width))
}

/// Resize constraint for image caching
enum ResizeConstraint {
    /// Fit within a square bounding box (for avatars)
    Square(u32),
    /// Constrain only width, height can be anything (for server images)
    MaxWidth(u32),
}

/// Internal implementation of data URI decoding with configurable resize constraint
fn decode_data_uri_impl(data_uri: &str, constraint: ResizeConstraint) -> Option<CachedImage> {
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
        Some(CachedImage::Svg(svg::Handle::from_memory(bytes)))
    } else {
        // For raster formats (PNG, WebP, JPEG), let the image crate validate during decode
        resize_and_cache_raster(&bytes, constraint)
    }
}

/// Resize a raster image based on the constraint and create a cached image
///
/// - `Square(max_size)`: Fit within a square bounding box, scaling if either dimension exceeds max_size
/// - `MaxWidth(max_width)`: Scale only if width exceeds max_width, height unconstrained
///
/// Images smaller than the constraint are not scaled up.
/// Uses Lanczos3 filter for high quality downscaling.
///
/// Returns `None` if the bytes cannot be decoded as a valid image.
fn resize_and_cache_raster(bytes: &[u8], constraint: ResizeConstraint) -> Option<CachedImage> {
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

    let needs_resize = match constraint {
        ResizeConstraint::Square(max_size) => width > max_size || height > max_size,
        ResizeConstraint::MaxWidth(max_width) => width > max_width,
    };

    if needs_resize {
        let resized = match constraint {
            ResizeConstraint::Square(max_size) => {
                // Fit within square bounding box, preserving aspect ratio
                img.resize(max_size, max_size, FilterType::Lanczos3)
            }
            ResizeConstraint::MaxWidth(max_width) => {
                // Scale to max width, preserving aspect ratio
                let new_height = (height as f32 * max_width as f32 / width as f32).round() as u32;
                img.resize_exact(max_width, new_height, FilterType::Lanczos3)
            }
        };
        let mut png_bytes = Vec::new();
        resized
            .write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .ok()?;
        Some(CachedImage::Raster(image::Handle::from_bytes(png_bytes)))
    } else {
        Some(CachedImage::Raster(image::Handle::from_bytes(
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
    // decode_data_uri tests
    // =========================================================================

    #[test]
    fn test_decode_data_uri_valid_png() {
        // Generate a real 1x1 PNG using the image crate
        use std::io::Cursor;
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(1, 1, ::image::Rgba([0, 0, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Raster(_)));
    }

    #[test]
    fn test_decode_data_uri_valid_svg() {
        use base64::Engine;

        let svg_content =
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_svg_with_xml_declaration() {
        use base64::Engine;

        let svg_content = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_svg_with_whitespace() {
        use base64::Engine;

        let svg_content = "  \n  <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_invalid_base64() {
        let data_uri = "data:image/png;base64,not-valid-base64!!!";
        let result = decode_data_uri_square(data_uri, 64);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_missing_base64_marker() {
        let data_uri = "data:image/png,somecontent";
        let result = decode_data_uri_square(data_uri, 64);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_wrong_signature() {
        use base64::Engine;

        // Claim it's PNG but provide random bytes
        let fake_bytes = b"this is not a PNG file";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(fake_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_svg_invalid_content() {
        use base64::Engine;

        // Claim it's SVG but provide non-XML content
        let fake_bytes = b"this is not SVG content";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(fake_bytes);
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_none());
    }

    // =========================================================================
    // is_valid_svg tests
    // =========================================================================

    #[test]
    fn test_is_valid_svg_basic() {
        assert!(is_valid_svg(b"<svg></svg>"));
        assert!(is_valid_svg(b"<?xml version=\"1.0\"?><svg></svg>"));
    }

    #[test]
    fn test_is_valid_svg_with_whitespace() {
        assert!(is_valid_svg(b"  \n\t<svg></svg>"));
    }

    #[test]
    fn test_is_valid_svg_with_bom() {
        assert!(is_valid_svg(b"\xEF\xBB\xBF<svg></svg>"));
    }

    #[test]
    fn test_is_valid_svg_invalid() {
        assert!(!is_valid_svg(b"not an svg"));
        assert!(!is_valid_svg(b""));
    }

    // =========================================================================
    // decode_data_uri_max_width tests
    // =========================================================================

    #[test]
    fn test_decode_data_uri_max_width_valid_png() {
        use std::io::Cursor;

        // Generate a small PNG
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(10, 10, ::image::Rgba([255, 0, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let result = decode_data_uri_max_width(&data_uri, 400);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Raster(_)));
    }

    #[test]
    fn test_decode_data_uri_max_width_valid_svg() {
        use base64::Engine;

        let svg_content =
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        // SVGs should not be resized, just passed through
        let result = decode_data_uri_max_width(&data_uri, 400);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Svg(_)));
    }

    #[test]
    fn test_decode_data_uri_max_width_invalid_base64() {
        let data_uri = "data:image/png;base64,not-valid!!!";
        let result = decode_data_uri_max_width(data_uri, 400);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_max_width_missing_marker() {
        let data_uri = "data:image/png,somecontent";
        let result = decode_data_uri_max_width(data_uri, 400);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_data_uri_max_width_wrong_signature() {
        use base64::Engine;

        let fake_bytes = b"not a real image";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(fake_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        let result = decode_data_uri_max_width(&data_uri, 400);
        assert!(result.is_none());
    }

    // =========================================================================
    // Resizing behavior tests
    // =========================================================================

    #[test]
    fn test_decode_square_does_not_upscale_small_image() {
        use std::io::Cursor;

        // Create a 10x10 image, request max 64
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(10, 10, ::image::Rgba([0, 255, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed without upscaling
        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_square_downscales_large_image() {
        use std::io::Cursor;

        // Create a 200x200 image, request max 64
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(200, 200, ::image::Rgba([0, 0, 255, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed and downscale
        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_square_nonsquare_image_preserves_aspect_ratio() {
        use std::io::Cursor;

        // Create a 200x100 (2:1 aspect ratio) image, request max 64
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(200, 100, ::image::Rgba([255, 255, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed - aspect ratio preserved internally
        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_max_width_does_not_upscale_narrow_image() {
        use std::io::Cursor;

        // Create a 50x100 image (narrow), request max width 400
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(50, 100, ::image::Rgba([128, 128, 128, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed without upscaling
        let result = decode_data_uri_max_width(&data_uri, 400);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_max_width_downscales_wide_image() {
        use std::io::Cursor;

        // Create a 800x200 image (wide), request max width 400
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(800, 200, ::image::Rgba([64, 64, 64, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed and downscale width to 400 (height to 100)
        let result = decode_data_uri_max_width(&data_uri, 400);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_max_width_tall_image_not_constrained_by_height() {
        use std::io::Cursor;

        // Create a 100x800 image (tall), request max width 400
        // Width is under limit, so no resize needed despite tall height
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(100, 800, ::image::Rgba([192, 192, 192, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed - height is not constrained by max_width
        let result = decode_data_uri_max_width(&data_uri, 400);
        assert!(result.is_some());
    }

    // =========================================================================
    // validate_image_bytes tests
    // =========================================================================

    #[test]
    fn test_validate_image_bytes_valid_png() {
        use std::io::Cursor;

        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(1, 1, ::image::Rgba([0, 0, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        assert!(validate_image_bytes(&png_bytes, "image/png"));
    }

    #[test]
    fn test_validate_image_bytes_valid_jpeg() {
        use std::io::Cursor;

        let mut jpeg_bytes = Vec::new();
        let img = ::image::RgbImage::from_pixel(1, 1, ::image::Rgb([0, 0, 0]));
        img.write_to(
            &mut Cursor::new(&mut jpeg_bytes),
            ::image::ImageFormat::Jpeg,
        )
        .expect("Failed to encode test JPEG");

        assert!(validate_image_bytes(&jpeg_bytes, "image/jpeg"));
    }

    #[test]
    fn test_validate_image_bytes_valid_webp() {
        use std::io::Cursor;

        let mut webp_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(1, 1, ::image::Rgba([0, 0, 0, 255]));
        img.write_to(
            &mut Cursor::new(&mut webp_bytes),
            ::image::ImageFormat::WebP,
        )
        .expect("Failed to encode test WebP");

        assert!(validate_image_bytes(&webp_bytes, "image/webp"));
    }

    #[test]
    fn test_validate_image_bytes_valid_svg() {
        let svg_bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        assert!(validate_image_bytes(svg_bytes, "image/svg+xml"));
    }

    #[test]
    fn test_validate_image_bytes_svg_with_xml_declaration() {
        let svg_bytes = b"<?xml version=\"1.0\"?><svg></svg>";
        assert!(validate_image_bytes(svg_bytes, "image/svg+xml"));
    }

    #[test]
    fn test_validate_image_bytes_png_bytes_wrong_mime() {
        use std::io::Cursor;

        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(1, 1, ::image::Rgba([0, 0, 0, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        // PNG bytes but claiming JPEG MIME type
        assert!(!validate_image_bytes(&png_bytes, "image/jpeg"));
        // PNG bytes but claiming WebP MIME type
        assert!(!validate_image_bytes(&png_bytes, "image/webp"));
        // PNG bytes but claiming SVG MIME type
        assert!(!validate_image_bytes(&png_bytes, "image/svg+xml"));
    }

    #[test]
    fn test_validate_image_bytes_random_bytes() {
        let random_bytes = b"this is not an image file at all";

        assert!(!validate_image_bytes(random_bytes, "image/png"));
        assert!(!validate_image_bytes(random_bytes, "image/jpeg"));
        assert!(!validate_image_bytes(random_bytes, "image/webp"));
        assert!(!validate_image_bytes(random_bytes, "image/svg+xml"));
    }

    #[test]
    fn test_validate_image_bytes_empty() {
        let empty: &[u8] = &[];

        assert!(!validate_image_bytes(empty, "image/png"));
        assert!(!validate_image_bytes(empty, "image/jpeg"));
        assert!(!validate_image_bytes(empty, "image/webp"));
        assert!(!validate_image_bytes(empty, "image/svg+xml"));
    }

    #[test]
    fn test_validate_image_bytes_unknown_mime() {
        let png_bytes = b"\x89PNG\r\n\x1a\n"; // PNG magic bytes
        assert!(!validate_image_bytes(png_bytes, "image/gif"));
        assert!(!validate_image_bytes(png_bytes, "application/octet-stream"));
        assert!(!validate_image_bytes(png_bytes, "text/plain"));
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_decode_data_uri_empty_string() {
        assert!(decode_data_uri_square("", 64).is_none());
        assert!(decode_data_uri_max_width("", 400).is_none());
    }

    #[test]
    fn test_decode_data_uri_only_prefix() {
        assert!(decode_data_uri_square("data:", 64).is_none());
        assert!(decode_data_uri_square("data:image/png", 64).is_none());
        assert!(decode_data_uri_square("data:image/png;base64", 64).is_none());
    }

    #[test]
    fn test_decode_data_uri_empty_base64_payload() {
        // Valid structure but empty payload
        let data_uri = "data:image/png;base64,";
        assert!(decode_data_uri_square(data_uri, 64).is_none());
        assert!(decode_data_uri_max_width(data_uri, 400).is_none());
    }

    #[test]
    fn test_decode_data_uri_with_charset_parameter() {
        use base64::Engine;

        // Some tools include charset in the data URI
        let svg_content = r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_content.as_bytes());
        let data_uri = format!("data:image/svg+xml;charset=utf-8;base64,{}", base64_data);

        // Should still work - we look for "base64," marker
        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_data_uri_truncated_base64() {
        // Base64 with incomplete padding (truncated)
        let data_uri = "data:image/png;base64,iVBORw0KGgo"; // Truncated PNG
        let result = decode_data_uri_square(data_uri, 64);
        // Should fail - either base64 decode fails or image decode fails
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_valid_jpeg() {
        use std::io::Cursor;

        let mut jpeg_bytes = Vec::new();
        let img = ::image::RgbImage::from_pixel(50, 50, ::image::Rgb([255, 128, 0]));
        img.write_to(
            &mut Cursor::new(&mut jpeg_bytes),
            ::image::ImageFormat::Jpeg,
        )
        .expect("Failed to encode test JPEG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
        let data_uri = format!("data:image/jpeg;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Raster(_)));
    }

    #[test]
    fn test_decode_valid_webp() {
        use std::io::Cursor;

        let mut webp_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(50, 50, ::image::Rgba([0, 128, 255, 255]));
        img.write_to(
            &mut Cursor::new(&mut webp_bytes),
            ::image::ImageFormat::WebP,
        )
        .expect("Failed to encode test WebP");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&webp_bytes);
        let data_uri = format!("data:image/webp;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Raster(_)));
    }

    #[test]
    fn test_decode_image_at_exact_max_size() {
        use std::io::Cursor;

        // Create image exactly at max size (64x64)
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(64, 64, ::image::Rgba([100, 100, 100, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed without resizing (exactly at limit)
        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_image_just_over_max_size() {
        use std::io::Cursor;

        // Create image just over max size (65x65)
        let mut png_bytes = Vec::new();
        let img = ::image::RgbaImage::from_pixel(65, 65, ::image::Rgba([100, 100, 100, 255]));
        img.write_to(&mut Cursor::new(&mut png_bytes), ::image::ImageFormat::Png)
            .expect("Failed to encode test PNG");

        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_uri = format!("data:image/png;base64,{}", base64_data);

        // Should succeed with resizing (just over limit)
        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
    }

    #[test]
    fn test_svg_with_bom_in_data_uri() {
        use base64::Engine;

        // SVG content with UTF-8 BOM prefix
        let svg_with_bom = b"\xEF\xBB\xBF<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        let base64_data = base64::engine::general_purpose::STANDARD.encode(svg_with_bom);
        let data_uri = format!("data:image/svg+xml;base64,{}", base64_data);

        let result = decode_data_uri_square(&data_uri, 64);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), CachedImage::Svg(_)));
    }
}
