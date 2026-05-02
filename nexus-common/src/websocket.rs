//! WebSocket byte-stream adapter
//!
//! Adapts a `tokio_tungstenite::WebSocketStream` (or any compatible
//! `Stream<Item = Result<Message, _>>` + `Sink<Message>`) so it presents
//! `AsyncRead + AsyncWrite`. Daemons that listen on both TCP and
//! WebSocket can share one connection task by passing either a raw
//! TLS-over-TCP stream or a `WebSocketAdapter`-wrapped one — the
//! framed-JSON protocol code doesn't care which one it has.
//!
//! The adapter buffers incoming WebSocket binary messages and presents
//! them as a contiguous byte stream for reading. For writing, it
//! buffers bytes and emits them as one WebSocket binary message per
//! `flush`. Text / Ping / Pong frames from peers are silently ignored
//! (they're a layer below the protocol).
//!
//! Available behind the `websocket` Cargo feature; consumers that
//! don't need WS plumbing don't pay for `tokio-tungstenite` /
//! `futures-util`.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::sink::Sink;
use futures_util::stream::Stream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::tungstenite::protocol::Message;

/// Maximum size of a single inbound WebSocket message (1 MB).
///
/// Prevents memory exhaustion from malicious clients sending huge
/// messages. The largest legitimate BBS-side messages are ~700 KB
/// (news / server-image uploads). File-data chunks are 64 KB, matching
/// TCP streaming behavior.
pub const MAX_WS_MESSAGE_SIZE: usize = 1024 * 1024;

/// Error message for an empty WebSocket Binary frame.
///
/// Our protocol embeds framed JSON inside every Binary frame, so an
/// empty Binary carries no payload and is never produced by a
/// legitimate client. WS-level keepalives use the dedicated Ping/Pong
/// opcodes (handled automatically by `tokio-tungstenite`) — empty
/// Binary is not a keepalive variant. Treating it as a protocol
/// violation gives developers writing WS clients a clear diagnostic
/// instead of an opaque "connection dropped" (which is what would
/// happen if we surfaced zero bytes filled — `AsyncRead` consumers
/// interpret that as EOF).
pub const ERR_WS_EMPTY_BINARY_FRAME: &str =
    "WebSocket empty Binary frame is not a valid protocol message";

/// Adapter that makes a WebSocket stream behave like an `AsyncRead +
/// AsyncWrite` byte stream.
///
/// Generic over any `S` that implements
/// `Stream<Item = Result<Message, _>>` (for reads) and `Sink<Message>`
/// (for writes), which `tokio_tungstenite::WebSocketStream` does
/// natively.
pub struct WebSocketAdapter<S> {
    inner: S,
    /// Pending bytes from the most recently received Binary message.
    read_buffer: Vec<u8>,
    /// Index into `read_buffer` of the next byte to surface to the reader.
    read_pos: usize,
    /// Outbound bytes accumulated since the last flush. Sent as one
    /// Binary message on `poll_flush`.
    write_buffer: Vec<u8>,
    /// Set on Close frame or stream end. Subsequent reads return EOF.
    closed: bool,
}

impl<S> WebSocketAdapter<S> {
    /// Wrap a stream/sink-shaped value in the adapter.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            read_buffer: Vec::new(),
            read_pos: 0,
            write_buffer: Vec::new(),
            closed: false,
        }
    }
}

