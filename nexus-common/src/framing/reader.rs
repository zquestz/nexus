//! Frame reader for parsing protocol messages from a stream

use std::io;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::timeout;

use super::error::FrameError;
use super::frame::RawFrame;
use super::limits::{is_known_message_type, max_payload_for_type};
use super::message_id::MessageId;
use super::{
    DELIMITER, MAGIC, MAX_PAYLOAD_LENGTH, MAX_PAYLOAD_LENGTH_DIGITS, MAX_TYPE_LENGTH,
    MAX_TYPE_LENGTH_DIGITS, MSG_ID_LENGTH, TERMINATOR,
};

/// Default timeout for completing a frame once the first byte is received
pub const DEFAULT_FRAME_TIMEOUT: Duration = Duration::from_secs(60);

/// Default idle timeout for transfer connections (waiting for first byte)
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

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
    ///
    /// # Note
    ///
    /// This method has no timeout - it will wait indefinitely for data.
    /// For production use, prefer [`read_frame_with_timeout`](Self::read_frame_with_timeout).
    pub async fn read_frame(&mut self) -> Result<Option<RawFrame>, FrameError> {
        // Step 1: Read the first byte of magic
        let first_byte = match self.read_byte_allow_eof().await? {
            Some(b) => b,
            None => return Ok(None), // Clean disconnect
        };

        // Complete the frame (no timeout)
        self.read_frame_after_first_byte(first_byte).await
    }

    /// Read the next frame from the stream with a timeout
    ///
    /// This method waits indefinitely for the first byte (allowing idle connections),
    /// but once the first byte is received, the entire frame must complete within
    /// the specified timeout.
    ///
    /// Returns `Ok(None)` if the connection is cleanly closed.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The frame is malformed
    /// - An I/O error occurs
    /// - The frame doesn't complete within the timeout after the first byte
    pub async fn read_frame_with_timeout(
        &mut self,
        frame_timeout: Duration,
    ) -> Result<Option<RawFrame>, FrameError> {
        // Wait indefinitely for the first byte (allows idle connections)
        let first_byte = match self.read_byte_allow_eof().await? {
            Some(b) => b,
            None => return Ok(None), // Clean disconnect
        };

        // Once we have the first byte, apply timeout for the rest of the frame
        match timeout(frame_timeout, self.read_frame_after_first_byte(first_byte)).await {
            Ok(result) => result,
            Err(_) => Err(FrameError::FrameTimeout),
        }
    }

    /// Read the next frame from the stream with a full timeout (including idle wait)
    ///
    /// Unlike [`read_frame_with_timeout`](Self::read_frame_with_timeout), this method
    /// applies a timeout to the entire read operation, including waiting for the first byte.
    /// This is appropriate for protocols where idle connections should be disconnected,
    /// such as the file transfer port.
    ///
    /// Returns `Ok(None)` if the connection is cleanly closed.
    ///
    /// # Arguments
    ///
    /// * `idle_timeout` - Maximum time to wait for the first byte
    /// * `frame_timeout` - Maximum time to complete the frame after the first byte
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The frame is malformed
    /// - An I/O error occurs
    /// - No data is received within `idle_timeout`
    /// - The frame doesn't complete within `frame_timeout` after the first byte
    pub async fn read_frame_with_full_timeout(
        &mut self,
        idle_timeout: Duration,
        frame_timeout: Duration,
    ) -> Result<Option<RawFrame>, FrameError> {
        // Apply timeout waiting for the first byte (no idle connections allowed)
        let first_byte = match timeout(idle_timeout, self.read_byte_allow_eof()).await {
            Ok(Ok(Some(b))) => b,
            Ok(Ok(None)) => return Ok(None), // Clean disconnect
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(FrameError::IdleTimeout),
        };

        // Once we have the first byte, apply timeout for the rest of the frame
        match timeout(frame_timeout, self.read_frame_after_first_byte(first_byte)).await {
            Ok(result) => result,
            Err(_) => Err(FrameError::FrameTimeout),
        }
    }

    /// Complete reading a frame after the first byte has been received
    ///
    /// This is the core frame parsing logic, called after we've received the first byte.
    async fn read_frame_after_first_byte(
        &mut self,
        first_byte: u8,
    ) -> Result<Option<RawFrame>, FrameError> {
        // Step 1: Complete reading magic bytes (we already have the first one)
        // Expected: 'N', 'X', '|'
        if first_byte != MAGIC[0] {
            return Err(FrameError::InvalidMagic);
        }

        let mut magic_rest = [0u8; 2];
        self.reader.read_exact(&mut magic_rest).await?;
        if magic_rest != MAGIC[1..] {
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

    /// Read a single byte, returning None on clean EOF
    async fn read_byte_allow_eof(&mut self) -> Result<Option<u8>, FrameError> {
        let mut buf = [0u8; 1];
        match self.reader.read_exact(&mut buf).await {
            Ok(_) => Ok(Some(buf[0])),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
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
    use tokio::io::{AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn test_frame_reader_valid_frame() {
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|20|{\"message\":\"Hello!\"}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "ChatSend");
        assert_eq!(
            frame.message_id,
            MessageId::from_bytes(b"a1b2c3d4e5f6").unwrap()
        );
        assert_eq!(frame.payload, b"{\"message\":\"Hello!\"}");
    }

    #[tokio::test]
    async fn test_frame_reader_empty_payload() {
        let data = b"NX|8|UserList|a1b2c3d4e5f6|0|\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "UserList");
        assert!(frame.payload.is_empty());
    }

    #[tokio::test]
    async fn test_frame_reader_multiple_frames() {
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|2|{}\nNX|8|UserList|b2c3d4e5f6a1|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame1 = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame1.message_type, "ChatSend");

        let frame2 = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame2.message_type, "UserList");
    }

    #[tokio::test]
    async fn test_frame_reader_connection_closed() {
        let data = b"";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_frame_reader_invalid_magic() {
        let data = b"XX|8|ChatSend|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidMagic)));
    }

    #[tokio::test]
    async fn test_frame_reader_invalid_message_id() {
        let data = b"NX|8|ChatSend|not_hex_chars|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidMessageId)));
    }

    #[tokio::test]
    async fn test_frame_reader_type_length_zero() {
        let data = b"NX|0||a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::TypeLengthOutOfRange)));
    }

    #[tokio::test]
    async fn test_frame_reader_type_length_too_many_digits() {
        // 4 digits before delimiter
        let data = b"NX|1234|X|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::TypeLengthTooManyDigits)));
    }

    #[tokio::test]
    async fn test_frame_reader_payload_length_too_many_digits() {
        // 11 digits before delimiter
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|12345678901|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::PayloadLengthTooManyDigits)
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_before_terminator() {
        // Missing newline at end
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|2|{}";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn test_frame_reader_wrong_terminator() {
        // Wrong terminator (space instead of newline)
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|2|{} ";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::MissingTerminator)));
    }

    #[tokio::test]
    async fn test_frame_reader_payload_exceeds_type_max() {
        // ChatSend has a limit of 1056 bytes, try to send more
        // Create a payload that claims to be 2000 bytes
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|2000|";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length: 2000,
                max: 1056
            }) if message_type == "ChatSend"
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_zero_length_payload() {
        let data = b"NX|8|UserList|a1b2c3d4e5f6|0|\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.payload.len(), 0);
    }

    #[tokio::test]
    async fn test_frame_reader_longest_known_type() {
        // ChatTopicUpdateResponse is 23 characters
        let data = b"NX|23|ChatTopicUpdateResponse|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "ChatTopicUpdateResponse");
    }

    #[tokio::test]
    async fn test_frame_reader_payload_at_type_limit() {
        // Handshake has a limit of 65 bytes
        // Create exactly 65 bytes of payload
        let payload = format!("{{\"version\":\"{}\"}}", "x".repeat(65 - 14));
        assert_eq!(payload.len(), 65);
        let data = format!("NX|9|Handshake|a1b2c3d4e5f6|65|{}\n", payload);

        let cursor = Cursor::new(data.as_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.payload.len(), 65);
    }

    #[tokio::test]
    async fn test_frame_reader_payload_one_over_type_limit() {
        // Handshake has a limit of 65 bytes
        // Create 66 bytes of payload (one over limit)
        let payload = format!("{{\"version\":\"{}\"}}", "x".repeat(66 - 14));
        assert_eq!(payload.len(), 66);
        let data = format!("NX|9|Handshake|a1b2c3d4e5f6|66|{}\n", payload);

        let cursor = Cursor::new(data.as_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length: 66,
                max: 65
            }) if message_type == "Handshake"
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_magic() {
        // Only partial magic bytes
        let data = b"NX";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        // Should get ConnectionClosed because EOF in middle of frame
        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_type() {
        let data = b"NX|8|Chat"; // Type should be 8 bytes but only 4 provided
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_message_id() {
        let data = b"NX|8|ChatSend|a1b2c3"; // Message ID should be 12 bytes
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn test_frame_reader_eof_mid_payload() {
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|10|short"; // Payload should be 10 bytes
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn test_frame_reader_rejects_unknown_type() {
        let data = b"NX|11|UnknownType|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::UnknownMessageType(t)) if t == "UnknownType"
        ));
    }

    #[tokio::test]
    async fn test_frame_reader_unlimited_payload_type() {
        // UserListResponse has no limit (0 = unlimited)
        // Create a large payload
        let payload = format!("{{\"users\":[{}]}}", "\"x\",".repeat(1000));
        let data = format!(
            "NX|16|UserListResponse|a1b2c3d4e5f6|{}|{}\n",
            payload.len(),
            payload
        );

        let cursor = Cursor::new(data.as_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "UserListResponse");
        assert_eq!(frame.payload.len(), payload.len());
    }

    #[tokio::test]
    async fn test_frame_reader_with_timeout_valid_frame() {
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|20|{\"message\":\"Hello!\"}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader
            .read_frame_with_timeout(DEFAULT_FRAME_TIMEOUT)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.message_type, "ChatSend");
    }

    #[tokio::test]
    async fn test_frame_reader_with_timeout_clean_disconnect() {
        let data = b"";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader
            .read_frame_with_timeout(DEFAULT_FRAME_TIMEOUT)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_frame_reader_with_timeout_frame_timeout() {
        use tokio::io::duplex;

        // Create a duplex stream where we control both ends
        let (client, server) = duplex(64);
        let buf_reader = BufReader::new(server);
        let mut reader = FrameReader::new(buf_reader);

        // Write the first byte to start the frame
        let mut client = client;
        client.write_all(b"N").await.unwrap();

        // Now try to read with a very short timeout - should timeout
        // because the rest of the frame never arrives
        let result = reader
            .read_frame_with_timeout(Duration::from_millis(10))
            .await;
        assert!(matches!(result, Err(FrameError::FrameTimeout)));
    }

    #[tokio::test]
    async fn test_frame_reader_with_timeout_completes_before_timeout() {
        use tokio::io::duplex;

        // Create a duplex stream
        let (client, server) = duplex(256);
        let buf_reader = BufReader::new(server);
        let mut reader = FrameReader::new(buf_reader);

        // Spawn a task to write the frame with a small delay between parts
        let mut client = client;
        tokio::spawn(async move {
            client.write_all(b"NX|8|ChatSend|").await.unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
            client
                .write_all(b"a1b2c3d4e5f6|20|{\"message\":\"Hello!\"}\n")
                .await
                .unwrap();
        });

        // Should complete successfully within the timeout
        let frame = reader
            .read_frame_with_timeout(Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.message_type, "ChatSend");
    }

    #[tokio::test]
    async fn test_frame_reader_with_full_timeout_valid_frame() {
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|20|{\"message\":\"Hello!\"}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader
            .read_frame_with_full_timeout(DEFAULT_IDLE_TIMEOUT, DEFAULT_FRAME_TIMEOUT)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.message_type, "ChatSend");
    }

    #[tokio::test]
    async fn test_frame_reader_with_full_timeout_clean_disconnect() {
        let data = b"";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader
            .read_frame_with_full_timeout(DEFAULT_IDLE_TIMEOUT, DEFAULT_FRAME_TIMEOUT)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_frame_reader_with_full_timeout_idle_timeout() {
        use tokio::io::duplex;

        // Create a duplex stream where we control both ends
        let (_client, server) = duplex(64);
        let buf_reader = BufReader::new(server);
        let mut reader = FrameReader::new(buf_reader);

        // Don't write anything - should hit idle timeout waiting for first byte
        let result = reader
            .read_frame_with_full_timeout(Duration::from_millis(10), DEFAULT_FRAME_TIMEOUT)
            .await;
        assert!(matches!(result, Err(FrameError::IdleTimeout)));
    }

    #[tokio::test]
    async fn test_frame_reader_with_full_timeout_frame_timeout() {
        use tokio::io::duplex;

        // Create a duplex stream where we control both ends
        let (client, server) = duplex(64);
        let buf_reader = BufReader::new(server);
        let mut reader = FrameReader::new(buf_reader);

        // Write the first byte to pass idle timeout, but don't complete the frame
        let mut client = client;
        client.write_all(b"N").await.unwrap();

        // Should pass idle timeout but fail frame timeout
        let result = reader
            .read_frame_with_full_timeout(Duration::from_secs(1), Duration::from_millis(10))
            .await;
        assert!(matches!(result, Err(FrameError::FrameTimeout)));
    }
}
