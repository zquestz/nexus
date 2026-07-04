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
//! coalesces bytes and emits them as Binary messages: at `flush`, and
//! whenever [`WS_WRITE_CHUNK_SIZE`] accumulates — so unbounded frames
//! stream with bounded memory instead of ballooning until flush. Text /
//! Ping / Pong frames from peers are silently ignored (they're a layer
//! below the protocol).
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
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};

/// Maximum size of a single inbound WebSocket message (1 MB).
///
/// Prevents memory exhaustion from malicious clients sending huge
/// messages. The largest legitimate BBS-side messages are ~700 KB
/// (news / server-image uploads). File-data chunks are 64 KB, matching
/// TCP streaming behavior.
pub const MAX_WS_MESSAGE_SIZE: usize = 1024 * 1024;

/// Outbound chunking threshold for the adapter's write buffer (64 KB).
///
/// `poll_write` drains the buffer as one Binary message once it holds
/// this much, so an unbounded frame (a direct-path file download, a
/// trusted news listing) streams as bounded messages instead of
/// accumulating whole in memory until `flush`. Bytes below the
/// threshold still coalesce until `flush`, so ordinary protocol frames
/// go out as one message each. Matches the 64 KB transfer streaming
/// chunk and stays far under peers' [`MAX_WS_MESSAGE_SIZE`] inbound
/// cap; WS message boundaries carry no protocol meaning (the
/// connection is a byte stream), so chunked emission is transparent.
pub const WS_WRITE_CHUNK_SIZE: usize = 64 * 1024;

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
    /// `Bytes` so the tungstenite message moves in without a copy.
    read_buffer: Bytes,
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
            read_buffer: Bytes::new(),
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

                    self.read_buffer = data;
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

impl<S> WebSocketAdapter<S>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    /// Send the accumulated `write_buffer` as one Binary message once
    /// the sink is ready. `Pending` leaves the buffer intact for the
    /// retry. Does not flush the sink — `poll_flush` decides when
    /// transmission must be pushed through.
    fn poll_emit_buffer(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_ready(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => {
                return Poll::Ready(Err(io::Error::other(format!(
                    "WebSocket ready error: {}",
                    e
                ))));
            }
            Poll::Pending => return Poll::Pending,
        }

        let data = std::mem::take(&mut self.write_buffer);
        if let Err(e) = Pin::new(&mut self.inner).start_send(Message::Binary(data.into())) {
            return Poll::Ready(Err(io::Error::other(format!(
                "WebSocket send error: {}",
                e
            ))));
        }
        Poll::Ready(Ok(()))
    }
}

impl<S> AsyncWrite for WebSocketAdapter<S>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        // Drain a full chunk before accepting more bytes. This bounds
        // adapter memory at WS_WRITE_CHUNK_SIZE and propagates sink
        // backpressure to the writer — Pending accepts nothing, so no
        // bytes are lost.
        while this.write_buffer.len() >= WS_WRITE_CHUNK_SIZE {
            match this.poll_emit_buffer(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        // Accept at most one chunk's worth (partial writes are the
        // AsyncWrite contract; `write_all` loops), keeping the buffer —
        // and therefore every emitted Binary message — within
        // WS_WRITE_CHUNK_SIZE. Bytes below the threshold coalesce until
        // `flush`, so small protocol frames still go out as one message.
        let capacity = WS_WRITE_CHUNK_SIZE - this.write_buffer.len();
        let n = buf.len().min(capacity);
        this.write_buffer.extend_from_slice(&buf[..n]);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        if !this.write_buffer.is_empty() {
            match this.poll_emit_buffer(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        match Pin::new(&mut this.inner).poll_flush(cx) {
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
/// Caps tungstenite's message/frame reassembly limits to
/// [`MAX_WS_MESSAGE_SIZE`] so an oversized inbound frame is rejected
/// incrementally instead of after buffering tungstenite's 64 MiB
/// default (the adapter's own size check then stays as
/// defense-in-depth). Wraps
/// [`tokio_tungstenite::accept_async_with_config`] in
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
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_MESSAGE_SIZE))
        .max_frame_size(Some(MAX_WS_MESSAGE_SIZE));
    match tokio::time::timeout(
        crate::WS_HANDSHAKE_TIMEOUT,
        tokio_tungstenite::accept_async_with_config(stream, Some(config)),
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
        /// Remaining `poll_ready` calls that report `Pending`, for
        /// backpressure tests.
        pending_ready_polls: usize,
    }

    impl MockWebSocket {
        fn new(messages: Vec<Message>) -> Self {
            Self {
                incoming: messages.into_iter().map(Ok).collect(),
                outgoing: Vec::new(),
                closed: false,
                pending_ready_polls: 0,
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
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            if self.pending_ready_polls > 0 {
                self.pending_ready_polls -= 1;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
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
    async fn test_large_write_emits_bounded_messages() {
        let mock = MockWebSocket::new(vec![]);
        let mut adapter = WebSocketAdapter::new(mock);

        // 200 KB → three full 64 KB messages mid-write + the remainder
        // at flush. Byte pattern catches reordering/loss on reassembly.
        let data: Vec<u8> = (0..200 * 1024).map(|i| (i % 251) as u8).collect();
        adapter.write_all(&data).await.unwrap();
        adapter.flush().await.unwrap();

        assert!(
            adapter.inner.outgoing.len() >= 4,
            "expected chunked messages, got {}",
            adapter.inner.outgoing.len()
        );
        let mut reassembled = Vec::new();
        for msg in &adapter.inner.outgoing {
            match msg {
                Message::Binary(bytes) => {
                    assert!(
                        bytes.len() <= WS_WRITE_CHUNK_SIZE,
                        "message exceeds chunk cap: {}",
                        bytes.len()
                    );
                    reassembled.extend_from_slice(bytes);
                }
                other => panic!("unexpected non-Binary message: {other:?}"),
            }
        }
        assert_eq!(reassembled, data);
    }

    #[tokio::test]
    async fn test_write_backpressure_when_sink_not_ready() {
        let mut mock = MockWebSocket::new(vec![]);
        mock.pending_ready_polls = 1;
        let mut adapter = WebSocketAdapter::new(mock);

        // Fill the buffer exactly to the emission threshold (accepted
        // without emitting — drain happens on the next write).
        let chunk = vec![0u8; WS_WRITE_CHUNK_SIZE];
        adapter.write_all(&chunk).await.unwrap();
        assert!(adapter.inner.outgoing.is_empty());

        // Sink not ready: poll_write must return Pending WITHOUT
        // accepting bytes, so nothing is lost under backpressure.
        let extra = [1u8; 16];
        let first =
            std::future::poll_fn(|cx| Poll::Ready(Pin::new(&mut adapter).poll_write(cx, &extra)))
                .await;
        assert!(
            matches!(first, Poll::Pending),
            "expected Pending while sink not ready"
        );
        assert!(adapter.inner.outgoing.is_empty());

        // Sink ready again: the retry drains the full chunk, then
        // accepts the new bytes.
        let n = adapter.write(&extra).await.unwrap();
        assert_eq!(n, extra.len());
        assert_eq!(adapter.inner.outgoing.len(), 1);
        assert!(matches!(
            &adapter.inner.outgoing[0],
            Message::Binary(bytes) if bytes.len() == WS_WRITE_CHUNK_SIZE
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