impl<S> AsyncRead for WebSocketAdapter<S>
where
    S: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Drain any buffered data first.
        if self.read_pos < self.read_buffer.len() {
            let remaining = &self.read_buffer[self.read_pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.read_pos += to_copy;

            if self.read_pos >= self.read_buffer.len() {
                self.read_buffer.clear();
                self.read_pos = 0;
            }

            return Poll::Ready(Ok(()));
        }

        if self.closed {
            return Poll::Ready(Ok(()));
        }

        let inner = Pin::new(&mut self.inner);
        match inner.poll_next(cx) {
            Poll::Ready(Some(Ok(msg))) => match msg {
                Message::Binary(data) => {
                    if data.is_empty() {
                        return Poll::Ready(Err(io::Error::other(ERR_WS_EMPTY_BINARY_FRAME)));
                    }
                    if data.len() > MAX_WS_MESSAGE_SIZE {
                        return Poll::Ready(Err(io::Error::other(format!(
                            "WebSocket message too large: {} bytes (max {})",
                            data.len(),
                            MAX_WS_MESSAGE_SIZE
                        ))));
                    }

                    self.read_buffer = data.to_vec();
                    self.read_pos = 0;

                    let to_copy = self.read_buffer.len().min(buf.remaining());
                    buf.put_slice(&self.read_buffer[..to_copy]);
                    self.read_pos = to_copy;

                    if self.read_pos >= self.read_buffer.len() {
                        self.read_buffer.clear();
                        self.read_pos = 0;
                    }

                    Poll::Ready(Ok(()))
                }
                Message::Close(_) => {
                    self.closed = true;
                    Poll::Ready(Ok(()))
                }
                Message::Ping(_) | Message::Pong(_) | Message::Text(_) | Message::Frame(_) => {
                    // Ignore non-Binary frames — they're below the
                    // protocol level. Reschedule and try again.
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            },
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Err(io::Error::other(format!("WebSocket error: {}", e))))
            }
            Poll::Ready(None) => {
                self.closed = true;
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S> AsyncWrite for WebSocketAdapter<S>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Buffer until flush so the entire frame goes out as one
        // WebSocket Binary message.
        self.write_buffer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.write_buffer.is_empty() {
            // Still need to flush the underlying sink for any prior send.
            let inner = Pin::new(&mut self.inner);
            return match inner.poll_flush(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                    "WebSocket flush error: {}",
                    e
                )))),
                Poll::Pending => Poll::Pending,
            };
        }

        // Sink readiness check.
        {
            let inner = Pin::new(&mut self.inner);
            match inner.poll_ready(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(io::Error::other(format!(
                        "WebSocket ready error: {}",
                        e
                    ))));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        let data = std::mem::take(&mut self.write_buffer);
        let msg = Message::Binary(data.into());

        {
            let inner = Pin::new(&mut self.inner);
            if let Err(e) = inner.start_send(msg) {
                return Poll::Ready(Err(io::Error::other(format!(
                    "WebSocket send error: {}",
                    e
                ))));
            }
        }

        let inner = Pin::new(&mut self.inner);
        match inner.poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                "WebSocket flush error: {}",
                e
            )))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Send a Close frame, then close the sink.
        {
            let inner = Pin::new(&mut self.inner);
            match inner.poll_ready(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(io::Error::other(format!(
                        "WebSocket ready error: {}",
                        e
                    ))));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        {
            let inner = Pin::new(&mut self.inner);
            if let Err(e) = inner.start_send(Message::Close(None)) {
                return Poll::Ready(Err(io::Error::other(format!(
                    "WebSocket close error: {}",
                    e
                ))));
            }
        }

        let inner = Pin::new(&mut self.inner);
        match inner.poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                "WebSocket close error: {}",
                e
            )))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Accept a WebSocket upgrade with a slowloris-defense timeout.
