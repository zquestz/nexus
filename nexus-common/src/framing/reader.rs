//! Frame reader for parsing protocol messages from a stream

use std::io;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

use super::error::FrameError;
use super::frame::RawFrame;
use super::limits::{is_known_message_type, max_payload_for_type};
use super::message_id::MessageId;
use super::{
    DELIMITER, MAGIC, MAX_PAYLOAD_LENGTH_DIGITS, MAX_TYPE_LENGTH, MAX_TYPE_LENGTH_DIGITS,
    MSG_ID_LENGTH, TERMINATOR,
};

/// Default timeout for completing a frame once the first byte is received
pub const DEFAULT_FRAME_TIMEOUT: Duration = Duration::from_secs(60);

/// Default idle timeout for transfer connections (waiting for first byte)
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default progress timeout for streaming transfers (must receive bytes within this time)
pub const DEFAULT_PROGRESS_TIMEOUT: Duration = Duration::from_secs(60);

/// Buffer size for streaming payload reads (64KB)
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Frame header information returned by `read_frame_header()`
///
/// This allows callers to inspect the frame metadata before deciding how to
/// handle the payload. For large payloads like `FileData`, callers should use
/// `stream_payload_to_writer()` instead of reading into memory.
#[derive(Debug, Clone)]
pub struct FrameHeader {
    /// The message type (e.g., "FileData", "ChatSend")
    pub message_type: String,
    /// The message ID for request-response correlation
    pub message_id: MessageId,
    /// The payload length in bytes
    pub payload_length: u64,
}

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

    /// Read just the frame header, leaving the payload unread
    ///
    /// This method reads and validates the frame header (magic, type, message_id,
    /// payload_length) but does NOT read the payload. The caller must then either:
    /// - Call `read_payload_into_vec()` to read the payload into memory
    /// - Call `stream_payload_to_writer()` to stream directly to a writer (for large payloads)
    ///
    /// Returns `Ok(None)` if the connection is cleanly closed.
    ///
    /// # Note
    ///
    /// This method has no timeout - wrap it with `tokio::time::timeout` if needed.
    pub async fn read_frame_header(&mut self) -> Result<Option<FrameHeader>, FrameError> {
        // Read the first byte of magic
        let first_byte = match self.read_byte_allow_eof().await? {
            Some(b) => b,
            None => return Ok(None), // Clean disconnect
        };

        self.read_frame_header_after_first_byte(first_byte).await
    }

    /// Read the payload into a Vec after reading the header
    ///
    /// Call this after `read_frame_header()` to read the payload into memory.
    /// For large payloads, use `stream_payload_to_writer()` instead.
    pub async fn read_payload_into_vec(
        &mut self,
        header: &FrameHeader,
    ) -> Result<Vec<u8>, FrameError> {
        let mut payload = vec![0u8; header.payload_length as usize];
        self.reader.read_exact(&mut payload).await?;

        // Read terminator
        let terminator = self.read_byte().await?;
        if terminator != TERMINATOR {
            return Err(FrameError::MissingTerminator);
        }

        Ok(payload)
    }

    /// Stream the payload directly to a writer with progress-based timeout
    ///
    /// This method streams `header.payload_length` bytes from the network directly
    /// to the provided writer, without loading the entire payload into memory.
    /// The timeout resets each time bytes are received.
    ///
    /// # Arguments
    ///
    /// * `header` - The frame header from `read_frame_header()`
    /// * `writer` - The destination to write payload bytes to
    /// * `progress_timeout` - Maximum time to wait between receiving bytes
    ///
    /// # Returns
    ///
    /// Returns the total number of bytes written on success.
    pub async fn stream_payload_to_writer<W>(
        &mut self,
        header: &FrameHeader,
        writer: &mut W,
        progress_timeout: Duration,
    ) -> Result<u64, FrameError>
    where
        W: AsyncWriteExt + Unpin,
    {
        let mut remaining = header.payload_length;
        let mut total_written: u64 = 0;
        let mut buffer = [0u8; STREAM_BUFFER_SIZE];

        while remaining > 0 {
            let to_read = (remaining as usize).min(buffer.len());

            // Read with progress timeout - resets on each successful read
            let bytes_read =
                match timeout(progress_timeout, self.reader.read(&mut buffer[..to_read])).await {
                    Ok(Ok(0)) => return Err(FrameError::ConnectionClosed),
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => return Err(FrameError::Io(e.to_string())),
                    Err(_) => return Err(FrameError::FrameTimeout),
                };

            // Write to destination
            writer.write_all(&buffer[..bytes_read]).await?;
            remaining -= bytes_read as u64;
            total_written += bytes_read as u64;
        }

        // Flush the writer
        writer.flush().await?;

        // Read terminator
        let terminator = match timeout(progress_timeout, self.read_byte()).await {
            Ok(Ok(b)) => b,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(FrameError::FrameTimeout),
        };
        if terminator != TERMINATOR {
            return Err(FrameError::MissingTerminator);
        }

        Ok(total_written)
    }

    /// Read the frame header after the first byte has been received
    async fn read_frame_header_after_first_byte(
        &mut self,
        first_byte: u8,
    ) -> Result<Option<FrameHeader>, FrameError> {
        // Step 1: Complete reading magic bytes (we already have the first one)
        if first_byte != MAGIC[0] {
            return Err(FrameError::InvalidMagic);
        }

        let mut magic_rest = [0u8; 2];
        self.reader.read_exact(&mut magic_rest).await?;
        if magic_rest != MAGIC[1..] {
            return Err(FrameError::InvalidMagic);
        }

        // Step 2: Read type length
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

        // Step 3: Read message type
        let mut type_bytes = vec![0u8; type_length as usize];
        self.reader.read_exact(&mut type_bytes).await?;
        let message_type = String::from_utf8(type_bytes)
            .map_err(|_| FrameError::UnknownMessageType("<invalid utf8>".to_string()))?;

        // Step 4: Reject unknown message types early
        if !is_known_message_type(&message_type) {
            return Err(FrameError::UnknownMessageType(message_type));
        }

        // Step 5: Read delimiter
        let delimiter = self.read_byte().await?;
        if delimiter != DELIMITER {
            return Err(FrameError::MissingDelimiter);
        }

        // Step 6: Read message ID
        let mut msg_id_bytes = [0u8; MSG_ID_LENGTH];
        self.reader.read_exact(&mut msg_id_bytes).await?;
        let message_id = MessageId::from_bytes(&msg_id_bytes)?;

        // Step 7: Read delimiter
        let delimiter = self.read_byte().await?;
        if delimiter != DELIMITER {
            return Err(FrameError::MissingDelimiter);
        }

        // Step 8: Read payload length
        let payload_length = self
            .read_length_field(
                MAX_PAYLOAD_LENGTH_DIGITS,
                FrameError::InvalidPayloadLength,
                FrameError::PayloadLengthTooManyDigits,
            )
            .await?;
        // Validate payload length against per-type maximum (0 = unlimited)
        let max_for_type = max_payload_for_type(&message_type);
        if max_for_type > 0 && payload_length > max_for_type {
            return Err(FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length: payload_length,
                max: max_for_type,
            });
        }

        Ok(Some(FrameHeader {
            message_type,
            message_id,
            payload_length,
        }))
    }

    /// Complete reading a frame after the first byte has been received
    async fn read_frame_after_first_byte(
        &mut self,
        first_byte: u8,
    ) -> Result<Option<RawFrame>, FrameError> {
        // Read the header first
        let header = match self.read_frame_header_after_first_byte(first_byte).await? {
            Some(h) => h,
            None => return Ok(None),
        };

        // Read the payload into memory
        let payload = self.read_payload_into_vec(&header).await?;

        Ok(Some(RawFrame::new(
            header.message_id,
            header.message_type,
            payload,
        )))
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
        // 21 digits before delimiter (exceeds MAX_PAYLOAD_LENGTH_DIGITS which is 20)
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|123456789012345678901|{}\n";
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
        // ChatSend has a base limit of 1101 bytes, padded 20% to 1321
        // Create a payload that claims to be 2000 bytes (well over limit)
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
                max: 1321  // 1101 * 1.2 = 1321
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
        // Handshake has a base limit of 65 bytes, padded 20% to 78
        // Create exactly 78 bytes of payload (at the padded limit)
        let payload = format!("{{\"version\":\"{}\"}}", "x".repeat(78 - 14));
        assert_eq!(payload.len(), 78);
        let data = format!("NX|9|Handshake|a1b2c3d4e5f6|78|{}\n", payload);

        let cursor = Cursor::new(data.as_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let frame = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(frame.payload.len(), 78);
    }

    #[tokio::test]
    async fn test_frame_reader_payload_one_over_type_limit() {
        // Handshake has a base limit of 65 bytes, padded 20% to 78
        // Create 79 bytes of payload (one over padded limit)
        let payload = format!("{{\"version\":\"{}\"}}", "x".repeat(79 - 14));
        assert_eq!(payload.len(), 79);
        let data = format!("NX|9|Handshake|a1b2c3d4e5f6|79|{}\n", payload);

        let cursor = Cursor::new(data.as_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(
            result,
            Err(FrameError::PayloadLengthExceedsTypeMax {
                message_type,
                length: 79,
                max: 78  // 65 * 1.2 = 78
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

    // =========================================================================
    // Streaming frame tests
    // =========================================================================

    #[tokio::test]
    async fn test_read_frame_header() {
        let data = b"NX|8|FileData|a1b2c3d4e5f6|5|hello\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.message_type, "FileData");
        assert_eq!(header.payload_length, 5);
    }

    #[tokio::test]
    async fn test_read_frame_header_then_payload_into_vec() {
        let data = b"NX|8|FileData|a1b2c3d4e5f6|5|hello\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        let payload = reader.read_payload_into_vec(&header).await.unwrap();
        assert_eq!(payload, b"hello");
    }

    #[tokio::test]
    async fn test_stream_payload_to_writer() {
        let payload = b"Hello, streaming world!";
        let data = format!(
            "NX|8|FileData|a1b2c3d4e5f6|{}|{}\n",
            payload.len(),
            String::from_utf8_lossy(payload)
        );
        let cursor = Cursor::new(data.as_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.message_type, "FileData");
        assert_eq!(header.payload_length, payload.len() as u64);

        let mut output = Vec::new();
        let bytes_written = reader
            .stream_payload_to_writer(&header, &mut output, DEFAULT_PROGRESS_TIMEOUT)
            .await
            .unwrap();

        assert_eq!(bytes_written, payload.len() as u64);
        assert_eq!(output, payload);
    }

    #[tokio::test]
    async fn test_stream_payload_to_writer_large() {
        // Test with a larger payload to ensure chunked reading works
        let payload: Vec<u8> = (0..=255).cycle().take(256 * 1024).collect(); // 256KB
        let mut data = format!("NX|8|FileData|a1b2c3d4e5f6|{}|", payload.len()).into_bytes();
        data.extend_from_slice(&payload);
        data.push(b'\n');

        let cursor = Cursor::new(data);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.payload_length, payload.len() as u64);

        let mut output = Vec::new();
        let bytes_written = reader
            .stream_payload_to_writer(&header, &mut output, DEFAULT_PROGRESS_TIMEOUT)
            .await
            .unwrap();

        assert_eq!(bytes_written, payload.len() as u64);
        assert_eq!(output, payload);
    }

    #[tokio::test]
    async fn test_stream_payload_to_writer_empty() {
        let data = b"NX|8|FileData|a1b2c3d4e5f6|0|\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.payload_length, 0);

        let mut output = Vec::new();
        let bytes_written = reader
            .stream_payload_to_writer(&header, &mut output, DEFAULT_PROGRESS_TIMEOUT)
            .await
            .unwrap();

        assert_eq!(bytes_written, 0);
        assert!(output.is_empty());
    }

    #[tokio::test]
    async fn test_stream_payload_progress_timeout() {
        use tokio::io::duplex;

        // Create a duplex stream where we control both ends
        let (client, server) = duplex(1024);
        let buf_reader = BufReader::new(server);
        let mut reader = FrameReader::new(buf_reader);

        // Write header claiming 1000 bytes payload
        let mut client = client;
        client
            .write_all(b"NX|8|FileData|a1b2c3d4e5f6|1000|")
            .await
            .unwrap();
        // Write some bytes but not all
        client.write_all(b"partial").await.unwrap();
        // Don't write any more - should timeout

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.payload_length, 1000);

        let mut output = Vec::new();
        let result = reader
            .stream_payload_to_writer(&header, &mut output, Duration::from_millis(50))
            .await;

        assert!(matches!(result, Err(FrameError::FrameTimeout)));
        // Partial data should have been written before timeout
        assert_eq!(output, b"partial");
    }

    #[tokio::test]
    async fn test_stream_payload_connection_closed() {
        use tokio::io::duplex;

        // Create a duplex stream where we control both ends
        let (client, server) = duplex(1024);
        let buf_reader = BufReader::new(server);
        let mut reader = FrameReader::new(buf_reader);

        // Write header claiming 1000 bytes payload
        let mut client = client;
        client
            .write_all(b"NX|8|FileData|a1b2c3d4e5f6|1000|")
            .await
            .unwrap();
        // Write some bytes then close
        client.write_all(b"partial").await.unwrap();
        drop(client); // Close the connection

        let header = reader.read_frame_header().await.unwrap().unwrap();

        let mut output = Vec::new();
        let result = reader
            .stream_payload_to_writer(&header, &mut output, Duration::from_secs(1))
            .await;

        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn test_stream_payload_missing_terminator() {
        // Payload is complete but missing the newline terminator
        let data = b"NX|8|FileData|a1b2c3d4e5f6|5|helloX"; // X instead of \n
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();

        let mut output = Vec::new();
        let result = reader
            .stream_payload_to_writer(&header, &mut output, DEFAULT_PROGRESS_TIMEOUT)
            .await;

        assert!(matches!(result, Err(FrameError::MissingTerminator)));
        // Payload should still have been written
        assert_eq!(output, b"hello");
    }

    #[tokio::test]
    async fn test_stream_payload_exactly_buffer_size() {
        // Test with payload exactly at buffer boundary (64KB)
        let payload: Vec<u8> = (0..=255).cycle().take(STREAM_BUFFER_SIZE).collect();
        let mut data = format!("NX|8|FileData|a1b2c3d4e5f6|{}|", payload.len()).into_bytes();
        data.extend_from_slice(&payload);
        data.push(b'\n');

        let cursor = Cursor::new(data);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.payload_length, STREAM_BUFFER_SIZE as u64);

        let mut output = Vec::new();
        let bytes_written = reader
            .stream_payload_to_writer(&header, &mut output, DEFAULT_PROGRESS_TIMEOUT)
            .await
            .unwrap();

        assert_eq!(bytes_written, STREAM_BUFFER_SIZE as u64);
        assert_eq!(output, payload);
    }

    // =========================================================================
    // read_frame_header edge cases
    // =========================================================================

    #[tokio::test]
    async fn test_read_frame_header_clean_disconnect() {
        let data = b"";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame_header().await.unwrap();
        assert!(result.is_none());
    }

    // =========================================================================
    // read_payload_into_vec edge cases
    // =========================================================================

    #[tokio::test]
    async fn test_read_payload_into_vec_empty() {
        let data = b"NX|8|FileData|a1b2c3d4e5f6|0|\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.payload_length, 0);

        let payload = reader.read_payload_into_vec(&header).await.unwrap();
        assert!(payload.is_empty());
    }

    #[tokio::test]
    async fn test_read_payload_into_vec_missing_terminator() {
        let data = b"NX|8|FileData|a1b2c3d4e5f6|5|helloX"; // X instead of \n
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        let result = reader.read_payload_into_vec(&header).await;

        assert!(matches!(result, Err(FrameError::MissingTerminator)));
    }

    #[tokio::test]
    async fn test_read_payload_into_vec_eof_mid_payload() {
        let data = b"NX|8|FileData|a1b2c3d4e5f6|100|short"; // Claims 100 bytes, only has 5
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let header = reader.read_frame_header().await.unwrap().unwrap();
        let result = reader.read_payload_into_vec(&header).await;

        assert!(matches!(result, Err(FrameError::ConnectionClosed)));
    }

    // =========================================================================
    // Length field parsing edge cases
    // =========================================================================

    #[tokio::test]
    async fn test_type_length_non_digit() {
        // Non-digit character in type length field
        let data = b"NX|1a|X|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidTypeLength)));
    }

    #[tokio::test]
    async fn test_payload_length_non_digit() {
        // Non-digit character in payload length field
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6|1x|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidPayloadLength)));
    }

    #[tokio::test]
    async fn test_type_length_empty() {
        // Empty type length field (delimiter immediately after magic)
        let data = b"NX||ChatSend|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidTypeLength)));
    }

    #[tokio::test]
    async fn test_payload_length_empty() {
        // Empty payload length field
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6||{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidPayloadLength)));
    }

    // =========================================================================
    // Delimiter and UTF-8 edge cases
    // =========================================================================

    #[tokio::test]
    async fn test_missing_delimiter_after_type() {
        // Missing delimiter after message type (wrong byte where delimiter should be)
        let data = b"NX|8|ChatSendXa1b2c3d4e5f6|2|{}\n"; // X instead of |
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::MissingDelimiter)));
    }

    #[tokio::test]
    async fn test_missing_delimiter_after_message_id() {
        // Missing delimiter after message ID
        let data = b"NX|8|ChatSend|a1b2c3d4e5f6X2|{}\n"; // X instead of |
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::MissingDelimiter)));
    }

    #[tokio::test]
    async fn test_invalid_utf8_in_message_type() {
        // Invalid UTF-8 sequence in message type
        // Type length says 8 bytes, but contains invalid UTF-8
        let mut data = b"NX|8|".to_vec();
        data.extend_from_slice(&[0xFF, 0xFE, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85]); // Invalid UTF-8
        data.extend_from_slice(b"|a1b2c3d4e5f6|2|{}\n");

        let cursor = Cursor::new(data);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::UnknownMessageType(_))));
    }

    #[tokio::test]
    async fn test_magic_wrong_second_byte() {
        // First byte correct (N), second byte wrong
        let data = b"NA|8|ChatSend|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidMagic)));
    }

    #[tokio::test]
    async fn test_magic_wrong_third_byte() {
        // First two bytes correct (NX), third byte wrong (not |)
        let data = b"NX-8|ChatSend|a1b2c3d4e5f6|2|{}\n";
        let cursor = Cursor::new(data.as_slice());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = reader.read_frame().await;
        assert!(matches!(result, Err(FrameError::InvalidMagic)));
    }
}
