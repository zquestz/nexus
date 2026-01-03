//! Transfer executor - processes file transfers in the background
//!
//! The executor watches for queued transfers and processes them one at a time.
//! It connects to the transfer port (7501), performs authentication, and
//! executes the download protocol.
//!
//! Supports cancellation via an atomic flag that is checked periodically
//! during the transfer.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_socks::tcp::Socks5Stream;
use uuid::Uuid;

use nexus_common::PROTOCOL_VERSION;
use nexus_common::framing::{FrameError, FrameHeader, FrameReader, FrameWriter};
use nexus_common::io::{read_server_message, send_client_message};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use super::subscription::ProxyConfig;
use super::{Transfer, TransferConnectionInfo, TransferError};
use crate::i18n::t;

// =============================================================================
// Constants
// =============================================================================

/// Connection timeout for TLS handshake
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Idle timeout waiting for a frame to start
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Frame completion timeout (once first byte received)
/// Currently unused - using PROGRESS_TIMEOUT for FileData frames instead.
/// Kept for potential future use with non-FileData frames.
#[allow(dead_code)]
const FRAME_TIMEOUT: Duration = Duration::from_secs(60);

/// Progress timeout for FileData (must receive some bytes within this time)
const PROGRESS_TIMEOUT: Duration = Duration::from_secs(60);

/// Buffer size for file I/O operations
const BUFFER_SIZE: usize = 64 * 1024; // 64KB

/// Suffix for incomplete downloads
const PART_SUFFIX: &str = ".part";

// =============================================================================
// Progress Events
// =============================================================================

/// Progress event sent from executor to UI
#[derive(Debug, Clone)]
pub enum TransferEvent {
    /// Transfer started connecting
    Connecting { id: Uuid },

    /// Transfer started (received FileDownloadResponse)
    Started {
        id: Uuid,
        total_bytes: u64,
        file_count: u64,
        server_transfer_id: String,
    },

    /// Progress update (periodically during transfer)
    Progress {
        id: Uuid,
        transferred_bytes: u64,
        files_completed: u64,
        current_file: Option<String>,
    },

    /// File completed (fields used for logging/debugging, not currently read by handler)
    #[allow(dead_code)]
    FileCompleted { id: Uuid, path: String },

    /// Transfer completed successfully
    Completed { id: Uuid },

    /// Transfer failed
    Failed {
        id: Uuid,
        error: String,
        error_kind: Option<TransferError>,
    },

    /// Transfer was paused (not yet implemented)
    Paused { id: Uuid },
}

// =============================================================================
// Executor
// =============================================================================

