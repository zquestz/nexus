//! Client connection handling

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, warn};

use nexus_common::framing::{FrameError, FrameReader, FrameWriter, MessageId};
use nexus_common::io::{
    read_client_message_with_full_timeout, read_client_message_with_timeout,
    send_server_message_with_id,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::channels::ChannelManager;
use crate::connection_tracker::ConnectionTracker;
use crate::constants::*;
use crate::db::Database;
use crate::files::FileIndex;
use crate::flood::{FloodConfig, FloodTracker};
use crate::handlers::{
    self, HandlerContext, err_invalid_message_format, err_message_not_supported,
};
use crate::ip_rule_cache::IpRuleCache;
use crate::transfers::TransferRegistry;
use crate::users::UserManager;
use crate::voice::{VoiceRegistry, send_voice_leave_notifications};

/// Parameters for handling a connection
pub struct ConnectionParams {
    pub peer_addr: SocketAddr,
    pub user_manager: UserManager,
    pub db: Database,
    pub file_root: Option<&'static Path>,
    pub transfer_port: u16,
    pub transfer_websocket_port: Option<u16>,
    pub connection_tracker: Arc<ConnectionTracker>,
    pub ip_rule_cache: Arc<RwLock<IpRuleCache>>,
    pub file_index: Arc<FileIndex>,
    pub channel_manager: ChannelManager,
    pub transfer_registry: Arc<TransferRegistry>,
    pub voice_registry: VoiceRegistry,
    pub fingerprint: &'static str,
    pub flood_config: Arc<FloodConfig>,
}

/// Connection state for a single client
struct ConnectionState {
    session_id: Option<u32>,
    handshake_complete: bool,
    locale: String,
    flood_tracker: FloodTracker,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            session_id: None,
            handshake_complete: false,
            locale: DEFAULT_LOCALE.to_string(),
            flood_tracker: FloodTracker::new(),
        }
    }
}

/// Handle a client connection (always with TLS)
pub async fn handle_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: ConnectionParams,
) -> io::Result<()> {
    // Perform TLS handshake (mandatory)
    let tls_stream = tls_acceptor.accept(socket).await.map_err(|e| {
        io::Error::other(format!(
            "{}{}",
            nexus_common::TLS_HANDSHAKE_FAILED_PREFIX,
            e
        ))
    })?;

    handle_connection_inner(tls_stream, params).await
}

