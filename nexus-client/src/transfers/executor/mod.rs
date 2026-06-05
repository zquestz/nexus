//! Transfer executor - processes file transfers in the background
//!
//! The executor watches for queued transfers and processes them one at a time.
//! It connects to the transfer port (7501), performs authentication, and
//! executes the download or upload protocol.
//!
//! Supports cancellation via an atomic flag that is checked periodically
//! during the transfer.
//!
//! ## Upload Optimization
//!
//! For uploads, SHA-256 hashes are computed lazily (not during initial scan).
//! Additionally, we use a prefetch strategy: while uploading file N, we compute
//! the hash for file N+1 in the background. This hides hashing latency for
//! multi-file uploads.
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

use uuid::Uuid;

use nexus_common::framing::FrameError;
use nexus_common::hash::StreamingHasher;
use nexus_common::io::send_client_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::{FALLBACK_FILE_NAME, PART_SUFFIX};

use super::types::{Transfer, TransferDirection, TransferError};
use crate::i18n::t;
use crate::network::ProxyConfig;

use connection::connect_and_authenticate;
use file_utils::{
    generate_unique_path, hash_file_with_keepalives, is_cancelled, is_safe_path,
    open_file_for_upload, scan_local_files,
};
use streaming::{
    ServerFileFrame, StreamError, read_file_data_or_file_hash, read_message_with_timeout,
    stream_file_to_server, stream_payload_to_file_with_progress,
};

// =============================================================================
// Constants
// =============================================================================

/// Connection timeout for TLS handshake
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Idle timeout waiting for a frame to start
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Directory downloads may need to scan the tree before the server can report
/// total size and file count in FileDownloadResponse.
const DOWNLOAD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// Progress timeout for FileData (must receive some bytes within this time)
const PROGRESS_TIMEOUT: Duration = Duration::from_secs(60);

/// Buffer size for file I/O operations (streaming)
const BUFFER_SIZE: usize = 64 * 1024; // 64KB

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
    server_error: Option<String>,
) -> TransferError {
    let error = server_error.unwrap_or_else(|| t(error_kind.to_i18n_key()));
    let _ = event_tx.send(TransferEvent::Failed {
        id,
        error,
        error_kind: Some(error_kind.clone()),
    });
    error_kind
}

