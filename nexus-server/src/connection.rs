//! Client connection handling

use crate::db::Database;
use crate::handlers::{self, HandlerContext};
use crate::users::UserManager;
use nexus_common::io::send_server_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::mpsc;

/// Handle a client connection
pub async fn handle_connection(
    socket: TcpStream,
    peer_addr: SocketAddr,
    user_manager: UserManager,
    db: Database,
    debug: bool,
) -> io::Result<()> {
    // Enable TCP keepalive to detect dead connections
    // Keepalive will probe the connection every 60 seconds
    let socket_ref = socket2::SockRef::from(&socket);
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(60))
        .with_interval(Duration::from_secs(10));
    socket_ref.set_tcp_keepalive(&keepalive)?;

    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);

    // Create channel for receiving server messages to send to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Connection state
    let mut session_id: Option<u32> = None; // Set after successful login
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
                    &mut session_id,
                    &mut handshake_complete,
                    peer_addr,
                    &mut writer,
                    &user_manager,
                    &db,
                    &tx,
                    debug,
                ).await {
                    eprintln!("Error handling message: {}", e);
                    break;
                }

                // Clear buffer for next message
                line.clear();
            }

            // Handle outgoing server messages/events
            Some(msg) = rx.recv() => {
                send_server_message(&mut writer, &msg).await?;
            }
        }
    }

    // Remove user on disconnect
    if let Some(id) = session_id
        && let Some(user) = user_manager.remove_user(id).await
    {
        if debug {
            println!("User '{}' disconnected", user.username);
        }
        // Broadcast disconnection to all users
        user_manager
            .broadcast(ServerMessage::UserDisconnected {
                session_id: id,
                username: user.username.clone(),
            })
            .await;
    }

    Ok(())
}

/// Handle a message from the client
#[allow(clippy::too_many_arguments)]
async fn handle_client_message(
    line: &str,
    session_id: &mut Option<u32>,
    handshake_complete: &mut bool,
    peer_addr: SocketAddr,
    writer: &mut OwnedWriteHalf,
    user_manager: &UserManager,
    db: &Database,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    debug: bool,
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
        db,
        tx,
        debug,
    };

    match serde_json::from_str::<ClientMessage>(line) {
        Ok(msg) => match msg {
            ClientMessage::ChatSend { message } => {
                handlers::handle_chat_send(message, *session_id, &mut ctx).await?;
            }
            ClientMessage::ChatTopicUpdate { topic } => {
                handlers::handle_chattopicupdate(topic, *session_id, &mut ctx).await?;
            }
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
                    session_id,
                    &mut ctx,
                )
                .await?;
            }
            ClientMessage::UserBroadcast { message } => {
                handlers::handle_user_broadcast(message, *session_id, &mut ctx).await?;
            }
            ClientMessage::UserCreate {
                username,
                password,
                is_admin,
                permissions,
            } => {
                handlers::handle_usercreate(
                    username,
                    password,
                    is_admin,
                    permissions,
                    *session_id,
                    &mut ctx,
                )
                .await?;
            }
            ClientMessage::UserDelete { username } => {
                handlers::handle_userdelete(username, *session_id, &mut ctx).await?;
            }
            ClientMessage::UserEdit { username } => {
                handlers::handle_useredit(username, *session_id, &mut ctx).await?;
            }
            ClientMessage::UserInfo { username } => {
                handlers::handle_userinfo(username, *session_id, &mut ctx).await?;
            }
            ClientMessage::UserList => {
                handlers::handle_userlist(*session_id, &mut ctx).await?;
            }
            ClientMessage::UserUpdate {
                username,
                requested_username,
                requested_password,
                requested_is_admin,
                requested_permissions,
            } => {
                handlers::handle_userupdate(
                    username,
                    requested_username,
                    requested_password,
                    requested_is_admin,
                    requested_permissions,
                    *session_id,
                    &mut ctx,
                )
                .await?;
            }
        },
        Err(e) => {
            // NOTE: Don't log the raw message - it might contain passwords if malformed
            eprintln!("Failed to parse message from {}: {}", peer_addr, e);
            return ctx
                .send_error_and_disconnect(&format!("Invalid message format: {}", e), None)
                .await;
        }
    }

    Ok(())
}
