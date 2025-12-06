//! Network streaming and channel management

use std::collections::HashMap;
use std::sync::Arc;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use once_cell::sync::Lazy;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, mpsc};

use nexus_common::framing::MessageId;
use nexus_common::io::{read_server_message, send_client_message};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::i18n::t;
use crate::types::{Message, NetworkConnection};

use super::constants::STREAM_CHANNEL_SIZE;
use super::types::{LoginInfo, Reader, Writer};

/// Type alias for the connection registry
type ConnectionRegistry =
    Arc<Mutex<HashMap<usize, mpsc::UnboundedReceiver<(MessageId, ServerMessage)>>>>;

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
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<(MessageId, ServerMessage)>();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn background task for bidirectional communication
    spawn_network_task(reader, writer, cmd_rx, msg_tx, shutdown_rx);

    // Register connection in global registry with pre-assigned ID
    register_connection(connection_id, msg_rx).await;

    Ok(NetworkConnection {
        tx: cmd_tx,
        session_id: login_info.session_id,
        connection_id,
        shutdown: Some(Arc::new(Mutex::new(Some(ShutdownHandle::new(shutdown_tx))))),
        is_admin: login_info.is_admin,
        permissions: login_info.permissions,
        chat_topic: login_info.chat_topic,
        chat_topic_set_by: login_info.chat_topic_set_by,
        certificate_fingerprint: fingerprint,
        locale: login_info.locale,
    })
}

/// Spawn background task to handle bidirectional communication
fn spawn_network_task(
    mut reader: Reader,
    mut writer: Writer,
    mut cmd_rx: mpsc::UnboundedReceiver<ClientMessage>,
    msg_tx: mpsc::UnboundedSender<(MessageId, ServerMessage)>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // Read from server using new framing format
                result = read_server_message(&mut reader) => {
                    match result {
                        Ok(Some(received)) => {
                            // Send message ID and message to UI
                            if msg_tx.send((received.message_id, received.message)).is_err() {
                                break; // UI closed
                            }
                        }
                        Ok(None) => break, // Connection closed cleanly
                        Err(_) => break, // Error reading
                    }
                }
                // Send to server using new framing format
                Some(msg) = cmd_rx.recv() => {
                    if send_client_message(&mut writer, &msg).await.is_err() {
                        break;
                    }
                }
                // Shutdown signal
                _ = &mut shutdown_rx => {
                    // Properly close TLS connection with shutdown
                    let _ = writer.get_mut().shutdown().await;
                    break;
                }
            }
        }
    });
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
pub fn network_stream(connection_id: usize) -> impl Stream<Item = Message> {
    stream::channel(STREAM_CHANNEL_SIZE, move |mut output| async move {
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
    })
}
