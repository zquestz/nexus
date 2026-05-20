//! WebSocket entry points for the BBS server
//!
//! TLS handshake → WebSocket handshake → wrap in [`WebSocketAdapter`]
//! (presents the WS connection as a byte stream, ignoring message
//! boundaries) → delegate to the standard connection / transfer
//! handlers. The adapter lives in `nexus_common::websocket` so it's
//! shared with `nexus-tracker`.

use std::io;

use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use nexus_common::tls::accept_tls_with_timeout;
use nexus_common::websocket::{WebSocketAdapter, accept_ws_with_timeout};

use crate::connection::{ConnectionParams, handle_connection_inner};
use crate::transfers::{TransferParams, handle_transfer_connection_inner};

/// Handle a WebSocket BBS connection
pub async fn handle_websocket_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: ConnectionParams,
) -> io::Result<()> {
    // TLS handshake (mandatory, same as TCP) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    let ws_stream = accept_ws_with_timeout(tls_stream).await?;

    let adapter = WebSocketAdapter::new(ws_stream);
    handle_connection_inner(adapter, params).await
}

/// Handle a WebSocket transfer connection
pub async fn handle_websocket_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    // TLS handshake (mandatory, same as TCP) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    let ws_stream = accept_ws_with_timeout(tls_stream).await?;

    let adapter = WebSocketAdapter::new(ws_stream);
    handle_transfer_connection_inner(adapter, params).await
}
