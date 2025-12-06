//! Protocol framing for Nexus BBS
//!
//! This module implements the Nexus Protocol Frame Format v2:
//!
//! ```text
//! NX|<type_length>|<object_type>|<msg_id>|<payload_length>|<json_payload>\n
//! ```
//!
//! ## Field Specification
//!
//! | Field | Format | Size | Description |
//! |-------|--------|------|-------------|
//! | Magic | `NX\|` | 3 bytes (literal) | Protocol identifier |
//! | Type Length | ASCII decimal | 1-3 digits (1-999) | Length of object type string |
//! | Delimiter | `\|` | 1 byte (literal) | Field separator |
//! | Object Type | ASCII string | N bytes | Message type e.g. `ChatSend` |
//! | Delimiter | `\|` | 1 byte (literal) | Field separator |
//! | Message ID | hex string | 12 bytes | Sender-generated, echoed in response |
//! | Delimiter | `\|` | 1 byte (literal) | Field separator |
//! | Payload Length | ASCII decimal | 1-10 digits | Length of JSON payload |
//! | Delimiter | `\|` | 1 byte (literal) | Field separator |
//! | JSON Payload | UTF-8 JSON | M bytes | Message data |
//! | Terminator | `\n` | 1 byte (literal) | Message terminator |
//!
//! ## Example
//!
//! ```text
//! NX|9|Handshake|a1b2c3d4e5f6|20|{"version":"0.4.0"}
//! ```

mod error;
mod frame;
mod limits;
mod message_id;
mod reader;
mod writer;

// Re-export public types
pub use error::FrameError;
pub use frame::RawFrame;
pub use limits::{is_known_message_type, known_message_types, max_payload_for_type};
pub use message_id::MessageId;
pub use reader::FrameReader;
pub use writer::FrameWriter;

// =============================================================================
// Constants
// =============================================================================

/// Magic bytes that identify a Nexus protocol frame
pub const MAGIC: &[u8] = b"NX|";

/// Delimiter character between frame fields
pub const DELIMITER: u8 = b'|';

/// Message terminator
pub const TERMINATOR: u8 = b'\n';

/// Length of the message ID in hex characters
pub const MSG_ID_LENGTH: usize = 12;

/// Maximum digits for type length field (1-999)
pub const MAX_TYPE_LENGTH_DIGITS: usize = 3;

/// Maximum type name length
pub const MAX_TYPE_LENGTH: usize = 999;

/// Maximum digits for payload length field (sanity check ~10GB)
pub const MAX_PAYLOAD_LENGTH_DIGITS: usize = 10;

/// Maximum payload length (sanity check, per-type limits are enforced separately)
pub const MAX_PAYLOAD_LENGTH: u64 = 9_999_999_999;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_frame_roundtrip() {
        let id = MessageId::new();
        let payload = b"{\"version\":\"0.4.0\"}".to_vec();

        // Write a frame
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            let frame = RawFrame::new(id, "Handshake".to_string(), payload.clone());
            writer.write_frame(&frame).await.unwrap();
        }

        // Read it back
        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, "Handshake");
        assert_eq!(frame.payload, payload);
    }
}