///
/// Wraps [`tokio_tungstenite::accept_async`] in
/// [`crate::WS_HANDSHAKE_TIMEOUT`]. On elapse or upgrade failure, returns
/// an `io::Error` whose message is prefixed with
/// [`crate::WS_HANDSHAKE_FAILED_PREFIX`] so the per-daemon
/// `log_connection_error` downgrades scanner / non-WS-client noise to
/// debug level.
///
/// # Errors
///
/// Returns `io::Error` on timeout or upgrade failure. The connection is
/// dropped silently — at this layer the upgrade hasn't completed, so
/// there's no useful response to send.
pub async fn accept_ws_with_timeout<S>(
    stream: S,
) -> io::Result<tokio_tungstenite::WebSocketStream<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match tokio::time::timeout(
        crate::WS_HANDSHAKE_TIMEOUT,
        tokio_tungstenite::accept_async(stream),
    )
    .await
    {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(io::Error::other(format!(
            "{}{}",
            crate::WS_HANDSHAKE_FAILED_PREFIX,
            e
        ))),
        Err(_) => Err(io::Error::other(crate::WS_HANDSHAKE_TIMEOUT_MSG)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Mock WebSocket stream/sink for adapter unit tests.
    struct MockWebSocket {
        incoming: VecDeque<Result<Message, tokio_tungstenite::tungstenite::Error>>,
        outgoing: Vec<Message>,
        closed: bool,
    }

    impl MockWebSocket {
        fn new(messages: Vec<Message>) -> Self {
            Self {
                incoming: messages.into_iter().map(Ok).collect(),
                outgoing: Vec::new(),
                closed: false,
            }
        }
    }

    impl Stream for MockWebSocket {
        type Item = Result<Message, tokio_tungstenite::tungstenite::Error>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.incoming.pop_front())
        }
    }

    impl Sink<Message> for MockWebSocket {
        type Error = tokio_tungstenite::tungstenite::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            self.outgoing.push(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            self.closed = true;
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_read_single_message() {
        let mock = MockWebSocket::new(vec![Message::Binary(b"hello".to_vec().into())]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];
        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"hello");
    }

    #[tokio::test]
    async fn test_read_multiple_messages() {
        let mock = MockWebSocket::new(vec![
            Message::Binary(b"hello".to_vec().into()),
            Message::Binary(b"world".to_vec().into()),
        ]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"hello");

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"world");
    }

    #[tokio::test]
    async fn test_read_partial_buffer() {
        let mock = MockWebSocket::new(vec![Message::Binary(b"hello world".to_vec().into())]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 5];

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"hello");

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b" worl");

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(&buf[..n], b"d");
    }

    #[tokio::test]
    async fn test_read_eof_on_close() {
        let mock = MockWebSocket::new(vec![
            Message::Binary(b"hello".to_vec().into()),
            Message::Close(None),
        ]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);

        let n = adapter.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_write_and_flush() {
        let mock = MockWebSocket::new(vec![]);
        let mut adapter = WebSocketAdapter::new(mock);

        adapter.write_all(b"hello").await.unwrap();
        adapter.flush().await.unwrap();

        assert_eq!(adapter.inner.outgoing.len(), 1);
        assert!(matches!(
            &adapter.inner.outgoing[0],
            Message::Binary(data) if data.as_ref() == b"hello"
        ));
    }

    #[tokio::test]
    async fn test_write_accumulates_before_flush() {
        let mock = MockWebSocket::new(vec![]);
        let mut adapter = WebSocketAdapter::new(mock);

        adapter.write_all(b"hello").await.unwrap();
        adapter.write_all(b" world").await.unwrap();
        adapter.flush().await.unwrap();

        assert_eq!(adapter.inner.outgoing.len(), 1);
        assert!(matches!(
            &adapter.inner.outgoing[0],
            Message::Binary(data) if data.as_ref() == b"hello world"
        ));
    }

    #[tokio::test]
    async fn test_shutdown_sends_close() {
        let mock = MockWebSocket::new(vec![]);
        let mut adapter = WebSocketAdapter::new(mock);

        adapter.shutdown().await.unwrap();

        assert!(adapter.inner.closed);
        assert!(
            adapter
                .inner
                .outgoing
                .iter()
                .any(|m| matches!(m, Message::Close(_)))
        );
    }

    #[tokio::test]
    async fn test_oversized_message_rejected() {
        let oversized_data = vec![0u8; 2 * 1024 * 1024]; // 2 MB
        let mock = MockWebSocket::new(vec![Message::Binary(oversized_data.into())]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];
        let result = adapter.read(&mut buf).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[tokio::test]
    async fn test_empty_binary_message_is_protocol_error() {
        // Empty Binary frames have no payload — our protocol can't
        // produce one, so we treat it as a protocol violation rather
        // than a no-op (which would look like EOF to AsyncRead
        // consumers and silently drop the connection).
        let mock = MockWebSocket::new(vec![Message::Binary(vec![].into())]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];
        let err = adapter
            .read(&mut buf)
            .await
            .expect_err("empty Binary frame must surface as an I/O error");
        assert!(
            err.to_string().contains(ERR_WS_EMPTY_BINARY_FRAME),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn test_text_message_ignored() {
        let mock = MockWebSocket::new(vec![
            Message::Text("ignored".to_string().into()),
            Message::Binary(b"hello".to_vec().into()),
        ]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];
        let n = adapter.read(&mut buf).await.unwrap();

        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"hello");
    }

    #[tokio::test]
    async fn test_stream_end_returns_eof() {
        let mock = MockWebSocket::new(vec![]);
        let mut adapter = WebSocketAdapter::new(mock);

        let mut buf = [0u8; 10];
        let n = adapter.read(&mut buf).await.unwrap();

        assert_eq!(n, 0);
    }
}
