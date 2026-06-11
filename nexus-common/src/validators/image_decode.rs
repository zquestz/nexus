//! Shared image decode limits for user-supplied image data.

use std::io::Cursor;

use base64::Engine;

/// Decode profile for user-supplied images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageDecodeProfile {
    /// Avatar images: small square identity images.
    Avatar,
    /// Server images: larger banner/logo images.
    Server,
    /// News images: larger editorial images.
    News,
}

/// Concrete image decode limits for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDecodeLimits {
    pub max_width: u32,
    pub max_height: u32,
    pub max_alloc: u64,
}

impl ImageDecodeProfile {
    /// Return the decode limits for this image class.
    pub const fn limits(self) -> ImageDecodeLimits {
        match self {
            ImageDecodeProfile::Avatar => ImageDecodeLimits {
                max_width: 512,
                max_height: 512,
                max_alloc: 4 * 1024 * 1024,
            },
            ImageDecodeProfile::Server => ImageDecodeLimits {
                max_width: 2048,
                max_height: 2048,
                max_alloc: 24 * 1024 * 1024,
            },
            ImageDecodeProfile::News => ImageDecodeLimits {
                max_width: 2048,
                max_height: 4096,
                max_alloc: 32 * 1024 * 1024,
            },
        }
    }
}

/// Decode-validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageDecodeError {
    InvalidFormat,
    Undecodable,
}

/// Decode the base64 payload from an image data URI.
pub fn decode_image_data_uri_payload(data_uri: &str) -> Result<(&str, Vec<u8>), ImageDecodeError> {
    if !data_uri.starts_with("data:") {
        return Err(ImageDecodeError::InvalidFormat);
    }

    let base64_pos = data_uri
        .find(";base64,")
        .ok_or(ImageDecodeError::InvalidFormat)?;
    let mime_segment = data_uri
        .get("data:".len()..base64_pos)
        .ok_or(ImageDecodeError::InvalidFormat)?;
    let mime = mime_segment.split(';').next().unwrap_or(mime_segment);
    let payload = &data_uri[base64_pos + ";base64,".len()..];

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| ImageDecodeError::Undecodable)?;

    Ok((mime, bytes))
}

/// Validate that image bytes decode under the selected profile.
pub fn validate_image_bytes_decode(
    bytes: &[u8],
    mime: &str,
    profile: ImageDecodeProfile,
) -> Result<(), ImageDecodeError> {
    if mime == "image/svg+xml" {
        validate_svg_image_bytes(bytes)
    } else {
        validate_raster_image_bytes(bytes, profile)
    }
}

/// Validate that an image data URI payload decodes under the selected profile.
pub fn validate_image_data_uri_decodes(
    data_uri: &str,
    profile: ImageDecodeProfile,
) -> Result<(), ImageDecodeError> {
    let (mime, bytes) = decode_image_data_uri_payload(data_uri)?;
    validate_image_bytes_decode(&bytes, mime, profile)
}

/// Build image crate limits for the selected profile.
pub fn image_crate_limits(profile: ImageDecodeProfile) -> image::Limits {
    let limits = profile.limits();
    let mut image_limits = image::Limits::default();
    image_limits.max_image_width = Some(limits.max_width);
    image_limits.max_image_height = Some(limits.max_height);
    image_limits.max_alloc = Some(limits.max_alloc);
    image_limits
}

fn validate_raster_image_bytes(
    bytes: &[u8],
    profile: ImageDecodeProfile,
) -> Result<(), ImageDecodeError> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ImageDecodeError::Undecodable)?;
    reader.limits(image_crate_limits(profile));
    reader
        .decode()
        .map(|_| ())
        .map_err(|_| ImageDecodeError::Undecodable)
}

fn validate_svg_image_bytes(bytes: &[u8]) -> Result<(), ImageDecodeError> {
    // SVGs are byte-capped before this parse and are later rasterized by the
    // client at widget layout size, so profile dimensions do not map cleanly to
    // an allocation ceiling here. Validate parseability without treating the
    // document's intrinsic dimensions as raster decode dimensions.
    usvg::Tree::from_data(bytes, &usvg::Options::default())
        .map(|_| ())
        .map_err(|_| ImageDecodeError::Undecodable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_data_uri_rejects_prefixed_garbage() {
        assert_eq!(
            decode_image_data_uri_payload("xdata:image/png;base64,AAAA"),
            Err(ImageDecodeError::InvalidFormat)
        );
    }
}
