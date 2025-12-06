//! Message ID for request-response correlation

use std::fmt;

use rand::Rng;

use super::MSG_ID_LENGTH;
use super::error::FrameError;

/// A 12-character hex message ID for request-response correlation
///
/// Message IDs provide 48 bits of entropy (12 hex characters × 4 bits each),
/// giving approximately 281 trillion possible values. Using the birthday paradox,
/// collision probability becomes significant around √(2^48) ≈ 16.7 million messages.
///
/// For a BBS application with per-connection ID scopes, this is more than sufficient.
/// Message IDs are used for request-response correlation only, not for security
/// purposes, so collisions have no security implications.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageId([u8; MSG_ID_LENGTH]);

impl MessageId {
    /// Generate a new random message ID
    #[must_use]
    pub fn new() -> Self {
        let mut rng = rand::rng();
        let mut bytes = [0u8; MSG_ID_LENGTH];
        const HEX_CHARS: &[u8] = b"0123456789abcdef";
        for byte in &mut bytes {
            *byte = HEX_CHARS[rng.random_range(0..16)];
        }
        Self(bytes)
    }

    /// Parse a message ID from a byte slice
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 12 valid hex characters.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FrameError> {
        if bytes.len() != MSG_ID_LENGTH {
            return Err(FrameError::InvalidMessageId);
        }

        // Validate all characters are hex
        for &b in bytes {
            if !b.is_ascii_hexdigit() {
                return Err(FrameError::InvalidMessageId);
            }
        }

        let mut id = [0u8; MSG_ID_LENGTH];
        id.copy_from_slice(bytes);
        Ok(Self(id))
    }

    /// Get the message ID as a byte slice
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Get the message ID as a string
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: We only store valid ASCII hex characters
        std::str::from_utf8(&self.0).expect("MessageId contains valid ASCII")
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MessageId({})", self.as_str())
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_new() {
        let id = MessageId::new();
        assert_eq!(id.as_bytes().len(), MSG_ID_LENGTH);

        // All bytes should be valid hex
        for &b in id.as_bytes() {
            assert!(b.is_ascii_hexdigit());
        }
    }

    #[test]
    fn test_message_id_from_bytes_roundtrip() {
        let original = MessageId::new();
        let parsed = MessageId::from_bytes(original.as_bytes()).unwrap();
        assert_eq!(parsed, original);
        assert_eq!(parsed.as_str(), original.as_str());
    }

    #[test]
    fn test_message_id_from_bytes_invalid_length() {
        let bytes = b"a1b2c3";
        assert_eq!(
            MessageId::from_bytes(bytes),
            Err(FrameError::InvalidMessageId)
        );
    }

    #[test]
    fn test_message_id_from_bytes_invalid_chars() {
        let bytes = b"a1b2c3d4e5g6"; // 'g' is not hex
        assert_eq!(
            MessageId::from_bytes(bytes),
            Err(FrameError::InvalidMessageId)
        );
    }

    #[test]
    fn test_message_id_display() {
        let id = MessageId::new();
        let display = format!("{id}");
        assert_eq!(display, id.as_str());
        assert_eq!(display.len(), MSG_ID_LENGTH);
    }

    #[test]
    fn test_message_id_uniqueness() {
        // Generate many IDs and verify they're unique
        let mut ids = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = MessageId::new();
            assert!(
                ids.insert(id.as_str().to_string()),
                "Duplicate MessageId generated"
            );
        }
    }
}
