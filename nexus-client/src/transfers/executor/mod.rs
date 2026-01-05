//! Transfer executor - processes file transfers in the background
//!
//! The executor watches for queued transfers and processes them one at a time.
//! It connects to the transfer port (7501), performs authentication, and
//! executes the download or upload protocol.
//!
//! Supports cancellation via an atomic flag that is checked periodically
//! during the transfer.
//!
//! ## Module Structure
//!
//! - `connection` - TLS connection and authentication
//! - `streaming` - Message reading and file data streaming (both directions)
//! - `file_utils` - File operations, hashing, scanning, and path validation

mod connection;
mod file_utils;
mod streaming;

use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

use nexus_common::framing::FrameError;
use nexus_common::io::send_client_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};

use super::types::{Transfer, TransferDirection, TransferError};
use crate::i18n::t;
use crate::network::ProxyConfig;

use connection::connect_and_authenticate;
use file_utils::{
    check_local_file, compute_file_sha256, compute_partial_sha256, generate_unique_path,
    is_cancelled, is_safe_path, open_file_for_upload, scan_local_files,
};
use streaming::{
    StreamError, read_message_with_timeout, stream_file_to_server,
    stream_payload_to_file_with_progress,
};

// =============================================================================
// Constants
// =============================================================================

/// Connection timeout for TLS handshake
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Idle timeout waiting for a frame to start
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

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
// Error Helpers
// =============================================================================

/// Helper to send a Failed event and return an error
///
/// Reduces repetition of the error sending pattern throughout the executor.
fn send_failed_event(
    event_tx: &mpsc::UnboundedSender<TransferEvent>,
    id: Uuid,
    error_kind: TransferError,
) -> TransferError {
    let _ = event_tx.send(TransferEvent::Failed {
        id,
        error: t(error_kind.to_i18n_key()),
        error_kind: Some(error_kind.clone()),
    });
    error_kind
}

// =============================================================================
// Executor
// =============================================================================

/// Execute a single transfer
///
/// This function handles the complete lifecycle of a transfer (download or upload):
/// 1. Connect to transfer port with TLS
/// 2. Verify certificate fingerprint
/// 3. Perform handshake and login
/// 4. Dispatch to download or upload handler
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
    let (mut reader, mut writer) =
        match connect_and_authenticate(&transfer.connection_info, proxy).await {
            Ok(result) => result,
            Err(e) => {
                return Err(send_failed_event(&event_tx, id, e));
            }
        };

    // Dispatch based on transfer direction
    match transfer.direction {
        TransferDirection::Download => {
            execute_download(transfer, &mut reader, &mut writer, &event_tx, &cancel_flag).await
        }
        TransferDirection::Upload => {
            execute_upload(transfer, &mut reader, &mut writer, &event_tx, &cancel_flag).await
        }
    }
}

// =============================================================================
// Download Executor
// =============================================================================

