//! Network streaming and channel management

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::futures::{SinkExt, Stream};
use iced::stream;
use once_cell::sync::Lazy;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, mpsc};

use nexus_common::framing::MessageId;
use nexus_common::io::{read_server_message, send_client_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::i18n::t;
use crate::types::connection::CommandSender;
use crate::types::{Message, NetworkConnection};

use super::constants::STREAM_CHANNEL_SIZE;
use super::types::{LoginInfo, Reader, Writer};

/// Type alias for the connection registry
type ConnectionRegistry =
    Arc<Mutex<HashMap<usize, mpsc::UnboundedReceiver<(MessageId, ServerMessage)>>>>;

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

    /// Signal the network task to shut down
    pub fn shutdown(self) {
        let _ = self.tx.send(());
    }
}

/// Set up bidirectional communication channels and spawn network task
pub(super) async fn setup_communication_channels(
    reader: Reader,
    writer: Writer,
    login_info: LoginInfo,
    connection_id: usize,
    fingerprint: String,
) -> Result<NetworkConnection, String> {
    // Create channels for bidirectional communication
    // Command channel includes MessageId for request-response correlation
    let (cmd_tx, cmd_rx): (CommandSender, CommandReceiver) = mpsc::unbounded_channel();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<(MessageId, ServerMessage)>();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn separate reader and writer tasks for cancel-safety
    // The reader task only reads and never gets cancelled mid-frame
    // The writer task uses select! safely since cmd_rx.recv() is cancel-safe
    spawn_reader_writer_tasks(reader, writer, cmd_rx, msg_tx, shutdown_rx);

    // Register connection in global registry with pre-assigned ID
    register_connection(connection_id, msg_rx).await;

    Ok(NetworkConnection {
        tx: cmd_tx,
        session_id: login_info.session_id,
        connection_id,
        shutdown: Some(Arc::new(Mutex::new(Some(ShutdownHandle::new(shutdown_tx))))),
        is_admin: login_info.is_admin,
        permissions: login_info.permissions,
        server_name: login_info.server_name,
        server_description: login_info.server_description,
        server_version: login_info.server_version,
        server_image: login_info.server_image,
        chat_topic: login_info.chat_topic,
        chat_topic_set_by: login_info.chat_topic_set_by,
        max_connections_per_ip: login_info.max_connections_per_ip,
        max_transfers_per_ip: login_info.max_transfers_per_ip,
        certificate_fingerprint: fingerprint,
        locale: login_info.locale,
    })
}

/// Spawn separate reader and writer tasks for cancel-safe bidirectional communication
///
/// This solves the cancel-safety issue where `tokio::select!` could cancel a read
/// mid-frame when a write was ready. By using separate tasks:
/// - The reader task runs a simple loop without select!, so reads are never cancelled
/// - The writer task uses select! safely because `cmd_rx.recv()` is cancel-safe
/// - Both tasks share an `AtomicBool` to signal each other to stop
fn spawn_reader_writer_tasks(
    reader: Reader,
    writer: Writer,
    cmd_rx: CommandReceiver,
    msg_tx: mpsc::UnboundedSender<(MessageId, ServerMessage)>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    // Shared flag to signal both tasks to stop
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Spawn reader task
    let reader_stop = stop_flag.clone();
    let reader_msg_tx = msg_tx;
    tokio::spawn(async move {
        spawn_reader_task(reader, reader_msg_tx, reader_stop).await;
    });

    // Spawn writer task
    let writer_stop = stop_flag;
    tokio::spawn(async move {
        spawn_writer_task(writer, cmd_rx, shutdown_rx, writer_stop).await;
    });
}

/// Reader task - reads messages from server and forwards to UI
///
/// This task runs a simple loop without `select!`, ensuring reads are never
/// cancelled mid-frame. When the connection closes or an error occurs,
/// it sets the stop flag to signal the writer task.
async fn spawn_reader_task(
    mut reader: Reader,
    msg_tx: mpsc::UnboundedSender<(MessageId, ServerMessage)>,
    stop_flag: Arc<AtomicBool>,
) {
    loop {
        // Check if we should stop (writer signaled an error)
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Read the next message - this is now cancel-safe since we're not in a select!
        match read_server_message(&mut reader).await {
            Ok(Some(received)) => {
                // Send message ID and message to UI
                if msg_tx
                    .send((received.message_id, received.message))
                    .is_err()
                {
                    // UI receiver dropped, signal writer to stop
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
            }
            Ok(None) => {
                // Connection closed cleanly, signal writer to stop
                stop_flag.store(true, Ordering::Relaxed);
                break;
            }
            Err(_) => {
                // Error reading, signal writer to stop
                stop_flag.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
}

/// Writer task - sends messages from UI to server
///
/// This task uses `select!` to handle both outgoing messages and shutdown signals.
/// This is safe because `cmd_rx.recv()` is cancel-safe (no partial state).
async fn spawn_writer_task(
    mut writer: Writer,
    mut cmd_rx: CommandReceiver,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    stop_flag: Arc<AtomicBool>,
) {
    loop {
        // Check if reader signaled us to stop
        if stop_flag.load(Ordering::Relaxed) {
            // Gracefully close the TLS connection
            let _ = writer.get_mut().shutdown().await;
            break;
        }

        tokio::select! {
            // Send to server using the message ID provided by caller
            Some((message_id, msg)) = cmd_rx.recv() => {
                if send_client_message_with_id(&mut writer, &msg, message_id).await.is_err() {
                    // Error sending, signal reader to stop
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
            }
            // Shutdown signal from UI
            _ = &mut shutdown_rx => {
                // Signal reader to stop
                stop_flag.store(true, Ordering::Relaxed);
                // Gracefully close the TLS connection
                let _ = writer.get_mut().shutdown().await;
                break;
            }
            // Also check the stop flag periodically
            // This ensures we exit if the reader stopped but cmd_rx is empty
            else => {
                // cmd_rx closed (UI dropped the sender)
                stop_flag.store(true, Ordering::Relaxed);
                let _ = writer.get_mut().shutdown().await;
                break;
            }
        }
    }
}

/// Register connection in global registry with pre-assigned ID
async fn register_connection(
    connection_id: usize,
    msg_rx: mpsc::UnboundedReceiver<(MessageId, ServerMessage)>,
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
                while let Some((message_id, msg)) = receiver.recv().await {
                    let _ = output
                        .send(Message::ServerMessageReceived(
                            connection_id,
                            message_id,
                            msg,
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
