//! Client connection handling

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use tokio::io::{AsyncRead, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, warn};

use nexus_common::address::is_private_network;
use nexus_common::framing::{FrameError, FrameReader, FrameWriter, MessageId};
use nexus_common::io::{
    ReceivedClientMessage, read_client_handshake_message_with_full_timeout,
    read_client_login_message_with_full_timeout, read_client_message_with_timeout,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::rate_limiter::RateLimiter;
use nexus_common::tls::accept_tls_with_timeout;

use crate::channels::ChannelManager;
use crate::connection_io::send_server_message_with_write_timeout;
use crate::connection_tracker::ConnectionTracker;
use crate::constants::*;
use crate::db::Database;
use crate::egress::{
    self, EgressEnqueueError,
    task::{EgressHandle, StageMessageError},
};
use crate::files::{FileActivityMap, FileIndex};
use crate::flood::FloodConfig;
use crate::handlers::{
    self, DirectWriter, HandlerContext, err_invalid_message_format, err_message_not_supported,
    err_slow_client_disconnect, err_unexpected_message_type,
};
use crate::ip_rule_cache::IpRuleState;
use crate::scheduler::{ConnectionClass, ConnectionId};
use crate::tracker::TrackerManager;
use crate::transfers::TransferRegistry;
use crate::users::UserManager;
use crate::users::user::{ConnectionWriter, SessionEvent, SessionRx};
use crate::voice::{VoiceControlHandle, VoiceRegistry};

/// Parameters for handling a connection
pub struct ConnectionParams {
    pub peer_addr: SocketAddr,
    pub user_manager: UserManager,
    pub db: Database,
    pub file_root: Option<&'static Path>,
    pub transfer_port: u16,
    pub transfer_websocket_port: Option<u16>,
    pub connection_tracker: Arc<ConnectionTracker>,
    pub ip_rule_cache: Arc<IpRuleState>,
    pub file_index: Arc<FileIndex>,
    pub file_activity: Arc<FileActivityMap>,
    pub channel_manager: ChannelManager,
    pub transfer_registry: Arc<TransferRegistry>,
    pub voice_registry: VoiceRegistry,
    pub voice_control: VoiceControlHandle,
    pub tracker_manager: Arc<TrackerManager>,
    pub fingerprint: &'static str,
    pub flood_config: Arc<FloodConfig>,
    pub login_limiter: Arc<RateLimiter>,
    pub egress: EgressHandle,
    pub egress_connection_id: ConnectionId,
    pub lan_egress_bypass_enabled: bool,
}

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
            locale: DEFAULT_LOCALE.to_string(),
        }
    }
}

#[derive(Clone, Copy)]
enum ClientReadPhase {
    Handshake,
    Login,
    Authenticated,
}

type PendingClientRead<R> = Pin<
    Box<
        dyn Future<
                Output = (
                    FrameReader<R>,
                    Result<Option<ReceivedClientMessage>, FrameError>,
                ),
            > + Send,
    >,
>;

fn client_read_phase(conn_state: &ConnectionState) -> ClientReadPhase {
    if conn_state.session_id.is_some() {
        ClientReadPhase::Authenticated
    } else if conn_state.handshake_complete {
        ClientReadPhase::Login
    } else {
        ClientReadPhase::Handshake
    }
}

fn start_client_read<R>(
    mut frame_reader: FrameReader<R>,
    phase: ClientReadPhase,
) -> PendingClientRead<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    Box::pin(async move {
        let result = match phase {
            ClientReadPhase::Authenticated => {
                read_client_message_with_timeout(&mut frame_reader).await
            }
            ClientReadPhase::Login => {
                read_client_login_message_with_full_timeout(&mut frame_reader, None, None).await
            }
            ClientReadPhase::Handshake => {
                read_client_handshake_message_with_full_timeout(&mut frame_reader, None, None).await
            }
        };

        (frame_reader, result)
    })
}

async fn recv_client_read<R>(
    pending_read: &mut Option<PendingClientRead<R>>,
) -> (
    FrameReader<R>,
    Result<Option<ReceivedClientMessage>, FrameError>,
)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    match pending_read {
        Some(read) => read.await,
        None => std::future::pending().await,
    }
}

/// Handle a client connection (always with TLS)
pub async fn handle_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: ConnectionParams,
) -> io::Result<()> {
    // Perform TLS handshake (mandatory) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    handle_connection_inner(tls_stream, params).await
}

/// Inner connection handler that works with any AsyncRead + AsyncWrite stream
pub async fn handle_connection_inner<S>(socket: S, params: ConnectionParams) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let peer_addr = params.peer_addr;
    let egress = params.egress.clone();
    let egress_connection_id = params.egress_connection_id;
    let egress_enabled = !params.lan_egress_bypass_enabled || !is_private_network(peer_addr.ip());
    let mut egress_registered = false;
    let mut egress_dispatch_rx = None;

    if egress_enabled {
        let (egress_dispatch_tx, rx) = mpsc::channel(egress::EGRESS_DISPATCH_QUEUE_CAPACITY);

        match egress
            .register_anon(
                egress_connection_id,
                ConnectionClass::Protocol,
                egress_dispatch_tx,
            )
            .await
        {
            Ok(true) => {
                egress_registered = true;
                egress_dispatch_rx = Some(rx);
            }
            Ok(false) => {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = egress_connection_id.get(),
                    "{}",
                    LOG_EGRESS_REGISTER_REJECTED
                );
            }
            Err(e) => {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = egress_connection_id.get(),
                    err = ?e,
                    "{}",
                    LOG_EGRESS_REGISTER_FAILED
                );
            }
        }
    }

    // Catch a panic in the handler so the egress cleanup below still runs — a
    // bare `.await` would unwind straight past it and leak the registration.
    // (The scheduler also self-heals on its next dispatch attempt to this
    // connection; catching here makes the cleanup deterministic on every exit.)
    let result = AssertUnwindSafe(handle_connection_inner_registered(
        socket,
        params,
        egress_dispatch_rx,
    ))
    .catch_unwind()
    .await;

    if egress_registered {
        match egress.unregister(egress_connection_id).await {
            Ok(()) => {}
            Err(e) => {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = egress_connection_id.get(),
                    err = ?e,
                    "{}",
                    LOG_EGRESS_UNREGISTER_FAILED
                );
            }
        }
    }

    // Re-raise a caught panic after cleanup so tokio still observes the task panic.
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

async fn recv_egress_dispatch(
    rx: &mut Option<egress::EgressDispatchRx>,
) -> Option<egress::EgressDispatch> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn notify_egress_write_failed(
    egress: &EgressHandle,
    connection_id: ConnectionId,
    peer_addr: SocketAddr,
) {
    if let Err(e) = egress.write_failed(connection_id).await {
        warn!(
            ip = %peer_addr,
            egress_connection_id = connection_id.get(),
            err = ?e,
            "{}",
            LOG_EGRESS_WRITE_FAILED_NOTIFY_FAILED
        );
    }
}

