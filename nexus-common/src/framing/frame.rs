//! Raw frame structure for protocol messages

use super::message_id::MessageId;
use super::{
    DELIMITER, MAGIC, MAX_PAYLOAD_LENGTH_DIGITS, MAX_TYPE_LENGTH_DIGITS, MSG_ID_LENGTH, TERMINATOR,
};

/// A parsed protocol frame before JSON deserialization
///
/// This represents the frame structure after parsing the header fields
/// but before interpreting the JSON payload.
#[derive(Debug, Clone, PartialEq)]
pub struct RawFrame {
    /// The message ID for request-response correlation
    pub message_id: MessageId,
    /// The message type string (e.g., "ChatSend", "Login")
    pub message_type: String,
    /// The raw JSON payload bytes
    pub payload: Vec<u8>,
}

impl RawFrame {
    /// Create a new raw frame
    #[must_use]
    pub fn new(message_id: MessageId, message_type: String, payload: Vec<u8>) -> Self {
        Self {
            message_id,
            message_type,
            payload,
        }
    }

    /// Serialize the frame to bytes
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let type_len = self.message_type.len();
        let payload_len = self.payload.len();

        // Pre-calculate capacity: NX| + type_len + | + type + | + msg_id + | + payload_len + | + payload + \n
        let capacity = 3
            + MAX_TYPE_LENGTH_DIGITS
            + 1
            + type_len
            + 1
            + MSG_ID_LENGTH
            + 1
            + MAX_PAYLOAD_LENGTH_DIGITS
            + 1
            + payload_len
            + 1;

        let mut buf = Vec::with_capacity(capacity);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(type_len.to_string().as_bytes());
        buf.push(DELIMITER);
        buf.extend_from_slice(self.message_type.as_bytes());
        buf.push(DELIMITER);
        buf.extend_from_slice(self.message_id.as_bytes());
        buf.push(DELIMITER);
        buf.extend_from_slice(payload_len.to_string().as_bytes());
        buf.push(DELIMITER);
        buf.extend_from_slice(&self.payload);
        buf.push(TERMINATOR);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_frame_to_bytes() {
        let id = MessageId::new();
        let frame = RawFrame::new(
            id,
            "ChatSend".to_string(),
            b"{\"message\":\"Hello!\"}".to_vec(),
        );

        let bytes = frame.to_bytes();
        let expected = format!("NX|8|ChatSend|{}|20|{{\"message\":\"Hello!\"}}\n", id);
        assert_eq!(bytes, expected.as_bytes());
    }

    #[test]
    fn test_raw_frame_to_bytes_minimal_payload() {
        let id = MessageId::new();
        let frame = RawFrame::new(id, "UserList".to_string(), b"{}".to_vec());

        let bytes = frame.to_bytes();
        let expected = format!("NX|8|UserList|{}|2|{{}}\n", id);
        assert_eq!(bytes, expected.as_bytes());
    }

    #[test]
    fn test_raw_frame_to_bytes_empty_payload() {
        let id = MessageId::new();
        let frame = RawFrame::new(id, "UserList".to_string(), vec![]);

        let bytes = frame.to_bytes();
        let expected = format!("NX|8|UserList|{}|0|\n", id);
        assert_eq!(bytes, expected.as_bytes());
    }
}
