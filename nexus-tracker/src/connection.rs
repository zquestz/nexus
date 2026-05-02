//! Per-connection task
//!
//! Drives a single accepted TCP connection through:
//!
//! 1. TLS handshake.
//! 2. Protocol handshake (BBS-shaped `Handshake` / `HandshakeResponse`).
//! 3. Post-handshake dispatch on the first `TrackerClientMessage`:
//!    - `TrackerServerRegister` → server connection. Enters the refresh loop;
//!      subsequent `TrackerServerRegister` messages on the same connection
//!      are refreshes. A `TrackerServerList` arrival on a server connection
//!      is a role violation: surface `Error`, close.
//!    - `TrackerServerList` → client connection. The handler sends one
//!      response and we close.
//! 4. Cleanup. Server connections are unregistered from the [`Registry`]
//!    via a drop guard so cancellation, panics, and timeouts all clean
//!    up.
//!
//! Errors from this function bubble out to the spawning task in
//! `main.rs`, which routes them through `log_connection_error` for
//! consistent filter / severity handling across all listener sites.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

use nexus_common::framing::{FrameError, FrameReader, FrameWriter};
use nexus_common::io::{
    client_message_type, read_client_message_with_full_timeout,
    read_tracker_client_message_with_full_timeout, send_server_message,
    send_tracker_server_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::tls::accept_tls_with_timeout;
use nexus_common::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};
use nexus_common::validators::MAX_LOCALE_LENGTH;

use crate::constants::{
    DEFAULT_LOCALE, ERR_REGISTRY_MUTEX_POISONED, HANDSHAKE_TIMEOUT, LOG_CONNECTION_RATE_LIMITED,
    LOG_HANDSHAKE_REQUIRED, LOG_REGISTER_DISCONNECTED, LOG_ROLE_VIOLATION, ROLE_ESTABLISH_TIMEOUT,
    STALE_TIMEOUT_REFRESH_MULTIPLIER,
};
use crate::errors::{
    err_tracker_frame_error, err_tracker_handshake_required, err_tracker_role_violation,
};
use crate::handlers;
use crate::handlers::tracker_server_list::{ListParams, handle_tracker_server_list};
use crate::handlers::tracker_server_register::{
    InitialRegisterOutcome, RefreshOutcome, RegisterParams, handle_initial_register, handle_refresh,
};
use crate::rate_limiter::RateCheck;
use crate::registry::ConnectionId;
use crate::state::TrackerState;

/// Drive a single accepted connection through TLS, handshake, and the
/// post-handshake protocol flow.
///
/// On peer-protocol violations a typed `Error` (or typed-response
/// failure for Register/List) is sent and `Ok(())` is returned — the
/// violation is part of the normal protocol surface, not an I/O failure.
///
/// On real I/O errors (TLS handshake failure, frame errors, write
/// failures), a best-effort error is sent before the underlying error
/// bubbles via `?`. The TLS handshake failure is wrapped with
/// [`TLS_HANDSHAKE_FAILED_PREFIX`] so the shared logger can downgrade
/// it to debug.
///
/// # Errors
///
/// Returns `io::Error` for TLS handshake failures, frame errors, and
/// write failures. The caller logs via `log_connection_error`.
pub async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
) -> io::Result<()> {
    // Per-IP connection-rate gate. We check pre-TLS so over-limit peers
    // don't pay (or make us pay) the cost of a TLS handshake. The
    // `stream` is dropped here, closing the TCP connection silently —
    // per protocol spec §Rate Limiting we may "drop connections at the
    // framing layer" without sending any response.
    if state.connection_rate_limiter.try_consume(peer_addr.ip()) == RateCheck::Limited {
        debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_RATE_LIMITED);
        return Ok(());
    }

    let tls_stream = accept_tls_with_timeout(&tls_acceptor, stream).await?;

    handle_connection_inner(tls_stream, peer_addr, fingerprint, state).await
}

/// Inner connection handler that works with any `AsyncRead + AsyncWrite`
/// stream. Used directly by both the TCP path (after the TLS handshake)
/// and the WebSocket path (after TLS *and* WS upgrade, with the stream
/// wrapped in `WebSocketAdapter`).
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
    // Wrap with `BufReader` so frame-header reads (magic byte, type
    // length, etc.) coalesce into one syscall per buffer fill instead
    // of one syscall per byte. Same posture as `nexus-server`.
    let mut reader = FrameReader::new(BufReader::new(read_half));
    let mut writer = FrameWriter::new(write_half);

    if !run_handshake_phase(&mut reader, &mut writer, &fingerprint, peer_addr).await? {
        // Handshake failed (version mismatch, invalid version string, or
        // peer sent the wrong message first). Response already on the
        // wire; just close.
        return Ok(());
    }

    dispatch_post_handshake(&mut reader, &mut writer, &state, peer_addr).await
}