/// Execute a single transfer
///
/// This function handles the complete lifecycle of a download:
/// 1. Connect to transfer port with TLS
/// 2. Verify certificate fingerprint
/// 3. Perform handshake and login
/// 4. Send FileDownload request
/// 5. Receive files and write to disk
/// 6. Handle resume via .part files
///
/// The optional `cancel_flag` is checked periodically during the transfer.
/// If set to true, the transfer is aborted and a Paused event is sent.
pub async fn execute_transfer(
    transfer: &Transfer,
    event_tx: mpsc::UnboundedSender<TransferEvent>,
    cancel_flag: Option<Arc<AtomicBool>>,
    proxy: Option<ProxyConfig>,
) -> Result<(), TransferError> {
    let id = transfer.id;

    // Check for cancellation before starting
    if is_cancelled(&cancel_flag) {
        let _ = event_tx.send(TransferEvent::Paused { id });
        return Ok(());
    }

    // Notify UI that we're connecting
    let _ = event_tx.send(TransferEvent::Connecting { id });

    // Connect and authenticate
    let (mut reader, mut writer, _fingerprint) = match connect_and_authenticate(
        &transfer.connection,
        &transfer.connection.certificate_fingerprint,
        proxy,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            let error = match e {
                TransferError::ConnectionError => t("transfer-error-connection"),
                TransferError::CertificateMismatch => t("transfer-error-certificate-mismatch"),
                TransferError::AuthenticationFailed => t("transfer-error-auth-failed"),
                TransferError::UnsupportedVersion => t("transfer-error-unsupported-version"),
                _ => t("transfer-error-unknown"),
            };
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error,
                error_kind: Some(e.clone()),
            });
            return Err(e);
        }
    };

    // Send FileDownload request
    let download_request = ClientMessage::FileDownload {
        path: transfer.remote_path.clone(),
        root: transfer.remote_root,
    };
    send_client_message(&mut writer, &download_request)
        .await
        .map_err(|e| {
            eprintln!("Failed to send FileDownload: {e}");
            TransferError::ConnectionError
        })?;

    // Read FileDownloadResponse
    let response = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;

    let (total_bytes, file_count, server_transfer_id) = match response {
        ServerMessage::FileDownloadResponse {
            success: true,
            size,
            file_count,
            transfer_id,
            ..
        } => (
            size.unwrap_or(0),
            file_count.unwrap_or(0),
            transfer_id.unwrap_or_default(),
        ),

        ServerMessage::FileDownloadResponse {
            success: false,
            error_kind,
            ..
        } => {
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            let error = match err_kind {
                TransferError::NotFound => t("transfer-error-not-found"),
                TransferError::Permission => t("transfer-error-permission"),
                TransferError::Invalid => t("transfer-error-invalid"),
                _ => t("transfer-error-unknown"),
            };
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error,
                error_kind: Some(err_kind.clone()),
            });
            return Err(err_kind);
        }

        other => {
            eprintln!("Unexpected response to FileDownload: {other:?}");
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: t("transfer-error-protocol"),
                error_kind: Some(TransferError::ProtocolError),
            });
            return Err(TransferError::ProtocolError);
        }
    };

    // Notify UI that transfer has started
    let _ = event_tx.send(TransferEvent::Started {
        id,
        total_bytes,
        file_count,
        server_transfer_id,
    });

    // Check for cancellation after connecting
    if is_cancelled(&cancel_flag) {
        let _ = event_tx.send(TransferEvent::Paused { id });
        return Ok(());
    }

    // Handle empty directory case
    if file_count == 0 {
        // Wait for TransferComplete
        let complete = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;
        return handle_transfer_complete(complete, id, &event_tx);
    }

    // Process each file
    let mut transferred_bytes: u64 = 0;
    let mut files_completed: u64 = 0;
    let base_path = &transfer.local_path;

    for _file_index in 0..file_count {
        // Check for cancellation before each file
        if is_cancelled(&cancel_flag) {
            let _ = event_tx.send(TransferEvent::Paused { id });
            return Ok(());
        }

        // Read FileStart
        let file_start = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;

        let (file_path, file_size, file_sha256) = match file_start {
            ServerMessage::FileStart { path, size, sha256 } => (path, size, sha256),
            ServerMessage::TransferComplete { .. } => {
                // Early completion (error case)
                return handle_transfer_complete(file_start, id, &event_tx);
            }
            other => {
                eprintln!("Expected FileStart, got: {other:?}");
                let _ = event_tx.send(TransferEvent::Failed {
                    id,
                    error: t("transfer-error-protocol"),
                    error_kind: Some(TransferError::ProtocolError),
                });
                return Err(TransferError::ProtocolError);
            }
        };

        // Validate path (security check)
        if !is_safe_path(&file_path) {
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: t("transfer-error-invalid"),
                error_kind: Some(TransferError::Invalid),
            });
            return Err(TransferError::Invalid);
        }

        // Update current file in progress
        let _ = event_tx.send(TransferEvent::Progress {
            id,
            transferred_bytes,
            files_completed,
            current_file: Some(file_path.clone()),
        });

        // Determine local file path
        // For single file downloads, use local_path directly (it already has the filename)
        // For directory downloads, use local_path as base and join the relative path
        let local_file_path = if transfer.is_directory {
            base_path.join(&file_path)
        } else {
            base_path.clone()
        };

        // Check if a COMPLETE file exists at the destination with DIFFERENT content.
        // This is separate from resume logic - we only auto-rename if:
        // 1. A complete file (not .part) exists at the destination
        // 2. Its size matches the server's file size (so it's a complete file)
        // 3. Its hash differs from the server's hash (different content)
        //
        // If a .part file exists, that's a partial download - we'll resume it.
        // If a complete file exists with the SAME hash, we skip the download.
        let local_file_path = if let Ok(metadata) = tokio::fs::metadata(&local_file_path).await
            && metadata.is_file()
            && metadata.len() == file_size
            && file_size > 0
        {
            // Complete file exists - check if it's the same content
            if let Ok(existing_hash) = compute_file_sha256(&local_file_path).await {
                if existing_hash != file_sha256 {
                    // Different file with same size - auto-rename to avoid overwriting
                    generate_unique_path(&local_file_path).await
                } else {
                    // Same file - will be skipped by the "already complete" check below
                    local_file_path
                }
            } else {
                // Couldn't hash existing file - just use original path
                local_file_path
            }
        } else {
            local_file_path
        };
        let part_path = PathBuf::from(format!("{}{}", local_file_path.display(), PART_SUFFIX));

        // Check for existing partial/complete file for resume
        let (local_size, local_hash) = check_local_file(&local_file_path, &part_path).await;

        // Send FileStartResponse
        let start_response = ClientMessage::FileStartResponse {
            size: local_size,
            sha256: local_hash.clone(),
        };
        send_client_message(&mut writer, &start_response)
            .await
            .map_err(|e| {
                eprintln!("Failed to send FileStartResponse: {e}");
                TransferError::ConnectionError
            })?;

        // If file is already complete (sizes and hashes match), server skips FileData
        if local_size == file_size && local_size > 0 {
            // File already complete, no FileData expected
            transferred_bytes += file_size;
            files_completed += 1;

            let _ = event_tx.send(TransferEvent::FileCompleted {
                id,
                path: file_path.clone(),
            });

            let _ = event_tx.send(TransferEvent::Progress {
                id,
                transferred_bytes,
                files_completed,
                current_file: None,
            });

            continue;
        }

        // Receive FileData and write to .part file
        if file_size > 0 {
            // Create parent directories if needed
            if let Some(parent) = local_file_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    eprintln!("Failed to create directory {}: {e}", parent.display());
                    TransferError::IoError
                })?;
            }

            // Open/create .part file for writing (append if resuming)
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(local_size == 0) // Truncate only if starting fresh
                .open(&part_path)
                .await
                .map_err(|e| {
                    eprintln!("Failed to open file {}: {e}", part_path.display());
                    TransferError::IoError
                })?;

            // Seek to end if resuming
            if local_size > 0 {
                file.seek(SeekFrom::End(0)).await.map_err(|e| {
                    eprintln!("Failed to seek in file: {e}");
                    TransferError::IoError
                })?;
            }

            // Calculate bytes to receive
            let bytes_to_receive = file_size - local_size;

            // Read the FileData frame header first (without loading payload into memory)
            let header = match timeout(IDLE_TIMEOUT, reader.read_frame_header()).await {
                Ok(Ok(Some(h))) => h,
                Ok(Ok(None)) => {
                    eprintln!("Connection closed while waiting for FileData");
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: t("transfer-error-connection"),
                        error_kind: Some(TransferError::ConnectionError),
                    });
                    return Err(TransferError::ConnectionError);
                }
                Ok(Err(e)) => {
                    eprintln!("Error reading FileData header: {e}");
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: t("transfer-error-protocol"),
                        error_kind: Some(TransferError::ProtocolError),
                    });
                    return Err(TransferError::ProtocolError);
                }
                Err(_) => {
                    eprintln!("Timeout waiting for FileData");
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: t("transfer-error-connection"),
                        error_kind: Some(TransferError::ConnectionError),
                    });
                    return Err(TransferError::ConnectionError);
                }
            };

            if header.message_type != "FileData" {
                eprintln!("Expected FileData, got: {}", header.message_type);
                let _ = event_tx.send(TransferEvent::Failed {
                    id,
                    error: t("transfer-error-protocol"),
                    error_kind: Some(TransferError::ProtocolError),
                });
                return Err(TransferError::ProtocolError);
            }

            if header.payload_length != bytes_to_receive {
                eprintln!(
                    "FileData size mismatch: expected {}, got {}",
                    bytes_to_receive, header.payload_length
                );
                let _ = event_tx.send(TransferEvent::Failed {
                    id,
                    error: t("transfer-error-protocol"),
                    error_kind: Some(TransferError::ProtocolError),
                });
                return Err(TransferError::ProtocolError);
            }

            // Stream FileData payload directly to file with progress-based timeout
            // and cancellation support
            let stream_result = stream_payload_to_file_with_progress(
                &mut reader,
                &header,
                &mut file,
                PROGRESS_TIMEOUT,
                &cancel_flag,
                |bytes_written| {
                    // Send progress update
                    let _ = event_tx.send(TransferEvent::Progress {
                        id,
                        transferred_bytes: transferred_bytes + bytes_written,
                        files_completed,
                        current_file: Some(file_path.clone()),
                    });
                },
            )
            .await;

            match stream_result {
                Ok(bytes_written) => {
                    transferred_bytes += bytes_written;
                }
                Err(StreamError::Cancelled) => {
                    file.flush().await.ok();
                    let _ = event_tx.send(TransferEvent::Paused { id });
                    return Ok(());
                }
                Err(StreamError::Frame(FrameError::FrameTimeout)) => {
                    eprintln!("Timeout during file transfer (no progress for 60s)");
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: t("transfer-error-connection"),
                        error_kind: Some(TransferError::ConnectionError),
                    });
                    return Err(TransferError::ConnectionError);
                }
                Err(StreamError::Frame(e)) => {
                    eprintln!("Error streaming file data: {e}");
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: t("transfer-error-io"),
                        error_kind: Some(TransferError::IoError),
                    });
                    return Err(TransferError::IoError);
                }
                Err(StreamError::Io(e)) => {
                    eprintln!("IO error writing file: {e}");
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: t("transfer-error-io"),
                        error_kind: Some(TransferError::IoError),
                    });
                    return Err(TransferError::IoError);
                }
            }

            // Flush and close file
            file.flush().await.map_err(|e| {
                eprintln!("Failed to flush file: {e}");
                TransferError::IoError
            })?;
            drop(file);

            // Verify SHA-256 hash
            let computed_hash = compute_file_sha256(&part_path).await?;
            if computed_hash != file_sha256 {
                // Delete the corrupt .part file
                let _ = tokio::fs::remove_file(&part_path).await;
                let _ = event_tx.send(TransferEvent::Failed {
                    id,
                    error: t("transfer-error-hash-mismatch"),
                    error_kind: Some(TransferError::HashMismatch),
                });
                return Err(TransferError::HashMismatch);
            }

            // Rename .part to final filename
            tokio::fs::rename(&part_path, &local_file_path)
                .await
                .map_err(|e| {
                    eprintln!(
                        "Failed to rename {} to {}: {e}",
                        part_path.display(),
                        local_file_path.display()
                    );
                    TransferError::IoError
                })?;
        } else {
            // 0-byte file - just create it
            if let Some(parent) = local_file_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    eprintln!("Failed to create directory {}: {e}", parent.display());
                    TransferError::IoError
                })?;
            }
            File::create(&local_file_path).await.map_err(|e| {
                eprintln!(
                    "Failed to create empty file {}: {e}",
                    local_file_path.display()
                );
                TransferError::IoError
            })?;
        }

        files_completed += 1;

        let _ = event_tx.send(TransferEvent::FileCompleted {
            id,
            path: file_path,
        });

        let _ = event_tx.send(TransferEvent::Progress {
            id,
            transferred_bytes,
            files_completed,
            current_file: None,
        });
    }

    // Read TransferComplete
    let complete = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;
    handle_transfer_complete(complete, id, &event_tx)
}