async fn stage_egress_message(
    egress: &EgressHandle,
    connection_id: ConnectionId,
    peer_addr: SocketAddr,
    message: Box<ServerMessage>,
    message_id: MessageId,
    staged_frames: &mut usize,
    pending_session_event: &mut Option<SessionEvent>,
) -> bool {
    match egress
        .stage_message(connection_id, message, message_id)
        .await
    {
        Ok(Ok(_chunks)) => {
            *staged_frames += 1;
            true
        }
        Ok(Err(StageMessageError::QueueFull {
            message,
            message_id,
        })) => {
            if *staged_frames == 0 {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = connection_id.get(),
                    "{}",
                    LOG_EGRESS_STAGE_FAILED
                );
                return false;
            }

            *pending_session_event = Some(SessionEvent::Message(message, Some(message_id)));
            true
        }
        Ok(Err(StageMessageError::Failed(e))) => {
            warn!(
                ip = %peer_addr,
                egress_connection_id = connection_id.get(),
                err = ?e,
                "{}",
                LOG_EGRESS_STAGE_FAILED
            );
            false
        }
        Err(e) => {
            warn!(
                ip = %peer_addr,
                egress_connection_id = connection_id.get(),
                err = ?e,
                "{}",
                LOG_EGRESS_STAGE_FAILED
            );
            false
        }
    }
}

async fn stage_egress_frame(
    egress: &EgressHandle,
    connection_id: ConnectionId,
    peer_addr: SocketAddr,
    frame: Arc<[u8]>,
    staged_frames: &mut usize,
    pending_session_event: &mut Option<SessionEvent>,
) -> bool {
    match egress.stage_frame(connection_id, Arc::clone(&frame)).await {
        Ok(Ok(_chunks)) => {
            *staged_frames += 1;
            true
        }
        Ok(Err(EgressEnqueueError::QueueFull)) => {
            if *staged_frames == 0 {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = connection_id.get(),
                    "{}",
                    LOG_EGRESS_STAGE_FAILED
                );
                return false;
            }

            *pending_session_event = Some(SessionEvent::Frame(frame));
            true
        }
        Ok(Err(e)) => {
            warn!(
                ip = %peer_addr,
                egress_connection_id = connection_id.get(),
                err = ?e,
                "{}",
                LOG_EGRESS_STAGE_FAILED
            );
            false
        }
        Err(e) => {
            warn!(
                ip = %peer_addr,
                egress_connection_id = connection_id.get(),
                err = ?e,
                "{}",
                LOG_EGRESS_STAGE_FAILED
            );
            false
        }
    }
}

async fn write_frame_bytes_with_timeout<W>(
    writer: &mut FrameWriter<W>,
    mut bytes: &[u8],
    flush: bool,
    timeout: Duration,
    timeout_error: &'static str,
) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    // These bytes are already complete frame bytes. They intentionally
    // bypass FrameWriter framing and are only safe while the loop enforces
    // frame-boundary writes for every other path.
    let inner = writer.get_mut();
    while !bytes.is_empty() {
        match time::timeout(timeout, inner.write(bytes)).await {
            Ok(Ok(0)) => {
                return Err(io::Error::new(io::ErrorKind::WriteZero, ERR_BBS_WRITE_ZERO));
            }
            Ok(Ok(written)) => bytes = &bytes[written..],
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(io::Error::new(io::ErrorKind::TimedOut, timeout_error)),
        }
    }

    if flush {
        match time::timeout(timeout, inner.flush()).await {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, timeout_error)),
        }
    } else {
        Ok(())
    }
}

