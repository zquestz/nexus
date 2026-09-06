//! Network streaming and channel management

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::futures::{SinkExt, Stream};
use iced::stream;
use once_cell::sync::Lazy;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_server_message_with_progress_timeout;
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::i18n::t;
use crate::types::connection::CommandSender;
use crate::types::{ConnectionInfo, Message, NetworkConnection};

use super::constants::{
    BBS_READ_PROGRESS_TIMEOUT, BBS_SHUTDOWN_TIMEOUT, BBS_WRITE_PROGRESS_TIMEOUT,
    ERR_BBS_WRITE_PROGRESS_TIMEOUT, PING_INTERVAL, STREAM_CHANNEL_SIZE,
};
use super::types::{LoginInfo, Reader, Writer};
use super::write_timeout::{
    send_client_message_with_progress_timeout, shutdown_writer_with_progress_timeout,
};

/// Type alias for the connection registry
/// The tuple is (message_id, message, receive_timestamp) where timestamp is Some for Pong messages
type ConnectionRegistry = Arc<
    Mutex<HashMap<usize, mpsc::UnboundedReceiver<(MessageId, ServerMessage, Option<Instant>)>>>,
>;

/// Type alias for the command channel receiver
type CommandReceiver = mpsc::UnboundedReceiver<(MessageId, ClientMessage)>;

/// Global registry for network receivers
pub static NETWORK_RECEIVERS: Lazy<ConnectionRegistry> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Handle for shutting down a network connection
#[derive(Debug)]
pub struct ShutdownHandle {
    tx: tokio::sync::oneshot::Sender<()>,
}

impl ShutdownHandle {
    /// Create a new shutdown handle
    pub(super) fn new(tx: tokio::sync::oneshot::Sender<()>) -> Self {
        Self { tx }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(tx: tokio::sync::oneshot::Sender<()>) -> Self {
        Self::new(tx)
    }

    /// Signal the network task to shut down
    pub fn shutdown(self) {
        let _ = self.tx.send(());
    }
}

/// Set up bidirectional communication channels and spawn network tasks
pub(super) async fn setup_communication_channels(
    reader: Reader,
    writer: Writer,
    login_info: LoginInfo,
    connection_info: ConnectionInfo,
    connection_id: usize,
) -> Result<NetworkConnection, String> {
    // Create channels for bidirectional communication
    // Command channel includes MessageId for request-response correlation
    let (cmd_tx, cmd_rx): (CommandSender, CommandReceiver) = mpsc::unbounded_channel();
    // Message channel includes optional timestamp for Pong messages (ping latency measurement)
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<(MessageId, ServerMessage, Option<Instant>)>();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    spawn_reader_writer_tasks(reader, writer, cmd_rx, msg_tx, shutdown_rx);

    // Register connection in global registry with pre-assigned ID
    register_connection(connection_id, msg_rx).await;

    Ok(NetworkConnection {
        tx: cmd_tx,
        connection_id,
        shutdown: Some(Arc::new(Mutex::new(Some(ShutdownHandle::new(shutdown_tx))))),
        is_admin: login_info.is_admin,
        user_id: login_info.user_id,
        nickname: login_info.nickname,
        permissions: login_info.permissions,
        features: login_info.features,
        server_name: login_info.server_name,
        server_description: login_info.server_description,
        public_address: login_info.public_address,
        server_version: login_info.server_version,
        server_image: login_info.server_image,
        channels: login_info.channels,
        chat_burst_limit: login_info.chat_burst_limit,
        chat_rate_limit: login_info.chat_rate_limit,
        max_connections_per_ip: login_info.max_connections_per_ip,
        max_outbound_rate: login_info.max_outbound_rate,
        max_transfers_per_ip: login_info.max_transfers_per_ip,
        file_reindex_interval: login_info.file_reindex_interval,
        persistent_channels: login_info.persistent_channels,
        auto_join_channels: login_info.auto_join_channels,
        min_password_strength: login_info.min_password_strength,
        log_level: login_info.log_level,
        scheduler_chunk_size: login_info.scheduler_chunk_size,
        connection_info,
    })
}

/// Spawn separate reader and writer tasks for bidirectional communication.
///
/// Independent scheduling keeps incoming traffic from starving commands or
/// keepalives. Ordinary I/O never cancels the other task's in-progress frame;
/// only connection termination does. The writer owns transport shutdown and
/// joins the reader before attempting it.
fn spawn_reader_writer_tasks(
    reader: Reader,
    writer: Writer,
    cmd_rx: CommandReceiver,
    msg_tx: mpsc::UnboundedSender<(MessageId, ServerMessage, Option<Instant>)>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    tokio::spawn(run_connection(reader, writer, cmd_rx, msg_tx, shutdown_rx));
}

/// Wire the peer-stop signals, start the reader, and run the writer in this task.
async fn run_connection<R: AsyncRead + Unpin + Send + 'static, W: AsyncWrite + Unpin>(
    reader: FrameReader<R>,
    writer: FrameWriter<W>,
    cmd_rx: CommandReceiver,
    msg_tx: mpsc::UnboundedSender<(MessageId, ServerMessage, Option<Instant>)>,
    shutdown_rx: oneshot::Receiver<()>,
) {
    // Dropping a sender wakes an idle peer, including on cancellation or unwind.
    // A shared flag alone cannot wake a task blocked on a socket or channel.
    let (stop_reader_tx, stop_reader_rx) = oneshot::channel::<()>();
    let (reader_stopped_tx, reader_stopped_rx) = oneshot::channel::<()>();
    let reader_task = tokio::spawn(run_reader_task(
        reader,
        msg_tx,
        stop_reader_rx,
        reader_stopped_tx,
    ));
    run_writer_task(
        writer,
        cmd_rx,
        shutdown_rx,
        reader_stopped_rx,
        stop_reader_tx,
        reader_task,
    )
    .await;
}

/// Reader task: receive complete server messages and forward them to the UI.
///
/// The read loop remains alive across all outgoing commands and keepalives.
/// This select only cancels it when the writer stops or the UI receiver closes,
/// at which point the connection is terminating and any partial frame is discarded.
/// EOF and read errors also end the task and wake the writer.
async fn run_reader_task<R: AsyncRead + Unpin>(
    reader: FrameReader<R>,
    msg_tx: mpsc::UnboundedSender<(MessageId, ServerMessage, Option<Instant>)>,
    mut stop_reader_rx: oneshot::Receiver<()>,
    reader_stopped_tx: oneshot::Sender<()>,
) {
    tokio::select! {
        biased;

        _ = &mut stop_reader_rx => {},
        _ = msg_tx.closed() => {},
        () = read_server_messages(reader, &msg_tx) => {},
    }
    // Close the UI channel before waking the writer. Queued complete messages
    // remain available while the writer spends up to 30 seconds shutting down TLS.
    drop(msg_tx);
    drop(reader_stopped_tx);
}

