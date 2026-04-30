//! WebSocket entry points for the BBS server
//!
//! TLS handshake → WebSocket handshake → wrap in
//! [`WebSocketAdapter`] → delegate to the standard connection /
//! transfer handlers. The byte-stream adapter itself lives in
//! `nexus_common::websocket` so it can be shared with `nexus-tracker`.

use std::io;

use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use nexus_common::websocket::WebSocketAdapter;
use nexus_common::{TLS_HANDSHAKE_FAILED_PREFIX, WS_HANDSHAKE_FAILED_PREFIX};

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
    // Perform TLS handshake (mandatory, same as TCP)
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("{}{}", TLS_HANDSHAKE_FAILED_PREFIX, e)))?;

    // Perform WebSocket handshake over TLS
    let ws_stream = tokio_tungstenite::accept_async(tls_stream)
        .await
        .map_err(|e| io::Error::other(format!("{}{}", WS_HANDSHAKE_FAILED_PREFIX, e)))?;

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
    // Perform TLS handshake (mandatory, same as TCP)
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("{}{}", TLS_HANDSHAKE_FAILED_PREFIX, e)))?;

    // Perform WebSocket handshake over TLS
    let ws_stream = tokio_tungstenite::accept_async(tls_stream)
        .await
        .map_err(|e| io::Error::other(format!("{}{}", WS_HANDSHAKE_FAILED_PREFIX, e)))?;

    // Wrap in adapter and delegate to standard transfer handler
    let adapter = WebSocketAdapter::new(ws_stream);
    handle_transfer_connection_inner(adapter, params).await
}
