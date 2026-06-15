//! Per-connection task: TLS handshake, BBS-shaped protocol handshake, then dispatch.
//!
//! The first post-handshake `TrackerClientMessage` locks the connection's role:
//! `TrackerServerRegister` → server connection (enters the refresh loop; later Registers are
//! refreshes, a List is a role violation → `Error` + close); `TrackerServerList` → client
//! connection (one response, then close). Server entries are unregistered via a drop guard so
//! every exit path (cancel, panic, timeout) cleans up. Errors bubble to `main.rs`'s
//! `log_connection_error`.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

use nexus_common::framing::{FrameError, FrameReader, FrameWriter};
use nexus_common::io::{
    client_message_type, read_client_handshake_message_with_full_timeout,
    read_tracker_client_message_with_full_timeout,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tls::accept_tls_with_timeout;
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};

use crate::connection_io::{
    send_server_message_with_write_timeout, send_tracker_server_message_with_write_timeout,
};
use crate::constants::{
    DEFAULT_LOCALE, HANDSHAKE_TIMEOUT, LOG_CONNECTION_RATE_LIMITED, LOG_HANDSHAKE_REQUIRED,
    LOG_REGISTER_DISCONNECTED, LOG_ROLE_VIOLATION, REASON_DISCONNECT_CLEAN_CLOSE,
    REASON_DISCONNECT_FRAME_ERROR, REASON_DISCONNECT_REJECTED, REASON_DISCONNECT_ROLE_VIOLATION,
    REASON_DISCONNECT_STALE_TIMEOUT, ROLE_ESTABLISH_TIMEOUT, STALE_TIMEOUT_REFRESH_MULTIPLIER,
};
use crate::errors::{
    err_tracker_frame_error, err_tracker_handshake_required, err_tracker_malformed_message,
    err_tracker_payload_too_large, err_tracker_role_violation, err_tracker_unexpected_message_type,
    err_tracker_unknown_message_type,
};
use crate::handlers;
use crate::handlers::tracker_server_list::{ListParams, handle_tracker_server_list};
use crate::handlers::tracker_server_register::{
    InitialRegisterOutcome, RefreshOutcome, RegisterParams, handle_initial_register, handle_refresh,
};
use crate::registry::ConnectionId;
use crate::state::TrackerState;
use nexus_common::rate_limiter::RateCheck;

/// Drive a single accepted connection through TLS, handshake, and the post-handshake flow.
///
/// Peer-protocol violations send a typed `Error` and return `Ok(())` (normal protocol surface).
/// Real I/O errors (TLS / frame / write) send a best-effort error, then bubble via `?` for the
/// caller's `log_connection_error`; the TLS failure is prefixed so the logger downgrades it.
pub async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
) -> io::Result<()> {
    // Per-IP rate gate, pre-TLS so over-limit peers don't cost a handshake. Dropping `stream`
    // closes the TCP connection silently — spec §Rate Limiting permits a framing-layer drop.
    if state.connection_rate_limiter.try_consume(peer_addr.ip()) == RateCheck::Limited {
        debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_RATE_LIMITED);
        return Ok(());
    }

    let tls_stream = accept_tls_with_timeout(&tls_acceptor, stream).await?;

    handle_connection_inner(tls_stream, peer_addr, fingerprint, state).await
}

/// Stream-generic connection handler shared by the TCP path (post-TLS) and the WS path
/// (post-TLS-and-upgrade, wrapped in `WebSocketAdapter`).
pub async fn handle_connection_inner<S>(
    socket: S,
    peer_addr: SocketAddr,
    fingerprint: String,
    state: Arc<TrackerState>,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (read_half, write_half) = tokio::io::split(socket);
    // `BufReader` coalesces frame-header reads into one syscall per fill. Same as `nexus-server`.
    let mut reader = FrameReader::new(BufReader::new(read_half));
    let mut writer = FrameWriter::new(write_half);

    if !run_handshake_phase(&mut reader, &mut writer, &fingerprint, peer_addr).await? {
        // Handshake failed; response already on the wire, just close.
        return Ok(());
    }

    dispatch_post_handshake(&mut reader, &mut writer, &state, peer_addr).await
}