/// Writer task: send commands and keepalives, then shut down the connection.
///
/// The write loop runs until the command queue drains and closes or a send fails.
/// UI shutdown (including a dropped shutdown handle) and reader termination can
/// interrupt it. The frame flag remains set through writes and the final flush,
/// so an error or cancellation mid-frame closes by dropping the transport, never
/// by flushing or attempting graceful shutdown.
async fn run_writer_task<W: AsyncWrite + Unpin>(
    mut writer: FrameWriter<W>,
    cmd_rx: CommandReceiver,
    mut shutdown_rx: oneshot::Receiver<()>,
    mut reader_stopped_rx: oneshot::Receiver<()>,
    stop_reader_tx: oneshot::Sender<()>,
    reader_task: JoinHandle<()>,
) {
    let mut frame_write_in_progress = false;
    let write_failed = tokio::select! {
        biased;

        _ = &mut shutdown_rx => false,
        _ = &mut reader_stopped_rx => false,
        result = write_client_messages(&mut writer, cmd_rx, &mut frame_write_in_progress) => {
            result.is_err()
        }
    };

    // Wake and join even an idle reader before shutdown. This releases its socket
    // half and closes the UI message channel without waiting for the TLS deadline.
    drop(stop_reader_tx);
    let _ = reader_task.await;
    // Graceful shutdown is bounded and permitted only at a clean frame boundary.
    if !write_failed && !frame_write_in_progress {
        let _ = shutdown_writer_with_progress_timeout(
            writer.get_mut(),
            BBS_SHUTDOWN_TIMEOUT,
            ERR_BBS_WRITE_PROGRESS_TIMEOUT,
        )
        .await;
    }
}

/// Read complete server messages in order until EOF, a read error, or UI closure.
/// Idle waits are indefinite; once a frame starts, reads must keep making progress.
async fn read_server_messages<R: AsyncRead + Unpin>(
    mut reader: FrameReader<R>,
    msg_tx: &mpsc::UnboundedSender<(MessageId, ServerMessage, Option<Instant>)>,
) {
    while let Ok(Some(received)) =
        read_server_message_with_progress_timeout(&mut reader, BBS_READ_PROGRESS_TIMEOUT).await
    {
        // Capture Pong arrival in the network task, before the Iced event-loop
        // delay, so UI scheduling does not inflate the measured ping latency.
        let timestamp = if matches!(received.message, ServerMessage::Pong) {
            Some(Instant::now())
        } else {
            None
        };
        if msg_tx
            .send((received.message_id, received.message, timestamp))
            .is_err()
        {
            break;
        }
    }
}

/// Send queued commands with their original IDs and periodic NAT keepalives.
///
/// Only cancel-safe channel and timer waits compete in the inner select. Each
/// selected frame is sent and flushed before waiting again. The caller may cancel
/// this loop on connection termination; the frame flag stays set in that case.
async fn write_client_messages<W: AsyncWrite + Unpin>(
    writer: &mut FrameWriter<W>,
    mut cmd_rx: CommandReceiver,
    frame_write_in_progress: &mut bool,
) -> io::Result<()> {
    // Ping interval timer for NAT keepalive
    let mut ping_interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL));
    // Don't send ping immediately on connect
    ping_interval.reset();

    loop {
        let (message_id, message, reset_ping_timer) = tokio::select! {
            command = cmd_rx.recv() => {
                let Some((id, message)) = command else {
                    // recv() returns None only after the closed queue is drained.
                    return Ok(());
                };
                (id, message, true)
            }
            _ = ping_interval.tick() => {
                (MessageId::new(), ClientMessage::Ping, false)
            }
        };
        *frame_write_in_progress = true;
        send_client_message_with_progress_timeout(
            writer,
            &message,
            message_id,
            BBS_WRITE_PROGRESS_TIMEOUT,
            ERR_BBS_WRITE_PROGRESS_TIMEOUT,
        )
        .await?;
        *frame_write_in_progress = false;
        if reset_ping_timer {
            // A command already refreshed the NAT mapping; defer the next ping.
            ping_interval.reset();
        }
    }
}

/// Register connection in global registry with pre-assigned ID
async fn register_connection(
    connection_id: usize,
    msg_rx: mpsc::UnboundedReceiver<(MessageId, ServerMessage, Option<Instant>)>,
) {
    let mut receivers = NETWORK_RECEIVERS.lock().await;
    receivers.insert(connection_id, msg_rx);
}

