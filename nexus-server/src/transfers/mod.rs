//! Transfer connection handler for file downloads and uploads (port 7501)
//!
//! This module handles file transfer connections on a separate port from the main
//! BBS protocol. The transfer protocol uses the same TLS certificate and framing
//! format, but with a simplified flow:
//!
//! **Download flow:**
//! 1. Client: Handshake → Server: HandshakeResponse
//! 2. Client: Login → Server: LoginResponse (simplified: just success/error)
//! 3. Client: FileDownload → Server: FileDownloadResponse
//! 4. For each file: Server: FileStart → Client: FileStartResponse → Server: FileData → Server: FileHash
//! 5. Server: TransferComplete
//! 6. Server closes connection
//!
//! **Upload flow:**
//! 1. Client: Handshake → Server: HandshakeResponse
//! 2. Client: Login → Server: LoginResponse (simplified: just success/error)
//! 3. Client: FileUpload → Server: FileUploadResponse
//! 4. For each file: Client: FileStart → Server: FileStartResponse → Client: FileData → Client: FileHash
//! 5. Server: TransferComplete
//! 6. Server closes connection

mod auth;
mod download;
mod hashing;
mod helpers;
pub mod registry;
#[cfg(test)]
mod test_helpers;
mod transfer;
mod types;
mod upload;

use std::io;

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tracing::debug;

use nexus_common::framing::{FrameReader, FrameWriter};

use crate::constants::*;
use crate::handlers::err_file_area_not_configured;

use auth::{handle_transfer_handshake, handle_transfer_login, handle_transfer_request};
use download::handle_download;
use helpers::send_error_and_close;
use registry::TransferDirection;
use transfer::Transfer;
use types::TransferRequest;
use upload::handle_upload;

// Re-export public types
pub use registry::TransferRegistry;
pub use types::TransferParams;

/// Handle a transfer connection (file downloads and uploads on port 7501)
pub async fn handle_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    // Perform TLS handshake (mandatory, same cert as main port)
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("TLS handshake failed: {e}")))?;

    handle_transfer_connection_inner(tls_stream, params).await
}

/// Inner transfer connection handler that works with any AsyncRead + AsyncWrite stream
///
/// This is used by both TCP and WebSocket connections.
pub async fn handle_transfer_connection_inner<S>(
    socket: S,
    params: TransferParams,
) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let TransferParams {
        peer_addr,
        db,
        file_root,
        file_index,
        transfer_registry,
    } = params;

    debug!(ip = %peer_addr, "{}", LOG_TRANSFER_CONNECTION);

    // Set up framed I/O
    let (reader, writer) = tokio::io::split(socket);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Default locale for error messages before login
    let mut locale = DEFAULT_LOCALE.to_string();

    // Phase 1: Handshake
    let handshake_result =
        handle_transfer_handshake(&mut frame_reader, &mut frame_writer, &locale).await;
    if let Err(e) = handshake_result {
        debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_HANDSHAKE_FAILED);
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Phase 2: Login (simplified - just authentication)
    let user =
        match handle_transfer_login(&mut frame_reader, &mut frame_writer, &db, &mut locale).await {
            Ok(user) => user,
            Err(e) => {
                debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_LOGIN_FAILED);
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    debug!(user = %user.username, ip = %peer_addr, "{}", LOG_TRANSFER_AUTHENTICATED);

    // Phase 3: Transfer request (FileDownload or FileUpload)
    let Some(file_root) = file_root else {
        // File area not configured - send generic error since we don't know
        // if this is a download or upload request yet
        return send_error_and_close(&mut frame_writer, &err_file_area_not_configured(&locale))
            .await;
    };

    let request = match handle_transfer_request(&mut frame_reader, &mut frame_writer, &locale).await
    {
        Ok(req) => req,
        Err(e) => {
            debug!(user = %user.username, ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_REQUEST_FAILED);
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    // Determine transfer direction, path, and size for registry metadata
    let (direction, path, total_size) = match &request {
        TransferRequest::Download(p) => {
            // For downloads, size is unknown until path resolution (set to 0, updated later)
            (TransferDirection::Download, p.path.clone(), 0)
        }
        TransferRequest::Upload(p) => (
            TransferDirection::Upload,
            p.destination.clone(),
            p.total_size,
        ),
    };

    // Register with transfer registry for ban signal handling
    // We do this after authentication so we have the user's locale for error messages
    let (transfer_id, info, ban_rx) = transfer_registry.register(
        peer_addr,
        user.nickname.clone(),
        user.username.clone(),
        user.is_admin,
        user.is_shared,
        direction,
        path,
        total_size,
    );

    // Create Transfer struct that owns the connection and handles ban signals
    // The Transfer is automatically unregistered when dropped via RAII guard
    let mut transfer = Transfer::new(
        frame_reader,
        frame_writer,
        ban_rx,
        info,
        user,
        locale,
        file_root,
        &file_index,
        &transfer_registry,
        transfer_id,
    );

    // Dispatch to appropriate handler
    let result = match request {
        TransferRequest::Download(params) => handle_download(&mut transfer, params).await,
        TransferRequest::Upload(params) => handle_upload(&mut transfer, params).await,
    };

    let elapsed = transfer.elapsed();
    let bytes = transfer.bytes_transferred();
    debug!(
        id = %transfer.id(),
        user = %transfer.user().username,
        ip = %peer_addr,
        bytes = %bytes,
        elapsed_secs = elapsed.as_secs_f64(),
        "{}", LOG_TRANSFER_COMPLETE
    );

    result
}