/// Read the first frame, expect a `Handshake`, run the handler. `Ok(true)` only on success.
async fn run_handshake_phase<R, W>(
    reader: &mut FrameReader<R>,
    writer: &mut FrameWriter<W>,
    fingerprint: &str,
    peer_addr: SocketAddr,
) -> io::Result<bool>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let received = match read_client_handshake_message_with_full_timeout(
        reader,
        Some(HANDSHAKE_TIMEOUT),
        None,
    )
    .await
    {
        Ok(Some(msg)) => msg,
        Ok(None) => return Ok(false), // clean pre-handshake disconnect
        Err(e) => {
            send_handshake_error(
                writer,
                frame_error_command(&e),
                frame_error_message(&e, DEFAULT_LOCALE),
            )
            .await;
            return Err(e.into());
        }
    };

    let version = match received.message {
        ClientMessage::Handshake { version } => version,
        other => {
            // Surface the offending message type as `command`, matching nexus-server.
            let cmd = client_message_type(&other);
            warn!(ip = %peer_addr.ip(), command = %cmd, "{}", LOG_HANDSHAKE_REQUIRED);
            send_handshake_error(
                writer,
                Some(cmd.to_string()),
                err_tracker_handshake_required(DEFAULT_LOCALE),
            )
            .await;
            return Ok(false);
        }
    };

    handlers::handshake::handle_handshake(&version, DEFAULT_LOCALE, fingerprint, writer).await
}

/// Read the first post-handshake message; this locks the connection's role.
/// `TrackerServerRegister` → server connection (refresh loop); `TrackerServerList` → client
/// connection (one response, then close).
async fn dispatch_post_handshake<R, W>(
    reader: &mut FrameReader<R>,
    writer: &mut FrameWriter<W>,
    state: &Arc<TrackerState>,
    peer_addr: SocketAddr,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let received = match read_tracker_client_message_with_full_timeout(
        reader,
        Some(ROLE_ESTABLISH_TIMEOUT),
        None,
    )
    .await
    {
        Ok(Some(msg)) => msg,
        Ok(None) => return Ok(()), // clean disconnect; nothing to clean up yet
        Err(e) => {
            // The unparsed message carried the `locale`, so per spec §Localization we render
            // this pre-locale `Error` in the default locale.
            send_tracker_error(
                writer,
                frame_error_command(&e),
                frame_error_message(&e, DEFAULT_LOCALE),
            )
            .await;
            return Err(e.into());
        }
    };

    match received.message {
        TrackerClientMessage::TrackerServerList {
            password,
            locale,
            version,
        } => {
            handle_tracker_server_list(
                ListParams {
                    password,
                    locale,
                    version,
                },
                state,
                writer,
                peer_addr,
            )
            .await?;
            Ok(())
        }
        TrackerClientMessage::TrackerServerRegister {
            password,
            locale,
            name,
            description,
            address,
            port,
            websocket_port,
            version,
            fingerprint,
            user_count,
            allows_guest,
        } => {
            let params = RegisterParams {
                password,
                locale,
                name,
                description,
                address,
                port,
                websocket_port,
                version,
                fingerprint,
                user_count,
                allows_guest,
            };
            let outcome = handle_initial_register(params, state, writer, peer_addr).await?;
            match outcome {
                InitialRegisterOutcome::Registered(guard) => {
                    // Guard frees the registry slot on every refresh-loop exit path.
                    let id = guard.id();
                    let _guard = guard;
                    run_refresh_loop(reader, writer, id, state, peer_addr).await
                }
                InitialRegisterOutcome::Rejected => Ok(()),
            }
        }
    }
}