async fn handle_connection_inner_registered<S>(
    socket: S,
    params: ConnectionParams,
    mut egress_dispatch_rx: Option<egress::EgressDispatchRx>,
) -> io::Result<()>
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
        file_activity,
        channel_manager,
        transfer_registry,
        voice_registry,
        voice_control,
        tracker_manager,
        fingerprint,
        flood_config,
        login_limiter,
        egress,
        egress_connection_id,
        lan_egress_bypass_enabled: _,
    } = params;

    let (reader, writer) = tokio::io::split(socket);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = Some(FrameReader::new(buf_reader));
    let mut frame_writer = FrameWriter::new(writer);

    // Channel for server messages destined to this client.
    let (tx, mut rx): (ConnectionWriter, SessionRx) = ConnectionWriter::channel();

    let mut conn_state = ConnectionState::new();
    let mut pending_session_event: Option<SessionEvent> = None;
    let mut pending_client_read = None;
    let mut egress_frame_in_progress = false;
    let mut egress_staged_frames = 0usize;
    let mut disconnect_after_egress_drain = false;

    // tokio::select! over incoming reads and outgoing events. Read timeout
    // depends on auth state: pre-login uses a 30s idle + 60s frame timeout
    // (resource-exhaustion defense); authenticated users may idle freely.
    loop {
        if disconnect_after_egress_drain && !egress_frame_in_progress && egress_staged_frames == 0 {
            break;
        }

        let read_enabled = !egress_frame_in_progress
            && pending_session_event.is_none()
            && !disconnect_after_egress_drain;

        if read_enabled && pending_client_read.is_none() {
            let phase = client_read_phase(&conn_state);
            let reader = frame_reader
                .take()
                .expect("frame reader must be available when no read is pending");
            pending_client_read = Some(start_client_read(reader, phase));
        }

        let poll_client_read = read_enabled && pending_client_read.is_some();

        tokio::select! {
            biased;

            dispatch = recv_egress_dispatch(&mut egress_dispatch_rx) => {
                let Some(dispatch) = dispatch else {
                    warn!(
                        ip = %peer_addr,
                        egress_connection_id = egress_connection_id.get(),
                        "{}",
                        LOG_EGRESS_DISPATCH_CLOSED
                    );
                    break;
                };

                if dispatch.connection_id != egress_connection_id {
                    warn!(
                        ip = %peer_addr,
                        egress_connection_id = egress_connection_id.get(),
                        dispatch_connection_id = dispatch.connection_id.get(),
                        "{}",
                        LOG_EGRESS_DISPATCH_WRONG_CONNECTION
                    );
                    notify_egress_write_failed(&egress, dispatch.connection_id, peer_addr).await;
                    continue;
                }

                let is_final_frame_chunk = dispatch.chunk.is_final_frame_chunk();
                if write_frame_bytes_with_timeout(
                    &mut frame_writer,
                    dispatch.chunk.as_bytes(),
                    is_final_frame_chunk,
                    BBS_CHUNK_WRITE_TIMEOUT,
                    ERR_BBS_CHUNK_WRITE_TIMEOUT,
                )
                .await
                .is_err()
                {
                    notify_egress_write_failed(&egress, egress_connection_id, peer_addr).await;
                    break;
                }

                egress_frame_in_progress = !is_final_frame_chunk;

                if let Err(e) = egress.ack(egress_connection_id).await {
                    warn!(
                        ip = %peer_addr,
                        egress_connection_id = egress_connection_id.get(),
                        err = ?e,
                        "{}",
                        LOG_EGRESS_ACK_FAILED
                    );
                    break;
                }

                if is_final_frame_chunk {
                    egress_staged_frames = egress_staged_frames.saturating_sub(1);
                    if let Some(event) = pending_session_event.take() {
                        let staged = match event {
                            SessionEvent::Message(msg, msg_id) => {
                                let id = msg_id.unwrap_or_else(MessageId::new);
                                stage_egress_message(
                                    &egress,
                                    egress_connection_id,
                                    peer_addr,
                                    msg,
                                    id,
                                    &mut egress_staged_frames,
                                    &mut pending_session_event,
                                )
                                .await
                            }
                            SessionEvent::Frame(frame) => {
                                stage_egress_frame(
                                    &egress,
                                    egress_connection_id,
                                    peer_addr,
                                    frame,
                                    &mut egress_staged_frames,
                                    &mut pending_session_event,
                                )
                                .await
                            }
                            SessionEvent::Disconnect | SessionEvent::SlowClientDisconnect => false,
                        };

                        if !staged {
                            break;
                        }
                    }
                }
            }

            read = recv_client_read(&mut pending_client_read), if poll_client_read => {
                let (reader, result) = read;
                pending_client_read = None;
                frame_reader = Some(reader);

                match result {
                    Ok(Some(received)) => {
                        // Clone locale to avoid borrow checker conflict with conn_state.
                        let locale = conn_state.locale.clone();

                        let writer = if egress_dispatch_rx.is_some() {
                            DirectWriter::egress(
                                &egress,
                                egress_connection_id,
                                &mut egress_staged_frames,
                            )
                        } else {
                            DirectWriter::new(&mut frame_writer)
                        };

                        let mut ctx = HandlerContext {
                            writer,
                            peer_addr,
                            user_manager: &user_manager,
                            db: &db,
                            tx: &tx,
                            egress: &egress,
                            egress_connection_id,
                            egress_connection_registered: egress_dispatch_rx.is_some(),
                            locale: &locale,
                            message_id: received.message_id,
                            file_root,
                            transfer_port,
                            transfer_websocket_port,
                            connection_tracker: connection_tracker.clone(),
                            ip_rule_cache: ip_rule_cache.clone(),
                            file_index: file_index.clone(),
                            file_activity: file_activity.clone(),
                            channel_manager: &channel_manager,
                            transfer_registry: transfer_registry.clone(),
                            voice_registry: &voice_registry,
                            voice_control: &voice_control,
                            tracker_manager: &tracker_manager,
                            fingerprint,
                            flood_config: flood_config.clone(),
                            login_limiter: login_limiter.clone(),
                        };

                        if let Err(e) = handle_client_message(
                            received.message,
                            &mut conn_state,
                            &mut ctx,
                        ).await {
                            error!(ip = %peer_addr, err = %e, "{}", LOG_ERROR_HANDLING_MESSAGE);
                            if egress_dispatch_rx.is_some() && egress_staged_frames > 0 {
                                disconnect_after_egress_drain = true;
                            } else {
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        // Connection closed cleanly
                        break;
                    }
                    Err(e) => {
                        // Invalid magic / timeouts are common (scanners, dropped
                        // connections); log those at debug to reduce noise.
                        let is_common_error = matches!(
                            e,
                            FrameError::InvalidMagic | FrameError::FrameTimeout | FrameError::IdleTimeout
                        );

                        if is_common_error {
                            debug!(ip = %peer_addr, err = %e, "{}", LOG_PARSE_MESSAGE_ERROR);
                        } else {
                            warn!(ip = %peer_addr, err = %e, "{}", LOG_PARSE_MESSAGE_ERROR);
                        }

                        // Try to send error before disconnecting.
                        let command = frame_error_command(&e);
                        let message = if matches!(&e, FrameError::UnexpectedMessageType(_)) {
                            err_unexpected_message_type(&conn_state.locale)
                        } else {
                            err_invalid_message_format(&conn_state.locale)
                        };
                        let error_msg = ServerMessage::Error {
                            message,
                            command,
                            disconnect: true,
                        };
                        if egress_dispatch_rx.is_some() {
                            match egress
                                .stage_priority_message(
                                    egress_connection_id,
                                    Box::new(error_msg),
                                    MessageId::new(),
                                )
                                .await
                            {
                                Ok(Ok(_chunks)) => {
                                    egress_staged_frames += 1;
                                    disconnect_after_egress_drain = true;
                                    continue;
                                }
                                Ok(Err(e)) => {
                                    warn!(
                                        ip = %peer_addr,
                                        egress_connection_id = egress_connection_id.get(),
                                        err = ?e,
                                        "{}",
                                        LOG_EGRESS_STAGE_FAILED
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        ip = %peer_addr,
                                        egress_connection_id = egress_connection_id.get(),
                                        err = ?e,
                                        "{}",
                                        LOG_EGRESS_STAGE_FAILED
                                    );
                                }
                            }
                        } else {
                            let _ = send_server_message_with_write_timeout(
                                &mut frame_writer,
                                &error_msg,
                                MessageId::new(),
                            ).await;
                        }
                        break;
                    }
                }
            }

            // Outgoing server messages/events.
            event = rx.recv(), if read_enabled => {
                match event {
                    Some(SessionEvent::Message(msg, msg_id)) => {
                        let id = msg_id.unwrap_or_else(MessageId::new);
                        if egress_dispatch_rx.is_some() {
                            if !stage_egress_message(
                                &egress,
                                egress_connection_id,
                                peer_addr,
                                msg,
                                id,
                                &mut egress_staged_frames,
                                &mut pending_session_event,
                            )
                            .await
                            {
                                break;
                            }
                        } else if send_server_message_with_write_timeout(&mut frame_writer, &msg, id).await.is_err() {
                            break;
                        }
                    }
                    Some(SessionEvent::Frame(frame)) => {
                        if egress_dispatch_rx.is_some() {
                            if !stage_egress_frame(
                                &egress,
                                egress_connection_id,
                                peer_addr,
                                frame,
                                &mut egress_staged_frames,
                                &mut pending_session_event,
                            )
                            .await
                            {
                                break;
                            }
                        } else if write_frame_bytes_with_timeout(
                            &mut frame_writer,
                            frame.as_ref(),
                            true,
                            BBS_DIRECT_WRITE_TIMEOUT,
                            ERR_BBS_WRITE_TIMEOUT,
                        )
                        .await
                        .is_err()
                        {
                            break;
                        }
                    }
                    Some(SessionEvent::Disconnect) => {
                        if egress_dispatch_rx.is_some() && egress_staged_frames > 0 {
                            disconnect_after_egress_drain = true;
                        } else {
                            break;
                        }
                    }
                    Some(SessionEvent::SlowClientDisconnect) => {
                        if egress_dispatch_rx.is_some() {
                            notify_egress_write_failed(&egress, egress_connection_id, peer_addr).await;
                        }
                        let error_msg = ServerMessage::Error {
                            message: err_slow_client_disconnect(&conn_state.locale),
                            command: None,
                            disconnect: true,
                        };
                        let _ = send_server_message_with_write_timeout(
                            &mut frame_writer,
                            &error_msg,
                            MessageId::new(),
                        ).await;
                        break;
                    }
                    None => {
                        // Channel closed (user removed from manager) — disconnect.
                        break;
                    }
                }
            }
        }
    }

    let _ = frame_writer.get_mut().shutdown().await;

    // Disconnect teardown. Remove from the map first so the map is the single
    // serialization point: if a forced path (kick/ban/delete) already removed this
    // session, `remove_users` returns empty and we do nothing — no double cleanup,
    // no double disconnect. Cleanup runs before `broadcast_disconnections` so
    // ChatUserLeft / VoiceUserLeft precede UserDisconnected. `notify_leaving_user`
    // is false: the user is already gone, there's nothing to tell them.
    if let Some(id) = conn_state.session_id {
        let removed = handlers::remove_user_sessions_with_cleanup(
            &user_manager,
            &voice_registry,
            &channel_manager,
            &[id],
            false,
        )
        .await;
        for user in &removed {
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
    // Non-passive messages count as activity for idle tracking.
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
            handlers::handle_chat_send(message, action, channel, conn_state.session_id, ctx)
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
            bandwidth_weight,
            inherit_bandwidth_weight,
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
                bandwidth_weight,
                inherit_bandwidth_weight,
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
            handlers::handle_user_message(to_nickname, message, action, conn_state.session_id, ctx)
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
            bandwidth_weight,
            inherit_bandwidth_weight,
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
                bandwidth_weight,
                inherit_bandwidth_weight,
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
            max_outbound_rate,
            scheduler_chunk_size,
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
                max_outbound_rate,
                scheduler_chunk_size,
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
            bandwidth_weight,
        } => {
            handlers::handle_group_create(
                name,
                is_shared,
                permissions,
                bandwidth_weight,
                conn_state.session_id,
                ctx,
            )
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
            bandwidth_weight,
        } => {
            handlers::handle_group_update(
                id,
                name,
                is_shared,
                permissions,
                bandwidth_weight,
                conn_state.session_id,
                ctx,
            )
            .await?;
        }
        ClientMessage::GroupDelete { id } => {
            handlers::handle_group_delete(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrackerAcceptFingerprint { id } => {
            handlers::handle_tracker_accept_fingerprint(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrackerAdd {
            address,
            port,
            fingerprint,
            password,
            name,
            enabled,
        } => {
            let request = handlers::TrackerAddRequest {
                address,
                port,
                fingerprint,
                password,
                name,
                enabled,
            };
            handlers::handle_tracker_add(request, conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrackerEdit { id } => {
            handlers::handle_tracker_edit(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrackerList => {
            handlers::handle_tracker_list(conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrackerRemove { id } => {
            handlers::handle_tracker_remove(id, conn_state.session_id, ctx).await?;
        }
        ClientMessage::TrackerUpdate {
            id,
            address,
            port,
            fingerprint,
            password,
            name,
            enabled,
        } => {
            let request = handlers::TrackerUpdateRequest {
                id,
                address,
                port,
                fingerprint,
                password,
                name,
                enabled,
            };
            handlers::handle_tracker_update(request, conn_state.session_id, ctx).await?;
        }
    }

    Ok(())
}

/// Passive messages don't count as user activity for idle tracking:
/// `Ping` (keepalive) and `UserAway` (setting away mustn't reset the timer).
fn is_passive_message(msg: &ClientMessage) -> bool {
    matches!(msg, ClientMessage::Ping | ClientMessage::UserAway { .. })
}

fn frame_error_command(err: &FrameError) -> Option<String> {
    match err {
        FrameError::UnexpectedMessageType(message_type) => Some(message_type.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use nexus_common::framing::{FrameContexts, RawFrame};
    use nexus_common::io::{
        read_server_message_in_context, send_client_message_with_id, server_message_to_frame_bytes,
    };
    use nexus_common::protocol::{ChatAction, ServerMessage};
    use nexus_common::validators::{DEFAULT_CHANNEL, MAX_MESSAGE_LENGTH};
    use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};

    use crate::egress::EgressManager;
    use crate::egress::task::{
        DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY, EgressCommand, EgressHandle, EgressSettingsCommand,
        EgressTask,
    };
    use crate::handlers::testing::{TEST_FINGERPRINT, TestContext, create_test_context};

    type TestReader = FrameReader<BufReader<ReadHalf<DuplexStream>>>;
    type TestWriter = FrameWriter<WriteHalf<DuplexStream>>;

    struct PendingWriter;

    impl AsyncWrite for PendingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct ProgressThenPendingWriter {
        writes_before_pending: usize,
        bytes: Vec<u8>,
    }

    impl ProgressThenPendingWriter {
        fn new(writes_before_pending: usize) -> Self {
            Self {
                writes_before_pending,
                bytes: Vec::new(),
            }
        }
    }

    impl AsyncWrite for ProgressThenPendingWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.writes_before_pending == 0 {
                return Poll::Pending;
            }

            self.writes_before_pending -= 1;
            let written = buf.len().min(2);
            self.bytes.extend_from_slice(&buf[..written]);
            Poll::Ready(Ok(written))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn frame_byte_write_uses_supplied_timeout_error() {
        let mut writer = FrameWriter::new(PendingWriter);

        let err = write_frame_bytes_with_timeout(
            &mut writer,
            b"chunk",
            false,
            Duration::from_millis(1),
            ERR_BBS_CHUNK_WRITE_TIMEOUT,
        )
        .await
        .expect_err("pending chunk write should time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), ERR_BBS_CHUNK_WRITE_TIMEOUT);
    }

    #[tokio::test]
    async fn frame_byte_write_timeout_resets_after_progress() {
        let mut writer = FrameWriter::new(ProgressThenPendingWriter::new(2));

        let err = write_frame_bytes_with_timeout(
            &mut writer,
            b"payload",
            false,
            Duration::from_millis(1),
            ERR_BBS_CHUNK_WRITE_TIMEOUT,
        )
        .await
        .expect_err("writer should time out only after progress stops");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(writer.get_ref().bytes, b"payl");
    }

    struct TestConnection {
        reader: TestReader,
        writer: TestWriter,
        user_manager: UserManager,
        egress: EgressHandle,
        egress_connection_id: ConnectionId,
        server_task: tokio::task::JoinHandle<io::Result<()>>,
        egress_task: tokio::task::JoinHandle<()>,
        _test_ctx: TestContext,
    }

    impl TestConnection {
        async fn shutdown(self) {
            let Self {
                reader,
                writer,
                egress,
                server_task,
                egress_task,
                _test_ctx,
                ..
            } = self;

            drop(reader);
            drop(writer);
            drop(egress);

            let server_result = time::timeout(Duration::from_secs(2), server_task)
                .await
                .expect("connection task should exit");
            server_result
                .expect("connection task should not panic")
                .expect("connection task should not fail");

            time::timeout(Duration::from_secs(2), egress_task)
                .await
                .expect("egress task should exit")
                .expect("egress task should not panic");
        }
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("test value must be non-zero")
    }

    fn message_id(bytes: &[u8]) -> MessageId {
        MessageId::from_bytes(bytes).expect("valid hex test message ID")
    }

    fn egress_handle_for_test() -> (
        EgressHandle,
        mpsc::Receiver<EgressCommand>,
        mpsc::UnboundedReceiver<EgressSettingsCommand>,
    ) {
        let (command_tx, command_rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        (
            EgressHandle::new(command_tx, settings_tx),
            command_rx,
            settings_rx,
        )
    }

    async fn start_test_connection(chunk_size: usize, stream_capacity: usize) -> TestConnection {
        start_test_connection_with_rate(chunk_size, stream_capacity, 0).await
    }

    async fn start_test_connection_with_rate(
        chunk_size: usize,
        stream_capacity: usize,
        bytes_per_second: u64,
    ) -> TestConnection {
        start_test_connection_with_manager(
            EgressManager::new(nonzero(chunk_size)),
            stream_capacity,
            bytes_per_second,
        )
        .await
    }

    async fn start_test_connection_with_manager(
        manager: EgressManager,
        stream_capacity: usize,
        bytes_per_second: u64,
    ) -> TestConnection {
        let test_ctx = create_test_context().await;
        let (client, server) = tokio::io::duplex(stream_capacity);
        let (client_read, client_write) = tokio::io::split(client);
        let reader = FrameReader::new(BufReader::new(client_read));
        let writer = FrameWriter::new(client_write);

        let user_manager = test_ctx.user_manager.clone();
        let tracker_context = Arc::new(crate::tracker::TrackerContext {
            db: test_ctx.db.clone(),
            user_manager: user_manager.clone(),
            server_fingerprint: TEST_FINGERPRINT,
            server_port: nexus_common::DEFAULT_PORT,
            server_websocket_port: None,
        });
        let tracker_manager = Arc::new(crate::tracker::TrackerManager::new(tracker_context));

        let (egress, egress_task) = EgressTask::channel_with_rate(manager, bytes_per_second);
        let egress_task = tokio::spawn(egress_task.run());
        let egress_connection_id = ConnectionId::new(10_001);

        let params = ConnectionParams {
            peer_addr: test_ctx.peer_addr,
            user_manager: user_manager.clone(),
            db: test_ctx.db.clone(),
            file_root: test_ctx.file_root,
            transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
            transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
            connection_tracker: test_ctx.connection_tracker.clone(),
            ip_rule_cache: test_ctx.ip_rule_cache.clone(),
            file_index: test_ctx.file_index.clone(),
            file_activity: test_ctx.file_activity.clone(),
            channel_manager: test_ctx.channel_manager.clone(),
            transfer_registry: test_ctx.transfer_registry.clone(),
            voice_registry: test_ctx.voice_registry.clone(),
            voice_control: test_ctx.voice_control.clone(),
            tracker_manager,
            fingerprint: TEST_FINGERPRINT,
            login_limiter: Arc::new(
                nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(
                    crate::constants::LOGIN_FAILURE_BURST,
                    crate::constants::LOGIN_FAILURE_REFILL_PER_MINUTE,
                )
                .key_ipv6_by_prefix(),
            ),
            flood_config: test_ctx.flood_config.clone(),
            egress: egress.clone(),
            egress_connection_id,
            lan_egress_bypass_enabled: false,
        };

        let server_task = tokio::spawn(handle_connection_inner(server, params));

        TestConnection {
            reader,
            writer,
            user_manager,
            egress,
            egress_connection_id,
            server_task,
            egress_task,
            _test_ctx: test_ctx,
        }
    }

    // Tests read handshake, login, and stream responses through this one
    // helper, so it spans all three server->client BBS phase contexts.
    async fn read_next_server_message(
        connection: &mut TestConnection,
    ) -> nexus_common::io::ReceivedServerMessage {
        time::timeout(
            Duration::from_secs(2),
            read_server_message_in_context(
                &mut connection.reader,
                FrameContexts::SERVER_HANDSHAKE_RESPONSE
                    | FrameContexts::SERVER_LOGIN_RESPONSE
                    | FrameContexts::BBS_SERVER,
            ),
        )
        .await
        .expect("timed out waiting for server message")
        .expect("failed to read server message")
        .expect("connection closed unexpectedly")
    }

    async fn read_next_raw_frame(connection: &mut TestConnection) -> RawFrame {
        time::timeout(
            Duration::from_secs(2),
            connection
                .reader
                .read_frame_in_context(FrameContexts::BBS_SERVER),
        )
        .await
        .expect("timed out waiting for server frame")
        .expect("failed to read server frame")
        .expect("connection closed unexpectedly")
    }

    async fn login_test_connection(connection: &mut TestConnection) -> u32 {
        send_client_message_with_id(
            &mut connection.writer,
            &ClientMessage::Handshake {
                version: nexus_common::PROTOCOL_VERSION.to_string(),
            },
            message_id(b"000000000301"),
        )
        .await
        .expect("handshake send should succeed");

        let handshake = read_next_server_message(connection).await;
        match handshake.message {
            ServerMessage::HandshakeResponse { success, .. } => {
                assert!(success, "handshake should succeed");
            }
            other => panic!("expected HandshakeResponse, got {other:?}"),
        }

        send_client_message_with_id(
            &mut connection.writer,
            &ClientMessage::Login {
                username: "alice".to_string(),
                password: "password".to_string(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: "en".to_string(),
                avatar: None,
                nickname: None,
            },
            message_id(b"000000000302"),
        )
        .await
        .expect("login send should succeed");

        let login = read_next_server_message(connection).await;
        match login.message {
            ServerMessage::LoginResponse {
                success,
                session_id,
                ..
            } => {
                assert!(success, "login should succeed");
                session_id.expect("successful login should include session_id")
            }
            other => panic!("expected LoginResponse, got {other:?}"),
        }
    }

    async fn queue_session_message(
        connection: &TestConnection,
        session_id: u32,
        message: ServerMessage,
        message_id: MessageId,
    ) {
        let session = connection
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .expect("session should exist");
        session
            .tx
            .send_message(message, Some(message_id))
            .expect("session queue should accept message");
    }

    async fn queue_session_frame(connection: &TestConnection, session_id: u32, frame: Arc<[u8]>) {
        let session = connection
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .expect("session should exist");
        session
            .tx
            .send_frame(frame)
            .expect("session queue should accept frame");
    }

    fn chat_message(session_id: u32, body: impl Into<String>) -> ServerMessage {
        ServerMessage::ChatMessage {
            session_id,
            nickname: "alice".to_string(),
            is_admin: true,
            is_shared: false,
            message: body.into(),
            action: ChatAction::Normal,
            channel: DEFAULT_CHANNEL.to_string(),
            timestamp: 1234,
        }
    }

    #[tokio::test]
    async fn lan_peer_bypasses_bbs_egress_registration_and_login_transition() {
        let test_ctx = create_test_context().await;
        let (client, server) = tokio::io::duplex(8192);
        let (client_read, client_write) = tokio::io::split(client);
        let mut reader = FrameReader::new(BufReader::new(client_read));
        let mut writer = FrameWriter::new(client_write);

        let user_manager = test_ctx.user_manager.clone();
        let tracker_context = Arc::new(crate::tracker::TrackerContext {
            db: test_ctx.db.clone(),
            user_manager: user_manager.clone(),
            server_fingerprint: TEST_FINGERPRINT,
            server_port: nexus_common::DEFAULT_PORT,
            server_websocket_port: None,
        });
        let tracker_manager = Arc::new(crate::tracker::TrackerManager::new(tracker_context));
        let (egress, mut command_rx, mut settings_rx) = egress_handle_for_test();

        let params = ConnectionParams {
            peer_addr: test_ctx.peer_addr,
            user_manager,
            db: test_ctx.db.clone(),
            file_root: test_ctx.file_root,
            transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
            transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
            connection_tracker: test_ctx.connection_tracker.clone(),
            ip_rule_cache: test_ctx.ip_rule_cache.clone(),
            file_index: test_ctx.file_index.clone(),
            file_activity: test_ctx.file_activity.clone(),
            channel_manager: test_ctx.channel_manager.clone(),
            transfer_registry: test_ctx.transfer_registry.clone(),
            voice_registry: test_ctx.voice_registry.clone(),
            voice_control: test_ctx.voice_control.clone(),
            tracker_manager,
            fingerprint: TEST_FINGERPRINT,
            login_limiter: Arc::new(
                nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(
                    crate::constants::LOGIN_FAILURE_BURST,
                    crate::constants::LOGIN_FAILURE_REFILL_PER_MINUTE,
                )
                .key_ipv6_by_prefix(),
            ),
            flood_config: test_ctx.flood_config.clone(),
            egress,
            egress_connection_id: ConnectionId::new(10_900),
            lan_egress_bypass_enabled: true,
        };

        let server_task = tokio::spawn(handle_connection_inner(server, params));

        send_client_message_with_id(
            &mut writer,
            &ClientMessage::Handshake {
                version: nexus_common::PROTOCOL_VERSION.to_string(),
            },
            message_id(b"000000000391"),
        )
        .await
        .expect("handshake send should succeed");
        let handshake = time::timeout(
            Duration::from_secs(2),
            nexus_common::io::read_server_handshake_response(&mut reader),
        )
        .await
        .expect("timed out waiting for handshake response")
        .expect("handshake read should succeed")
        .expect("handshake response");
        assert!(matches!(
            handshake.message,
            ServerMessage::HandshakeResponse { success: true, .. }
        ));

        send_client_message_with_id(
            &mut writer,
            &ClientMessage::Login {
                username: "alice".to_string(),
                password: "password".to_string(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: None,
            },
            message_id(b"000000000392"),
        )
        .await
        .expect("login send should succeed");
        let login = time::timeout(
            Duration::from_secs(2),
            nexus_common::io::read_server_login_response(&mut reader),
        )
        .await
        .expect("timed out waiting for login response")
        .expect("login read should succeed")
        .expect("login response");
        assert!(matches!(
            login.message,
            ServerMessage::LoginResponse { success: true, .. }
        ));
        assert!(command_rx.try_recv().is_err());
        assert!(settings_rx.try_recv().is_err());

        drop(reader);
        drop(writer);
        time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("connection task should exit")
            .expect("connection task should not panic")
            .expect("connection task should not fail");
        assert!(command_rx.try_recv().is_err());
        assert!(settings_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn yggdrasil_peer_uses_bbs_egress_registration_under_lan_bypass() {
        let test_ctx = create_test_context().await;
        let (client, server) = tokio::io::duplex(4096);
        let user_manager = test_ctx.user_manager.clone();
        let tracker_context = Arc::new(crate::tracker::TrackerContext {
            db: test_ctx.db.clone(),
            user_manager: user_manager.clone(),
            server_fingerprint: TEST_FINGERPRINT,
            server_port: nexus_common::DEFAULT_PORT,
            server_websocket_port: None,
        });
        let tracker_manager = Arc::new(crate::tracker::TrackerManager::new(tracker_context));
        let (egress, mut command_rx, _settings_rx) = egress_handle_for_test();
        let egress_connection_id = ConnectionId::new(10_901);

        let params = ConnectionParams {
            peer_addr: SocketAddr::from(([0x0200, 0, 0, 0, 0, 0, 0, 1], 7500)),
            user_manager,
            db: test_ctx.db.clone(),
            file_root: test_ctx.file_root,
            transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
            transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
            connection_tracker: test_ctx.connection_tracker.clone(),
            ip_rule_cache: test_ctx.ip_rule_cache.clone(),
            file_index: test_ctx.file_index.clone(),
            file_activity: test_ctx.file_activity.clone(),
            channel_manager: test_ctx.channel_manager.clone(),
            transfer_registry: test_ctx.transfer_registry.clone(),
            voice_registry: test_ctx.voice_registry.clone(),
            voice_control: test_ctx.voice_control.clone(),
            tracker_manager,
            fingerprint: TEST_FINGERPRINT,
            login_limiter: Arc::new(
                nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(
                    crate::constants::LOGIN_FAILURE_BURST,
                    crate::constants::LOGIN_FAILURE_REFILL_PER_MINUTE,
                )
                .key_ipv6_by_prefix(),
            ),
            flood_config: test_ctx.flood_config.clone(),
            egress,
            egress_connection_id,
            lan_egress_bypass_enabled: true,
        };

        let server_task = tokio::spawn(handle_connection_inner(server, params));
        let register = time::timeout(Duration::from_secs(2), command_rx.recv())
            .await
            .expect("timed out waiting for Yggdrasil BBS registration")
            .expect("register command");
        match register {
            EgressCommand::RegisterAnon {
                connection_id,
                class,
                reply_tx,
                ..
            } => {
                assert_eq!(connection_id, egress_connection_id);
                assert_eq!(class, ConnectionClass::Protocol);
                reply_tx.send(true).unwrap();
            }
            _ => panic!("expected RegisterAnon"),
        }

        drop(client);
        let unregister = time::timeout(Duration::from_secs(2), command_rx.recv())
            .await
            .expect("timed out waiting for Yggdrasil BBS unregister")
            .expect("unregister command");
        match unregister {
            EgressCommand::Unregister { connection_id } => {
                assert_eq!(connection_id, egress_connection_id);
            }
            _ => panic!("expected Unregister"),
        }

        time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("connection task should exit")
            .expect("connection task should not panic")
            .expect("connection task should not fail");
    }

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

    #[tokio::test]
    async fn queued_egress_message_writes_wire_identical_frame() {
        let mut connection = start_test_connection(64, 8192).await;
        let session_id = login_test_connection(&mut connection).await;
        let queued_id = message_id(b"000000000303");
        let queued_message = chat_message(session_id, "queued over egress");

        queue_session_message(&connection, session_id, queued_message.clone(), queued_id).await;

        let frame = read_next_raw_frame(&mut connection).await;
        let expected = server_message_to_frame_bytes(&queued_message, queued_id)
            .expect("expected frame should serialize");
        assert_eq!(frame.to_bytes().as_slice(), expected.as_ref());

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn queued_egress_frame_writes_preframed_bytes() {
        let mut connection = start_test_connection(64, 8192).await;
        let session_id = login_test_connection(&mut connection).await;
        let queued_id = message_id(b"000000000342");
        let queued_message = chat_message(session_id, "preframed over egress");
        let expected = server_message_to_frame_bytes(&queued_message, queued_id)
            .expect("expected frame should serialize");

        queue_session_frame(&connection, session_id, Arc::clone(&expected)).await;

        let frame = read_next_raw_frame(&mut connection).await;
        assert_eq!(frame.to_bytes().as_slice(), expected.as_ref());

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn partial_inbound_frame_survives_queued_egress() {
        let mut connection = start_test_connection(32, 8192).await;
        let session_id = login_test_connection(&mut connection).await;
        let queued_id = message_id(b"000000000345");
        let ping_id = message_id(b"000000000346");
        let queued_message = chat_message(session_id, "egress while inbound waits");
        let ping_payload = serde_json::to_vec(&ClientMessage::Ping).unwrap();
        let ping_frame = RawFrame::new(ping_id, "Ping", ping_payload).to_bytes();
        let split = ping_frame.len() - 1;

        connection
            .writer
            .get_mut()
            .write_all(&ping_frame[..split])
            .await
            .expect("partial ping frame should write");

        // Let the connection task consume the incomplete frame and park inside
        // the read before an outbound event wakes the same select loop.
        time::sleep(Duration::from_millis(25)).await;
        queue_session_message(&connection, session_id, queued_message.clone(), queued_id).await;

        let queued = read_next_raw_frame(&mut connection).await;
        let expected = server_message_to_frame_bytes(&queued_message, queued_id)
            .expect("expected queued frame should serialize");
        assert_eq!(queued.to_bytes().as_slice(), expected.as_ref());

        connection
            .writer
            .get_mut()
            .write_all(&ping_frame[split..])
            .await
            .expect("final ping byte should write");

        let pong = read_next_server_message(&mut connection).await;
        assert_eq!(pong.message_id, ping_id);
        assert!(matches!(pong.message, ServerMessage::Pong));

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn pre_login_command_gets_unexpected_message_type_error() {
        let mut connection = start_test_connection(64, 8192).await;

        send_client_message_with_id(
            &mut connection.writer,
            &ClientMessage::Handshake {
                version: nexus_common::PROTOCOL_VERSION.to_string(),
            },
            message_id(b"000000000331"),
        )
        .await
        .expect("handshake send should succeed");

        let handshake = read_next_server_message(&mut connection).await;
        assert!(matches!(
            handshake.message,
            ServerMessage::HandshakeResponse { success: true, .. }
        ));

        send_client_message_with_id(
            &mut connection.writer,
            &ClientMessage::ChatSend {
                message: "too early".to_string(),
                action: ChatAction::Normal,
                channel: DEFAULT_CHANNEL.to_string(),
            },
            message_id(b"000000000332"),
        )
        .await
        .expect("pre-login command send should succeed");

        let error = read_next_server_message(&mut connection).await;
        match error.message {
            ServerMessage::Error {
                message,
                command,
                disconnect,
                ..
            } => {
                assert_eq!(message, err_unexpected_message_type("en"));
                assert_eq!(command.as_deref(), Some("ChatSend"));
                assert!(disconnect);
            }
            other => panic!("expected Error, got {other:?}"),
        }

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn relogin_after_login_gets_unexpected_message_type_error() {
        let mut connection = start_test_connection(64, 8192).await;
        login_test_connection(&mut connection).await;

        send_client_message_with_id(
            &mut connection.writer,
            &ClientMessage::Login {
                username: "alice".to_string(),
                password: "password".to_string(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: None,
            },
            message_id(b"000000000341"),
        )
        .await
        .expect("re-login send should succeed");

        let error = read_next_server_message(&mut connection).await;
        match error.message {
            ServerMessage::Error {
                message,
                command,
                disconnect,
                ..
            } => {
                assert_eq!(message, err_unexpected_message_type("en"));
                assert_eq!(command.as_deref(), Some("Login"));
                assert!(disconnect);
            }
            other => panic!("expected Error, got {other:?}"),
        }

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn direct_response_does_not_interleave_with_chunked_egress_frame() {
        let mut connection = start_test_connection(32, 128).await;
        let session_id = login_test_connection(&mut connection).await;
        let queued_id = message_id(b"000000000304");
        let ping_id = message_id(b"000000000305");
        let queued_message = chat_message(session_id, "x".repeat(MAX_MESSAGE_LENGTH));

        queue_session_message(&connection, session_id, queued_message.clone(), queued_id).await;

        // Give the server loop a chance to start writing the queued frame into
        // the deliberately tiny duplex buffer before the direct Ping arrives.
        time::sleep(Duration::from_millis(25)).await;
        send_client_message_with_id(&mut connection.writer, &ClientMessage::Ping, ping_id)
            .await
            .expect("ping send should succeed");

        let frame = read_next_raw_frame(&mut connection).await;
        let expected = server_message_to_frame_bytes(&queued_message, queued_id)
            .expect("expected frame should serialize");
        assert_eq!(frame.to_bytes().as_slice(), expected.as_ref());

        let pong = read_next_server_message(&mut connection).await;
        assert_eq!(pong.message_id, ping_id);
        assert!(matches!(pong.message, ServerMessage::Pong));

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn priority_queue_full_drains_existing_frame_then_disconnects() {
        let mut connection = start_test_connection_with_manager(
            EgressManager::with_frame_limit(nonzero(64), 1),
            8192,
            0,
        )
        .await;
        let session_id = login_test_connection(&mut connection).await;
        let queued_id = message_id(b"000000000306");
        let ping_id = message_id(b"000000000307");
        let queued_message = chat_message(session_id, "drain before disconnect");

        connection
            .egress
            .set_blocked(connection.egress_connection_id, true)
            .await
            .expect("block command should queue");
        queue_session_message(&connection, session_id, queued_message.clone(), queued_id).await;

        // Let the connection task stage the normal queued frame while egress is
        // blocked, filling the one-frame staging limit.
        time::sleep(Duration::from_millis(25)).await;
        send_client_message_with_id(&mut connection.writer, &ClientMessage::Ping, ping_id)
            .await
            .expect("ping send should succeed");

        // Keep dispatch blocked long enough for the direct response to hit
        // QueueFull and arm drain-then-disconnect instead of racing a freed slot.
        time::sleep(Duration::from_millis(25)).await;
        connection
            .egress
            .set_blocked(connection.egress_connection_id, false)
            .await
            .expect("unblock command should queue");

        let frame = read_next_raw_frame(&mut connection).await;
        let expected = server_message_to_frame_bytes(&queued_message, queued_id)
            .expect("expected queued frame should serialize");
        assert_eq!(frame.to_bytes().as_slice(), expected.as_ref());

        let closed = time::timeout(
            Duration::from_secs(2),
            connection
                .reader
                .read_frame_in_context(FrameContexts::BBS_SERVER),
        )
        .await
        .expect("timed out waiting for drain disconnect")
        .expect("read after drain should not fail");
        assert!(
            closed.is_none(),
            "priority QueueFull should drain staged egress and close without sending Pong"
        );

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn queued_egress_frame_queue_full_retries_after_existing_frame_drains() {
        let mut connection = start_test_connection_with_manager(
            EgressManager::with_frame_limit(nonzero(64), 1),
            8192,
            0,
        )
        .await;
        let session_id = login_test_connection(&mut connection).await;
        let first_id = message_id(b"000000000343");
        let second_id = message_id(b"000000000344");
        let first_message = chat_message(session_id, "first preframed");
        let second_message = chat_message(session_id, "second preframed");
        let first_frame = server_message_to_frame_bytes(&first_message, first_id)
            .expect("first frame should serialize");
        let second_frame = server_message_to_frame_bytes(&second_message, second_id)
            .expect("second frame should serialize");

        connection
            .egress
            .set_blocked(connection.egress_connection_id, true)
            .await
            .expect("block command should queue");
        queue_session_frame(&connection, session_id, Arc::clone(&first_frame)).await;
        queue_session_frame(&connection, session_id, Arc::clone(&second_frame)).await;

        // Let the connection task stage the first frame and retain the second
        // as pending while the one-frame egress queue is full.
        time::sleep(Duration::from_millis(25)).await;
        connection
            .egress
            .set_blocked(connection.egress_connection_id, false)
            .await
            .expect("unblock command should queue");

        let first = read_next_raw_frame(&mut connection).await;
        assert_eq!(first.to_bytes().as_slice(), first_frame.as_ref());

        let second = read_next_raw_frame(&mut connection).await;
        assert_eq!(second.to_bytes().as_slice(), second_frame.as_ref());

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn slow_client_disconnect_under_egress_sends_error_directly_then_closes() {
        let mut connection = start_test_connection(64, 8192).await;
        let session_id = login_test_connection(&mut connection).await;
        let session = connection
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .expect("session should exist");

        session.tx.arm_slow_client_disconnect_for_test();

        let error = read_next_server_message(&mut connection).await;
        match error.message {
            ServerMessage::Error { disconnect, .. } => {
                assert!(disconnect, "slow-client error must disconnect");
            }
            other => panic!("expected slow-client Error, got {other:?}"),
        }

        let closed = time::timeout(
            Duration::from_secs(2),
            connection
                .reader
                .read_frame_in_context(FrameContexts::BBS_SERVER),
        )
        .await
        .expect("timed out waiting for slow-client close")
        .expect("read after slow-client error should not fail");
        assert!(
            closed.is_none(),
            "slow-client egress abort should close after the direct error frame"
        );

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn decode_error_under_egress_stages_disconnect_error_before_close() {
        let mut connection = start_test_connection(16, 8192).await;
        login_test_connection(&mut connection).await;

        let invalid_id = message_id(b"000000000309");
        let payload = serde_json::to_vec(&ClientMessage::Ping).unwrap();
        let invalid_frame = RawFrame::new(invalid_id, "ChatSend", payload);
        connection
            .writer
            .get_mut()
            .write_all(&invalid_frame.to_bytes())
            .await
            .expect("invalid frame write should succeed");

        let error = read_next_server_message(&mut connection).await;
        match error.message {
            ServerMessage::Error { disconnect, .. } => {
                assert!(disconnect, "decode error must disconnect");
            }
            other => panic!("expected decode Error, got {other:?}"),
        }

        let closed = time::timeout(
            Duration::from_secs(2),
            connection
                .reader
                .read_frame_in_context(FrameContexts::BBS_SERVER),
        )
        .await
        .expect("timed out waiting for decode-error close")
        .expect("read after decode error should not fail");
        assert!(
            closed.is_none(),
            "decode-error egress path should send one error frame then close"
        );

        connection.shutdown().await;
    }

    #[tokio::test]
    async fn direct_response_is_throttled_by_egress_rate_limit() {
        let mut connection = start_test_connection(4, 8192).await;
        login_test_connection(&mut connection).await;
        let ping_id = message_id(b"000000000308");
        let expected = server_message_to_frame_bytes(&ServerMessage::Pong, ping_id)
            .expect("expected pong frame should serialize");
        assert!(expected.len() > 16);

        connection
            .egress
            .set_max_outbound_rate(32)
            .expect("rate update should queue");

        send_client_message_with_id(&mut connection.writer, &ClientMessage::Ping, ping_id)
            .await
            .expect("ping send should succeed");

        let started = time::Instant::now();
        let pong = read_next_server_message(&mut connection).await;
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "direct response should be paced by the egress limiter"
        );
        assert_eq!(pong.message_id, ping_id);
        assert!(matches!(pong.message, ServerMessage::Pong));

        connection.shutdown().await;
    }
}
