//! File download/upload handler on port 7501. Same TLS cert and framing as the
//! BBS port, with a simplified flow. Per-file streaming is
//! `FileStart → FileStartResponse → [FileData →] FileHash` (FileData omitted for
//! zero-byte/already-complete files); the sender drives downloads, the client
//! drives uploads.
//!
//! Both directions: Handshake → Login (success/error only) → FileDownload or
//! FileUpload → per-file loop → TransferComplete → server closes.

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
use nexus_common::tls::accept_tls_with_timeout;

use crate::constants::*;
use crate::handlers::err_file_area_not_configured;

use auth::{handle_transfer_handshake, handle_transfer_login, handle_transfer_request};
use download::handle_download;
use helpers::send_error_and_close;
use registry::{TransferDirection, TransferRegistration};
use transfer::{Transfer, TransferContext};
use types::TransferRequest;
use upload::handle_upload;

pub use registry::TransferRegistry;
pub use types::TransferParams;

/// Handle a transfer connection (downloads and uploads on port 7501).
pub async fn handle_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    // Mandatory TLS (same cert as main port) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    handle_transfer_connection_inner(tls_stream, params).await
}

/// Inner handler over any byte stream, shared by TCP and WebSocket connections.
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
        file_mutation_locks,
        transfer_registry,
        fingerprint,
    } = params;

    debug!(ip = %peer_addr, "{}", LOG_TRANSFER_CONNECTION);

    let (reader, writer) = tokio::io::split(socket);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Default locale for errors sent before login.
    let mut locale = DEFAULT_LOCALE.to_string();

    // Phase 1: Handshake
    let handshake_result =
        handle_transfer_handshake(&mut frame_reader, &mut frame_writer, &locale, fingerprint).await;
    if let Err(e) = handshake_result {
        debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_HANDSHAKE_FAILED);
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Phase 2: Login (authentication only)
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
        // Generic error: direction (download/upload) isn't known yet.
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

    // Registry metadata. Download size is unknown until path resolution (0 now,
    // updated later); upload size is known up front.
    let (direction, path, total_size) = match &request {
        TransferRequest::Download(p) => (TransferDirection::Download, p.path.clone(), 0),
        TransferRequest::Upload(p) => (
            TransferDirection::Upload,
            p.destination.clone(),
            p.total_size,
        ),
    };

    // Register after auth so the user's locale is available for error messages.
    let (info, ban_rx) = transfer_registry.register(TransferRegistration {
        peer_addr,
        nickname: user.nickname.clone(),
        username: user.username.clone(),
        is_admin: user.is_admin,
        is_shared: user.is_shared,
        direction,
        path,
        total_size,
    });

    // Owns the connection and ban handling; unregisters on drop via RAII guard.
    let mut transfer = Transfer::new(
        frame_reader,
        frame_writer,
        ban_rx,
        info,
        TransferContext {
            user,
            locale,
            file_root,
            file_index: &file_index,
            file_mutation_locks: &file_mutation_locks,
            registry: &transfer_registry,
        },
    );

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
