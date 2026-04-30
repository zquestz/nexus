//! WebSocket entry point for `nexus-tracker`.
//!
//! Performs TLS + WebSocket handshake, wraps the result in
//! [`WebSocketAdapter`], and delegates to
//! [`crate::connection::handle_connection_inner`]. The connection task
//! itself is shared with the TCP path — only the wire framing differs.
//!
//! [`WebSocketAdapter`]: nexus_common::websocket::WebSocketAdapter

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::debug;

use nexus_common::websocket::WebSocketAdapter;
use nexus_common::{TLS_HANDSHAKE_FAILED_PREFIX, WS_HANDSHAKE_FAILED_PREFIX};

use crate::connection::handle_connection_inner;
use crate::constants::LOG_CONNECTION_RATE_LIMITED;
use crate::rate_limiter::RateCheck;
use crate::state::TrackerState;

/// Drive a single WebSocket connection. Mirrors `handle_connection`
/// from `connection.rs` but adds the WS upgrade between TLS and the
/// protocol layer.
///
/// # Errors
///
/// Returns `io::Error` for TLS handshake failures (wrapped with
/// [`TLS_HANDSHAKE_FAILED_PREFIX`]), WebSocket handshake failures,
/// frame errors, and write failures. The caller logs via
/// `log_connection_error`.
pub async fn handle_tracker_websocket_connection(
    socket: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
) -> io::Result<()> {
    // Per-IP connection-rate gate, identical to the TCP path. Drop
    // pre-TLS / pre-WS so over-limit peers don't pay for the upgrade
    // dance.
    if state.connection_rate_limiter.try_consume(peer_addr.ip()) == RateCheck::Limited {
        debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_RATE_LIMITED);
        return Ok(());
    }

    // TLS first, same as the TCP path. Wrapping with the prefix lets
    // `log_connection_error` downgrade scanner / incompatible-client
    // noise to debug.
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("{}{}", TLS_HANDSHAKE_FAILED_PREFIX, e)))?;

    // WebSocket upgrade over TLS. Failures here are typically benign
    // (peer not actually speaking WebSocket); they bubble up as plain
    // `io::Error` and `log_connection_error` reports them at error
    // level.
    let ws_stream = tokio_tungstenite::accept_async(tls_stream)
        .await
        .map_err(|e| io::Error::other(format!("{}{}", WS_HANDSHAKE_FAILED_PREFIX, e)))?;

    let adapter = WebSocketAdapter::new(ws_stream);
    handle_connection_inner(adapter, peer_addr, fingerprint, state).await
}