/// Create Iced stream for network messages
///
/// Creates a subscription stream that receives messages from the server
/// for a specific connection. When the connection closes, sends a NetworkError
/// message and ends the stream.
///
/// Takes a reference to connection_id for compatibility with Subscription::run_with.
/// Returns a boxed stream to allow use as a function pointer.
pub fn network_stream(
    connection_id: &usize,
) -> std::pin::Pin<Box<dyn Stream<Item = Message> + Send>> {
    let connection_id = *connection_id;
    Box::pin(stream::channel(
        STREAM_CHANNEL_SIZE,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            // Get the receiver from the registry
            let mut rx = {
                let mut receivers = NETWORK_RECEIVERS.lock().await;
                receivers.remove(&connection_id)
            };

            if let Some(ref mut receiver) = rx {
                while let Some((message_id, msg, timestamp)) = receiver.recv().await {
                    let _ = output
                        .send(Message::ServerMessageReceived(
                            connection_id,
                            message_id,
                            msg,
                            timestamp,
                        ))
                        .await;
                }
            }

            // Connection closed - send error and end stream naturally
            let _ = output
                .send(Message::NetworkError(
                    connection_id,
                    t("err-connection-closed"),
                ))
                .await;

            // Stream ends naturally here, allowing Iced to clean up the subscription
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::Mutex as StdMutex;
    use std::task::{Context, Poll, ready};

    use iced::futures::StreamExt;
    use nexus_common::io::{
        client_message_to_frame_bytes, read_client_message, server_message_to_frame_bytes,
    };
    use rcgen::generate_simple_self_signed;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadBuf};
    use tokio::sync::{Notify, oneshot};
    use tokio::time::{self, Sleep};
    use tokio_rustls::rustls::ServerConfig;
    use tokio_rustls::rustls::crypto::ring;
    use tokio_rustls::rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    use super::*;

    #[derive(Default)]
    struct TrafficStats {
        bytes_read: usize,
        bytes_written: usize,
        first_write_after_read: Option<usize>,
        first_read_after_write: Option<usize>,
        write_polls: usize,
        stall_writes: bool,
        dropped: bool,
    }

    struct ObservedStream {
        inner: DuplexStream,
        stats: Arc<StdMutex<TrafficStats>>,
        blocked: Arc<Notify>,
        read_started: Arc<Notify>,
    }

    impl ObservedStream {
        fn new(inner: DuplexStream) -> Self {
            Self {
                inner,
                stats: Arc::new(StdMutex::new(TrafficStats::default())),
                blocked: Arc::new(Notify::new()),
                read_started: Arc::new(Notify::new()),
            }
        }
    }

    impl Drop for ObservedStream {
        fn drop(&mut self) {
            if let Ok(mut stats) = self.stats.lock() {
                stats.dropped = true;
            }
        }
    }

    impl AsyncRead for ObservedStream {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            let before = buf.filled().len();
            ready!(Pin::new(&mut this.inner).poll_read(cx, buf))?;
            let read = buf.filled().len() - before;
            if read > 0 {
                let mut stats = this.stats.lock().unwrap();
                let first_read = stats.bytes_read == 0;
                let written = stats.bytes_written;
                stats.first_read_after_write.get_or_insert(written);
                stats.bytes_read += read;
                if first_read {
                    this.read_started.notify_one();
                }
            }
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for ObservedStream {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            let mut stats = this.stats.lock().unwrap();
            stats.write_polls += 1;
            if stats.stall_writes {
                this.blocked.notify_one();
                return Poll::Pending;
            }
            let written = ready!(Pin::new(&mut this.inner).poll_write(cx, buf))?;
            if written > 0 {
                let read = stats.bytes_read;
                stats.first_write_after_read.get_or_insert(read);
                stats.bytes_written += written;
            }
            Poll::Ready(Ok(written))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
        }
    }

    struct TlsFixture {
        client: tokio_rustls::client::TlsStream<ObservedStream>,
        peer: tokio_rustls::server::TlsStream<DuplexStream>,
        stats: Arc<StdMutex<TrafficStats>>,
        blocked: Arc<Notify>,
    }

    async fn tls_fixture() -> TlsFixture {
        let _ = ring::default_provider().install_default();
        let config = crate::network::tls::create_tls_config();
        let provider = Arc::clone(config.crypto_provider());
        let certificate = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let server_config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.cert.der().clone()],
                PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
            )
            .unwrap();
        let connector = TlsConnector::from(Arc::new(config));
        let acceptor = TlsAcceptor::from(Arc::new(server_config));
        let (client, peer) = tokio::io::duplex(8192);
        let client = ObservedStream::new(client);
        let stats = Arc::clone(&client.stats);
        let blocked = Arc::clone(&client.blocked);
        let (client, peer) = time::timeout(Duration::from_secs(30), async {
            tokio::try_join!(
                connector.connect(ServerName::try_from("localhost").unwrap(), client),
                acceptor.accept(peer),
            )
        })
        .await
        .unwrap()
        .unwrap();
        *stats.lock().unwrap() = TrafficStats::default();
        TlsFixture {
            client,
            peer,
            stats,
            blocked,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn tls_shutdown_is_bounded_and_interrupted_frame_remains_drop_only() {
        for during_write in [false, true] {
            let TlsFixture {
                client,
                mut peer,
                stats,
                blocked,
            } = tls_fixture().await;
            stats.lock().unwrap().stall_writes = true;
            let (client_read, client_write) = tokio::io::split(client);
            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            if during_write {
                cmd_tx
                    .send((MessageId::new(), ClientMessage::Ping))
                    .unwrap();
            }
            let task = tokio::spawn(run_connection(
                FrameReader::new(BufReader::new(client_read)),
                FrameWriter::new(client_write),
                cmd_rx,
                msg_tx,
                shutdown_rx,
            ));
            if during_write {
                time::timeout(Duration::from_secs(1), blocked.notified())
                    .await
                    .expect("outgoing TLS frame must be stalled before cancellation");
            }
            let write_polls = stats.lock().unwrap().write_polls;
            let started = time::Instant::now();
            shutdown_tx.send(()).unwrap();
            assert!(
                time::timeout(Duration::from_secs(1), msg_rx.recv())
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                started.elapsed(),
                Duration::ZERO,
                "UI closure must not wait for TLS"
            );
            time::timeout(Duration::from_secs(120), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                started.elapsed(),
                Duration::from_secs(if during_write { 0 } else { 30 })
            );
            {
                let stats = stats.lock().unwrap();
                assert!(stats.dropped, "both halves must release the TLS transport");
                assert_eq!(stats.bytes_written, 0);
                if during_write {
                    assert_eq!(
                        stats.write_polls, write_polls,
                        "do not flush or send close_notify after cancellation"
                    );
                } else {
                    assert!(
                        stats.write_polls > 0,
                        "graceful close must reach the stalled transport"
                    );
                }
            }
            let mut received = Vec::new();
            let result = time::timeout(Duration::from_secs(1), peer.read_to_end(&mut received))
                .await
                .expect("peer must observe transport closure");
            assert!(result.is_err(), "no close_notify reached the peer");
            assert!(received.is_empty());
        }
    }

    async fn assert_hot_inbound_remains_responsive(trigger: &str) {
        let incoming =
            server_message_to_frame_bytes(&ServerMessage::Pong, MessageId::new()).unwrap();
        // More than a cooperative budget's worth of buffered reads: input stays
        // available across task yields, without an unbounded flood or timing race.
        let burst = incoming.repeat((2 * 1024 * 1024_usize).div_ceil(incoming.len()));
        let (peer, client) = tokio::io::duplex(burst.len());
        let client = ObservedStream::new(client);
        let stats = Arc::clone(&client.stats);
        let read_started = Arc::clone(&client.read_started);
        let (client_read, client_write) = tokio::io::split(client);
        let (peer_read, mut peer_write) = tokio::io::split(peer);
        let mut peer_reader = FrameReader::new(BufReader::new(peer_read));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let mut shutdown_tx = Some(shutdown_tx);
        let connection = run_connection(
            FrameReader::new(BufReader::new(client_read)),
            FrameWriter::new(client_write),
            cmd_rx,
            msg_tx,
            shutdown_rx,
        );
        tokio::pin!(connection);
        // Prime the keepalive timer while idle. Timing out this borrow leaves
        // both connection-loop futures alive for the incoming burst.
        assert!(
            time::timeout(Duration::from_secs(1), connection.as_mut())
                .await
                .is_err()
        );
        let id = MessageId::new();
        match trigger {
            "command" => cmd_tx.send((id, ClientMessage::Ping)).unwrap(),
            "keepalive" => time::advance(Duration::from_secs(PING_INTERVAL - 1)).await,
            "shutdown" => shutdown_tx.take().unwrap().send(()).unwrap(),
            _ => unreachable!(),
        }
        // Make the ping due before supplying input: advance yields to the
        // independent reader, while this test deliberately holds the writer.
        peer_write.write_all(&burst).await.unwrap();
        // Keep the writer unpolled until the independent reader has started.
        // A notification avoids a yield loop that could prevent paused-clock
        // timeouts from advancing if the reader never makes progress.
        time::timeout(Duration::from_secs(1), read_started.notified())
            .await
            .expect("independent reader must start while the writer is unpolled");
        {
            let stats = stats.lock().unwrap();
            assert!(
                stats.bytes_read > 0 && stats.bytes_read < burst.len(),
                "fixture must start reading without draining the inbound burst"
            );
            assert!(stats.first_write_after_read.is_none());
        }
        let receive = async {
            let received = read_client_message(&mut peer_reader).await.unwrap();
            if trigger == "shutdown" {
                assert!(received.is_none());
            } else {
                let received = received.expect("outgoing ping must arrive");
                assert!(matches!(received.message, ClientMessage::Ping));
                if trigger == "command" {
                    assert_eq!(received.message_id, id);
                }
                shutdown_tx.take().unwrap().send(()).unwrap();
            }
        };
        let drain = async {
            while let Some((_, message, timestamp)) = msg_rx.recv().await {
                assert!(matches!(message, ServerMessage::Pong));
                assert!(timestamp.is_some());
            }
        };
        time::timeout(Duration::from_secs(120), async {
            tokio::join!(connection.as_mut(), receive, drain)
        })
        .await
        .expect("finite inbound burst must not hang the diagnostic");
        let stats = stats.lock().unwrap();
        assert!(stats.dropped);
        if trigger == "shutdown" {
            assert!(
                stats.bytes_read > 0,
                "shutdown must occur after reading starts"
            );
            assert!(
                stats.bytes_read < burst.len(),
                "shutdown must not wait for the inbound burst to drain"
            );
            assert!(stats.first_write_after_read.is_none());
        } else {
            let read = stats
                .first_write_after_read
                .expect("outgoing frame must be written");
            assert!(read > 0, "{trigger} must be sent after reading starts");
            assert!(
                read < burst.len(),
                "{trigger} waited for all {read} bytes of continuously available inbound traffic",
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn hot_inbound_does_not_starve_queued_commands() {
        assert_hot_inbound_remains_responsive("command").await;
    }

    #[tokio::test(start_paused = true)]
    async fn hot_inbound_does_not_starve_keepalives() {
        assert_hot_inbound_remains_responsive("keepalive").await;
    }

    #[tokio::test(start_paused = true)]
    async fn hot_inbound_does_not_delay_shutdown() {
        assert_hot_inbound_remains_responsive("shutdown").await;
    }

    #[tokio::test(start_paused = true)]
    async fn hot_outbound_does_not_starve_incoming_messages() {
        let id = MessageId::new();
        let frame = client_message_to_frame_bytes(&ClientMessage::Ping, id).unwrap();
        let count = (2 * 1024 * 1024_usize).div_ceil(frame.len());
        let total_bytes = count * frame.len();
        let (mut peer, client) = tokio::io::duplex(total_bytes);
        let client = ObservedStream::new(client);
        let stats = Arc::clone(&client.stats);
        let (client_read, client_write) = tokio::io::split(client);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let connection = run_connection(
            FrameReader::new(BufReader::new(client_read)),
            FrameWriter::new(client_write),
            cmd_rx,
            msg_tx,
            shutdown_rx,
        );
        tokio::pin!(connection);
        assert!(
            time::timeout(Duration::from_secs(1), connection.as_mut())
                .await
                .is_err()
        );
        for _ in 0..count {
            cmd_tx.send((id, ClientMessage::Ping)).unwrap();
        }
        peer.write_all(&server_message_to_frame_bytes(&ServerMessage::Pong, id).unwrap())
            .await
            .unwrap();
        let receive = async {
            let (received_id, message, timestamp) = msg_rx.recv().await.unwrap();
            assert_eq!(received_id, id);
            assert!(matches!(message, ServerMessage::Pong));
            assert!(timestamp.is_some());
            shutdown_tx.send(()).unwrap();
        };
        time::timeout(Duration::from_secs(120), async {
            tokio::join!(connection.as_mut(), receive)
        })
        .await
        .expect("incoming message must arrive while commands remain queued");
        assert!(msg_rx.recv().await.is_none());
        let stats = stats.lock().unwrap();
        assert!(stats.dropped);
        assert!(
            stats.first_read_after_write.unwrap() < total_bytes,
            "incoming message waited for the entire outgoing burst"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn aborting_writer_task_wakes_and_releases_independent_reader() {
        for during_write in [false, true] {
            time::timeout(Duration::from_secs(5), async {
                let (mut peer, client) = tokio::io::duplex(8192);
                let client = ObservedStream::new(client);
                let stats = Arc::clone(&client.stats);
                let blocked = Arc::clone(&client.blocked);
                stats.lock().unwrap().stall_writes = during_write;
                let (client_read, client_write) = tokio::io::split(client);
                let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
                let (_shutdown_tx, shutdown_rx) = oneshot::channel();
                let task = tokio::spawn(run_connection(
                    FrameReader::new(BufReader::new(client_read)),
                    FrameWriter::new(client_write),
                    cmd_rx,
                    msg_tx,
                    shutdown_rx,
                ));
                let id = MessageId::new();
                peer.write_all(&server_message_to_frame_bytes(&ServerMessage::Pong, id).unwrap())
                    .await
                    .unwrap();
                let (received_id, message, _) = msg_rx.recv().await.unwrap();
                assert_eq!(received_id, id);
                assert!(matches!(message, ServerMessage::Pong));
                if during_write {
                    cmd_tx.send((id, ClientMessage::Ping)).unwrap();
                    blocked.notified().await;
                }

                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
                assert!(msg_rx.recv().await.is_none());
                assert!(cmd_tx.send((id, ClientMessage::Ping)).is_err());
                let mut received = Vec::new();
                peer.read_to_end(&mut received).await.unwrap();
                assert!(received.is_empty());
                assert!(stats.lock().unwrap().dropped);
            })
            .await
            .expect("cancelling the writer must wake and release the idle reader");
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum WriteFailure {
        Error,
        Zero,
        Stall,
        FlushError,
        FlushStall,
    }

    #[derive(Default)]
    struct WriteStats {
        bytes: Vec<u8>,
        flushes: usize,
        shutdowns: usize,
    }

    #[derive(Default)]
    struct TestWriter {
        stats: Arc<StdMutex<WriteStats>>,
        flushed: Arc<Notify>,
        blocked: Arc<Notify>,
        failure: Option<WriteFailure>,
        prefix_remaining: usize,
        delay: Option<Duration>,
        sleep: Option<Pin<Box<Sleep>>>,
        chunk_size: Option<usize>,
        stall_shutdown: bool,
    }

    impl TestWriter {
        fn poll_delay(&mut self, cx: &mut Context<'_>) -> Poll<()> {
            if let Some(delay) = self.delay {
                self.sleep
                    .get_or_insert_with(|| Box::pin(time::sleep(delay)))
                    .as_mut()
                    .poll(cx)
            } else {
                Poll::Ready(())
            }
        }
    }

    impl AsyncWrite for TestWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx));
            let mut len = buf.len().min(this.chunk_size.unwrap_or(usize::MAX));
            if let Some(
                failure @ (WriteFailure::Error | WriteFailure::Zero | WriteFailure::Stall),
            ) = this.failure
            {
                if this.prefix_remaining == 0 {
                    if matches!(failure, WriteFailure::Stall) {
                        this.blocked.notify_one();
                        return Poll::Pending;
                    }
                    // Recover immediately so an incorrect later send leaves extra bytes.
                    this.failure = None;
                    return Poll::Ready(match failure {
                        WriteFailure::Zero => Ok(0),
                        _ => Err(io::Error::other("injected BBS write failure")),
                    });
                }
                len = len.min(this.prefix_remaining);
            }
            this.stats
                .lock()
                .unwrap()
                .bytes
                .extend_from_slice(&buf[..len]);
            this.prefix_remaining = this.prefix_remaining.saturating_sub(len);
            this.sleep = None;
            Poll::Ready(Ok(len))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx));
            if let Some(failure @ (WriteFailure::FlushError | WriteFailure::FlushStall)) =
                this.failure
            {
                if matches!(failure, WriteFailure::FlushStall) {
                    this.blocked.notify_one();
                    return Poll::Pending;
                }
                this.failure = None;
                return Poll::Ready(Err(io::Error::other("injected BBS flush failure")));
            }
            this.stats.lock().unwrap().flushes += 1;
            this.flushed.notify_one();
            this.sleep = None;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.stats.lock().unwrap().shutdowns += 1;
            if self.stall_shutdown {
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        }
    }

    async fn assert_writer_failure_stops_sending(keepalive: bool) {
        let first_id = MessageId::from_bytes(b"000000000501").unwrap();
        let second_id = MessageId::from_bytes(b"000000000502").unwrap();
        let expected = client_message_to_frame_bytes(&ClientMessage::Ping, first_id).unwrap();

        for failure in [
            WriteFailure::Error,
            WriteFailure::Zero,
            WriteFailure::Stall,
            WriteFailure::FlushError,
            WriteFailure::FlushStall,
        ] {
            let writer = TestWriter {
                failure: Some(failure),
                prefix_remaining: 4,
                ..TestWriter::default()
            };
            let stats = Arc::clone(&writer.stats);
            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (_shutdown_tx, shutdown_rx) = oneshot::channel();
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
            let (mut peer, client) = tokio::io::duplex(8192);
            if !keepalive {
                cmd_tx.send((first_id, ClientMessage::Ping)).unwrap();
                cmd_tx.send((second_id, ClientMessage::Ping)).unwrap();
            }

            let start = time::Instant::now();
            time::timeout(
                Duration::from_secs(PING_INTERVAL + 120),
                run_connection(
                    FrameReader::new(BufReader::new(client)),
                    FrameWriter::new(writer),
                    cmd_rx,
                    msg_tx,
                    shutdown_rx,
                ),
            )
            .await
            .expect("writer must stop after a failed command or keepalive");

            let stalled = matches!(failure, WriteFailure::Stall | WriteFailure::FlushStall);
            assert_eq!(
                start.elapsed(),
                Duration::from_secs(
                    if keepalive { PING_INTERVAL } else { 0 } + if stalled { 60 } else { 0 }
                ),
                "keepalive={keepalive} / {failure:?}"
            );
            assert!(msg_rx.recv().await.is_none());
            assert!(
                peer.write_all(b"N").await.is_err(),
                "idle reader must be dropped"
            );
            assert!(cmd_tx.send((second_id, ClientMessage::Ping)).is_err());
            let stats = stats.lock().unwrap();
            if matches!(failure, WriteFailure::FlushError | WriteFailure::FlushStall) {
                // Keepalive IDs are generated inside the loop; queued commands retain their ID.
                if keepalive {
                    assert_eq!(stats.bytes.len(), expected.len());
                    assert!(stats.bytes.ends_with(b"{\"type\":\"Ping\"}\n"));
                } else {
                    assert_eq!(stats.bytes, expected.as_ref());
                }
            } else {
                assert_eq!(stats.bytes, expected[..4]);
            }
            assert_eq!(stats.flushes, 0);
            assert_eq!(stats.shutdowns, 0, "write failures remain drop-only paths");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn failed_bbs_commands_stop_without_sending_the_next_queued_frame() {
        assert_writer_failure_stops_sending(false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn failed_bbs_keepalives_stop_at_the_same_progress_deadline() {
        assert_writer_failure_stops_sending(true).await;
    }

    async fn assert_slow_writer_completes(keepalive: bool) {
        let writer = TestWriter {
            delay: Some(Duration::from_secs(45)),
            // A keepalive completes before the next ping tick. Commands exercise
            // partial writes over multiple ping intervals and must reset the timer.
            chunk_size: if keepalive { None } else { Some(4) },
            ..TestWriter::default()
        };
        let stats = Arc::clone(&writer.stats);
        let flushed = Arc::clone(&writer.flushed);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (_peer, client) = tokio::io::duplex(8192);
        let mut expected = Vec::new();
        if !keepalive {
            for id in [b"000000000501", b"000000000502"] {
                let id = MessageId::from_bytes(id).unwrap();
                expected.extend_from_slice(
                    &client_message_to_frame_bytes(&ClientMessage::Ping, id).unwrap(),
                );
                cmd_tx.send((id, ClientMessage::Ping)).unwrap();
            }
        }
        let start = time::Instant::now();
        let task = tokio::spawn(run_connection(
            FrameReader::new(BufReader::new(client)),
            FrameWriter::new(writer),
            cmd_rx,
            msg_tx,
            shutdown_rx,
        ));
        let expected_flushes = if keepalive { 1 } else { 2 };
        time::timeout(Duration::from_secs(3000), async {
            while stats.lock().unwrap().flushes < expected_flushes {
                flushed.notified().await;
            }
        })
        .await
        .expect("slow writes and flushes must complete with 45-second progress");
        assert!(start.elapsed() > Duration::from_secs(60));
        assert!(!task.is_finished());
        if keepalive {
            assert_eq!(start.elapsed(), Duration::from_secs(PING_INTERVAL + 45 * 2));
        }
        shutdown_tx.send(()).unwrap();
        time::timeout(Duration::from_secs(30), task)
            .await
            .unwrap()
            .unwrap();
        assert!(msg_rx.recv().await.is_none());
        let stats = stats.lock().unwrap();
        assert_eq!(stats.flushes, expected_flushes);
        assert_eq!(stats.shutdowns, 1);
        if keepalive {
            let reference =
                client_message_to_frame_bytes(&ClientMessage::Ping, MessageId::new()).unwrap();
            assert_eq!(stats.bytes.len(), reference.len());
            assert!(stats.bytes.ends_with(b"{\"type\":\"Ping\"}\n"));
        } else {
            assert_eq!(
                stats.bytes, expected,
                "no ping or other frame may interleave with commands"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_commands_allow_slow_partial_writes_and_flushes() {
        assert_slow_writer_completes(false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_keepalive_allows_slow_write_and_flush() {
        assert_slow_writer_completes(true).await;
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_shutdown_retains_its_thirty_second_whole_operation_limit() {
        for trigger in [
            "reader EOF",
            "reader error",
            "shutdown signal",
            "dropped shutdown handle",
            "closed command channel",
            "closed UI channel",
        ] {
            let writer = TestWriter {
                stall_shutdown: true,
                ..TestWriter::default()
            };
            let stats = Arc::clone(&writer.stats);
            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let (msg_tx, msg_rx) = mpsc::unbounded_channel();
            let (mut peer, client) = tokio::io::duplex(8192);
            let mut cmd_tx = Some(cmd_tx);
            let mut shutdown_tx = Some(shutdown_tx);
            let mut msg_rx = Some(msg_rx);
            // Only the writer can prioritize its own already-ready shutdown
            // signal ahead of commands. Reader completion races independently.
            if matches!(trigger, "shutdown signal" | "dropped shutdown handle") {
                cmd_tx
                    .as_ref()
                    .unwrap()
                    .send((MessageId::new(), ClientMessage::Ping))
                    .unwrap();
            }
            match trigger {
                "reader EOF" => peer.shutdown().await.unwrap(),
                "reader error" => peer.write_all(b"invalid").await.unwrap(),
                "shutdown signal" => shutdown_tx.take().unwrap().send(()).unwrap(),
                "dropped shutdown handle" => drop(shutdown_tx.take()),
                "closed command channel" => drop(cmd_tx.take()),
                "closed UI channel" => drop(msg_rx.take()),
                _ => {}
            }

            let start = time::Instant::now();
            let task = tokio::spawn(run_connection(
                FrameReader::new(BufReader::new(client)),
                FrameWriter::new(writer),
                cmd_rx,
                msg_tx,
                shutdown_rx,
            ));
            if let Some(ref mut msg_rx) = msg_rx {
                assert!(
                    time::timeout(Duration::from_secs(1), msg_rx.recv())
                        .await
                        .unwrap()
                        .is_none()
                );
                assert!(
                    !task.is_finished(),
                    "UI must be notified before stalled TLS shutdown finishes"
                );
                assert_eq!(start.elapsed(), Duration::ZERO);
            }
            time::timeout(Duration::from_secs(120), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(start.elapsed(), Duration::from_secs(30), "{trigger}");
            let stats = stats.lock().unwrap();
            assert!(stats.bytes.is_empty());
            assert!(stats.shutdowns > 0);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_reader_stalls_are_bounded_in_header_payload_and_terminator() {
        let message = ServerMessage::Pong;
        let bytes = server_message_to_frame_bytes(&message, MessageId::new()).unwrap();
        let payload_len = serde_json::to_vec(&message).unwrap().len();
        let header_len = bytes.len() - payload_len - 1;
        for prefix in [
            1,
            header_len - 1,
            header_len,
            header_len + payload_len / 2,
            bytes.len() - 1,
        ] {
            let (mut peer, client) = tokio::io::duplex(8192);
            peer.write_all(&bytes[..prefix]).await.unwrap();
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
            let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (_shutdown_tx, shutdown_rx) = oneshot::channel();
            let writer = TestWriter::default();
            let stats = Arc::clone(&writer.stats);
            let start = time::Instant::now();
            time::timeout(
                Duration::from_secs(120),
                run_connection(
                    FrameReader::new(BufReader::new(client)),
                    FrameWriter::new(writer),
                    cmd_rx,
                    msg_tx,
                    shutdown_rx,
                ),
            )
            .await
            .expect("a partial frame must not leave the reader parked indefinitely");
            assert_eq!(start.elapsed(), Duration::from_secs(60), "prefix={prefix}");
            assert_eq!(
                stats.lock().unwrap().shutdowns,
                1,
                "idle writer must stop too"
            );
            assert!(
                msg_rx.recv().await.is_none(),
                "partial frame must not reach the UI"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_reader_deadline_renews_on_byte_progress() {
        let (mut peer, client) = tokio::io::duplex(8192);
        peer.write_all(b"N").await.unwrap();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = oneshot::channel();
        let start = time::Instant::now();
        // join! retains the peer after the second byte, preventing an early EOF.
        let (_peer, ()) = time::timeout(Duration::from_secs(120), async {
            tokio::join!(
                async {
                    time::sleep(Duration::from_secs(45)).await;
                    peer.write_all(b"X").await.unwrap();
                    peer
                },
                run_connection(
                    FrameReader::new(BufReader::new(client)),
                    FrameWriter::new(TestWriter::default()),
                    cmd_rx,
                    msg_tx,
                    shutdown_rx,
                ),
            )
        })
        .await
        .unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(45 + 60));
        assert!(msg_rx.recv().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_reader_allows_idle_and_slow_frames_without_losing_order_or_timestamps() {
        let (mut peer, client) = tokio::io::duplex(8192);
        let first_id = MessageId::from_bytes(b"000000000503").unwrap();
        let second_id = MessageId::from_bytes(b"000000000504").unwrap();
        let frames = [
            server_message_to_frame_bytes(&ServerMessage::Pong, first_id).unwrap(),
            server_message_to_frame_bytes(
                &ServerMessage::Error {
                    message: "test notice".to_string(),
                    command: None,
                    disconnect: false,
                },
                second_id,
            )
            .unwrap(),
        ];
        let expected_elapsed = Duration::from_secs(
            600 * 2
                + 45 * frames
                    .iter()
                    .map(|frame| frame.len().div_ceil(3) as u64)
                    .sum::<u64>(),
        );
        let sender = tokio::spawn(async move {
            for frame in frames {
                time::sleep(Duration::from_secs(600)).await;
                for chunk in frame.chunks(3) {
                    time::sleep(Duration::from_secs(45)).await;
                    peer.write_all(chunk).await.unwrap();
                }
            }
        });
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = oneshot::channel();
        let start = time::Instant::now();
        time::timeout(
            expected_elapsed + Duration::from_secs(120),
            run_connection(
                FrameReader::new(BufReader::new(client)),
                FrameWriter::new(TestWriter::default()),
                cmd_rx,
                msg_tx,
                shutdown_rx,
            ),
        )
        .await
        .unwrap();
        sender.await.unwrap();
        assert_eq!(start.elapsed(), expected_elapsed);
        let (id, message, timestamp) = msg_rx.recv().await.unwrap();
        assert_eq!(id, first_id);
        assert!(matches!(message, ServerMessage::Pong));
        assert!(timestamp.is_some());
        let (id, message, timestamp) = msg_rx.recv().await.unwrap();
        assert_eq!(id, second_id);
        assert!(
            matches!(message, ServerMessage::Error { message, disconnect: false, .. } if message == "test notice")
        );
        assert!(timestamp.is_none());
        assert!(msg_rx.recv().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn bbs_reader_eof_does_not_forward_an_incomplete_frame() {
        let bytes = server_message_to_frame_bytes(&ServerMessage::Pong, MessageId::new()).unwrap();
        for prefix in [0, 1, bytes.len() - 1] {
            let (mut peer, client) = tokio::io::duplex(8192);
            peer.write_all(&bytes[..prefix]).await.unwrap();
            drop(peer);
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
            let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (_shutdown_tx, shutdown_rx) = oneshot::channel();
            let start = time::Instant::now();
            time::timeout(
                Duration::from_secs(120),
                run_connection(
                    FrameReader::new(BufReader::new(client)),
                    FrameWriter::new(TestWriter::default()),
                    cmd_rx,
                    msg_tx,
                    shutdown_rx,
                ),
            )
            .await
            .unwrap();
            assert_eq!(start.elapsed(), Duration::ZERO);
            assert!(msg_rx.recv().await.is_none());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_socket_write_releases_both_halves_and_the_ui_channel() {
        let (mut peer, client) = tokio::io::duplex(4);
        let (client_read, client_write) = tokio::io::split(client);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = oneshot::channel();
        let id = MessageId::new();
        let expected = client_message_to_frame_bytes(&ClientMessage::Ping, id).unwrap();
        cmd_tx.send((id, ClientMessage::Ping)).unwrap();

        // The peer keeps both directions open but never reads or sends. A flag alone
        // cannot wake the idle reader after the four-byte write buffer fills.
        let start = time::Instant::now();
        time::timeout(
            Duration::from_secs(120),
            run_connection(
                FrameReader::new(BufReader::new(client_read)),
                FrameWriter::new(client_write),
                cmd_rx,
                msg_tx,
                shutdown_rx,
            ),
        )
        .await
        .expect("writer timeout must terminate the entire connection");
        assert_eq!(start.elapsed(), Duration::from_secs(60));
        assert!(msg_rx.recv().await.is_none());
        assert!(cmd_tx.send((id, ClientMessage::Ping)).is_err());

        let mut received = Vec::new();
        time::timeout(Duration::from_secs(1), peer.read_to_end(&mut received))
            .await
            .expect("peer must observe EOF when both connection halves are dropped")
            .unwrap();
        assert_eq!(received, expected[..4]);
        assert!(peer.write_all(b"N").await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn terminal_events_cancel_in_progress_writes_without_flushing_or_shutdown() {
        let id = MessageId::new();
        let expected = client_message_to_frame_bytes(&ClientMessage::Ping, id).unwrap();
        for failure in [WriteFailure::Stall, WriteFailure::FlushStall] {
            for trigger in [
                "reader EOF",
                "reader error",
                "shutdown signal",
                "dropped shutdown handle",
                "closed UI channel",
            ] {
                let (mut peer, client) = tokio::io::duplex(8192);
                peer.write_all(b"N").await.unwrap();
                let writer = TestWriter {
                    failure: Some(failure),
                    prefix_remaining: 4,
                    ..TestWriter::default()
                };
                let stats = Arc::clone(&writer.stats);
                let blocked = Arc::clone(&writer.blocked);
                let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                let (msg_tx, msg_rx) = mpsc::unbounded_channel();
                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                let mut msg_rx = Some(msg_rx);
                let mut shutdown_tx = Some(shutdown_tx);
                cmd_tx.send((id, ClientMessage::Ping)).unwrap();
                cmd_tx
                    .send((MessageId::new(), ClientMessage::Ping))
                    .unwrap();
                let task = tokio::spawn(run_connection(
                    FrameReader::new(BufReader::new(client)),
                    FrameWriter::new(writer),
                    cmd_rx,
                    msg_tx,
                    shutdown_rx,
                ));
                time::timeout(Duration::from_secs(1), blocked.notified())
                    .await
                    .expect("test must reach the pending write or flush before cancellation");

                let start = time::Instant::now();
                match trigger {
                    "reader EOF" => peer.shutdown().await.unwrap(),
                    "reader error" => peer.write_all(b"invalid").await.unwrap(),
                    "shutdown signal" => shutdown_tx.take().unwrap().send(()).unwrap(),
                    "dropped shutdown handle" => drop(shutdown_tx.take()),
                    "closed UI channel" => drop(msg_rx.take()),
                    _ => unreachable!(),
                }
                time::timeout(Duration::from_secs(1), task)
                    .await
                    .expect("terminal event must not wait for the write-progress deadline")
                    .unwrap();
                assert_eq!(start.elapsed(), Duration::ZERO, "{trigger} / {failure:?}");
                assert!(cmd_tx.send((id, ClientMessage::Ping)).is_err());
                if let Some(ref mut msg_rx) = msg_rx {
                    assert!(msg_rx.recv().await.is_none());
                }
                let stats = stats.lock().unwrap();
                let expected_len = if matches!(failure, WriteFailure::FlushStall) {
                    expected.len()
                } else {
                    4
                };
                assert_eq!(stats.bytes, expected[..expected_len]);
                assert_eq!(stats.flushes, 0);
                assert_eq!(stats.shutdowns, 0, "{trigger} / {failure:?}");
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn closing_command_channel_drains_queued_frames_then_stops() {
        let (_peer, client) = tokio::io::duplex(8192);
        let writer = TestWriter::default();
        let stats = Arc::clone(&writer.stats);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = oneshot::channel();
        let mut expected = Vec::new();
        for _ in 0..2 {
            let id = MessageId::new();
            expected.extend_from_slice(
                &client_message_to_frame_bytes(&ClientMessage::Ping, id).unwrap(),
            );
            cmd_tx.send((id, ClientMessage::Ping)).unwrap();
        }
        drop(cmd_tx);
        let start = time::Instant::now();
        time::timeout(
            Duration::from_secs(1),
            run_connection(
                FrameReader::new(BufReader::new(client)),
                FrameWriter::new(writer),
                cmd_rx,
                msg_tx,
                shutdown_rx,
            ),
        )
        .await
        .expect("closed command channel must not wait for a ping or shutdown signal");
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert!(msg_rx.recv().await.is_none());
        let stats = stats.lock().unwrap();
        assert_eq!(stats.bytes, expected);
        assert_eq!(stats.flushes, 2);
        assert_eq!(stats.shutdowns, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn ordinary_writes_do_not_cancel_a_partial_incoming_frame() {
        let incoming_id = MessageId::new();
        let incoming = server_message_to_frame_bytes(&ServerMessage::Pong, incoming_id).unwrap();
        let header_len =
            incoming.len() - serde_json::to_vec(&ServerMessage::Pong).unwrap().len() - 1;
        for prefix in [1, header_len, incoming.len() - 1] {
            let (peer, client) = tokio::io::duplex(8192);
            let (peer_read, mut peer_write) = tokio::io::split(peer);
            let (client_read, client_write) = tokio::io::split(client);
            let mut peer_reader = FrameReader::new(BufReader::new(peer_read));
            peer_write.write_all(&incoming[..prefix]).await.unwrap();
            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let task = tokio::spawn(run_connection(
                FrameReader::new(BufReader::new(client_read)),
                FrameWriter::new(client_write),
                cmd_rx,
                msg_tx,
                shutdown_rx,
            ));

            for _ in 0..2 {
                let id = MessageId::new();
                cmd_tx.send((id, ClientMessage::Ping)).unwrap();
                let sent = time::timeout(
                    Duration::from_secs(1),
                    read_client_message(&mut peer_reader),
                )
                .await
                .unwrap()
                .unwrap()
                .unwrap();
                assert_eq!(sent.message_id, id);
                assert!(matches!(sent.message, ClientMessage::Ping));
                assert!(matches!(
                    msg_rx.try_recv(),
                    Err(mpsc::error::TryRecvError::Empty)
                ));
            }

            peer_write.write_all(&incoming[prefix..]).await.unwrap();
            let (id, message, timestamp) = time::timeout(Duration::from_secs(1), msg_rx.recv())
                .await
                .unwrap()
                .expect("partial frame must survive both outgoing commands");
            assert_eq!(id, incoming_id);
            assert!(matches!(message, ServerMessage::Pong));
            assert!(timestamp.is_some());
            assert!(!task.is_finished());

            shutdown_tx.send(()).unwrap();
            time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap();
            assert!(msg_rx.recv().await.is_none());
            assert!(
                time::timeout(
                    Duration::from_secs(1),
                    read_client_message(&mut peer_reader)
                )
                .await
                .unwrap()
                .unwrap()
                .is_none()
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn writer_failure_delivers_queued_messages_then_exactly_one_disconnect_notification() {
        const CONNECTION_ID: usize = usize::MAX - 501;
        let (mut peer, client) = tokio::io::duplex(8192);
        let first_id = MessageId::new();
        let second_id = MessageId::new();
        for (id, message) in [
            (first_id, ServerMessage::Pong),
            (
                second_id,
                ServerMessage::Error {
                    message: "queued notice".to_string(),
                    command: None,
                    disconnect: false,
                },
            ),
        ] {
            peer.write_all(&server_message_to_frame_bytes(&message, id).unwrap())
                .await
                .unwrap();
        }
        peer.write_all(b"N").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = oneshot::channel();
        let writer = TestWriter {
            failure: Some(WriteFailure::Error),
            prefix_remaining: 4,
            ..TestWriter::default()
        };
        let connection = run_connection(
            FrameReader::new(BufReader::new(client)),
            FrameWriter::new(writer),
            cmd_rx,
            msg_tx,
            shutdown_rx,
        );
        tokio::pin!(connection);
        assert!(
            time::timeout(Duration::from_secs(1), connection.as_mut())
                .await
                .is_err()
        );
        // Confirm both messages reached the UI queue before failing the writer;
        // independent tasks do not guarantee a reader-first execution order.
        assert_eq!(msg_rx.len(), 2);
        register_connection(CONNECTION_ID, msg_rx).await;
        cmd_tx
            .send((MessageId::new(), ClientMessage::Ping))
            .unwrap();
        time::timeout(Duration::from_secs(1), connection.as_mut())
            .await
            .unwrap();

        let mut events = network_stream(&CONNECTION_ID);
        let first = time::timeout(Duration::from_secs(1), events.next())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(first, Message::ServerMessageReceived(CONNECTION_ID, id, ServerMessage::Pong, Some(_)) if id == first_id)
        );
        let second = time::timeout(Duration::from_secs(1), events.next())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(second, Message::ServerMessageReceived(CONNECTION_ID, id, ServerMessage::Error { message, .. }, None) if id == second_id && message == "queued notice")
        );
        let terminal = time::timeout(Duration::from_secs(1), events.next())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(terminal, Message::NetworkError(CONNECTION_ID, _)));
        assert!(
            time::timeout(Duration::from_secs(1), events.next())
                .await
                .unwrap()
                .is_none()
        );
        assert!(!NETWORK_RECEIVERS.lock().await.contains_key(&CONNECTION_ID));
    }
}
