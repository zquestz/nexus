use std::io;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::{server_message_to_frame_bytes, tracker_server_message_to_frame_bytes};
use nexus_common::protocol::ServerMessage;
use nexus_common::tracker_protocol::TrackerServerMessage;

use crate::constants::{
    ERR_TRACKER_WRITE_TIMEOUT, ERR_TRACKER_WRITE_ZERO, TRACKER_WRITE_CHUNK_SIZE,
    TRACKER_WRITE_PROGRESS_TIMEOUT,
};

pub(crate) async fn send_server_message_with_write_timeout<W>(
    writer: &mut FrameWriter<W>,
    message: &ServerMessage,
) -> io::Result<MessageId>
where
    W: AsyncWrite + Unpin,
{
    let message_id = MessageId::new();
    let bytes = server_message_to_frame_bytes(message, message_id)?;
    write_frame_bytes_with_progress_timeout(writer, &bytes).await?;
    Ok(message_id)
}

pub(crate) async fn send_tracker_server_message_with_write_timeout<W>(
    writer: &mut FrameWriter<W>,
    message: &TrackerServerMessage,
) -> io::Result<MessageId>
where
    W: AsyncWrite + Unpin,
{
    let message_id = MessageId::new();
    let bytes = tracker_server_message_to_frame_bytes(message, message_id)?;
    write_frame_bytes_with_progress_timeout(writer, &bytes).await?;
    Ok(message_id)
}

async fn write_frame_bytes_with_progress_timeout<W>(
    writer: &mut FrameWriter<W>,
    bytes: &[u8],
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    for chunk in bytes.chunks(TRACKER_WRITE_CHUNK_SIZE) {
        write_all_with_progress_timeout(writer.get_mut(), chunk).await?;
        flush_with_progress_timeout(writer.get_mut()).await?;
    }
    Ok(())
}

async fn write_all_with_progress_timeout<W>(writer: &mut W, mut bytes: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    while !bytes.is_empty() {
        match time::timeout(TRACKER_WRITE_PROGRESS_TIMEOUT, writer.write(bytes)).await {
            Ok(Ok(0)) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    ERR_TRACKER_WRITE_ZERO,
                ));
            }
            Ok(Ok(written)) => bytes = &bytes[written..],
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    ERR_TRACKER_WRITE_TIMEOUT,
                ));
            }
        }
    }
    Ok(())
}

async fn flush_with_progress_timeout<W>(writer: &mut W) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match time::timeout(TRACKER_WRITE_PROGRESS_TIMEOUT, writer.flush()).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            ERR_TRACKER_WRITE_TIMEOUT,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use nexus_common::protocol::ServerMessage;
    use nexus_common::tracker_protocol::TrackerServerMessage;
    use tokio::io::AsyncWrite;

    struct PendingWriter;

    impl AsyncWrite for PendingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct PartialThenPendingWriter {
        first_write: bool,
        bytes: Vec<u8>,
    }

    impl PartialThenPendingWriter {
        fn new() -> Self {
            Self {
                first_write: true,
                bytes: Vec::new(),
            }
        }
    }

    impl AsyncWrite for PartialThenPendingWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.first_write {
                self.first_write = false;
                let written = buf.len().min(3);
                self.bytes.extend_from_slice(&buf[..written]);
                Poll::Ready(Ok(written))
            } else {
                Poll::Pending
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct ProgressWriter {
        bytes: Vec<u8>,
    }

    impl ProgressWriter {
        fn new() -> Self {
            Self { bytes: Vec::new() }
        }
    }

    impl AsyncWrite for ProgressWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let written = buf.len().min(2);
            self.bytes.extend_from_slice(&buf[..written]);
            Poll::Ready(Ok(written))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct FlushPendingWriter {
        bytes: Vec<u8>,
    }

    impl FlushPendingWriter {
        fn new() -> Self {
            Self { bytes: Vec::new() }
        }
    }

    impl AsyncWrite for FlushPendingWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.bytes.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn tracker_response_write_times_out_without_progress() {
        let mut writer = FrameWriter::new(PendingWriter);
        let message = TrackerServerMessage::TrackerServerListResponse {
            success: true,
            servers: Vec::new(),
            error: None,
            error_kind: None,
        };

        let err = send_tracker_server_message_with_write_timeout(&mut writer, &message)
            .await
            .expect_err("pending write should time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), ERR_TRACKER_WRITE_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn tracker_response_write_times_out_after_partial_progress() {
        let mut writer = FrameWriter::new(PartialThenPendingWriter::new());
        let message = TrackerServerMessage::TrackerServerRegisterResponse {
            success: true,
            refresh_interval: Some(300),
            error: None,
            error_kind: None,
        };

        let err = send_tracker_server_message_with_write_timeout(&mut writer, &message)
            .await
            .expect_err("stalled second write should time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(!writer.get_ref().bytes.is_empty());
    }

    #[tokio::test]
    async fn tracker_response_write_succeeds_with_steady_progress() {
        let mut writer = FrameWriter::new(ProgressWriter::new());
        let message = TrackerServerMessage::Error {
            message: "bad request".to_string(),
            command: Some("TrackerServerList".to_string()),
        };

        send_tracker_server_message_with_write_timeout(&mut writer, &message)
            .await
            .expect("steady progress should complete");

        assert!(!writer.get_ref().bytes.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn tracker_response_flush_times_out() {
        let mut writer = FrameWriter::new(FlushPendingWriter::new());
        let message = ServerMessage::Error {
            message: "bad handshake".to_string(),
            command: Some("Login".to_string()),
            disconnect: true,
        };

        let err = send_server_message_with_write_timeout(&mut writer, &message)
            .await
            .expect_err("pending flush should time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(!writer.get_ref().bytes.is_empty());
    }
}
