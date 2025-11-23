//! Network connection and message handling

use crate::types::{Message, NetworkConnection};
use iced::futures::{SinkExt, Stream};
use iced::stream;
use nexus_common::PROTOCOL_VERSION;
use nexus_common::io::send_client_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::net::Ipv6Addr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};

/// Connection timeout duration (30 seconds)
const CONNECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Type alias for TCP read half with buffering
type Reader = BufReader<tokio::net::tcp::OwnedReadHalf>;

/// Type alias for TCP write half
type Writer = tokio::net::tcp::OwnedWriteHalf;

/// Type alias for the connection registry
type ConnectionRegistry =
    Arc<Mutex<std::collections::HashMap<usize, mpsc::UnboundedReceiver<ServerMessage>>>>;

/// Global registry for network receivers
pub static NETWORK_RECEIVERS: once_cell::sync::Lazy<ConnectionRegistry> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashMap::new())));

/// Handle for shutting down a network connection
#[derive(Debug)]
pub struct ShutdownHandle {
    tx: tokio::sync::oneshot::Sender<()>,
}

impl ShutdownHandle {
    /// Signal the network task to shut down
    pub fn shutdown(self) {
        let _ = self.tx.send(());
    }
}

/// Connect to server, perform handshake and login
pub async fn connect_to_server(
    server_address: String,
    port: String,
    username: String,
    password: String,
    connection_id: usize,
) -> Result<NetworkConnection, String> {
    // Establish TCP connection
    let stream = establish_connection(&server_address, &port).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Perform handshake and login
    perform_handshake(&mut reader, &mut writer).await?;
    let session_id = perform_login(&mut reader, &mut writer, username, password).await?;

    // Set up bidirectional communication
    setup_communication_channels(reader, writer, session_id, connection_id).await
}

/// Establish TCP connection to the server
async fn establish_connection(address: &str, port: &str) -> Result<TcpStream, String> {
    let addr: Ipv6Addr = address
        .parse()
        .map_err(|e| format!("Invalid IPv6 address '{}': {}", address, e))?;
    let port: u16 = port
        .parse()
        .map_err(|e| format!("Invalid port number '{}': {}", port, e))?;

    let socket_addr = std::net::SocketAddr::from((addr, port));

    // Add timeout for connection
    tokio::time::timeout(CONNECTION_TIMEOUT, TcpStream::connect(socket_addr))
        .await
        .map_err(|_| {
            format!(
                "Connection timed out after {} seconds",
                CONNECTION_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("Connection failed: {}", e))
}

/// Perform protocol handshake with the server
async fn perform_handshake(reader: &mut Reader, writer: &mut Writer) -> Result<(), String> {
    // Send handshake
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(writer, &handshake)
        .await
        .map_err(|e| format!("Failed to send handshake: {}", e))?;

    // Wait for handshake response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read handshake response: {}", e))?;

    match serde_json::from_str::<ServerMessage>(&line.trim()) {
        Ok(ServerMessage::HandshakeResponse { success: true, .. }) => Ok(()),
        Ok(ServerMessage::HandshakeResponse {
            success: false,
            error,
            ..
        }) => Err(format!("Handshake failed: {}", error.unwrap_or_default())),
        Ok(_) => Err("Unexpected handshake response".to_string()),
        Err(e) => Err(format!("Failed to parse handshake response: {}", e)),
    }
}

/// Perform login and return session ID
async fn perform_login(
    reader: &mut Reader,
    writer: &mut Writer,
    username: String,
    password: String,
) -> Result<String, String> {
    // Send login
    let login = ClientMessage::Login {
        username,
        password,
        features: vec!["chat".to_string()],
    };
    send_client_message(writer, &login)
        .await
        .map_err(|e| format!("Failed to send login: {}", e))?;

    // Wait for login response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read login response: {}", e))?;

    match serde_json::from_str::<ServerMessage>(&line.trim()) {
        Ok(ServerMessage::LoginResponse {
            success: true,
            session_id: Some(id),
            ..
        }) => Ok(id),
        Ok(ServerMessage::LoginResponse {
            success: true,
            session_id: None,
            ..
        }) => Err("No session ID received".to_string()),
        Ok(ServerMessage::LoginResponse {
            success: false,
            error: Some(msg),
            ..
        }) => Err(msg),
        Ok(ServerMessage::LoginResponse {
            success: false,
            error: None,
            ..
        }) => Err("Login failed".to_string()),
        Ok(ServerMessage::Error { message, .. }) => Err(message),
        Ok(_) => Err("Unexpected login response".to_string()),
        Err(e) => Err(format!("Failed to parse login response: {}", e)),
    }
}

/// Set up bidirectional communication channels and spawn network task
async fn setup_communication_channels(
    reader: Reader,
    writer: Writer,
    session_id: String,
    connection_id: usize,
) -> Result<NetworkConnection, String> {
    // Create channels for bidirectional communication
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn background task for bidirectional communication
    spawn_network_task(reader, writer, cmd_rx, msg_tx, shutdown_rx);

    // Register connection in global registry with pre-assigned ID
    register_connection(connection_id, msg_rx).await;

    Ok(NetworkConnection {
        tx: cmd_tx,
        session_id,
        connection_id,
        shutdown: Some(Arc::new(Mutex::new(Some(ShutdownHandle {
            tx: shutdown_tx,
        })))),
    })
}

/// Spawn background task to handle bidirectional communication
fn spawn_network_task(
    mut reader: Reader,
    mut writer: Writer,
    mut cmd_rx: mpsc::UnboundedReceiver<ClientMessage>,
    msg_tx: mpsc::UnboundedSender<ServerMessage>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            tokio::select! {
                // Read from server
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => break, // Connection closed
                        Ok(_) => {
                            // Parse and send message to UI
                            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(line.trim()) {
                                if msg_tx.send(server_msg).is_err() {
                                    break; // UI closed
                                }
                            }
                            line.clear();
                        }
                        Err(_) => break,
                    }
                }
                // Send to server
                Some(msg) = cmd_rx.recv() => {
                    if send_client_message(&mut writer, &msg).await.is_err() {
                        break;
                    }
                }
                // Shutdown signal
                _ = &mut shutdown_rx => {
                    // Explicitly drop writer to close TCP connection
                    drop(writer);
                    break;
                }
            }
        }
    });
}

/// Register connection in global registry with pre-assigned ID
async fn register_connection(connection_id: usize, msg_rx: mpsc::UnboundedReceiver<ServerMessage>) {
    let mut receivers = NETWORK_RECEIVERS.lock().await;
    receivers.insert(connection_id, msg_rx);
}

/// Create Iced stream for network messages
pub fn network_stream(connection_id: usize) -> impl Stream<Item = Message> {
    stream::channel(100, move |mut output| async move {
        // Get the receiver from the registry
        let mut rx = {
            let mut receivers = NETWORK_RECEIVERS.lock().await;
            receivers.remove(&connection_id)
        };

        if let Some(ref mut receiver) = rx {
            while let Some(msg) = receiver.recv().await {
                let _ = output
                    .send(Message::ServerMessageReceived(connection_id, msg))
                    .await;
            }
        }

        // Connection closed - send error and end stream naturally
        let _ = output
            .send(Message::NetworkError(
                connection_id,
                "Connection closed".to_string(),
            ))
            .await;

        // Stream ends naturally here, allowing Iced to clean up the subscription
    })
}
