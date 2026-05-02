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

use nexus_common::tls::accept_tls_with_timeout;
use nexus_common::websocket::{WebSocketAdapter, accept_ws_with_timeout};

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

    // TLS first, same as the TCP path. The shared helper wraps the
    // accept in `TLS_HANDSHAKE_TIMEOUT` (slowloris defense) and prefixes
    // failures with `TLS_HANDSHAKE_FAILED_PREFIX` so `log_connection_error`
    // downgrades scanner / incompatible-client noise to debug.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    // WebSocket upgrade over TLS. The shared helper wraps the upgrade
    // in `WS_HANDSHAKE_TIMEOUT`. Failures here are typically benign
    // (peer not actually speaking WebSocket); the prefix is recognized
    // by `log_connection_error`.
    let ws_stream = accept_ws_with_timeout(tls_stream).await?;

    let adapter = WebSocketAdapter::new(ws_stream);
    handle_connection_inner(adapter, peer_addr, fingerprint, state).await
}
