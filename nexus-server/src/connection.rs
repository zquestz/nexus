//! Client connection handling

use crate::db::UserDb;
use crate::handlers::{self, HandlerContext};
use crate::users::UserManager;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::io;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// Handle a client connection
pub async fn handle_connection(
    socket: TcpStream,
    peer_addr: SocketAddr,
    user_manager: UserManager,
    user_db: UserDb,
) -> io::Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);

    // Create channel for receiving server messages to send to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Connection state
    let mut user_id: Option<u32> = None; // Set after successful login
    let mut handshake_complete = false; // Set to true after successful handshake
    let mut line = String::new(); // Reusable buffer for reading lines

    // Main loop - handle both incoming messages and outgoing events
    // Uses tokio::select! to handle both reading from client and sending to client concurrently
    loop {
        tokio::select! {
            // Handle incoming client messages
            result = reader.read_line(&mut line) => {
                let n = result?;

                // Connection closed
                if n == 0 {
                    break;
                }

                // Handle the message
                if let Err(e) = handle_client_message(
                    &line,
                    &mut user_id,
                    &mut handshake_complete,
                    &mut writer,
                    &user_manager,
                    &user_db,
                    peer_addr,
                    &tx,
                ).await {
                    eprintln!("Error handling message: {}", e);
                    break;
                }

                // Clear buffer for next message
                line.clear();
            }

            // Handle outgoing server messages/events
            Some(msg) = rx.recv() => {
                let json = serde_json::to_string(&msg)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
    }

    // Remove user on disconnect
    if let Some(id) = user_id {
        if let Some(user) = user_manager.remove_user(id).await {
            // Broadcast disconnection to all users
            user_manager
                .broadcast(ServerMessage::UserDisconnected {
                    user_id: id,
                    username: user.username.clone(),
                })
                .await;
        }
    }

    Ok(())
}

/// Handle a message from the client
async fn handle_client_message(
    line: &str,
    user_id: &mut Option<u32>,
    handshake_complete: &mut bool,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    user_manager: &UserManager,
    user_db: &UserDb,
    peer_addr: SocketAddr,
    tx: &mpsc::UnboundedSender<ServerMessage>,
) -> io::Result<()> {
    let line = line.trim();
    // Ignore empty lines (e.g., from keepalive or client mistakes)
    if line.is_empty() {
        return Ok(());
    }

    // NOTE: We don't log the raw message here to avoid leaking passwords

    // Create handler context with shared resources
    let mut ctx = HandlerContext {
        writer,
        peer_addr,
        user_manager,
        user_db,
        tx,
    };

    match serde_json::from_str::<ClientMessage>(line) {
        Ok(msg) => match msg {
            ClientMessage::Handshake { version } => {
                handlers::handle_handshake(version, handshake_complete, &mut ctx).await?;
            }
            ClientMessage::Login {
                username,
                password,
                features,
            } => {
                handlers::handle_login(
                    username,
                    password,
                    features,
                    *handshake_complete,
                    user_id,
                    &mut ctx,
                )
                .await?;
            }
            ClientMessage::UserList => {
                handlers::handle_userlist(*user_id, &mut ctx).await?;
            }
            ClientMessage::ChatSend { message } => {
                handlers::handle_chat_send(message, *user_id, &mut ctx).await?;
            }
        },
        Err(e) => {
            // NOTE: Don't log the raw message - it might contain passwords if malformed
            eprintln!(
                "Failed to parse message from {}: {}",
                peer_addr, e
            );
            return ctx
                .send_error_and_disconnect(&format!("Invalid message format: {}", e), None)
                .await;
        }
    }

    Ok(())
}
