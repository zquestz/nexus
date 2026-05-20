//! WebSocket entry point: TLS + WS handshake, wrap in `WebSocketAdapter`, then delegate to
//! [`crate::connection::handle_connection_inner`]. The connection task is shared with the TCP
//! path — only the wire framing differs.

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

/// Drive a single WS connection: like `handle_connection`, plus the WS upgrade between TLS and
/// the protocol layer.
///
/// # Errors
///
/// `io::Error` for TLS / WS handshake, frame, and write failures (TLS prefixed for the logger).
pub async fn handle_tracker_websocket_connection(
    socket: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
    state: Arc<TrackerState>,
) -> io::Result<()> {
    // Per-IP rate gate, pre-TLS / pre-WS, as in the TCP path.
    if state.connection_rate_limiter.try_consume(peer_addr.ip()) == RateCheck::Limited {
        debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_RATE_LIMITED);
        return Ok(());
    }

    // TLS first (slowloris-bounded, failures prefixed for the logger), then the WS upgrade over it
    // (also timeout-bounded; failures are usually benign non-WS peers).
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;
    let ws_stream = accept_ws_with_timeout(tls_stream).await?;

    let adapter = WebSocketAdapter::new(ws_stream);
    handle_connection_inner(adapter, peer_addr, fingerprint, state).await
}