/// Inner connection handler that works with any AsyncRead + AsyncWrite stream
pub async fn handle_connection_inner<S>(socket: S, params: ConnectionParams) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let ConnectionParams {
        peer_addr,
        user_manager,
        db,
        file_root,
        transfer_port,
        transfer_websocket_port,
        connection_tracker,
        ip_rule_cache,
        file_index,
        channel_manager,
        transfer_registry,
        voice_registry,
        fingerprint,
        flood_config,
    } = params;

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
        // Choose read function based on authentication state:
        // - Before login: use full timeout (30s idle + 60s frame) to prevent resource exhaustion
        // - After login: allow idle connections (only 60s frame timeout once data arrives)
        let is_authenticated = conn_state.session_id.is_some();

        tokio::select! {
            // Handle incoming client messages
            result = async {
                if is_authenticated {
                    // Authenticated users can idle indefinitely
                    read_client_message_with_timeout(&mut frame_reader).await
                } else {
                    // Unauthenticated connections must send data within 30 seconds
                    read_client_message_with_full_timeout(&mut frame_reader, None, None).await
                }
            } => {
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
                            locale: &locale,
                            message_id: received.message_id,
                            file_root,
                            transfer_port,
                            transfer_websocket_port,
                            connection_tracker: connection_tracker.clone(),
                            ip_rule_cache: ip_rule_cache.clone(),
                            file_index: file_index.clone(),
                            channel_manager: &channel_manager,
                            transfer_registry: transfer_registry.clone(),
                            voice_registry: &voice_registry,
                            fingerprint,
                            flood_config: flood_config.clone(),
                        };

                        if let Err(e) = handle_client_message(
                            received.message,
                            &mut conn_state,
                            &mut ctx,
                        ).await {
                            error!(ip = %peer_addr, err = %e, "{}", LOG_ERROR_HANDLING_MESSAGE);
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
                            FrameError::InvalidMagic | FrameError::FrameTimeout | FrameError::IdleTimeout
                        );

                        if is_common_error {
                            debug!(ip = %peer_addr, err = %e, "{}", LOG_PARSE_MESSAGE_ERROR);
                        } else {
                            warn!(ip = %peer_addr, err = %e, "{}", LOG_PARSE_MESSAGE_ERROR);
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

    // Remove user on disconnect and broadcast to other clients
    if let Some(id) = conn_state.session_id {
        // Remove from all channels, then notify remaining channel members if needed.
        if let Some(user) = user_manager.get_user_by_session_id(id).await {
            let channel_names = channel_manager.remove_from_all(id).await;

            for channel_name in channel_names {
                // Get remaining members (if channel still exists)
                if let Some(remaining_members) = channel_manager.get_members(&channel_name).await {
                    let nickname_present_elsewhere = user_manager
                        .sessions_contain_nickname(&remaining_members, &user.nickname, None)
                        .await;

                    if !nickname_present_elsewhere {
                        let leave_msg = ServerMessage::ChatUserLeft {
                            channel: channel_name,
                            nickname: user.nickname.clone(),
                        };

                        for member_session_id in remaining_members {
                            user_manager
                                .send_to_session(member_session_id, leave_msg.clone())
                                .await;
                        }
                    }
                }
            }

            // Remove from voice session and notify remaining participants
            if let Some(info) = voice_registry.remove_by_session_id(id).await {
                // Note: we don't notify the leaving user here since they're disconnecting
                send_voice_leave_notifications(&info, None, &user_manager, &channel_manager).await;
            }
        }

        // Now remove from UserManager and broadcast UserDisconnected
        if let Some(user) = user_manager.remove_user_and_broadcast(id).await {
            debug!(user = %user.username, ip = %peer_addr, "{}", LOG_DISCONNECTED);
        }
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
    // Update last_activity for non-passive messages (used for idle tracking)
    if let Some(session_id) = conn_state.session_id
        && !is_passive_message(&msg)
    {
        ctx.user_manager.update_last_activity(session_id).await;
    }

    match msg {
        ClientMessage::ChatSend {
            message,
            action,
            channel,
        } => {
            handlers::handle_chat_send(
                message,
                action,
                channel,
                conn_state.session_id,
                &mut conn_state.flood_tracker,
                ctx,
            )
            .await?;
        }
        ClientMessage::ChatTopicUpdate { topic, channel } => {
            handlers::handle_chat_topic_update(topic, channel, conn_state.session_id, ctx).await?;
        }
        ClientMessage::ChatJoin { channel } => {
            handlers::handle_chat_join(channel, conn_state.session_id, ctx).await?;
        }
        ClientMessage::ChatLeave { channel } => {
            handlers::handle_chat_leave(channel, conn_state.session_id, ctx).await?;
        }
        ClientMessage::ChatList {} => {
            handlers::handle_chat_list(conn_state.session_id, ctx).await?;
        }
        ClientMessage::ChatSecret { channel, secret } => {
            handlers::handle_chat_secret(channel, secret, conn_state.session_id, ctx).await?;
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
            nickname,
        } => {
            let request = handlers::LoginRequest {
                username,
                password,
                features,
                locale: locale.clone(),
                avatar,
                nickname,
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
            is_shared,
            enabled,
            permissions,
            group_id,
            revokes,
        } => {
            let request = handlers::UserCreateRequest {
                username,
                password,
                is_admin,
                is_shared,
                enabled,
                permissions,
                group_id,
                revokes,
            };
            handlers::handle_user_create(request, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserDelete { id } => {
            handlers::handle_user_delete(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserEdit { id } => {
            handlers::handle_user_edit(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserInfo { nickname } => {
            handlers::handle_user_info(nickname, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserKick { nickname, reason } => {
            handlers::handle_user_kick(nickname, reason, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserList { all } => {
            handlers::handle_user_list(all, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserMessage {
            to_nickname,
            message,
            action,
        } => {
            handlers::handle_user_message(
                to_nickname,
                message,
                action,
                conn_state.session_id,
                &mut conn_state.flood_tracker,
                ctx,
            )
            .await?;
        }
        ClientMessage::UserUpdate {
            id,
            current_password,
            username,
            password,
            is_admin,
            enabled,
            permissions,
            group_id,
            remove_group,
            revokes,
        } => {
            let request = handlers::UserUpdateRequest {
                id,
                current_password,
                username,
                password,
                is_admin,
                enabled,
                permissions,
                group_id,
                remove_group,
                revokes,
                session_id: conn_state.session_id,
            };
            handlers::handle_user_update(request, ctx).await?;
        }
        ClientMessage::UserAway { message } => {
            handlers::handle_user_away(message, conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserBack => {
            handlers::handle_user_back(conn_state.session_id, ctx).await?;
        }
        ClientMessage::UserStatus { status } => {
            handlers::handle_user_status(status, conn_state.session_id, ctx).await?;
        }
        ClientMessage::ServerInfoUpdate {
            name,
            description,
            public_address,
            max_connections_per_ip,
            max_transfers_per_ip,
            image,
            file_reindex_interval,
            persistent_channels,
            auto_join_channels,
            chat_burst_limit,
            chat_rate_limit,
            min_password_strength,
        } => {
            let request = handlers::ServerInfoUpdateRequest {
                name,
                description,
                public_address,
                max_connections_per_ip,
                max_transfers_per_ip,
                image,
                file_reindex_interval,
                persistent_channels,
                auto_join_channels,
                chat_burst_limit,
                chat_rate_limit,
                min_password_strength,
                session_id: conn_state.session_id,
            };
            handlers::handle_server_info_update(request, ctx).await?;
        }
        ClientMessage::NewsList => {
            handlers::handle_news_list(conn_state.session_id, ctx).await?;
        }
        ClientMessage::NewsShow { id } => {
            handlers::handle_news_show(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::NewsCreate { body, image } => {
            handlers::handle_news_create(body, image, conn_state.session_id, ctx).await?;
        }
        ClientMessage::NewsEdit { id } => {
            handlers::handle_news_edit(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::NewsUpdate { id, body, image } => {
            handlers::handle_news_update(id, body, image, conn_state.session_id, ctx).await?;
        }
        ClientMessage::NewsDelete { id } => {
            handlers::handle_news_delete(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileList {
            path,
            root,
            show_hidden,
        } => {
            handlers::handle_file_list(path, root, show_hidden, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileCreateDir { path, name, root } => {
            handlers::handle_file_create_dir(path, name, root, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileDelete { path, root } => {
            handlers::handle_file_delete(path, root, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileInfo { path, root } => {
            handlers::handle_file_info(path, root, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileRename {
            path,
            new_name,
            root,
        } => {
            handlers::handle_file_rename(path, new_name, root, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileMove {
            source_path,
            destination_dir,
            overwrite,
            source_root,
            destination_root,
        } => {
            handlers::handle_file_move(
                source_path,
                destination_dir,
                overwrite,
                source_root,
                destination_root,
                conn_state.session_id,
                ctx,
            )
            .await?;
        }
        ClientMessage::FileCopy {
            source_path,
            destination_dir,
            overwrite,
            source_root,
            destination_root,
        } => {
            handlers::handle_file_copy(
                source_path,
                destination_dir,
                overwrite,
                source_root,
                destination_root,
                conn_state.session_id,
                ctx,
            )
            .await?;
        }
        ClientMessage::FileDownload { .. }
        | ClientMessage::FileStartResponse { .. }
        | ClientMessage::FileUpload { .. }
        | ClientMessage::FileStart { .. }
        | ClientMessage::FileData
        | ClientMessage::FileHashing { .. }
        | ClientMessage::FileHash { .. } => {
            // These messages are only valid on the transfer port (7501), not the main BBS port
            warn!(ip = %ctx.peer_addr, "{}", LOG_TRANSFER_ON_MAIN_PORT);
            return ctx
                .send_error_and_disconnect(&err_message_not_supported(ctx.locale), None)
                .await;
        }
        ClientMessage::BanCreate {
            target,
            duration,
            reason,
        } => {
            handlers::handle_ban_create(target, duration, reason, conn_state.session_id, ctx)
                .await?;
        }
        ClientMessage::BanDelete { target } => {
            handlers::handle_ban_delete(target, conn_state.session_id, ctx).await?;
        }
        ClientMessage::BanList => {
            handlers::handle_ban_list(conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrustCreate {
            target,
            duration,
            reason,
        } => {
            handlers::handle_trust_create(target, duration, reason, conn_state.session_id, ctx)
                .await?;
        }
        ClientMessage::TrustDelete { target } => {
            handlers::handle_trust_delete(target, conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrustList => {
            handlers::handle_trust_list(conn_state.session_id, ctx).await?;
        }
        ClientMessage::ConnectionMonitor => {
            handlers::handle_connection_monitor(conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileSearch { query, root } => {
            handlers::handle_file_search(query, root, conn_state.session_id, ctx).await?;
        }
        ClientMessage::FileReindex => {
            handlers::handle_file_reindex(conn_state.session_id, ctx).await?;
        }
        ClientMessage::VoiceJoin { target } => {
            handlers::handle_voice_join(target, conn_state.session_id, ctx).await?;
        }
        ClientMessage::VoiceLeave => {
            handlers::handle_voice_leave(conn_state.session_id, ctx).await?;
        }
        ClientMessage::Ping => {
            ctx.send_message(&ServerMessage::Pong).await?;
        }
        ClientMessage::GroupList => {
            handlers::handle_group_list(conn_state.session_id, ctx).await?;
        }
        ClientMessage::GroupCreate {
            name,
            is_shared,
            permissions,
        } => {
            handlers::handle_group_create(name, is_shared, permissions, conn_state.session_id, ctx)
                .await?;
        }
        ClientMessage::GroupEdit { id } => {
            handlers::handle_group_edit(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::GroupUpdate {
            id,
            name,
            is_shared,
            permissions,
        } => {
            handlers::handle_group_update(
                id,
                name,
                is_shared,
                permissions,
                conn_state.session_id,
                ctx,
            )
            .await?;
        }
        ClientMessage::GroupDelete { id } => {
            handlers::handle_group_delete(id, conn_state.session_id, ctx).await?;
        }
    }

    Ok(())
}

/// Check if a client message is passive (should not reset idle tracking).
///
/// Passive messages don't count as user activity for idle time calculation:
/// - `Ping`: keepalive, not user-initiated
/// - `UserAway`: setting away status shouldn't reset idle timer
fn is_passive_message(msg: &ClientMessage) -> bool {
    matches!(msg, ClientMessage::Ping | ClientMessage::UserAway { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::protocol::ChatAction;

    #[test]
    fn test_passive_messages() {
        assert!(is_passive_message(&ClientMessage::Ping));
        assert!(is_passive_message(&ClientMessage::UserAway {
            message: None
        }));
        assert!(is_passive_message(&ClientMessage::UserAway {
            message: Some("brb".to_string()),
        }));
    }

    #[test]
    fn test_non_passive_messages() {
        assert!(!is_passive_message(&ClientMessage::UserBack));
        assert!(!is_passive_message(&ClientMessage::UserList { all: false }));
        assert!(!is_passive_message(&ClientMessage::ChatSend {
            message: "hello".to_string(),
            action: ChatAction::Normal,
            channel: "#test".to_string(),
        }));
        assert!(!is_passive_message(&ClientMessage::UserInfo {
            nickname: "alice".to_string(),
        }));
        assert!(!is_passive_message(&ClientMessage::UserStatus {
            status: Some("busy".to_string()),
        }));
    }
}