/// Execute a download transfer
///
/// Handles:
/// 1. Send FileDownload request
/// 2. Receive files and write to disk
/// 3. Handle resume via .part files
async fn execute_download<R, W>(
    transfer: &Transfer,
    reader: &mut nexus_common::framing::FrameReader<tokio::io::BufReader<R>>,
    writer: &mut nexus_common::framing::FrameWriter<W>,
    event_tx: &mpsc::UnboundedSender<TransferEvent>,
    cancel_flag: &Option<Arc<AtomicBool>>,
) -> Result<(), TransferError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let id = transfer.id;

    // Send FileDownload request
    let download_request = ClientMessage::FileDownload {
        path: transfer.remote_path.clone(),
        root: transfer.remote_root,
    };
    send_client_message(writer, &download_request)
        .await
        .map_err(|_| TransferError::ConnectionError)?;

    // Read FileDownloadResponse
    let response = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;

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
            return Err(send_failed_event(event_tx, id, err_kind));
        }

        _other => {
            return Err(send_failed_event(
                event_tx,
                id,
                TransferError::ProtocolError,
            ));
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
    if is_cancelled(cancel_flag) {
        let _ = event_tx.send(TransferEvent::Paused { id });
        return Ok(());
    }

    // Process each file (loop doesn't run if file_count == 0)
    let mut transferred_bytes: u64 = 0;
    let mut files_completed: u64 = 0;
    let base_path = &transfer.local_path;

    for _file_index in 0..file_count {
        // Check for cancellation before each file
        if is_cancelled(cancel_flag) {
            let _ = event_tx.send(TransferEvent::Paused { id });
            return Ok(());
        }

        // Read FileStart
        let file_start = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;

        let (file_path, file_size, file_sha256) = match file_start {
            ServerMessage::FileStart { path, size, sha256 } => (path, size, sha256),
            ServerMessage::TransferComplete { .. } => {
                // Early completion (error case)
                return handle_transfer_complete(file_start, id, event_tx);
            }
            _other => {
                return Err(send_failed_event(
                    event_tx,
                    id,
                    TransferError::ProtocolError,
                ));
            }
        };

        // Validate path (security check)
        if !is_safe_path(&file_path) {
            return Err(send_failed_event(event_tx, id, TransferError::Invalid));
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
                    match generate_unique_path(&local_file_path).await {
                        Ok(path) => path,
                        Err(_) => {
                            return Err(send_failed_event(event_tx, id, TransferError::IoError));
                        }
                    }
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
        send_client_message(writer, &start_response)
            .await
            .map_err(|_| TransferError::ConnectionError)?;

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
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|_| TransferError::IoError)?;
            }

            // Open/create .part file for writing (append if resuming)
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(local_size == 0) // Truncate only if starting fresh
                .open(&part_path)
                .await
                .map_err(|_| TransferError::IoError)?;

            // Seek to end if resuming
            if local_size > 0 {
                file.seek(SeekFrom::End(0))
                    .await
                    .map_err(|_| TransferError::IoError)?;

                // Account for already-downloaded bytes in progress
                transferred_bytes += local_size;
            }

            // Calculate bytes to receive
            let bytes_to_receive = file_size - local_size;

            // Read the FileData frame header first (without loading payload into memory)
            let header = match timeout(IDLE_TIMEOUT, reader.read_frame_header()).await {
                Ok(Ok(Some(h))) => h,
                Ok(Ok(None)) => {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::ConnectionError,
                    ));
                }
                Ok(Err(_)) => {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::ProtocolError,
                    ));
                }
                Err(_) => {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::ConnectionError,
                    ));
                }
            };

            if header.message_type != "FileData" {
                return Err(send_failed_event(
                    event_tx,
                    id,
                    TransferError::ProtocolError,
                ));
            }

            if header.payload_length != bytes_to_receive {
                return Err(send_failed_event(
                    event_tx,
                    id,
                    TransferError::ProtocolError,
                ));
            }

            // Stream FileData payload directly to file with progress-based timeout
            // and cancellation support
            let stream_result = stream_payload_to_file_with_progress(
                reader,
                &header,
                &mut file,
                PROGRESS_TIMEOUT,
                cancel_flag,
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
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::ConnectionError,
                    ));
                }
                Err(StreamError::Frame(_)) => {
                    return Err(send_failed_event(event_tx, id, TransferError::IoError));
                }
                Err(StreamError::Io) => {
                    return Err(send_failed_event(event_tx, id, TransferError::IoError));
                }
            }

            // Flush and close file
            file.flush().await.map_err(|_| TransferError::IoError)?;
            drop(file);

            // Verify SHA-256 hash
            let computed_hash = compute_file_sha256(&part_path).await?;
            if computed_hash != file_sha256 {
                // Delete the corrupt .part file
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(send_failed_event(event_tx, id, TransferError::HashMismatch));
            }

            // Rename .part to final filename
            tokio::fs::rename(&part_path, &local_file_path)
                .await
                .map_err(|_| TransferError::IoError)?;
        } else {
            // 0-byte file - just create it
            if let Some(parent) = local_file_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|_| TransferError::IoError)?;
            }
            File::create(&local_file_path)
                .await
                .map_err(|_| TransferError::IoError)?;
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
    let complete = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;
    handle_transfer_complete(complete, id, event_tx)
}

// =============================================================================
// Upload Executor
// =============================================================================