// =============================================================================
// Connection Helpers
// =============================================================================

/// Connect to transfer port, verify certificate, and authenticate
///
/// Returns boxed trait objects for the reader/writer to support both direct
/// and proxied connections with different underlying stream types.
async fn connect_and_authenticate(
    conn_info: &TransferConnectionInfo,
    expected_fingerprint: &str,
    proxy: Option<ProxyConfig>,
) -> Result<
    (
        FrameReader<BufReader<Box<dyn AsyncRead + Unpin + Send>>>,
        FrameWriter<Box<dyn AsyncWrite + Unpin + Send>>,
        String,
    ),
    TransferError,
> {
    let target_addr = &conn_info.server_address;
    let target_port = conn_info.transfer_port;

    // Set up TLS config
    let tls_config = crate::network::tls::create_tls_config();
    let connector = TlsConnector::from(Arc::new(tls_config));

    // Use "localhost" for SNI since we disable hostname verification
    let server_name = "localhost"
        .try_into()
        .expect("localhost is valid server name");

    // Check if we should bypass proxy for this address (localhost, Yggdrasil)
    let use_proxy = proxy.filter(|_| !crate::network::tls::should_bypass_proxy(target_addr));

    // Connect and perform TLS handshake - either direct or through proxy
    let (fingerprint, read_half, write_half): (
        String,
        Box<dyn AsyncRead + Unpin + Send>,
        Box<dyn AsyncWrite + Unpin + Send>,
    ) = if let Some(proxy_config) = use_proxy {
        // Proxied connection via SOCKS5
        let proxy_addr = format!("{}:{}", proxy_config.address, proxy_config.port);

        let socks_stream = timeout(CONNECTION_TIMEOUT, async {
            match (&proxy_config.username, &proxy_config.password) {
                (Some(username), Some(password)) => {
                    Socks5Stream::connect_with_password(
                        proxy_addr.as_str(),
                        (target_addr.as_str(), target_port),
                        username.as_str(),
                        password.as_str(),
                    )
                    .await
                }
                _ => {
                    Socks5Stream::connect(proxy_addr.as_str(), (target_addr.as_str(), target_port))
                        .await
                }
            }
        })
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

        let tls_stream = timeout(
            CONNECTION_TIMEOUT,
            connector.connect(server_name, socks_stream),
        )
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

        // Get fingerprint before splitting
        let (_, session) = tls_stream.get_ref();
        let fp = crate::network::tls::get_certificate_fingerprint(session)
            .ok_or(TransferError::CertificateMismatch)?;

        let (r, w) = tokio::io::split(tls_stream);
        (fp, Box::new(r), Box::new(w))
    } else {
        // Direct connection
        let addr = format!("{}:{}", target_addr, target_port);

        let tcp_stream = timeout(CONNECTION_TIMEOUT, TcpStream::connect(&addr))
            .await
            .map_err(|_| TransferError::ConnectionError)?
            .map_err(|_| TransferError::ConnectionError)?;

        let tls_stream = timeout(
            CONNECTION_TIMEOUT,
            connector.connect(server_name, tcp_stream),
        )
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

        // Get fingerprint before splitting
        let (_, session) = tls_stream.get_ref();
        let fp = crate::network::tls::get_certificate_fingerprint(session)
            .ok_or(TransferError::CertificateMismatch)?;

        let (r, w) = tokio::io::split(tls_stream);
        (fp, Box::new(r), Box::new(w))
    };

    // Verify certificate fingerprint
    if fingerprint != expected_fingerprint {
        return Err(TransferError::CertificateMismatch);
    }

    // Set up framing
    let buf_reader = BufReader::new(read_half);
    let mut reader = FrameReader::new(buf_reader);
    let mut writer = FrameWriter::new(write_half);

    // Perform handshake
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(&mut writer, &handshake)
        .await
        .map_err(|_| TransferError::ConnectionError)?;

    let handshake_response = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;

    match handshake_response {
        ServerMessage::HandshakeResponse { success: true, .. } => {}
        ServerMessage::HandshakeResponse { success: false, .. } => {
            return Err(TransferError::UnsupportedVersion);
        }
        _ => {
            return Err(TransferError::ProtocolError);
        }
    }

    // Perform login
    let login = ClientMessage::Login {
        username: conn_info.username.clone(),
        password: conn_info.password.clone(),
        features: vec![],
        locale: String::new(),
        avatar: None,
        nickname: conn_info.nickname.clone(),
    };
    send_client_message(&mut writer, &login)
        .await
        .map_err(|_| TransferError::ConnectionError)?;

    let login_response = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;

    match login_response {
        ServerMessage::LoginResponse { success: true, .. } => {}
        ServerMessage::LoginResponse { success: false, .. } => {
            return Err(TransferError::AuthenticationFailed);
        }
        _ => {
            return Err(TransferError::ProtocolError);
        }
    }

    Ok((reader, writer, fingerprint))
}

