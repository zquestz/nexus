//! Per-connection task
//!
//! Handles the lifecycle of a single accepted TCP connection from TLS
//! through protocol handshake. After the handshake, the connection is
//! closed (subsequent steps will read the role-establishing message
//! here and dispatch to TrackerRegister / TrackerList handlers).
//!
//! Errors from this function bubble out to the spawning task in
//! `main.rs`, which routes them through `log_connection_error` for
//! consistent filter / severity handling across all listener sites
//! (matching `nexus-server`'s pattern).

use std::io;
use std::net::SocketAddr;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::warn;

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    client_message_type, read_client_message_with_full_timeout, send_server_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::constants::{
    DEFAULT_LOCALE, HANDSHAKE_TIMEOUT, LOG_HANDSHAKE_REQUIRED, TLS_HANDSHAKE_FAILED_PREFIX,
};
use crate::errors::{err_tracker_frame_error, err_tracker_handshake_required};
use crate::handlers;

/// Drive a single accepted connection through TLS + handshake.
///
/// On peer-protocol violations (non-Handshake first message), a typed
/// `Error` response is sent and `Ok(())` is returned — the violation
/// is part of the normal protocol surface, not an I/O failure.
///
/// On real I/O errors (TLS handshake failure, frame errors, write
/// failures), a best-effort `Error` is sent before the underlying
/// error bubbles via `?`. The TLS handshake failure is wrapped with
/// `TLS_HANDSHAKE_FAILED_PREFIX` so the shared logger can downgrade
/// it to debug.
///
/// # Errors
///
/// Returns `io::Error` for TLS handshake failures, frame errors, and
/// write failures during the handshake response. The caller is expected
/// to log via `log_connection_error`.
pub async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    fingerprint: String,
) -> io::Result<()> {
    let tls_stream = tls_acceptor
        .accept(stream)
        .await
        .map_err(|e| io::Error::other(format!("{} {}", TLS_HANDSHAKE_FAILED_PREFIX, e)))?;

    let (read_half, write_half) = tokio::io::split(tls_stream);
    let mut reader = FrameReader::new(read_half);
    let mut writer = FrameWriter::new(write_half);

    // The handshake doesn't carry a locale field, so any error sent
    // before/during the handshake uses the default locale.
    let locale = DEFAULT_LOCALE;

    let received =
        match read_client_message_with_full_timeout(&mut reader, Some(HANDSHAKE_TIMEOUT), None)
            .await
        {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                // Clean disconnect before sending anything.
                return Ok(());
            }
            Err(e) => {
                // Best-effort `Error` to the peer, then bubble the underlying
                // frame error so the shared logger can categorize it.
                send_error(&mut writer, None, err_tracker_frame_error(locale)).await;
                return Err(e.into());
            }
        };

    let version = match received.message {
        ClientMessage::Handshake { version } => version,
        other => {
            // Per-spec: any non-Handshake first message is a protocol
            // violation. Match nexus-server's convention and surface the
            // offending message type as the `command` field on the Error.
            // Returning Ok because this is a typed-response outcome, not
            // an I/O failure.
            let cmd = client_message_type(&other);
            warn!(ip = %peer_addr, command = %cmd, "{}", LOG_HANDSHAKE_REQUIRED);
            send_error(
                &mut writer,
                Some(cmd.to_string()),
                err_tracker_handshake_required(locale),
            )
            .await;
            return Ok(());
        }
    };

    let _handshake_ok =
        handlers::handshake::handle_handshake(&version, locale, &fingerprint, &mut writer).await?;

    // Step #3 closes after the handshake; later steps will read the
    // role-establishing message (`TrackerRegister` or `TrackerList`).
    Ok(())
}

/// Best-effort `Error` send. Failures are silent — the connection is
/// already closing or about to surface a real error to the caller.
/// `command` carries the offending message type when known (matching
/// `nexus-server`'s convention), and `None` otherwise.
async fn send_error<W>(writer: &mut FrameWriter<W>, command: Option<String>, message: String)
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::Error { message, command };
    let _ = send_server_message(writer, &response).await;
}
