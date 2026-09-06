use std::io;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::server_message_to_frame_bytes;
use nexus_common::protocol::ServerMessage;

use crate::constants::{BBS_WRITE_PROGRESS_TIMEOUT, ERR_BBS_WRITE_TIMEOUT, ERR_BBS_WRITE_ZERO};

/// Send one BBS frame. After a write or flush failure, close without another frame.
pub(crate) async fn send_server_message_with_progress_timeout<W>(
    writer: &mut FrameWriter<W>,
    message: &ServerMessage,
    message_id: MessageId,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let bytes = server_message_to_frame_bytes(message, message_id)?;
    write_frame_bytes_with_progress_timeout(writer, &bytes, true, ERR_BBS_WRITE_TIMEOUT).await
}

/// Write serialized BBS bytes without adding framing. Callers must preserve frame
/// boundaries and flush only at the end of a complete frame.
pub(crate) async fn write_frame_bytes_with_progress_timeout<W>(
    writer: &mut FrameWriter<W>,
    mut bytes: &[u8],
    flush: bool,
    timeout_error: &'static str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let inner = writer.get_mut();
    while !bytes.is_empty() {
        match time::timeout(BBS_WRITE_PROGRESS_TIMEOUT, inner.write(bytes)).await {
            Ok(Ok(0)) => {
                return Err(io::Error::new(io::ErrorKind::WriteZero, ERR_BBS_WRITE_ZERO));
            }
            Ok(Ok(written)) => bytes = &bytes[written..],
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(io::Error::new(io::ErrorKind::TimedOut, timeout_error)),
        }
    }

    if flush {
        match time::timeout(BBS_WRITE_PROGRESS_TIMEOUT, inner.flush()).await {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, timeout_error)),
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    //! Deterministic stream behavior shared by writer and connection tests.

    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, ready};
    use std::time::Duration;

    use nexus_common::websocket::WebSocketAdapter;
    use rcgen::generate_simple_self_signed;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio::time::{self, Instant, Sleep};
    use tokio_rustls::rustls::crypto::ring;
    use tokio_rustls::rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
    use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    pub(crate) trait TestIo: AsyncRead + AsyncWrite + Unpin + Send + Sync {}
    impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> TestIo for T {}
    pub(crate) type BoxedTestIo = Box<dyn TestIo>;

    #[derive(Clone, Copy, Debug)]
    pub(crate) enum TestTransport {
        Plain,
        Tls,
        WebSocket,
    }

    pub(crate) struct TestTransportPair {
        pub(crate) client: BoxedTestIo,
        pub(crate) server: BoxedTestIo,
        pub(crate) state: Arc<Mutex<WriteState>>,
    }

    impl TestTransport {
        pub(crate) async fn pair(self, capacity: usize) -> TestTransportPair {
            let (client, server) = tokio::io::duplex(capacity);
            let (server, state) = TestStream::new(server);
            if matches!(self, Self::Plain) {
                return TestTransportPair {
                    client: Box::new(client),
                    server: Box::new(server),
                    state,
                };
            }

            let certificate = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let provider = Arc::new(ring::default_provider());
            let mut roots = RootCertStore::empty();
            roots.add(certificate.cert.der().clone()).unwrap();
            let client_config = ClientConfig::builder_with_provider(Arc::clone(&provider))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let server_config = ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![certificate.cert.der().clone()],
                    PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
                )
                .unwrap();
            let connector = TlsConnector::from(Arc::new(client_config));
            let acceptor = TlsAcceptor::from(Arc::new(server_config));
            let (client, server) = time::timeout(Duration::from_secs(30), async {
                tokio::try_join!(
                    connector.connect(ServerName::try_from("localhost").unwrap(), client),
                    acceptor.accept(server),
                )
            })
            .await
            .unwrap()
            .unwrap();
            if matches!(self, Self::Tls) {
                return TestTransportPair {
                    client: Box::new(client),
                    server: Box::new(server),
                    state,
                };
            }
            let ((client, _), server) = time::timeout(Duration::from_secs(30), async {
                tokio::try_join!(
                    tokio_tungstenite::client_async("wss://localhost/", client),
                    tokio_tungstenite::accept_async(server),
                )
            })
            .await
            .unwrap()
            .unwrap();
            TestTransportPair {
                client: Box::new(WebSocketAdapter::new(client)),
                server: Box::new(WebSocketAdapter::new(server)),
                state,
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub(crate) enum WriteFailure {
        Error,
        Zero,
        Stall,
        FlushError,
        FlushStall,
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub(crate) enum ShutdownBehavior {
        #[default]
        Immediate,
        Error,
        Delayed(Duration),
        Stall,
    }

    #[derive(Default)]
    pub(crate) struct WriteState {
        pub(crate) bytes: Vec<u8>,
        pub(crate) flushes: usize,
        pub(crate) shutdowns: usize,
        pub(crate) shutdown_behavior: ShutdownBehavior,
        pub(crate) shutdown_started: Option<Instant>,
        pub(crate) failure_at: Option<Instant>,
        pub(crate) delay: Option<Duration>,
        pub(crate) dropped: bool,
        failure: Option<WriteFailure>,
        prefix_remaining: usize,
    }

    impl WriteState {
        pub(crate) fn failing(failure: WriteFailure, prefix_len: usize) -> Self {
            Self {
                failure: Some(failure),
                prefix_remaining: prefix_len,
                ..Self::default()
            }
        }

        pub(crate) fn slow() -> Self {
            Self {
                delay: Some(Duration::from_secs(45)),
                ..Self::default()
            }
        }
    }

    pub(crate) struct TestStream<S> {
        inner: S,
        state: Arc<Mutex<WriteState>>,
        delay: Option<Pin<Box<Sleep>>>,
        shutdown_delay: Option<Pin<Box<Sleep>>>,
    }

    impl<S> TestStream<S> {
        pub(crate) fn new(inner: S) -> (Self, Arc<Mutex<WriteState>>) {
            let state = Arc::new(Mutex::new(WriteState::default()));
            (
                Self {
                    inner,
                    state: Arc::clone(&state),
                    delay: None,
                    shutdown_delay: None,
                },
                state,
            )
        }

        fn poll_delay(&mut self, cx: &mut Context<'_>) -> Poll<()> {
            if let Some(duration) = self.state.lock().unwrap().delay {
                self.delay
                    .get_or_insert_with(|| Box::pin(time::sleep(duration)))
                    .as_mut()
                    .poll(cx)
            } else {
                Poll::Ready(())
            }
        }
    }

    impl<S> Drop for TestStream<S> {
        fn drop(&mut self) {
            if let Ok(mut state) = self.state.lock() {
                state.dropped = true;
            }
        }
    }

    impl<S: AsyncRead + Unpin> AsyncRead for TestStream<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
        }
    }

    impl<S: AsyncWrite + Unpin> AsyncWrite for TestStream<S> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx));
            let mut state = this.state.lock().unwrap();
            let mut len = buf.len();
            if state.delay.is_some() {
                len = len.min(4);
            }
            if let Some(
                failure @ (WriteFailure::Error | WriteFailure::Zero | WriteFailure::Stall),
            ) = state.failure
            {
                if state.prefix_remaining == 0 {
                    state.failure_at.get_or_insert_with(Instant::now);
                    if matches!(failure, WriteFailure::Stall) {
                        return Poll::Pending;
                    }
                    // Recover after an error so an incorrect later send is observable.
                    state.failure = None;
                    return Poll::Ready(match failure {
                        WriteFailure::Zero => Ok(0),
                        _ => Err(io::Error::other("injected write failure")),
                    });
                }
                len = len.min(state.prefix_remaining);
            }
            let written = ready!(Pin::new(&mut this.inner).poll_write(cx, &buf[..len]))?;
            state.bytes.extend_from_slice(&buf[..written]);
            state.prefix_remaining = state.prefix_remaining.saturating_sub(written);
            this.delay = None;
            Poll::Ready(Ok(written))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx));
            let mut state = this.state.lock().unwrap();
            if let Some(failure @ (WriteFailure::FlushError | WriteFailure::FlushStall)) =
                state.failure
            {
                state.failure_at.get_or_insert_with(Instant::now);
                if matches!(failure, WriteFailure::FlushStall) {
                    return Poll::Pending;
                }
                state.failure = None;
                return Poll::Ready(Err(io::Error::other("injected flush failure")));
            }
            ready!(Pin::new(&mut this.inner).poll_flush(cx))?;
            state.flushes += 1;
            this.delay = None;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let behavior = {
                let mut state = this.state.lock().unwrap();
                state.shutdowns += 1;
                state.shutdown_started.get_or_insert_with(Instant::now);
                state.shutdown_behavior
            };
            match behavior {
                ShutdownBehavior::Immediate => {}
                ShutdownBehavior::Error => {
                    return Poll::Ready(Err(io::Error::other("injected shutdown failure")));
                }
                ShutdownBehavior::Delayed(duration) => {
                    ready!(
                        this.shutdown_delay
                            .get_or_insert_with(|| Box::pin(time::sleep(duration)))
                            .as_mut()
                            .poll(cx)
                    );
                }
                ShutdownBehavior::Stall => return Poll::Pending,
            }
            Pin::new(&mut this.inner).poll_shutdown(cx)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nexus_common::io::send_server_message_with_id;

    use super::test_helpers::{TestStream, TestTransport, WriteFailure, WriteState};
    use super::*;
    use crate::constants::ERR_BBS_CHUNK_WRITE_TIMEOUT;
    use crate::handlers::DirectWriter;

    #[derive(Clone, Copy, Debug)]
    enum WritePath {
        Message,
        Handler,
        Frame,
        Chunks,
    }

    const WRITE_PATHS: [WritePath; 4] = [
        WritePath::Message,
        WritePath::Handler,
        WritePath::Frame,
        WritePath::Chunks,
    ];

    impl WritePath {
        async fn send<W: AsyncWrite + Unpin>(
            self,
            writer: &mut FrameWriter<W>,
            message: &ServerMessage,
            id: MessageId,
        ) -> io::Result<()> {
            match self {
                Self::Message => {
                    send_server_message_with_progress_timeout(writer, message, id).await
                }
                Self::Handler => DirectWriter::new(writer).send_message(message, id).await,
                Self::Frame | Self::Chunks => {
                    let bytes = server_message_to_frame_bytes(message, id)?;
                    if matches!(self, Self::Chunks) {
                        write_frame_bytes_with_progress_timeout(
                            writer,
                            &bytes[..16],
                            false,
                            self.timeout_error(),
                        )
                        .await?;
                        write_frame_bytes_with_progress_timeout(
                            writer,
                            &bytes[16..],
                            true,
                            self.timeout_error(),
                        )
                        .await
                    } else {
                        write_frame_bytes_with_progress_timeout(
                            writer,
                            &bytes,
                            true,
                            self.timeout_error(),
                        )
                        .await
                    }
                }
            }
        }

        fn timeout_error(self) -> &'static str {
            if matches!(self, Self::Chunks) {
                ERR_BBS_CHUNK_WRITE_TIMEOUT
            } else {
                ERR_BBS_WRITE_TIMEOUT
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn encrypted_bbs_writes_observe_transport_backpressure() {
        for transport in [TestTransport::Tls, TestTransport::WebSocket] {
            for path in WRITE_PATHS {
                // TLS can absorb a transient raw-socket zero write; application-level
                // WriteZero handling is covered by the plaintext writer matrix below.
                for failure in [
                    WriteFailure::Error,
                    WriteFailure::Stall,
                    WriteFailure::FlushError,
                    WriteFailure::FlushStall,
                ] {
                    let pair = transport.pair(8192).await;
                    *pair.state.lock().unwrap() = WriteState::failing(failure, 8);
                    let mut writer = FrameWriter::new(pair.server);
                    let started = time::Instant::now();
                    let result = time::timeout(
                        Duration::from_secs(120),
                        path.send(&mut writer, &ServerMessage::Pong, MessageId::new()),
                    )
                    .await
                    .expect("encrypted write must observe its progress limit");
                    assert!(result.is_err(), "{transport:?} / {path:?} / {failure:?}");
                    let stalled = matches!(failure, WriteFailure::Stall | WriteFailure::FlushStall);
                    assert_eq!(
                        started.elapsed(),
                        Duration::from_secs(if stalled { 60 } else { 0 })
                    );
                    assert!(pair.state.lock().unwrap().failure_at.is_some());
                    drop(writer);
                    assert!(pair.state.lock().unwrap().dropped);
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_write_failures_stop_at_the_failed_operation() {
        let message = ServerMessage::Pong;
        let id = MessageId::new();
        let bytes = server_message_to_frame_bytes(&message, id).unwrap();

        for path in WRITE_PATHS {
            for failure in [
                WriteFailure::Error,
                WriteFailure::Zero,
                WriteFailure::Stall,
                WriteFailure::FlushError,
                WriteFailure::FlushStall,
            ] {
                for prefix in [0, 4, bytes.len() - 1] {
                    let (stream, state) = TestStream::new(Vec::new());
                    *state.lock().unwrap() = WriteState::failing(failure, prefix);
                    let mut writer = FrameWriter::new(stream);
                    let start = time::Instant::now();
                    let err = path.send(&mut writer, &message, id).await.unwrap_err();
                    let stalled = matches!(failure, WriteFailure::Stall | WriteFailure::FlushStall);

                    let expected_kind = match failure {
                        WriteFailure::Stall | WriteFailure::FlushStall => io::ErrorKind::TimedOut,
                        WriteFailure::Zero => io::ErrorKind::WriteZero,
                        _ => io::ErrorKind::Other,
                    };
                    assert_eq!(err.kind(), expected_kind, "{path:?} / {failure:?}");
                    assert_eq!(
                        start.elapsed(),
                        Duration::from_secs(if stalled { 60 } else { 0 }),
                        "{path:?} / {failure:?}"
                    );
                    if stalled {
                        assert_eq!(err.to_string(), path.timeout_error());
                    }
                    let state = state.lock().unwrap();
                    let expected_len =
                        if matches!(failure, WriteFailure::FlushError | WriteFailure::FlushStall) {
                            bytes.len()
                        } else {
                            prefix
                        };
                    assert_eq!(state.bytes, bytes[..expected_len]);
                    assert!(state.failure_at.is_some());
                    assert_eq!(state.flushes, 0);
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_writes_allow_slow_progress_beyond_the_old_whole_frame_deadline() {
        let message = ServerMessage::Error {
            message: "x".repeat(256),
            command: None,
            disconnect: false,
        };
        let id = MessageId::new();
        let mut reference = FrameWriter::new(Vec::new());
        send_server_message_with_id(&mut reference, &message, id)
            .await
            .unwrap();
        let expected = reference.into_inner();

        for path in WRITE_PATHS {
            let (stream, state) = TestStream::new(Vec::new());
            *state.lock().unwrap() = WriteState::slow();
            let mut writer = FrameWriter::new(stream);
            let start = time::Instant::now();
            path.send(&mut writer, &message, id).await.unwrap();

            assert!(start.elapsed() > Duration::from_secs(30 * 60), "{path:?}");
            assert_eq!(
                start.elapsed(),
                Duration::from_secs(45 * (expected.len().div_ceil(4) as u64 + 1))
            );
            let state = state.lock().unwrap();
            assert_eq!(state.bytes, expected);
            assert_eq!(state.flushes, 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_write_deadline_renews_after_each_successful_write() {
        for path in WRITE_PATHS {
            let (stream, state) = TestStream::new(Vec::new());
            let mut failure = WriteState::failing(WriteFailure::Stall, 8);
            failure.delay = Some(Duration::from_secs(45));
            *state.lock().unwrap() = failure;
            let mut writer = FrameWriter::new(stream);
            let start = time::Instant::now();
            let err = path
                .send(&mut writer, &ServerMessage::Pong, MessageId::new())
                .await
                .unwrap_err();

            assert_eq!(err.kind(), io::ErrorKind::TimedOut);
            assert_eq!(start.elapsed(), Duration::from_secs(45 * 2 + 60));
            assert_eq!(state.lock().unwrap().bytes.len(), 8);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn nonfinal_egress_chunk_does_not_flush() {
        let (stream, state) = TestStream::new(Vec::new());
        *state.lock().unwrap() = WriteState::failing(WriteFailure::FlushStall, 0);
        let mut writer = FrameWriter::new(stream);
        write_frame_bytes_with_progress_timeout(
            &mut writer,
            b"frame prefix",
            false,
            ERR_BBS_CHUNK_WRITE_TIMEOUT,
        )
        .await
        .unwrap();
        assert!(state.lock().unwrap().failure_at.is_none());

        let start = time::Instant::now();
        let err = write_frame_bytes_with_progress_timeout(
            &mut writer,
            b"\n",
            true,
            ERR_BBS_CHUNK_WRITE_TIMEOUT,
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), ERR_BBS_CHUNK_WRITE_TIMEOUT);
        assert_eq!(start.elapsed(), Duration::from_secs(60));
        assert_eq!(state.lock().unwrap().bytes, b"frame prefix\n");
    }
}