// =============================================================================
// Message Reading Helpers
// =============================================================================

/// Read a server message with timeout
async fn read_message_with_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Duration,
) -> Result<ServerMessage, TransferError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let result = timeout(idle_timeout, read_server_message(reader)).await;

    match result {
        Ok(Ok(Some(received))) => Ok(received.message),
        Ok(Ok(None)) => Err(TransferError::ConnectionError),
        Ok(Err(_)) => Err(TransferError::ProtocolError),
        Err(_) => Err(TransferError::ConnectionError),
    }
}

/// Error type for streaming operations
enum StreamError {
    /// Transfer was cancelled by user
    Cancelled,
    /// Frame/protocol error
    Frame(FrameError),
    /// IO error writing to file
    Io(std::io::Error),
}

/// Stream FileData payload directly to a file with progress-based timeout and cancellation
///
/// This function streams the payload bytes directly to the file without loading
/// the entire payload into memory. The timeout resets each time bytes are received.
/// Cancellation is checked between each chunk read.
///
/// The progress callback is called periodically with the total bytes written so far.
async fn stream_payload_to_file_with_progress<R, F>(
    reader: &mut FrameReader<R>,
    header: &FrameHeader,
    file: &mut File,
    progress_timeout: Duration,
    cancel_flag: &Option<Arc<AtomicBool>>,
    mut on_progress: F,
) -> Result<u64, StreamError>
where
    R: tokio::io::AsyncBufRead + Unpin,
    F: FnMut(u64),
{
    let mut remaining = header.payload_length;
    let mut total_written: u64 = 0;
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut last_progress_update = 0u64;

    while remaining > 0 {
        // Check for cancellation before each read
        if is_cancelled(cancel_flag) {
            return Err(StreamError::Cancelled);
        }

        let to_read = (remaining as usize).min(buffer.len());

        // Read with progress timeout - resets on each successful read
        let bytes_read = match timeout(
            progress_timeout,
            reader.get_mut().read(&mut buffer[..to_read]),
        )
        .await
        {
            Ok(Ok(0)) => return Err(StreamError::Frame(FrameError::ConnectionClosed)),
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(StreamError::Frame(FrameError::Io(e.to_string()))),
            Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
        };

        // Write to file
        file.write_all(&buffer[..bytes_read])
            .await
            .map_err(StreamError::Io)?;

        remaining -= bytes_read as u64;
        total_written += bytes_read as u64;

        // Send progress updates every 64KB or so
        if total_written - last_progress_update >= BUFFER_SIZE as u64 {
            on_progress(total_written);
            last_progress_update = total_written;
        }
    }

    // Final progress update
    if total_written != last_progress_update {
        on_progress(total_written);
    }

    // Flush the file
    file.flush().await.map_err(StreamError::Io)?;

    // Read terminator byte
    let mut terminator = [0u8; 1];
    match timeout(
        progress_timeout,
        reader.get_mut().read_exact(&mut terminator),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(StreamError::Frame(FrameError::Io(e.to_string()))),
        Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
    }

    if terminator[0] != b'\n' {
        return Err(StreamError::Frame(FrameError::MissingTerminator));
    }

    Ok(total_written)
}

