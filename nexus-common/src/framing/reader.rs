//! Frame reader for parsing protocol messages from a stream

use std::io;

use tokio::io::AsyncReadExt;

use super::error::FrameError;
use super::frame::RawFrame;
use super::limits::{is_known_message_type, max_payload_for_type};
use super::message_id::MessageId;
use super::{
    DELIMITER, MAGIC, MAX_PAYLOAD_LENGTH, MAX_PAYLOAD_LENGTH_DIGITS, MAX_TYPE_LENGTH,
    MAX_TYPE_LENGTH_DIGITS, MSG_ID_LENGTH, TERMINATOR,
};

/// Reads protocol frames from an async reader
pub struct FrameReader<R> {
    reader: R,
}

impl<R> FrameReader<R> {
    /// Create a new frame reader
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Get a reference to the underlying reader
    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    /// Get a mutable reference to the underlying reader
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Consume the frame reader and return the underlying reader
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: AsyncReadExt + Unpin> FrameReader<R> {
    /// Read the next frame from the stream
    ///
    /// Returns `Ok(None)` if the connection is cleanly closed.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame is malformed or an I/O error occurs.
    pub async fn read_frame(&mut self) -> Result<Option<RawFrame>, FrameError> {
        // Step 1: Read exactly 3 bytes — must be "NX|"
        let mut magic = [0u8; 3];
        match self.reader.read_exact(&mut magic).await {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }

        if magic != MAGIC {
            return Err(FrameError::InvalidMagic);
        }

        // Step 2: Read until "|" (max 3 digits) — parse as integer (type length, 1-999)
        let type_length = self
            .read_length_field(
                MAX_TYPE_LENGTH_DIGITS,
                FrameError::InvalidTypeLength,
                FrameError::TypeLengthTooManyDigits,
            )
            .await?;
        if type_length == 0 || type_length > MAX_TYPE_LENGTH as u64 {
            return Err(FrameError::TypeLengthOutOfRange);
        }

        // Step 3: Read exactly N bytes — object type
        let mut type_bytes = vec![0u8; type_length as usize];
        self.reader.read_exact(&mut type_bytes).await?;
        let message_type = String::from_utf8(type_bytes)
            .map_err(|_| FrameError::UnknownMessageType("<invalid utf8>".to_string()))?;

        // Step 4: Reject unknown message types early
        if !is_known_message_type(&message_type) {
            return Err(FrameError::UnknownMessageType(message_type));
        }

        // Step 5: Read exactly 1 byte — must be "|"
        let delimiter = self.read_byte().await?;
        if delimiter != DELIMITER {
            return Err(FrameError::MissingDelimiter);
        }

        // Step 6: Read exactly 12 bytes — message ID (hex)
        let mut msg_id_bytes = [0u8; MSG_ID_LENGTH];
        self.reader.read_exact(&mut msg_id_bytes).await?;
        let message_id = MessageId::from_bytes(&msg_id_bytes)?;

        // Step 7: Read exactly 1 byte — must be "|"
        let delimiter = self.read_byte().await?;
        if delimiter != DELIMITER {
            return Err(FrameError::MissingDelimiter);
        }

        // Step 8: Read until "|" (max 10 digits) — parse as integer (payload length)
        let payload_length = self
            .read_length_field(
                MAX_PAYLOAD_LENGTH_DIGITS,
                FrameError::InvalidPayloadLength,
                FrameError::PayloadLengthTooManyDigits,
            )
            .await?;
        if payload_length > MAX_PAYLOAD_LENGTH {
            return Err(FrameError::PayloadLengthTooLarge);
        }

        // Step 9: Validate payload length against per-type maximum (0 = unlimited)
        let max_for_type = max_payload_for_type(&message_type);
        if max_for_type > 0 && payload_length > max_for_type {
            return Err(FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length: payload_length,
                max: max_for_type,
            });
        }

        // Step 10: Read exactly M bytes — JSON payload
        let mut payload = vec![0u8; payload_length as usize];
        self.reader.read_exact(&mut payload).await?;

        // Step 11: Read exactly 1 byte — must be "\n"
        let terminator = self.read_byte().await?;
        if terminator != TERMINATOR {
            return Err(FrameError::MissingTerminator);
        }

        Ok(Some(RawFrame::new(message_id, message_type, payload)))
    }

