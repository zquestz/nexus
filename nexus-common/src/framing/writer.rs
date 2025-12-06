//! Frame writer for sending protocol messages to a stream

use tokio::io::AsyncWriteExt;

use super::error::FrameError;
use super::frame::RawFrame;
use super::message_id::MessageId;

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
}
