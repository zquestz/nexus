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

/// Global registry for network receivers (public for cleanup on disconnect)
pub static NETWORK_RECEIVERS: once_cell::sync::Lazy<
    Arc<Mutex<std::collections::HashMap<usize, mpsc::UnboundedReceiver<ServerMessage>>>>,
> = once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashMap::new())));

/// Global counter for connection IDs
pub static NEXT_CONNECTION_ID: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Connect to the server, perform handshake and login
pub async fn connect_to_server(
    server_address: String,
    port: String,
    username: String,
    password: String,
) -> Result<NetworkConnection, String> {
    // Parse address and port
    let addr: Ipv6Addr = server_address
        .parse()
        .map_err(|_| "Invalid IPv6 address".to_string())?;
    let port: u16 = port
        .parse()
        .map_err(|_| "Invalid port number".to_string())?;

    // Connect to server
    let socket_addr = std::net::SocketAddr::from((addr, port));
    let stream = TcpStream::connect(socket_addr)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send handshake
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(&mut writer, &handshake)
        .await
        .map_err(|e| format!("Failed to send handshake: {}", e))?;

    // Wait for handshake response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read handshake response: {}", e))?;

    match serde_json::from_str::<ServerMessage>(&line.trim()) {
        Ok(ServerMessage::HandshakeResponse {
            success,
            version: _,
            error,
        }) => {
            if !success {
                return Err(format!("Handshake failed: {}", error.unwrap_or_default()));
            }
        }
        _ => return Err("Unexpected handshake response".to_string()),
    }

    // Send login
    let login = ClientMessage::Login {
        username: username.clone(),
        password: password.clone(),
        features: vec!["chat".to_string()],
    };
    send_client_message(&mut writer, &login)
        .await
        .map_err(|e| format!("Failed to send login: {}", e))?;

    // Wait for login response
    line.clear();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read login response: {}", e))?;

    let session_id = match serde_json::from_str::<ServerMessage>(&line.trim()) {
        Ok(ServerMessage::LoginResponse {
            success,
            session_id,
            error,
        }) => {
            if !success {
                return Err(format!("Login failed: {}", error.unwrap_or_default()));
            }
            session_id.ok_or_else(|| "No session ID received".to_string())?
        }
        _ => return Err("Unexpected login response".to_string()),
    };

    // Create channels for bidirectional communication
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Spawn task to handle bidirectional communication
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
            }
        }
    });

    // Generate unique connection ID and store receiver
    let connection_id = NEXT_CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    {
        let mut receivers = NETWORK_RECEIVERS.lock().await;
        receivers.insert(connection_id, msg_rx);
    }

    Ok(NetworkConnection {
        tx: cmd_tx,
        session_id: session_id.to_string(),
        connection_id,
    })
}

/// Create a stream that reads messages from the network receiver
pub fn network_stream(connection_id: usize) -> impl Stream<Item = Message> {
    stream::channel(100, move |mut output| async move {
        // Get the receiver from the registry
        let mut rx = {
            let mut receivers = NETWORK_RECEIVERS.lock().await;
            receivers.remove(&connection_id)
        };

        if let Some(ref mut receiver) = rx {
            while let Some(msg) = receiver.recv().await {
                let _ = output.send(Message::ServerMessageReceived(msg)).await;
            }
        }

        // Connection closed
        let _ = output
            .send(Message::NetworkError("Connection closed".to_string()))
            .await;

        // Keep the stream alive but do nothing
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    })
}