// =============================================================================
// File Helpers
// =============================================================================

/// Check for existing local file (complete or .part)
///
/// Returns (size, Option<sha256_hash>)
async fn check_local_file(complete_path: &PathBuf, part_path: &PathBuf) -> (u64, Option<String>) {
    // First check for complete file
    if let Ok(metadata) = tokio::fs::metadata(complete_path).await
        && metadata.is_file()
    {
        let size = metadata.len();
        if size > 0
            && let Ok(hash) = compute_file_sha256(complete_path).await
        {
            return (size, Some(hash));
        }
        return (size, None);
    }

    // Check for .part file
    if let Ok(metadata) = tokio::fs::metadata(part_path).await
        && metadata.is_file()
    {
        let size = metadata.len();
        if size > 0
            && let Ok(hash) = compute_file_sha256(part_path).await
        {
            return (size, Some(hash));
        }
        return (size, None);
    }

    (0, None)
}

/// Generate a unique file path by appending (1), (2), etc.
///
/// Given "/path/to/file.txt", tries:
/// - /path/to/file (1).txt
/// - /path/to/file (2).txt
/// - etc.
async fn generate_unique_path(original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let extension = original.extension().and_then(|s| s.to_str());
    let parent = original.parent();

    for i in 1..1000 {
        let new_name = if let Some(ext) = extension {
            format!("{} ({}).{}", stem, i, ext)
        } else {
            format!("{} ({})", stem, i)
        };

        let new_path = if let Some(parent) = parent {
            parent.join(&new_name)
        } else {
            PathBuf::from(&new_name)
        };

        // Check if this path is available (no file and no .part file)
        if tokio::fs::metadata(&new_path).await.is_err()
            && tokio::fs::metadata(format!("{}{}", new_path.display(), PART_SUFFIX))
                .await
                .is_err()
        {
            return new_path;
        }
    }

    // Fallback: just use the original (will overwrite)
    original.to_path_buf()
}

