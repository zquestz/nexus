//! Frame writer for sending protocol messages to a stream

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::error::FrameError;
use super::frame::RawFrame;
use super::message_id::MessageId;
use super::{
    DELIMITER, MAGIC, MAX_PAYLOAD_LENGTH_DIGITS, MAX_TYPE_LENGTH, MAX_TYPE_LENGTH_DIGITS,
    MSG_ID_LENGTH, TERMINATOR,
};

/// Fixed overhead for frame header (excluding variable-length message type)
/// Format: NX|<type_len>|<type>|<msg_id>|<payload_len>|
const HEADER_FIXED_OVERHEAD: usize = MAGIC.len() // "NX|"
    + MAX_TYPE_LENGTH_DIGITS                     // type length (up to 3 digits)
    + 1                                          // '|' delimiter
    // + message_type.len()                      // (variable, added separately)
    + 1                                          // '|' delimiter
    + MSG_ID_LENGTH                              // message ID (12 hex chars)
    + 1                                          // '|' delimiter
    + MAX_PAYLOAD_LENGTH_DIGITS                  // payload length (up to 20 digits)
    + 1; // '|' delimiter

/// Writes protocol frames to an async writer
pub struct FrameWriter<W> {
    writer: W,
}

impl<W> FrameWriter<W> {
    /// Create a new frame writer
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Get a reference to the underlying writer
    pub fn get_ref(&self) -> &W {
        &self.writer
    }

    /// Get a mutable reference to the underlying writer
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Consume the frame writer and return the underlying writer
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: AsyncWriteExt + Unpin> FrameWriter<W> {
    /// Write a frame to the stream
    ///
    /// # Errors
    ///
    /// Returns an error if an I/O error occurs.
    pub async fn write_frame(&mut self, frame: &RawFrame) -> Result<(), FrameError> {
        let bytes = frame.to_bytes();
        self.writer.write_all(&bytes).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Write a frame with the given components
    ///
    /// This is a convenience method that constructs a frame and writes it.
    ///
    /// # Errors
    ///
    /// Returns an error if an I/O error occurs.
    pub async fn write(
        &mut self,
        message_id: MessageId,
        message_type: &str,
        payload: &[u8],
    ) -> Result<(), FrameError> {
        // Validate message type to catch programming errors early
        if message_type.is_empty() || message_type.len() > MAX_TYPE_LENGTH {
            return Err(FrameError::TypeLengthOutOfRange);
        }

        let frame = RawFrame::new(message_id, message_type.to_string(), payload.to_vec());
        self.write_frame(&frame).await
    }

    /// Write a streaming frame by copying from a reader
    ///
    /// This method writes the frame header, streams `payload_len` bytes from the reader,
    /// then writes the terminator. This is more efficient than `write_frame` for large
    /// payloads (e.g., file transfers) because it doesn't require loading the entire
    /// payload into memory.
    ///
    /// # Arguments
    ///
    /// * `message_id` - The message ID for request-response correlation
    /// * `message_type` - The message type string (e.g., "FileData")
    /// * `reader` - The source to read payload bytes from
    /// * `payload_len` - The exact number of bytes to read and write
    ///
    /// # Errors
    ///
    /// Returns an error if an I/O error occurs or if the reader provides fewer
    /// than `payload_len` bytes.
    pub async fn write_streaming_frame<R>(
        &mut self,
        message_id: MessageId,
        message_type: &str,
        reader: &mut R,
        payload_len: u64,
    ) -> Result<(), FrameError>
    where
        R: AsyncRead + Unpin,
    {
        // Validate message type to catch programming errors early
        if message_type.is_empty() || message_type.len() > MAX_TYPE_LENGTH {
            return Err(FrameError::TypeLengthOutOfRange);
        }

        // Build frame header in a single buffer to reduce syscalls
        let mut header = Vec::with_capacity(HEADER_FIXED_OVERHEAD + message_type.len());
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(message_type.len().to_string().as_bytes());
        header.push(DELIMITER);
        header.extend_from_slice(message_type.as_bytes());
        header.push(DELIMITER);
        header.extend_from_slice(message_id.as_bytes());
        header.push(DELIMITER);
        header.extend_from_slice(payload_len.to_string().as_bytes());
        header.push(DELIMITER);

        self.writer.write_all(&header).await?;

        // Stream payload bytes from reader to writer
        let bytes_copied = tokio::io::copy(&mut reader.take(payload_len), &mut self.writer).await?;

        if bytes_copied < payload_len {
            return Err(FrameError::Io(format!(
                "Reader ended early: expected {} bytes, got {}",
                payload_len, bytes_copied
            )));
        }

        // Write terminator
        self.writer.write_all(&[TERMINATOR]).await?;
        self.writer.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_frame_writer() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            let frame = RawFrame::new(id, "ChatSend".to_string(), b"{\"message\":\"Hi\"}".to_vec());
            writer.write_frame(&frame).await.unwrap();
        }

        let expected = format!("NX|8|ChatSend|{}|16|{{\"message\":\"Hi\"}}\n", id);
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_convenience_method() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            writer
                .write(id, "Handshake", b"{\"version\":\"0.4.0\"}")
                .await
                .unwrap();
        }

        let expected = format!("NX|9|Handshake|{}|19|{{\"version\":\"0.4.0\"}}\n", id);
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_streaming() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        let payload = b"Hello, streaming world!";

        {
            let mut writer = FrameWriter::new(cursor);
            let mut reader = Cursor::new(payload.as_slice());
            writer
                .write_streaming_frame(id, "FileData", &mut reader, payload.len() as u64)
                .await
                .unwrap();
        }

        let expected = format!(
            "NX|8|FileData|{}|{}|Hello, streaming world!\n",
            id,
            payload.len()
        );
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_streaming_empty() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            let mut reader = Cursor::new(&[] as &[u8]);
            writer
                .write_streaming_frame(id, "FileData", &mut reader, 0)
                .await
                .unwrap();
        }