/// Read the first frame, expect a `Handshake`, and run the handshake
/// handler. Returns `Ok(true)` only when the handshake completed
/// successfully and the connection should continue into the
/// post-handshake phase.
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
    let received =
        match read_client_message_with_full_timeout(reader, Some(HANDSHAKE_TIMEOUT), None).await {
            Ok(Some(msg)) => msg,
            Ok(None) => return Ok(false), // clean pre-handshake disconnect
            Err(e) => {
                send_handshake_error(writer, None, err_tracker_frame_error(DEFAULT_LOCALE)).await;
                return Err(e.into());
            }
        };

    let version = match received.message {
        ClientMessage::Handshake { version } => version,
        other => {
            // Match nexus-server's convention: surface the offending
            // message type as the `command` field.
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

/// Read the first post-handshake message and dispatch it to the
/// matching handler. The connection's role is locked here:
/// `TrackerServerRegister` → server connection (entering the refresh loop);
/// `TrackerServerList` → client connection (handler runs and we close).
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
            send_tracker_error(writer, None, err_tracker_frame_error(DEFAULT_LOCALE)).await;
            return Err(e.into());
        }
    };

    match received.message {
        TrackerClientMessage::TrackerServerList { password, locale } => {
            handle_tracker_server_list(ListParams { password, locale }, state, writer, peer_addr)
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
                InitialRegisterOutcome::Registered(id) => {
                    // The drop guard ensures the registry slot is
                    // freed when the refresh loop exits — clean
                    // disconnect, role violation, timeout, panic.
                    let _guard = RegistrationGuard {
                        state: Arc::clone(state),
                        id,
                    };
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
            // Clean TCP close (Ok(None)) and idle-timeout (stale entry)
            // are both silent disconnects per spec; drop guard handles
            // unregister and logs the reason.
            Ok(None) => {
                info!(
                    ip = %peer_addr.ip(),
                    id = id,
                    reason = "clean_close",
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                return Ok(());
            }
            Err(FrameError::IdleTimeout) => {
                info!(
                    ip = %peer_addr.ip(),
                    id = id,
                    reason = "stale_timeout",
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                return Ok(());
            }
            // Other frame errors (malformed JSON, payload too large,
            // bad framing): match `dispatch_post_handshake`'s
            // best-effort `Error` send before closing.
            Err(e) => {
                warn!(
                    ip = %peer_addr.ip(),
                    id = id,
                    err = ?e,
                    reason = "frame_error",
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                send_tracker_error(writer, None, err_tracker_frame_error(DEFAULT_LOCALE)).await;
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
                            reason = "rejected",
                            "{}",
                            LOG_REGISTER_DISCONNECTED
                        );
                        return Ok(());
                    }
                }
            }
            TrackerClientMessage::TrackerServerList { locale, .. } => {
                // Role violation: surface and close. Match the locale-length
                // contract enforced by the real handlers (`tracker_server_register`
                // and `tracker_server_list`); fall back to DEFAULT_LOCALE if the
                // locale is empty or oversized rather than threading an
                // untrusted string into `LanguageIdentifier::parse`.
                warn!(ip = %peer_addr.ip(), command = "TrackerServerList", "{}", LOG_ROLE_VIOLATION);
                let translation_locale = if locale.is_empty() || locale.len() > MAX_LOCALE_LENGTH {
                    DEFAULT_LOCALE
                } else {
                    &locale
                };
                send_tracker_error(
                    writer,
                    Some("TrackerServerList".to_string()),
                    err_tracker_role_violation(translation_locale),
                )
                .await;
                info!(
                    ip = %peer_addr.ip(),
                    id = id,
                    reason = "role_violation",
                    "{}",
                    LOG_REGISTER_DISCONNECTED
                );
                return Ok(());
            }
        }
    }
}

/// Drop guard that unregisters a server connection's entry when the
/// per-connection task exits.
///
/// **Why a Drop guard rather than explicit cleanup** (which is what
/// `nexus-server` does for sessions): tracker connections have many
/// exit paths — clean TCP close, idle timeout, role violation, frame
/// error, register-rejection, panic — and the registry must release
/// its slot in *every* case. A guard catches them all uniformly,
/// including async cancellation. `unregister` is idempotent so a
/// future explicit unregister wouldn't be wrong, but the guard is the
/// load-bearing mechanism today.
///
/// `run_refresh_loop` logs the disconnect reason before returning, so
/// the guard's `Drop` performs only the registry mutation (no log
/// here, to avoid double-logging on normal exit paths). On a panic
/// the registry is still cleaned up, just without an attribution log.
struct RegistrationGuard {
    state: Arc<TrackerState>,
    id: ConnectionId,
}

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        self.state
            .registry
            .lock()
            .expect(ERR_REGISTRY_MUTEX_POISONED)
            .unregister(self.id);
    }
}

/// Best-effort `ServerMessage::Error` send during the BBS-shaped
/// handshake phase. Failures silent — connection is closing.
async fn send_handshake_error<W>(
    writer: &mut FrameWriter<W>,
    command: Option<String>,
    message: String,
) where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::Error { message, command };
    let _ = send_server_message(writer, &response).await;
}

/// Best-effort `TrackerServerMessage::Error` send for post-handshake
/// protocol violations (frame error, role violation). Failures silent.
async fn send_tracker_error<W>(
    writer: &mut FrameWriter<W>,
    command: Option<String>,
    message: String,
) where
    W: AsyncWriteExt + Unpin,
{
    let response = TrackerServerMessage::Error { message, command };
    let _ = send_tracker_server_message(writer, &response).await;
}