    /// Read a single byte
    async fn read_byte(&mut self) -> Result<u8, FrameError> {
        let mut buf = [0u8; 1];
        self.reader.read_exact(&mut buf).await?;
        Ok(buf[0])
    }

    /// Read a length field (digits terminated by delimiter)
    ///
    /// # Arguments
    ///
    /// * `max_digits` - Maximum number of digits allowed
    /// * `invalid_err` - Error to return if the field is invalid (empty, non-digit, unparseable)
    /// * `too_many_err` - Error to return if the field exceeds max_digits
    async fn read_length_field(
        &mut self,
        max_digits: usize,
        invalid_err: FrameError,
        too_many_err: FrameError,
    ) -> Result<u64, FrameError> {
        let mut digits = Vec::with_capacity(max_digits);

        for _ in 0..=max_digits {
            let byte = self.read_byte().await?;

            if byte == DELIMITER {
                // Parse the accumulated digits
                if digits.is_empty() {
                    return Err(invalid_err);
                }
                let s = std::str::from_utf8(&digits).map_err(|_| invalid_err.clone())?;
                return s.parse().map_err(|_| invalid_err.clone());
            }

            if !byte.is_ascii_digit() {
                return Err(invalid_err);
            }

            digits.push(byte);
        }

        // If we get here, we read max_digits + 1 without finding a delimiter
        Err(too_many_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_frame_reader_valid_frame() {
        let id = MessageId::new();
        let payload = b"{\"message\":\"Hello!\"}";
        let data = format!(
            "NX|8|ChatSend|{}|{}|{}\n",
            id,
            payload.len(),
            String::from_utf8_lossy(payload)
        );
        let cursor = Cursor::new(data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, "ChatSend");
        assert_eq!(frame.payload, payload);
    }

    #[tokio::test]
    async fn test_frame_reader_empty_payload() {
        let id = MessageId::new();
        let data = format!("NX|8|UserList|{}|2|{{}}\n", id);
        let cursor = Cursor::new(data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, "UserList");
        assert_eq!(frame.payload, b"{}");
    }

    #[tokio::test]
    async fn test_frame_reader_multiple_frames() {
        let id1 = MessageId::new();
        let id2 = MessageId::new();
        let data = format!("NX|5|Login|{}|2|{{}}\nNX|8|ChatSend|{}|2|{{}}\n", id1, id2);
        let cursor = Cursor::new(data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame1 = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame1.message_id, id1);
        assert_eq!(frame1.message_type, "Login");

        let frame2 = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame2.message_id, id2);
        assert_eq!(frame2.message_type, "ChatSend");
    }

    #[tokio::test]
    async fn test_frame_reader_connection_closed() {
        let data = b"";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_frame_reader_invalid_magic() {
        let data = b"XX|8|ChatSend|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::InvalidMagic));
    }

    #[tokio::test]
    async fn test_frame_reader_invalid_message_id() {
        let data = b"NX|8|ChatSend|not_hex_here|2|{}\n";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::InvalidMessageId));
    }

    #[tokio::test]
    async fn test_frame_reader_type_length_zero() {
        let data = b"NX|0||a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::TypeLengthOutOfRange));
    }

    #[tokio::test]
    async fn test_frame_reader_type_length_too_many_digits() {
        // Type length "1000" has 4 digits which exceeds max 3 digits
        let data = b"NX|1000|SomeType|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::TypeLengthTooManyDigits));
    }

    #[tokio::test]
    async fn test_frame_reader_payload_length_too_many_digits() {
        // Payload length has 11 digits which exceeds max 10 digits
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|12345678901|{}\n";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::PayloadLengthTooManyDigits));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_before_terminator() {
        // Connection closes before the terminator byte is received
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|2|{}";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::ConnectionClosed));
    }

