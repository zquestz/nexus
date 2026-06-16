//! Progress-bounded client frame writes.

use std::io;
use std::time::Duration;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::client_message_to_frame_bytes;
use nexus_common::protocol::ClientMessage;

pub(crate) async fn send_client_message_with_progress_timeout<W>(
    writer: &mut FrameWriter<W>,
    message: &ClientMessage,
    message_id: MessageId,
    progress_timeout: Duration,
    timeout_error: &'static str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let bytes = client_message_to_frame_bytes(message, message_id)?;

    write_all_with_progress_timeout(
        writer.get_mut(),
        bytes.as_ref(),
        progress_timeout,
        timeout_error,
    )
    .await?;
    flush_with_progress_timeout(writer.get_mut(), progress_timeout, timeout_error).await
}

pub(crate) async fn shutdown_writer_with_progress_timeout<W>(
    writer: &mut W,
    progress_timeout: Duration,
    timeout_error: &'static str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match timeout(progress_timeout, writer.shutdown()).await {
        Ok(result) => result,
        Err(_) => Err(write_timeout_error(timeout_error)),
    }
}

async fn write_all_with_progress_timeout<W>(
    writer: &mut W,
    mut buf: &[u8],
    progress_timeout: Duration,
    timeout_error: &'static str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    while !buf.is_empty() {
        let bytes_written = match timeout(progress_timeout, writer.write(buf)).await {
            Ok(Ok(bytes_written)) => bytes_written,
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err(write_timeout_error(timeout_error)),
        };

        if bytes_written == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }

        buf = &buf[bytes_written..];
    }

    Ok(())
}

async fn flush_with_progress_timeout<W>(
    writer: &mut W,
    progress_timeout: Duration,
    timeout_error: &'static str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match timeout(progress_timeout, writer.flush()).await {
        Ok(result) => result,
        Err(_) => Err(write_timeout_error(timeout_error)),
    }
}

fn write_timeout_error(timeout_error: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, timeout_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    const ERR_TEST_TIMEOUT: &str = "test writer made no progress before timeout";
    const ERR_TEST_WRITE_FAILED: &str = "test write failed";

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
            Poll::Pending
        }
    }

    struct PendingFlushWriter;

    impl AsyncWrite for PendingFlushWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct ZeroByteWriter;

    impl AsyncWrite for ZeroByteWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(0))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct ErrorWriter;

    impl AsyncWrite for ErrorWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                ERR_TEST_WRITE_FAILED,
            )))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[derive(Default)]
    struct OneByteWriter {
        bytes: Vec<u8>,
    }

    impl AsyncWrite for OneByteWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if buf.is_empty() {
                return Poll::Ready(Ok(0));
            }

            self.get_mut().bytes.push(buf[0]);
            Poll::Ready(Ok(1))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct DelayedOneByteWriter {
        bytes: Vec<u8>,
        delay: Duration,
        sleep: Option<Pin<Box<tokio::time::Sleep>>>,
    }

    impl DelayedOneByteWriter {
        fn new(delay: Duration) -> Self {
            Self {
                bytes: Vec::new(),
                delay,
                sleep: None,
            }
        }
    }

    impl AsyncWrite for DelayedOneByteWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if buf.is_empty() {
                return Poll::Ready(Ok(0));
            }

            if self.sleep.is_none() {
                let delay = self.delay;
                self.sleep = Some(Box::pin(tokio::time::sleep(delay)));
            }

            let Some(sleep) = self.sleep.as_mut() else {
                return Poll::Ready(Ok(0));
            };
            if sleep.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }

            self.sleep = None;
            self.bytes.push(buf[0]);
            Poll::Ready(Ok(1))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn client_writer_times_out_when_write_makes_no_progress() {
        let mut writer = FrameWriter::new(PendingWriter);
        let err = send_client_message_with_progress_timeout(
            &mut writer,
            &ClientMessage::Ping,
            MessageId::new(),
            Duration::from_millis(1),
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect_err("stalled write must time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), ERR_TEST_TIMEOUT);
    }

    #[tokio::test]
    async fn client_writer_times_out_when_flush_makes_no_progress() {
        let mut writer = FrameWriter::new(PendingFlushWriter);
        let err = send_client_message_with_progress_timeout(
            &mut writer,
            &ClientMessage::Ping,
            MessageId::new(),
            Duration::from_millis(1),
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect_err("stalled flush must time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), ERR_TEST_TIMEOUT);
    }

    #[tokio::test]
    async fn client_writer_reports_write_zero() {
        let mut writer = FrameWriter::new(ZeroByteWriter);
        let err = send_client_message_with_progress_timeout(
            &mut writer,
            &ClientMessage::Ping,
            MessageId::new(),
            Duration::from_secs(30),
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect_err("zero-byte write must fail");

        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }

    #[tokio::test]
    async fn client_writer_preserves_io_write_errors() {
        let mut writer = FrameWriter::new(ErrorWriter);
        let err = send_client_message_with_progress_timeout(
            &mut writer,
            &ClientMessage::Ping,
            MessageId::new(),
            Duration::from_secs(30),
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect_err("write error must pass through");

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(err.to_string(), ERR_TEST_WRITE_FAILED);
    }

    #[tokio::test]
    async fn client_writer_shutdown_times_out_when_it_stalls() {
        let mut writer = PendingWriter;
        let err = shutdown_writer_with_progress_timeout(
            &mut writer,
            Duration::from_millis(1),
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect_err("stalled shutdown must time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), ERR_TEST_TIMEOUT);
    }

    #[tokio::test]
    async fn client_writer_allows_slow_partial_write_progress() {
        let mut writer = FrameWriter::new(OneByteWriter::default());
        send_client_message_with_progress_timeout(
            &mut writer,
            &ClientMessage::Ping,
            MessageId::new(),
            Duration::from_secs(30),
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect("partial progress should complete");

        let bytes = writer.into_inner().bytes;
        let frame = String::from_utf8(bytes).expect("frame bytes should be utf8");
        assert!(frame.contains("|Ping|"), "{frame}");
        assert!(frame.contains(r#""type":"Ping""#), "{frame}");
        assert!(frame.ends_with('\n'), "{frame}");
    }

    #[tokio::test]
    async fn client_writer_allows_delayed_progress_past_total_timeout() {
        let progress_timeout = Duration::from_millis(100);
        let mut writer = FrameWriter::new(DelayedOneByteWriter::new(Duration::from_millis(5)));
        let start = tokio::time::Instant::now();

        send_client_message_with_progress_timeout(
            &mut writer,
            &ClientMessage::Ping,
            MessageId::new(),
            progress_timeout,
            ERR_TEST_TIMEOUT,
        )
        .await
        .expect("delayed per-byte progress should complete");

        assert!(start.elapsed() >= progress_timeout);
        let bytes = writer.into_inner().bytes;
        let frame = String::from_utf8(bytes).expect("frame bytes should be utf8");
        assert!(frame.contains("|Ping|"), "{frame}");
        assert!(frame.contains(r#""type":"Ping""#), "{frame}");
        assert!(frame.ends_with('\n'), "{frame}");
    }
}
