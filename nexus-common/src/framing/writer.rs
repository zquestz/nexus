//! Frame writer for sending protocol messages to a stream

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::error::FrameError;
use super::frame::RawFrame;
use super::message_id::MessageId;
use super::{DELIMITER, MAGIC, TERMINATOR};

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
        // Write frame header: NX|<type_len>|<type>|<msg_id>|<payload_len>|
        self.writer.write_all(MAGIC).await?;
        self.writer
            .write_all(message_type.len().to_string().as_bytes())
            .await?;
        self.writer.write_all(&[DELIMITER]).await?;
        self.writer.write_all(message_type.as_bytes()).await?;
        self.writer.write_all(&[DELIMITER]).await?;
        self.writer.write_all(message_id.as_bytes()).await?;
        self.writer.write_all(&[DELIMITER]).await?;
        self.writer
            .write_all(payload_len.to_string().as_bytes())
            .await?;
        self.writer.write_all(&[DELIMITER]).await?;

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

            assert!(result.is_err());
        }
    }
}
