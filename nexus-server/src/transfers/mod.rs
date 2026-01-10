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
//! 4. For each file: Server: FileStart → Client: FileStartResponse → Server: FileData
//! 5. Server: TransferComplete
//! 6. Server closes connection
//!
//! **Upload flow:**
//! 1. Client: Handshake → Server: HandshakeResponse
//! 2. Client: Login → Server: LoginResponse (simplified: just success/error)
//! 3. Client: FileUpload → Server: FileUploadResponse
//! 4. For each file: Client: FileStart → Server: FileStartResponse → Client: FileData
//! 5. Server: TransferComplete
//! 6. Server closes connection

mod auth;
mod download;
mod hash;
mod helpers;
mod types;
mod upload;

use std::io;

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use nexus_common::framing::{FrameReader, FrameWriter};

use crate::constants::DEFAULT_LOCALE;
use crate::handlers::err_file_area_not_configured;

use auth::{handle_transfer_handshake, handle_transfer_login, handle_transfer_request};
use download::handle_download;
use helpers::send_error_and_close;
use types::TransferContext;
use types::TransferRequest;
use upload::handle_upload;

// Re-export public types
pub use types::TransferParams;

/// Handle a transfer connection (file downloads and uploads on port 7501)
pub async fn handle_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    let TransferParams {
        peer_addr,
        db,
        debug,
        file_root,
        file_index,
    } = params;

    // Perform TLS handshake (mandatory, same cert as main port)
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("TLS handshake failed: {e}")))?;

    if debug {
        eprintln!("Transfer connection from {peer_addr}");
    }

    // Set up framed I/O
    let (reader, writer) = tokio::io::split(tls_stream);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Default locale for error messages before login
    let mut locale = DEFAULT_LOCALE.to_string();

    // Phase 1: Handshake
    let handshake_result =
        handle_transfer_handshake(&mut frame_reader, &mut frame_writer, &locale).await;
    if let Err(e) = handshake_result {
        if debug {
            eprintln!("Transfer handshake failed from {peer_addr}: {e}");
        }
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Phase 2: Login (simplified - just authentication)
    let user =
        match handle_transfer_login(&mut frame_reader, &mut frame_writer, &db, &mut locale).await {
            Ok(user) => user,
            Err(e) => {
                if debug {
                    eprintln!("Transfer login failed from {peer_addr}: {e}");
                }
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    if debug {
        eprintln!("Transfer authenticated: {} from {peer_addr}", user.username);
    }

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
            if debug {
                eprintln!("Transfer request failed from {peer_addr}: {e}");
            }
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    // Create transfer context
    let mut ctx = TransferContext {
        frame_reader: &mut frame_reader,
        frame_writer: &mut frame_writer,
        user: &user,
        file_root,
        locale: &locale,
        peer_addr,
        debug,
        file_index: &file_index,
    };

    // Dispatch to appropriate handler
    match request {
        TransferRequest::Download(params) => handle_download(&mut ctx, params).await,
        TransferRequest::Upload(params) => handle_upload(&mut ctx, params).await,
    }
}
