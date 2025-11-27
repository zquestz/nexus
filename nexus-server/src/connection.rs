//! Client connection handling

/// Connection state for a single client
struct ConnectionState {
    session_id: Option<u32>,
    handshake_complete: bool,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            session_id: None,
            handshake_complete: false,
        }
    }
}

use crate::db::Database;
use crate::handlers::{self, HandlerContext};
use crate::users::UserManager;
use nexus_common::io::send_server_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::io;
use std::net::SocketAddr;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use tokio::sync::mpsc;

/// Handle a client connection (always with TLS)
pub async fn handle_connection(
    socket: TcpStream,
    peer_addr: SocketAddr,
    user_manager: UserManager,
    db: Database,
    debug: bool,
    tls_acceptor: TlsAcceptor,
) -> io::Result<()> {
    // Perform TLS handshake (mandatory)
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("TLS handshake failed: {}", e)))?;

    handle_connection_inner(tls_stream, peer_addr, user_manager, db, debug).await
}

/// Inner connection handler that works with any AsyncRead + AsyncWrite stream
async fn handle_connection_inner<S>(
    socket: S,
    peer_addr: SocketAddr,
    user_manager: UserManager,
    db: Database,
    debug: bool,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (reader, writer) = tokio::io::split(socket);
    let mut reader = BufReader::new(reader);
    let mut writer: handlers::Writer = Box::pin(writer);

    // Create channel for receiving server messages to send to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Connection state
    let mut conn_state = ConnectionState::new();
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
                let mut ctx = HandlerContext {
                    writer: &mut writer,
                    peer_addr,
                    user_manager: &user_manager,
                    db: &db,
                    tx: &tx,
                    debug,
                };

                if let Err(e) = handle_client_message(
                    &line,
                    &mut conn_state,
                    &mut ctx,
                ).await {
                    eprintln!("Error handling message: {}", e);
                    break;
                }

                // Clear buffer for next message
                line.clear();
            }

            // Handle outgoing server messages/events
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        send_server_message(&mut writer, &msg).await?;
                    }
                    None => {
                        // Channel closed (user was removed from manager) - disconnect
                        break;
                    }
                }
            }
        }
    }

    // Remove user on disconnect
    if let Some(id) = conn_state.session_id
        && let Some(user) = user_manager.remove_user(id).await
    {
        if debug {
            println!("User '{}' disconnected", user.username);
        }
        // Broadcast disconnection to users with user_list permission
        user_manager
            .broadcast_user_event(
                ServerMessage::UserDisconnected {
                    session_id: id,
                    username: user.username.clone(),
                },
                &db.users,
                Some(id), // Exclude the disconnecting user
            )
            .await;
    }

    Ok(())
}

/// Handle a message from the client
async fn handle_client_message(
    line: &str,
    conn_state: &mut ConnectionState,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    let line = line.trim();
    // Ignore empty lines (e.g., from keepalive or client mistakes)
    if line.is_empty() {
        return Ok(());
    }

    // NOTE: We don't log the raw message here to avoid leaking passwords

    match serde_json::from_str::<ClientMessage>(line) {
        Ok(msg) => match msg {
            ClientMessage::ChatSend { message } => {
                handlers::handle_chat_send(message, conn_state.session_id, ctx).await?;
            }
            ClientMessage::ChatTopicUpdate { topic } => {
                handlers::handle_chattopicupdate(topic, conn_state.session_id, ctx).await?;
            }
            ClientMessage::Handshake { version } => {
                handlers::handle_handshake(version, &mut conn_state.handshake_complete, ctx)
                    .await?;
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
                    conn_state.handshake_complete,
                    &mut conn_state.session_id,
                    ctx,
                )
                .await?;
            }
            ClientMessage::UserBroadcast { message } => {
                handlers::handle_user_broadcast(message, conn_state.session_id, ctx).await?;
            }
            ClientMessage::UserCreate {
                username,
                password,
                is_admin,
                enabled,
                permissions,
            } => {
                handlers::handle_usercreate(
                    username,
                    password,
                    is_admin,
                    enabled,
                    permissions,
                    conn_state.session_id,
                    ctx,
                )
                .await?;
            }
            ClientMessage::UserDelete { username } => {
                handlers::handle_userdelete(username, conn_state.session_id, ctx).await?;
            }
            ClientMessage::UserEdit { username } => {
                handlers::handle_useredit(username, conn_state.session_id, ctx).await?;
            }
            ClientMessage::UserInfo { username } => {
                handlers::handle_userinfo(username, conn_state.session_id, ctx).await?;
            }
            ClientMessage::UserKick { username } => {
                handlers::handle_userkick(username, conn_state.session_id, ctx).await?;
            }
            ClientMessage::UserList => {
                handlers::handle_userlist(conn_state.session_id, ctx).await?;
            }
            ClientMessage::UserMessage {
                to_username,
                message,
            } => {
                handlers::handle_usermessage(to_username, message, conn_state.session_id, ctx)
                    .await?;
            }
            ClientMessage::UserUpdate {
                username,
                requested_username,
                requested_password,
                requested_is_admin,
                requested_enabled,
                requested_permissions,
            } => {
                let request = handlers::UserUpdateRequest {
                    username,
                    requested_username,
                    requested_password,
                    requested_is_admin,
                    requested_enabled,
                    requested_permissions,
                    session_id: conn_state.session_id,
                };
                handlers::handle_userupdate(request, ctx).await?;
            }
        },
        Err(e) => {
            // NOTE: Don't log the raw message - it might contain passwords if malformed
            eprintln!("Failed to parse message from {}: {}", ctx.peer_addr, e);
            return ctx
                .send_error_and_disconnect(&format!("Invalid message format: {}", e), None)
                .await;
        }
    }

    Ok(())
}