    #[tokio::test]
    async fn test_frame_reader_wrong_terminator() {
        // Wrong terminator byte (space instead of newline)
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|2|{} ";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::MissingTerminator));
    }

    #[tokio::test]
    async fn test_frame_reader_payload_exceeds_type_max() {
        let id = MessageId::new();
        // ChatSend has a ~1KB limit, try to send more
        let large_payload = format!("{{\"message\":\"{}\"}}", "x".repeat(3000));
        let frame_data = format!(
            "NX|8|ChatSend|{}|{}|{}\n",
            id,
            large_payload.len(),
            large_payload
        );
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::PayloadLengthExceedsTypeMax { .. })
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_zero_length_payload() {
        let id = MessageId::new();
        let data = format!("NX|8|UserList|{}|0|\n", id);
        let cursor = Cursor::new(data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, "UserList");
        assert_eq!(frame.payload, b"");
    }

    #[tokio::test]
    async fn test_frame_reader_longest_known_type() {
        let id = MessageId::new();
        let msg_type = "ChatTopicUpdateResponse";
        let data = format!("NX|{}|{}|{}|2|{{}}\n", msg_type.len(), msg_type, id);
        let cursor = Cursor::new(data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, msg_type);
        assert_eq!(frame.message_type.len(), 23);
    }

    #[tokio::test]
    async fn test_frame_reader_payload_at_type_limit() {
        let id = MessageId::new();
        // Payload exactly at the per-type limit (Handshake = 65 bytes)
        // {"version":""} is 14 chars, so we need 51 x's to make 65
        let payload = format!("{{\"version\":\"{}\"}}", "x".repeat(51));
        assert_eq!(payload.len(), 65);
        let frame_data = format!("NX|9|Handshake|{}|{}|{}\n", id, payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, "Handshake");
        assert_eq!(frame.payload.len(), 65);
    }

    #[tokio::test]
    async fn test_frame_reader_payload_one_over_type_limit() {
        let id = MessageId::new();
        // Payload one byte over the per-type limit (Handshake = 65 bytes)
        // {"version":""} is 14 chars, so we need 52 x's to make 66
        let payload = format!("{{\"version\":\"{}\"}}", "x".repeat(52));
        assert_eq!(payload.len(), 66);
        let frame_data = format!("NX|9|Handshake|{}|{}|{}\n", id, payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length: 66,
                max: 65,
            }) if message_type == "Handshake"
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_magic() {
        // Connection closes after first byte of magic
        // This is treated as a clean close (Ok(None)) because read_exact on magic
        // returns UnexpectedEof which we interpret as connection closed
        let data = b"N";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        // Partial read during magic is treated as clean close
        assert_eq!(result, Ok(None));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_type() {
        // Connection closes while reading type name
        let data = b"NX|8|Chat";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::ConnectionClosed));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_message_id() {
        // Connection closes while reading message ID
        let data = b"NX|8|ChatSend|a1b2c3";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::ConnectionClosed));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_payload() {
        // Connection closes while reading payload
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|10|hello";
        let cursor = Cursor::new(data.to_vec());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert_eq!(result, Err(FrameError::ConnectionClosed));
    }

    #[tokio::test]
    async fn test_frame_reader_rejects_unknown_type() {
        let id = MessageId::new();
        let unknown_type = "UnknownType";
        let frame_data = format!("NX|{}|{}|{}|2|{{}}\n", unknown_type.len(), unknown_type, id);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::UnknownMessageType(t)) if t == unknown_type
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_unlimited_payload_type() {
        let id = MessageId::new();
        // UserListResponse has limit=0 (unlimited), so large payloads should be accepted
        let large_payload = "x".repeat(50000);
        let payload = format!("{{\"success\":true,\"users\":[],\"padding\":\"{large_payload}\"}}");
        let frame_data = format!(
            "NX|16|UserListResponse|{}|{}|{}\n",
            id,
            payload.len(),
            payload
        );
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_id, id);
        assert_eq!(frame.message_type, "UserListResponse");
    }
}