/// Compute SHA-256 hash of a file
async fn compute_file_sha256(path: &Path) -> Result<String, TransferError> {
    let file = File::open(path).await.map_err(|_| TransferError::IoError)?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .await
            .map_err(|_| TransferError::IoError)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Check if the transfer has been cancelled
fn is_cancelled(cancel_flag: &Option<Arc<AtomicBool>>) -> bool {
    cancel_flag
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
}

/// Validate that a path from the server is safe
///
/// Rejects absolute paths, paths with "..", and other dangerous patterns
fn is_safe_path(path: &str) -> bool {
    // Reject empty paths
    if path.is_empty() {
        return false;
    }

    // Reject absolute paths (Unix or Windows style)
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }

    // Check for Windows drive letters (e.g., "C:")
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        return false;
    }

    // Check each component
    for component in path.split(['/', '\\']) {
        // Reject empty components (double slashes)
        if component.is_empty() {
            continue; // Allow trailing/leading slashes that result in empty components
        }

        // Reject parent directory references
        if component == ".." {
            return false;
        }

        // Reject current directory references anywhere (e.g., "./foo", "foo/./bar")
        // These serve no purpose and could be used to confuse path matching/logging
        if component == "." {
            return false;
        }
    }

    true
}

/// Handle TransferComplete message
fn handle_transfer_complete(
    message: ServerMessage,
    id: Uuid,
    event_tx: &mpsc::UnboundedSender<TransferEvent>,
) -> Result<(), TransferError> {
    match message {
        ServerMessage::TransferComplete { success: true, .. } => {
            let _ = event_tx.send(TransferEvent::Completed { id });
            Ok(())
        }
        ServerMessage::TransferComplete {
            success: false,
            error_kind,
            ..
        } => {
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            let error = match err_kind {
                TransferError::NotFound => t("transfer-error-not-found"),
                TransferError::Permission => t("transfer-error-permission"),
                TransferError::Invalid => t("transfer-error-invalid"),
                TransferError::IoError => t("transfer-error-io"),
                TransferError::DiskFull => t("transfer-error-disk-full"),
                _ => t("transfer-error-unknown"),
            };
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error,
                error_kind: Some(err_kind.clone()),
            });
            Err(err_kind)
        }
        other => {
            eprintln!("Expected TransferComplete, got: {other:?}");
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: t("transfer-error-protocol"),
                error_kind: Some(TransferError::ProtocolError),
            });
            Err(TransferError::ProtocolError)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_path_valid() {
        assert!(is_safe_path("file.txt"));
        assert!(is_safe_path("dir/file.txt"));
        assert!(is_safe_path("dir/subdir/file.txt"));
        assert!(is_safe_path("Games/app.zip"));
        assert!(is_safe_path("Documents/report.pdf"));
    }

    #[test]
    fn test_is_safe_path_rejects_absolute() {
        assert!(!is_safe_path("/etc/passwd"));
        assert!(!is_safe_path("/home/user/file.txt"));
        assert!(!is_safe_path("\\Windows\\System32"));
        assert!(!is_safe_path("C:\\Windows\\System32"));
        assert!(!is_safe_path("D:file.txt"));
    }

    #[test]
    fn test_is_safe_path_rejects_parent_refs() {
        assert!(!is_safe_path(".."));
        assert!(!is_safe_path("../file.txt"));
        assert!(!is_safe_path("dir/../file.txt"));
        assert!(!is_safe_path("dir/subdir/../../file.txt"));
        assert!(!is_safe_path("dir\\..\\file.txt"));
    }

    #[test]
    fn test_is_safe_path_rejects_empty() {
        assert!(!is_safe_path(""));
    }

    #[test]
    fn test_is_safe_path_rejects_dot_components() {
        // Reject "." anywhere in path - serves no purpose and could confuse logging
        assert!(!is_safe_path("./file.txt"));
        assert!(!is_safe_path(".\\file.txt"));
        assert!(!is_safe_path("foo/./bar"));
        assert!(!is_safe_path("dir/./subdir/file.txt"));
        assert!(!is_safe_path("."));
    }
}