        let expected = format!("NX|8|FileData|{}|0|\n", id);
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_streaming_reader_too_short() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        let payload = b"Short";

        {
            let mut writer = FrameWriter::new(cursor);
            let mut reader = Cursor::new(payload.as_slice());
            let result = writer
                .write_streaming_frame(id, "FileData", &mut reader, 100) // Claim 100 bytes but only have 5
                .await;

            assert!(
                matches!(result, Err(FrameError::Io(msg)) if msg.contains("expected 100 bytes, got 5"))
            );
        }
    }

    #[tokio::test]
    async fn test_frame_writer_streaming_large() {
        // Test with a larger payload to ensure streaming works correctly
        let payload: Vec<u8> = (0..=255).cycle().take(256 * 1024).collect(); // 256KB
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            let mut reader = Cursor::new(payload.as_slice());
            writer
                .write_streaming_frame(id, "FileData", &mut reader, payload.len() as u64)
                .await
                .unwrap();
        }

        // Verify header
        let header = format!("NX|8|FileData|{}|{}|", id, payload.len());
        assert!(buffer.starts_with(header.as_bytes()));

        // Verify terminator
        assert_eq!(buffer.last(), Some(&b'\n'));

        // Verify payload
        let payload_start = header.len();
        let payload_end = buffer.len() - 1; // exclude terminator
        assert_eq!(&buffer[payload_start..payload_end], payload.as_slice());
    }

    #[tokio::test]
    async fn test_frame_writer_empty_payload() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            let frame = RawFrame::new(id, "UserList".to_string(), vec![]);
            writer.write_frame(&frame).await.unwrap();
        }

        let expected = format!("NX|8|UserList|{}|0|\n", id);
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_binary_payload() {
        // Test with binary payload containing all byte values including null and non-UTF8
        let payload: Vec<u8> = (0..=255).collect();
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            let frame = RawFrame::new(id, "FileData".to_string(), payload.clone());
            writer.write_frame(&frame).await.unwrap();
        }

        // Verify header
        let header = format!("NX|8|FileData|{}|256|", id);
        assert!(buffer.starts_with(header.as_bytes()));

        // Verify terminator
        assert_eq!(buffer.last(), Some(&b'\n'));

        // Verify payload
        let payload_start = header.len();
        let payload_end = buffer.len() - 1;
        assert_eq!(&buffer[payload_start..payload_end], payload.as_slice());
    }

    #[tokio::test]
    async fn test_frame_writer_streaming_binary_payload() {
        // Test streaming with binary payload
        let payload: Vec<u8> = (0..=255).collect();
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            let mut reader = Cursor::new(payload.as_slice());
            writer
                .write_streaming_frame(id, "FileData", &mut reader, payload.len() as u64)
                .await
                .unwrap();
        }

        // Verify header
        let header = format!("NX|8|FileData|{}|256|", id);
        assert!(buffer.starts_with(header.as_bytes()));

        // Verify terminator
        assert_eq!(buffer.last(), Some(&b'\n'));

        // Verify payload
        let payload_start = header.len();
        let payload_end = buffer.len() - 1;
        assert_eq!(&buffer[payload_start..payload_end], payload.as_slice());
    }

    #[tokio::test]
    async fn test_frame_writer_multiple_frames() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id1 = MessageId::new();
        let id2 = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);

            let frame1 = RawFrame::new(id1, "ChatSend".to_string(), b"first".to_vec());
            writer.write_frame(&frame1).await.unwrap();

            let frame2 = RawFrame::new(id2, "ChatSend".to_string(), b"second".to_vec());
            writer.write_frame(&frame2).await.unwrap();
        }

        let expected = format!(
            "NX|8|ChatSend|{}|5|first\nNX|8|ChatSend|{}|6|second\n",
            id1, id2
        );
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_long_message_type() {
        // Use a real long message type name
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        {
            let mut writer = FrameWriter::new(cursor);
            writer
                .write(id, "ChatTopicUpdateResponse", b"{}")
                .await
                .unwrap();
        }

        // ChatTopicUpdateResponse is 23 characters
        let expected = format!("NX|23|ChatTopicUpdateResponse|{}|2|{{}}\n", id);
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_frame_writer_streaming_multiple_frames() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id1 = MessageId::new();
        let id2 = MessageId::new();

        let payload1 = b"first payload";
        let payload2 = b"second payload";

        {
            let mut writer = FrameWriter::new(cursor);

            let mut reader1 = Cursor::new(payload1.as_slice());
            writer
                .write_streaming_frame(id1, "FileData", &mut reader1, payload1.len() as u64)
                .await
                .unwrap();

            let mut reader2 = Cursor::new(payload2.as_slice());
            writer
                .write_streaming_frame(id2, "FileData", &mut reader2, payload2.len() as u64)
                .await
                .unwrap();
        }

        let expected = format!(
            "NX|8|FileData|{}|{}|first payload\nNX|8|FileData|{}|{}|second payload\n",
            id1,
            payload1.len(),
            id2,
            payload2.len()
        );
        assert_eq!(buffer, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_write_empty_message_type_rejected() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        let mut writer = FrameWriter::new(cursor);
        let result = writer.write(id, "", b"{}").await;

        assert_eq!(result, Err(FrameError::TypeLengthOutOfRange));
    }

    #[tokio::test]
    async fn test_write_message_type_too_long_rejected() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        // MAX_TYPE_LENGTH is 999, so 1000 chars should be rejected
        let long_type = "X".repeat(1000);

        let mut writer = FrameWriter::new(cursor);
        let result = writer.write(id, &long_type, b"{}").await;

        assert_eq!(result, Err(FrameError::TypeLengthOutOfRange));
    }

    #[tokio::test]
    async fn test_write_streaming_empty_message_type_rejected() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        let mut writer = FrameWriter::new(cursor);
        let mut reader = Cursor::new(b"payload".as_slice());
        let result = writer.write_streaming_frame(id, "", &mut reader, 7).await;

        assert_eq!(result, Err(FrameError::TypeLengthOutOfRange));
    }

    #[tokio::test]
    async fn test_write_streaming_message_type_too_long_rejected() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        // MAX_TYPE_LENGTH is 999, so 1000 chars should be rejected
        let long_type = "X".repeat(1000);

        let mut writer = FrameWriter::new(cursor);
        let mut reader = Cursor::new(b"payload".as_slice());
        let result = writer
            .write_streaming_frame(id, &long_type, &mut reader, 7)
            .await;

        assert_eq!(result, Err(FrameError::TypeLengthOutOfRange));
    }

    #[tokio::test]
    async fn test_write_max_length_message_type_accepted() {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let id = MessageId::new();

        // MAX_TYPE_LENGTH is 999, so exactly 999 chars should be accepted
        let max_type = "X".repeat(999);

        let mut writer = FrameWriter::new(cursor);
        let result = writer.write(id, &max_type, b"{}").await;

        assert!(result.is_ok());
        // Verify the type length is written as "999"
        assert!(buffer.starts_with(b"NX|999|"));
    }
}
