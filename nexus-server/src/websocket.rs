//! WebSocket entry points for the BBS server
//!
//! TLS handshake → WebSocket handshake → wrap in
//! [`WebSocketAdapter`] → delegate to the standard connection /
//! transfer handlers. The byte-stream adapter itself lives in
//! `nexus_common::websocket` so it can be shared with `nexus-tracker`.

use std::io;

use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use nexus_common::tls::accept_tls_with_timeout;
use nexus_common::websocket::{WebSocketAdapter, accept_ws_with_timeout};

use crate::connection::{ConnectionParams, handle_connection_inner};
use crate::transfers::{TransferParams, handle_transfer_connection_inner};

/// Handle a WebSocket BBS connection
///
/// Performs TLS handshake, then WebSocket handshake, then wraps in adapter
/// and delegates to the standard connection handler.
pub async fn handle_websocket_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: ConnectionParams,
) -> io::Result<()> {
    // TLS handshake (mandatory, same as TCP) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    // WebSocket upgrade over TLS, also timeout-bounded.
    let ws_stream = accept_ws_with_timeout(tls_stream).await?;

    // Wrap in adapter and delegate to standard handler
    let adapter = WebSocketAdapter::new(ws_stream);
    handle_connection_inner(adapter, params).await
}

/// Handle a WebSocket transfer connection
///
/// Performs TLS handshake, then WebSocket handshake, then wraps in adapter
/// and delegates to the standard transfer handler.
pub async fn handle_websocket_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    // TLS handshake (mandatory, same as TCP) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    // WebSocket upgrade over TLS, also timeout-bounded.
    let ws_stream = accept_ws_with_timeout(tls_stream).await?;

    // Wrap in adapter and delegate to standard transfer handler
    let adapter = WebSocketAdapter::new(ws_stream);
    handle_transfer_connection_inner(adapter, params).await
}