/// Loop reading further `TrackerServerRegister` (refresh) messages on a
/// server connection until the peer disconnects, refresh times out,
/// or sends a non-Register message (role violation).
async fn run_refresh_loop<R, W>(
    reader: &mut FrameReader<R>,
    writer: &mut FrameWriter<W>,
    id: ConnectionId,
    state: &Arc<TrackerState>,
    peer_addr: SocketAddr,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let timeout = Duration::from_secs(
        u64::from(state.refresh_interval) * u64::from(STALE_TIMEOUT_REFRESH_MULTIPLIER),
    );

    loop {
        let received = match read_tracker_client_message_with_full_timeout(
            reader,
            Some(timeout),
            None,
        )
        .await
        {
            Ok(Some(msg)) => msg,
            // Clean close and idle-timeout are both silent disconnects per spec; guard unregisters.
            Ok(None) => {
                info!(
                    ip = %peer_addr.ip(),
                    id = id,
                    reason = REASON_DISCONNECT_CLEAN_CLOSE,
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                return Ok(());
            }
            Err(FrameError::IdleTimeout) => {
                info!(
                    ip = %peer_addr.ip(),
                    id = id,
                    reason = REASON_DISCONNECT_STALE_TIMEOUT,
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                return Ok(());
            }
            // Other frame errors: best-effort `Error` send before closing, as in dispatch.
            Err(e) => {
                warn!(
                    ip = %peer_addr.ip(),
                    id = id,
                    err = ?e,
                    reason = REASON_DISCONNECT_FRAME_ERROR,
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                send_tracker_error(
                    writer,
                    frame_error_command(&e),
                    frame_error_message(&e, DEFAULT_LOCALE),
                )
                .await;
                return Ok(());
            }
        };

        match received.message {
            TrackerClientMessage::TrackerServerRegister {
                password,
                locale,
                name,
                description,
                address,
                port,
                websocket_port,
                version,
                fingerprint,
                user_count,
                allows_guest,
            } => {
                let params = RegisterParams {
                    password,
                    locale,
                    name,
                    description,
                    address,
                    port,
                    websocket_port,
                    version,
                    fingerprint,
                    user_count,
                    allows_guest,
                };
                let outcome = handle_refresh(params, id, state, writer, peer_addr).await?;
                match outcome {
                    RefreshOutcome::Refreshed => continue,
                    RefreshOutcome::Rejected => {
                        info!(
                            ip = %peer_addr.ip(),
                            id = id,
                            reason = REASON_DISCONNECT_REJECTED,
                            "{}",
                            LOG_REGISTER_DISCONNECTED
                        );
                        return Ok(());
                    }
                }
            }
            TrackerClientMessage::TrackerServerList { locale, .. } => {
                // Role violation: surface and close. Fall back to DEFAULT_LOCALE
                // when the request locale itself is suspect.
                warn!(ip = %peer_addr.ip(), command = "TrackerServerList", "{}", LOG_ROLE_VIOLATION);
                let translation_locale = role_violation_locale(&locale);
                send_tracker_error(
                    writer,
                    Some("TrackerServerList".to_string()),
                    err_tracker_role_violation(translation_locale),
                )
                .await;
                info!(
                    ip = %peer_addr.ip(),
                    id = id,
                    reason = REASON_DISCONNECT_ROLE_VIOLATION,
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                return Ok(());
            }
        }
    }
}

fn role_violation_locale(locale: &str) -> &str {
    if locale.is_empty() || handlers::validate_locale(locale).is_some() {
        DEFAULT_LOCALE
    } else {
        locale
    }
}

/// Best-effort `ServerMessage::Error` during the handshake phase. Failures silent.
async fn send_handshake_error<W>(
    writer: &mut FrameWriter<W>,
    command: Option<String>,
    message: String,
) where
    W: AsyncWrite + Unpin,
{
    let response = ServerMessage::Error {
        message,
        command,
        disconnect: true,
    };
    let _ = send_server_message_with_write_timeout(writer, &response).await;
}

/// Best-effort `TrackerServerMessage::Error` for post-handshake violations. Failures silent.
async fn send_tracker_error<W>(
    writer: &mut FrameWriter<W>,
    command: Option<String>,
    message: String,
) where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::Error { message, command };
    let _ = send_tracker_server_message_with_write_timeout(writer, &response).await;
}

/// Map a [`FrameError`] to a translated diagnostic. Differentiates malformed JSON,
/// wrong-port peers, wrong-phase/wrong-direction messages, and oversize payloads from generic
/// frame violations so registrants get an actionable message instead of a catch-all.
fn frame_error_message(err: &FrameError, locale: &str) -> String {
    match err {
        FrameError::InvalidJson(_) => err_tracker_malformed_message(locale),
        FrameError::UnknownMessageType(_) => err_tracker_unknown_message_type(locale),
        FrameError::UnexpectedMessageType(_) => err_tracker_unexpected_message_type(locale),
        FrameError::PayloadLengthExceedsTypeMax { .. } => err_tracker_payload_too_large(locale),
        _ => err_tracker_frame_error(locale),
    }
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

    #[test]
    fn role_violation_locale_keeps_valid_locale() {
        assert_eq!(role_violation_locale("pt-BR"), "pt-BR");
    }

    #[test]
    fn role_violation_locale_falls_back_for_suspect_locale() {
        assert_eq!(role_violation_locale(""), DEFAULT_LOCALE);
        assert_eq!(role_violation_locale("en\nUS"), DEFAULT_LOCALE);
        assert_eq!(role_violation_locale(&"a".repeat(64)), DEFAULT_LOCALE);
    }
}