/// Execute an upload transfer
///
/// Handles:
/// 1. Scan local files and compute SHA-256 hashes
/// 2. Send FileUpload request
/// 3. Send files to server with resume support
async fn execute_upload<R, W>(
    transfer: &Transfer,
    reader: &mut nexus_common::framing::FrameReader<tokio::io::BufReader<R>>,
    writer: &mut nexus_common::framing::FrameWriter<W>,
    event_tx: &mpsc::UnboundedSender<TransferEvent>,
    cancel_flag: &Option<Arc<AtomicBool>>,
) -> Result<(), TransferError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let id = transfer.id;

    // Scan local files and compute hashes
    // This must be done before sending FileUpload so we know file_count and total_size
    let files = scan_local_files(&transfer.local_path, transfer.is_directory).await?;

    if files.is_empty() {
        // Nothing to upload - this is an error (server rejects empty uploads)
        return Err(send_failed_event(event_tx, id, TransferError::Invalid));
    }

    let file_count = files.len() as u64;
    let total_size: u64 = files.iter().map(|f| f.size).sum();

    // Send FileUpload request
    let upload_request = ClientMessage::FileUpload {
        destination: transfer.remote_path.clone(),
        file_count,
        total_size,
        root: transfer.remote_root,
    };
    send_client_message(writer, &upload_request)
        .await
        .map_err(|_| TransferError::ConnectionError)?;

    // Read FileUploadResponse
    let response = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;

    let server_transfer_id = match response {
        ServerMessage::FileUploadResponse {
            success: true,
            transfer_id,
            ..
        } => transfer_id.unwrap_or_default(),

        ServerMessage::FileUploadResponse {
            success: false,
            error_kind,
            ..
        } => {
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            return Err(send_failed_event(event_tx, id, err_kind));
        }

        _other => {
            return Err(send_failed_event(
                event_tx,
                id,
                TransferError::ProtocolError,
            ));
        }
    };

    // Notify UI that transfer has started
    let _ = event_tx.send(TransferEvent::Started {
        id,
        total_bytes: total_size,
        file_count,
        server_transfer_id,
    });

    // Check for cancellation after connecting
    if is_cancelled(cancel_flag) {
        let _ = event_tx.send(TransferEvent::Paused { id });
        return Ok(());
    }

    // Upload each file
    let mut transferred_bytes: u64 = 0;
    let mut files_completed: u64 = 0;
    let base_path = &transfer.local_path;

    for file_info in &files {
        // Check for cancellation before each file
        if is_cancelled(cancel_flag) {
            let _ = event_tx.send(TransferEvent::Paused { id });
            return Ok(());
        }

        // Update current file in progress
        let _ = event_tx.send(TransferEvent::Progress {
            id,
            transferred_bytes,
            files_completed,
            current_file: Some(file_info.relative_path.clone()),
        });

        // Determine local file path
        let local_file_path = if transfer.is_directory {
            base_path.join(&file_info.relative_path)
        } else {
            base_path.clone()
        };

        // Send FileStart
        let file_start = ClientMessage::FileStart {
            path: file_info.relative_path.clone(),
            size: file_info.size,
            sha256: file_info.sha256.clone(),
        };
        send_client_message(writer, &file_start)
            .await
            .map_err(|_| TransferError::ConnectionError)?;

        // Read FileStartResponse
        let start_response = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;

        let (server_size, server_hash) = match start_response {
            ServerMessage::FileStartResponse { size, sha256 } => (size, sha256),
            ServerMessage::TransferComplete { .. } => {
                // Early completion (error case)
                return handle_transfer_complete(start_response, id, event_tx);
            }
            _other => {
                return Err(send_failed_event(
                    event_tx,
                    id,
                    TransferError::ProtocolError,
                ));
            }
        };

        // Determine upload offset based on server's response
        let offset = if server_size == 0 {
            // Server has no file - upload from beginning
            0
        } else if server_size == file_info.size && server_hash.as_ref() == Some(&file_info.sha256) {
            // File already complete on server - skip upload
            transferred_bytes += file_info.size;
            files_completed += 1;

            let _ = event_tx.send(TransferEvent::FileCompleted {
                id,
                path: file_info.relative_path.clone(),
            });

            let _ = event_tx.send(TransferEvent::Progress {
                id,
                transferred_bytes,
                files_completed,
                current_file: None,
            });

            continue;
        } else if server_size < file_info.size {
            // Server has partial file - verify hash and resume
            if let Some(ref server_partial_hash) = server_hash {
                let local_partial_hash =
                    compute_partial_sha256(&local_file_path, server_size).await?;
                if &local_partial_hash == server_partial_hash {
                    // Hashes match - resume from server_size
                    server_size
                } else {
                    // Hash mismatch - upload from beginning
                    0
                }
            } else {
                // No server hash - upload from beginning
                0
            }
        } else {
            // Server file is larger than ours - something is wrong, start over
            0
        };

        // Upload file data if needed
        let bytes_to_send = file_info.size - offset;

        if bytes_to_send > 0 {
            // Open file at the correct offset
            let mut file = open_file_for_upload(&local_file_path, offset).await?;

            // Account for already-uploaded bytes in progress
            if offset > 0 {
                transferred_bytes += offset;
            }

            // Stream file data to server
            let stream_result = stream_file_to_server(
                writer,
                &mut file,
                bytes_to_send,
                PROGRESS_TIMEOUT,
                cancel_flag,
                |bytes_written| {
                    // Send progress update
                    let _ = event_tx.send(TransferEvent::Progress {
                        id,
                        transferred_bytes: transferred_bytes + bytes_written,
                        files_completed,
                        current_file: Some(file_info.relative_path.clone()),
                    });
                },
            )
            .await;

            match stream_result {
                Ok(bytes_written) => {
                    transferred_bytes += bytes_written;
                }
                Err(StreamError::Cancelled) => {
                    let _ = event_tx.send(TransferEvent::Paused { id });
                    return Ok(());
                }
                Err(StreamError::Frame(FrameError::FrameTimeout)) => {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::ConnectionError,
                    ));
                }
                Err(StreamError::Frame(_)) => {
                    return Err(send_failed_event(event_tx, id, TransferError::IoError));
                }
                Err(StreamError::Io) => {
                    return Err(send_failed_event(event_tx, id, TransferError::IoError));
                }
            }
        }
        // 0-byte files: no FileData sent per protocol spec, just proceed to mark complete

        files_completed += 1;

        let _ = event_tx.send(TransferEvent::FileCompleted {
            id,
            path: file_info.relative_path.clone(),
        });

        let _ = event_tx.send(TransferEvent::Progress {
            id,
            transferred_bytes,
            files_completed,
            current_file: None,
        });
    }

    // Read TransferComplete
    let complete = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;
    handle_transfer_complete(complete, id, event_tx)
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
            Err(send_failed_event(event_tx, id, err_kind))
        }
        _ => Err(send_failed_event(
            event_tx,
            id,
            TransferError::ProtocolError,
        )),
    }
}
