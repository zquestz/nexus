//! Client connection handling

use std::io;
use std::net::SocketAddr;

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;

use nexus_common::framing::{FrameError, FrameReader, FrameWriter, MessageId};
use nexus_common::io::{read_client_message_with_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::constants::*;
use crate::db::Database;
use crate::handlers::{self, HandlerContext, err_invalid_message_format};
use crate::users::UserManager;

/// Connection state for a single client
struct ConnectionState {
    session_id: Option<u32>,
    handshake_complete: bool,
    locale: String,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            session_id: None,
            handshake_complete: false,
            locale: "en".to_string(),
        }
    }
}

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
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (reader, writer) = tokio::io::split(socket);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Create channel for receiving server messages to send to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<(ServerMessage, Option<MessageId>)>();

    // Connection state
    let mut conn_state = ConnectionState::new();

    // Main loop - handle both incoming messages and outgoing events
    // Uses tokio::select! to handle both reading from client and sending to client concurrently
    loop {
        tokio::select! {
            // Handle incoming client messages (with 60s timeout for DoS protection)
            result = read_client_message_with_timeout(&mut frame_reader) => {
                match result {
                    Ok(Some(received)) => {
                        // Handle the message
                        // Clone locale to avoid borrow checker conflict
                        let locale = conn_state.locale.clone();

                        let mut ctx = HandlerContext {
                            writer: &mut frame_writer,
                            peer_addr,
                            user_manager: &user_manager,
                            db: &db,
                            tx: &tx,
                            debug,
                            locale: &locale,
                            message_id: received.message_id,
                        };

                        if let Err(e) = handle_client_message(
                            received.message,
                            &mut conn_state,
                            &mut ctx,
                        ).await {
                            eprintln!("{}{}", ERR_HANDLING_MESSAGE, e);
                            break;
                        }
                    }
                    Ok(None) => {
                        // Connection closed cleanly
                        break;
                    }
                    Err(e) => {
                        // Invalid magic and timeouts are common (scanners, dropped connections)
                        // Only log in debug mode to reduce noise
                        let is_common_error = matches!(
                            e,
                            FrameError::InvalidMagic | FrameError::FrameTimeout
                        );

                        if !is_common_error || debug {
                            eprintln!("{}{}: {}", ERR_PARSE_MESSAGE, peer_addr, e);
                        }

                        // Try to send error before disconnecting
                        let error_msg = ServerMessage::Error {
                            message: err_invalid_message_format(&conn_state.locale),
                            command: None,
                        };
                        let _ = send_server_message_with_id(
                            &mut frame_writer,
                            &error_msg,
                            MessageId::new(),
                        ).await;
                        break;
                    }
                }
            }

            // Handle outgoing server messages/events
            msg = rx.recv() => {
                match msg {
                    Some((msg, msg_id)) => {
                        // Use provided message ID or generate a new one
                        let id = msg_id.unwrap_or_else(MessageId::new);
                        if send_server_message_with_id(&mut frame_writer, &msg, id).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        // Channel closed (user was removed from manager) - disconnect
                        break;
                    }
                }
            }
        }
    }

    // Shutdown the writer gracefully
    let _ = frame_writer.get_mut().shutdown().await;

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
async fn handle_client_message<W>(
    msg: ClientMessage,
    conn_state: &mut ConnectionState,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    match msg {
        ClientMessage::ChatSend { message } => {
            handlers::handle_chat_send(message, conn_state.session_id, ctx).await?;
        }
        ClientMessage::ChatTopicUpdate { topic } => {
            handlers::handle_chat_topic_update(topic, conn_state.session_id, ctx).await?;
        }
        ClientMessage::Handshake { version } => {
            handlers::handle_handshake(version, &mut conn_state.handshake_complete, ctx).await?;
        }
        ClientMessage::Login {
            username,
            password,
            features,
            locale,
            avatar,
        } => {
            let request = handlers::LoginRequest {
                username,
                password,
                features,
                locale: locale.clone(),
                avatar,
                handshake_complete: conn_state.handshake_complete,
            };
            handlers::handle_login(request, &mut conn_state.session_id, ctx).await?;

            // Update connection locale after successful login
            conn_state.locale = locale;
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
            handlers::handle_user_create(
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
            handlers::handle_user_delete(username, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserEdit { username } => {
            handlers::handle_user_edit(username, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserInfo { username } => {
            handlers::handle_user_info(username, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserKick { username } => {
            handlers::handle_user_kick(username, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserList { all } => {
            handlers::handle_user_list(all, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserMessage {
            to_username,
            message,
        } => {
            handlers::handle_user_message(to_username, message, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserUpdate {
            username,
            current_password,
            requested_username,
            requested_password,
            requested_is_admin,
            requested_enabled,
            requested_permissions,
        } => {
            let request = handlers::UserUpdateRequest {
                username,
                current_password,
                requested_username,
                requested_password,
                requested_is_admin,
                requested_enabled,
                requested_permissions,
                session_id: conn_state.session_id,
            };
            handlers::handle_user_update(request, ctx).await?;
        }
        ClientMessage::ServerInfoUpdate {
            name,
            description,
            max_connections_per_ip,
            image,
        } => {
            handlers::handle_server_info_update(
                name,
                description,
                max_connections_per_ip,
                image,
                conn_state.session_id,
                ctx,
            )
            .await?;
        }
    }

    Ok(())
}