/// Helper to handle errors that might be cancellations
///
/// If the error is `TransferError::Cancelled`, sends a Paused event instead of Failed.
/// This allows hash computation cancellation to be treated as a pause, not a failure.
fn handle_possible_cancellation(
    event_tx: &mpsc::UnboundedSender<TransferEvent>,
    id: Uuid,
    error_kind: TransferError,
) -> TransferError {
    if error_kind == TransferError::Cancelled {
        let _ = event_tx.send(TransferEvent::Paused { id });
        error_kind
    } else {
        send_failed_event(event_tx, id, error_kind, None)
    }
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
                return Err(send_failed_event(&event_tx, id, e, None));
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
    let response = read_message_with_timeout(reader, DOWNLOAD_RESPONSE_TIMEOUT).await?;

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
            error,
            error_kind,
            ..
        } => {
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            return Err(send_failed_event(event_tx, id, err_kind, error));
        }

        _other => {
            return Err(send_failed_event(
                event_tx,
                id,
                TransferError::ProtocolError,
                None,
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
    let base_path = &transfer.local_path;

    for file_index in 0..file_count {
        // Check for cancellation before each file
        if is_cancelled(cancel_flag) {
            let _ = event_tx.send(TransferEvent::Paused { id });
            return Ok(());
        }

        // Read FileStart
        let file_start = read_message_with_timeout(reader, IDLE_TIMEOUT).await?;

        let (file_path, file_size) = match file_start {
            ServerMessage::FileStart { path, size } => (path, size),
            ServerMessage::TransferComplete { .. } => {
                // Early completion (error case)
                return handle_transfer_complete(file_start, id, event_tx);
            }
            _other => {
                return Err(send_failed_event(
                    event_tx,
                    id,
                    TransferError::ProtocolError,
                    None,
                ));
            }
        };

        // Validate path (security check)
        if !is_safe_path(&file_path) {
            return Err(send_failed_event(
                event_tx,
                id,
                TransferError::Invalid,
                None,
            ));
        }

        // Update current file in progress
        let _ = event_tx.send(TransferEvent::Progress {
            id,
            transferred_bytes,
            files_completed: file_index,
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

        // Check if a COMPLETE file exists at the destination with DIFFERENT size.
        // Without the server hash upfront, we can only auto-rename based on size difference.
        // If sizes match, we use the original path — the server will verify via FileHash.
        //
        // If a .part file exists, that's a partial download — we'll resume it.
        let local_file_path = if let Ok(metadata) = tokio::fs::metadata(&local_file_path).await
            && metadata.is_file()
        {
            let existing_size = metadata.len();
            if existing_size != file_size {
                // Different sizes — definitely different file, auto-rename
                match generate_unique_path(&local_file_path).await {
                    Ok(path) => path,
                    Err(_) => {
                        return Err(send_failed_event(
                            event_tx,
                            id,
                            TransferError::IoError,
                            None,
                        ));
                    }
                }
            } else {
                // Same size — will be verified by hash later
                local_file_path
            }
        } else {
            local_file_path
        };
        let part_path = PathBuf::from(format!("{}{}", local_file_path.display(), PART_SUFFIX));

        // Check for existing partial/complete file and create StreamingHasher
        // The hasher is pre-fed with existing file content for single-pass hashing.
        // Sends FileHashing keepalives to server while hashing large existing files.
        let (local_size, mut hasher) = if let Ok(metadata) =
            tokio::fs::metadata(&local_file_path).await
            && metadata.is_file()
            && metadata.len() > 0
        {
            let size = metadata.len();
            let file_name = local_file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(FALLBACK_FILE_NAME)
                .to_string();
            match hash_file_with_keepalives(&local_file_path, size, file_name, writer, cancel_flag)
                .await
            {
                Ok(h) => (size, h),
                Err(TransferError::Cancelled) => {
                    return Err(handle_possible_cancellation(
                        event_tx,
                        id,
                        TransferError::Cancelled,
                    ));
                }
                Err(_) => (0, StreamingHasher::new()),
            }
        } else if let Ok(metadata) = tokio::fs::metadata(&part_path).await
            && metadata.is_file()
            && metadata.len() > 0
        {
            let size = metadata.len();
            let file_name = part_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(FALLBACK_FILE_NAME)
                .to_string();
            match hash_file_with_keepalives(&part_path, size, file_name, writer, cancel_flag).await
            {
                Ok(h) => (size, h),
                Err(TransferError::Cancelled) => {
                    return Err(handle_possible_cancellation(
                        event_tx,
                        id,
                        TransferError::Cancelled,
                    ));
                }
                Err(_) => (0, StreamingHasher::new()),
            }
        } else {
            (0, StreamingHasher::new())
        };

        // Build hash for FileStartResponse (clone+finalize, hasher not consumed)
        let local_hash = if local_size > 0 {
            Some(hasher.partial_hash())
        } else {
            None
        };

        // Send FileStartResponse
        let start_response = ClientMessage::FileStartResponse {
            size: local_size,
            sha256: local_hash,
        };
        send_client_message(writer, &start_response)
            .await
            .map_err(|_| TransferError::ConnectionError)?;

        // Read next frame from server: FileData or FileHash
        // FileHashing keepalives are skipped automatically
        let server_frame = match read_file_data_or_file_hash(reader).await {
            Ok(f) => f,
            Err(e) => {
                return Err(send_failed_event(event_tx, id, e, None));
            }
        };

        match server_frame {
            ServerFileFrame::TransferComplete { error, error_kind } => {
                // Server terminated the transfer early (e.g., resume hash mismatch)
                let err_kind = error_kind
                    .as_deref()
                    .map(TransferError::from_server_error_kind)
                    .unwrap_or(TransferError::Unknown);
                let error_msg = error.unwrap_or_else(|| t(err_kind.to_i18n_key()));
                let _ = event_tx.send(TransferEvent::Failed {
                    id,
                    error: error_msg,
                    error_kind: Some(err_kind.clone()),
                });
                return Err(err_kind);
            }
            ServerFileFrame::FileHash {
                sha256: server_hash,
            } => {
                // Server says: file is already complete or zero-byte — no FileData
                if file_size == 0 {
                    // Zero-byte file — create it
                    if let Some(parent) = local_file_path.parent() {
                        tokio::fs::create_dir_all(parent)
                            .await
                            .map_err(|_| TransferError::IoError)?;
                    }
                    File::create(&local_file_path)
                        .await
                        .map_err(|_| TransferError::IoError)?;
                }

                // Independent verification: compare our hash against server's FileHash
                // For zero-byte: hasher is fresh, finalize = empty hash
                // For already-complete: hasher consumed full file during resume verification
                let computed_hash = hasher.finalize();
                if computed_hash != server_hash {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::HashMismatch,
                        None,
                    ));
                }

                // Account for the full file size in progress
                transferred_bytes += file_size;
            }
            ServerFileFrame::FileData(header) => {
                // Server is sending file data — stream to .part file

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

                if header.payload_length != bytes_to_receive {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::ProtocolError,
                        None,
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
                    Some(&mut hasher),
                    |bytes_written| {
                        // Send progress update
                        let _ = event_tx.send(TransferEvent::Progress {
                            id,
                            transferred_bytes: transferred_bytes + bytes_written,
                            files_completed: file_index,
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
                            None,
                        ));
                    }
                    Err(StreamError::Frame(_)) => {
                        return Err(send_failed_event(
                            event_tx,
                            id,
                            TransferError::IoError,
                            None,
                        ));
                    }
                    Err(StreamError::Io) => {
                        return Err(send_failed_event(
                            event_tx,
                            id,
                            TransferError::IoError,
                            None,
                        ));
                    }
                }

                // Flush and close file
                file.flush().await.map_err(|_| TransferError::IoError)?;
                drop(file);

                // Read FileHash from server (sent after FileData)
                let hash_frame = match read_file_data_or_file_hash(reader).await {
                    Ok(f) => f,
                    Err(e) => {
                        return Err(send_failed_event(event_tx, id, e, None));
                    }
                };
                let server_hash = match hash_frame {
                    ServerFileFrame::FileHash { sha256 } => sha256,
                    _ => {
                        return Err(send_failed_event(
                            event_tx,
                            id,
                            TransferError::ProtocolError,
                            None,
                        ));
                    }
                };

                // Verify: client-computed hash (StreamingHasher) must match server's FileHash
                // Hasher consumed existing .part bytes (0..local_size) + received bytes
                let computed_hash = hasher.finalize();
                if computed_hash != server_hash {
                    // Delete the corrupt .part file
                    let _ = tokio::fs::remove_file(&part_path).await;
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::HashMismatch,
                        None,
                    ));
                }

                // Rename .part to final filename
                tokio::fs::rename(&part_path, &local_file_path)
                    .await
                    .map_err(|_| TransferError::IoError)?;
            }
        }

        let _ = event_tx.send(TransferEvent::Progress {
            id,
            transferred_bytes,
            files_completed: file_index + 1,
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
/// 1. Scan local files (no SHA-256 yet - computed lazily per file)
/// 2. Send FileUpload request
/// 3. Send files to server with resume support
///
/// For large files, sends FileHashing keepalive messages periodically during
/// hash computation to prevent server timeouts.
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

    // Scan local files (just paths and sizes - SHA-256 computed lazily per file)
    let files = scan_local_files(&transfer.local_path, transfer.is_directory).await?;

    if files.is_empty() {
        // Nothing to upload - this is an error (server rejects empty uploads)
        return Err(send_failed_event(
            event_tx,
            id,
            TransferError::Invalid,
            None,
        ));
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
            error,
            error_kind,
            ..
        } => {
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            return Err(send_failed_event(event_tx, id, err_kind, error));
        }

        _other => {
            return Err(send_failed_event(
                event_tx,
                id,
                TransferError::ProtocolError,
                None,
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

        // Send FileStart — no sha256, hash will be sent in FileHash after data
        let file_start = ClientMessage::FileStart {
            path: file_info.relative_path.clone(),
            size: file_info.size,
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
            _ => {
                return Err(send_failed_event(
                    event_tx,
                    id,
                    TransferError::ProtocolError,
                    None,
                ));
            }
        };

        // Determine upload offset and create StreamingHasher
        // The hasher is used for single-pass hashing: resume verification via
        // clone+finalize, then continued during streaming, then finalized for FileHash.
        let (offset, mut hasher) = if server_size == 0 {
            // Server has no file — upload from beginning
            (0, StreamingHasher::new())
        } else if server_size == file_info.size {
            // Server has same size file — check if already complete
            if let Some(ref server_hash_val) = server_hash {
                // Hash our full file to compare (on-demand, not upfront)
                let h = match hash_file_with_keepalives(
                    &file_info.absolute_path,
                    file_info.size,
                    file_info.relative_path.clone(),
                    writer,
                    cancel_flag,
                )
                .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        return Err(handle_possible_cancellation(event_tx, id, e));
                    }
                };
                let our_hash = h.partial_hash();
                if &our_hash == server_hash_val {
                    // File already complete on server — send FileHash only, skip FileData
                    let file_hash = ClientMessage::FileHash {
                        sha256: h.finalize(),
                    };
                    send_client_message(writer, &file_hash)
                        .await
                        .map_err(|_| TransferError::ConnectionError)?;

                    transferred_bytes += file_info.size;
                    files_completed += 1;

                    let _ = event_tx.send(TransferEvent::Progress {
                        id,
                        transferred_bytes,
                        files_completed,
                        current_file: None,
                    });

                    continue;
                } else {
                    // Hash mismatch — different file with same size, start over
                    (0, StreamingHasher::new())
                }
            } else {
                // No server hash — start over
                (0, StreamingHasher::new())
            }
        } else if server_size < file_info.size {
            // Server has partial file — verify hash and resume
            if let Some(ref server_partial_hash) = server_hash {
                // Hash 0..server_size of our file using StreamingHasher
                let h = match hash_file_with_keepalives(
                    &file_info.absolute_path,
                    server_size,
                    file_info.relative_path.clone(),
                    writer,
                    cancel_flag,
                )
                .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        return Err(handle_possible_cancellation(event_tx, id, e));
                    }
                };
                let partial = h.partial_hash();
                if &partial == server_partial_hash {
                    // Hashes match — resume from server_size (hasher retains 0..server_size)
                    (server_size, h)
                } else {
                    // Hash mismatch — upload from beginning
                    (0, StreamingHasher::new())
                }
            } else {
                // No server hash — upload from beginning
                (0, StreamingHasher::new())
            }
        } else {
            // Server file is larger than ours — something is wrong, start over
            (0, StreamingHasher::new())
        };

        // Upload file data if needed
        let bytes_to_send = file_info.size - offset;

        if bytes_to_send > 0 {
            // Open file at the correct offset
            let mut file = open_file_for_upload(&file_info.absolute_path, offset).await?;

            // Account for already-uploaded bytes in progress
            if offset > 0 {
                transferred_bytes += offset;
            }

            // Stream file data to server with integrated hasher
            let stream_result = stream_file_to_server(
                writer,
                &mut file,
                bytes_to_send,
                PROGRESS_TIMEOUT,
                cancel_flag,
                Some(&mut hasher),
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
                        None,
                    ));
                }
                Err(StreamError::Frame(_)) => {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::IoError,
                        None,
                    ));
                }
                Err(StreamError::Io) => {
                    return Err(send_failed_event(
                        event_tx,
                        id,
                        TransferError::IoError,
                        None,
                    ));
                }
            }
        }

        // Send FileHash with full file hash
        // Hasher consumed 0..offset (from resume verification) + offset..end (from streaming)
        // For zero-byte files: hasher is fresh, finalize = hash of empty content
        let file_hash = ClientMessage::FileHash {
            sha256: hasher.finalize(),
        };
        send_client_message(writer, &file_hash)
            .await
            .map_err(|_| TransferError::ConnectionError)?;

        files_completed += 1;

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
            error,
            error_kind,
            ..
        } => {
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            Err(send_failed_event(event_tx, id, err_kind, error))
        }
        _ => Err(send_failed_event(
            event_tx,
            id,
            TransferError::ProtocolError,
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ConnectionInfo;
    use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
    use nexus_common::hash::StreamingHasher;
    use nexus_common::io::{read_client_message, send_server_message};
    use std::sync::atomic::AtomicBool;
    use tokio::io::{BufReader, duplex};

    fn test_connection_info() -> ConnectionInfo {
        ConnectionInfo {
            server_name: "Test Server".to_string(),
            address: "127.0.0.1".to_string(),
            port: 7500,
            transfer_port: 7501,
            certificate_fingerprint: "AA:BB:CC".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            nickname: String::new(),
        }
    }

    fn hash_bytes(bytes: &[u8]) -> String {
        let mut hasher = StreamingHasher::new();
        hasher.update(bytes);
        hasher.finalize()
    }

    async fn send_file_data<W>(writer: &mut FrameWriter<W>, payload: &[u8]) -> std::io::Result<()>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        let mut reader = payload;
        writer
            .write_streaming_frame(
                MessageId::new(),
                "FileData",
                &mut reader,
                payload.len() as u64,
            )
            .await
            .map_err(Into::into)
    }

    #[tokio::test]
    async fn test_execute_download_directory_sends_file_start_response() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let local_base = temp_dir.path().join("bundle");
        let transfer = Transfer::new_download(
            test_connection_info(),
            "bundle".to_string(),
            false,
            true,
            local_base.clone(),
            None,
            0,
        );

        let (client_io, server_io) = duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (server_read, server_write) = tokio::io::split(server_io);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let mut server_reader = FrameReader::new(BufReader::new(server_read));
        let mut server_writer = FrameWriter::new(server_write);
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let cancel_flag = Some(Arc::new(AtomicBool::new(false)));

        let client = async {
            execute_download(
                &transfer,
                &mut client_reader,
                &mut client_writer,
                &event_tx,
                &cancel_flag,
            )
            .await
        };

        let server = async {
            let request = read_client_message(&mut server_reader)
                .await
                .expect("read FileDownload")
                .expect("FileDownload message")
                .message;
            assert!(matches!(
                request,
                ClientMessage::FileDownload { path, .. } if path == "bundle"
            ));

            send_server_message(
                &mut server_writer,
                &ServerMessage::FileDownloadResponse {
                    success: true,
                    error: None,
                    error_kind: None,
                    size: Some(11),
                    file_count: Some(2),
                    transfer_id: Some("test-xfer".to_string()),
                },
            )
            .await
            .expect("send FileDownloadResponse");

            for (path, payload) in [
                ("first.txt", b"first".as_slice()),
                ("nested/second.txt", b"second".as_slice()),
            ] {
                send_server_message(
                    &mut server_writer,
                    &ServerMessage::FileStart {
                        path: path.to_string(),
                        size: payload.len() as u64,
                    },
                )
                .await
                .expect("send FileStart");

                let response = read_client_message(&mut server_reader)
                    .await
                    .expect("read FileStartResponse")
                    .expect("FileStartResponse message")
                    .message;
                assert!(matches!(
                    response,
                    ClientMessage::FileStartResponse {
                        size: 0,
                        sha256: None
                    }
                ));

                send_file_data(&mut server_writer, payload)
                    .await
                    .expect("send FileData");
                send_server_message(
                    &mut server_writer,
                    &ServerMessage::FileHash {
                        sha256: hash_bytes(payload),
                    },
                )
                .await
                .expect("send FileHash");
            }

            send_server_message(
                &mut server_writer,
                &ServerMessage::TransferComplete {
                    success: true,
                    error: None,
                    error_kind: None,
                },
            )
            .await
            .expect("send TransferComplete");
        };

        let (client_result, _) = tokio::join!(client, server);
        client_result.expect("download succeeds");

        let first = tokio::fs::read(local_base.join("first.txt"))
            .await
            .expect("read first file");
        let second = tokio::fs::read(local_base.join("nested/second.txt"))
            .await
            .expect("read second file");
        assert_eq!(first, b"first");
        assert_eq!(second, b"second");
        assert!(matches!(
            event_rx.recv().await,
            Some(TransferEvent::Started { file_count: 2, .. })
        ));
        assert!(matches!(
            event_rx.recv().await,
            Some(TransferEvent::Progress { .. })
        ));
    }

    #[tokio::test]
    async fn test_execute_download_directory_resume_reports_part_hash() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let local_base = temp_dir.path().join("bundle");
        tokio::fs::create_dir_all(&local_base)
            .await
            .expect("create local base");
        let partial = b"first";
        let full = b"first-second";
        let part_path = local_base.join("first.bin.part");
        tokio::fs::write(&part_path, partial)
            .await
            .expect("write partial file");

        let transfer = Transfer::new_download(
            test_connection_info(),
            "bundle".to_string(),
            false,
            true,
            local_base.clone(),
            None,
            0,
        );

        let (client_io, server_io) = duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (server_read, server_write) = tokio::io::split(server_io);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let mut server_reader = FrameReader::new(BufReader::new(server_read));
        let mut server_writer = FrameWriter::new(server_write);
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let cancel_flag = Some(Arc::new(AtomicBool::new(false)));

        let client = async {
            execute_download(
                &transfer,
                &mut client_reader,
                &mut client_writer,
                &event_tx,
                &cancel_flag,
            )
            .await
        };

        let server = async {
            let _request = read_client_message(&mut server_reader)
                .await
                .expect("read FileDownload")
                .expect("FileDownload message");

            send_server_message(
                &mut server_writer,
                &ServerMessage::FileDownloadResponse {
                    success: true,
                    error: None,
                    error_kind: None,
                    size: Some(full.len() as u64),
                    file_count: Some(1),
                    transfer_id: Some("test-xfer".to_string()),
                },
            )
            .await
            .expect("send FileDownloadResponse");

            send_server_message(
                &mut server_writer,
                &ServerMessage::FileStart {
                    path: "first.bin".to_string(),
                    size: full.len() as u64,
                },
            )
            .await
            .expect("send FileStart");

            let response = read_client_message(&mut server_reader)
                .await
                .expect("read FileStartResponse")
                .expect("FileStartResponse message")
                .message;
            assert!(matches!(
                response,
                ClientMessage::FileStartResponse {
                    size,
                    sha256: Some(hash),
                } if size == partial.len() as u64 && hash == hash_bytes(partial)
            ));

            send_file_data(&mut server_writer, &full[partial.len()..])
                .await
                .expect("send FileData");
            send_server_message(
                &mut server_writer,
                &ServerMessage::FileHash {
                    sha256: hash_bytes(full),
                },
            )
            .await
            .expect("send FileHash");
            send_server_message(
                &mut server_writer,
                &ServerMessage::TransferComplete {
                    success: true,
                    error: None,
                    error_kind: None,
                },
            )
            .await
            .expect("send TransferComplete");
        };

        let (client_result, _) = tokio::join!(client, server);
        client_result.expect("download succeeds");

        let final_file = tokio::fs::read(local_base.join("first.bin"))
            .await
            .expect("read final file");
        assert_eq!(final_file, full);
        assert!(!part_path.exists());
    }
}
