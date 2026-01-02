//! Transfer executor - processes file transfers in the background
//!
//! The executor watches for queued transfers and processes them one at a time.
//! It connects to the transfer port (7501), performs authentication, and
//! executes the download protocol.

use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use nexus_common::PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{read_server_message, send_client_message};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use super::{Transfer, TransferConnectionInfo, TransferError};

// =============================================================================
// Constants
// =============================================================================

/// Connection timeout for TLS handshake
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Idle timeout waiting for a frame to start
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Frame completion timeout (once first byte received)
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

    /// File completed
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
pub async fn execute_transfer(
    transfer: &Transfer,
    event_tx: mpsc::UnboundedSender<TransferEvent>,
) -> Result<(), TransferError> {
    let id = transfer.id;

    // Notify UI that we're connecting
    let _ = event_tx.send(TransferEvent::Connecting { id });

    // Connect and authenticate
    let (mut reader, mut writer, _fingerprint) = connect_and_authenticate(
        &transfer.connection,
        &transfer.connection.certificate_fingerprint,
    )
    .await?;

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
            error,
            error_kind,
            ..
        } => {
            let err_msg = error.unwrap_or_else(|| "Download failed".to_string());
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: err_msg,
                error_kind: Some(err_kind.clone()),
            });
            return Err(err_kind);
        }

        other => {
            eprintln!("Unexpected response to FileDownload: {other:?}");
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: "Unexpected server response".to_string(),
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
                    error: "Expected FileStart message".to_string(),
                    error_kind: Some(TransferError::ProtocolError),
                });
                return Err(TransferError::ProtocolError);
            }
        };

        // Validate path (security check)
        if !is_safe_path(&file_path) {
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: format!("Invalid file path from server: {file_path}"),
                error_kind: Some(TransferError::ProtocolError),
            });
            return Err(TransferError::ProtocolError);
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
        let part_path = PathBuf::from(format!("{}{}", local_file_path.display(), PART_SUFFIX));

        // Check for existing partial/complete file
        // TODO: If a complete file exists but has a different hash than the server's file,
        // we should auto-rename the new file (e.g., "foo.txt" -> "foo (1).txt") like
        // browsers do, rather than overwriting. For now, the file will be overwritten silently.
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
            let mut bytes_received: u64 = 0;

            // Receive FileData frames
            while bytes_received < bytes_to_receive {
                let frame = read_raw_frame_with_progress_timeout(&mut reader).await?;

                if frame.message_type != "FileData" {
                    // Check if it's an error
                    if frame.message_type == "TransferComplete" || frame.message_type == "Error" {
                        eprintln!("Received {} during file transfer", frame.message_type);
                        let _ = event_tx.send(TransferEvent::Failed {
                            id,
                            error: "Transfer interrupted".to_string(),
                            error_kind: Some(TransferError::ConnectionError),
                        });
                        return Err(TransferError::ConnectionError);
                    }

                    eprintln!("Expected FileData, got: {}", frame.message_type);
                    let _ = event_tx.send(TransferEvent::Failed {
                        id,
                        error: "Expected FileData message".to_string(),
                        error_kind: Some(TransferError::ProtocolError),
                    });
                    return Err(TransferError::ProtocolError);
                }

                // Write data to file
                file.write_all(&frame.payload).await.map_err(|e| {
                    eprintln!("Failed to write to file: {e}");
                    TransferError::IoError
                })?;

                bytes_received += frame.payload.len() as u64;
                transferred_bytes += frame.payload.len() as u64;

                // Send periodic progress updates (every 64KB or so)
                let _ = event_tx.send(TransferEvent::Progress {
                    id,
                    transferred_bytes,
                    files_completed,
                    current_file: Some(file_path.clone()),
                });
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
                    error: format!("Hash verification failed for {file_path}"),
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
async fn connect_and_authenticate(
    conn_info: &TransferConnectionInfo,
    expected_fingerprint: &str,
) -> Result<
    (
        FrameReader<BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>>,
        FrameWriter<tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
        String,
    ),
    TransferError,
> {
    // Connect with timeout
    let addr = format!("{}:{}", conn_info.server_address, conn_info.transfer_port);

    let tcp_stream = timeout(CONNECTION_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

    // Set up TLS
    let tls_config = crate::network::tls::create_tls_config();
    let connector = TlsConnector::from(Arc::new(tls_config));

    // Use server address as SNI
    let server_name = conn_info
        .server_address
        .clone()
        .try_into()
        .unwrap_or_else(|_| "localhost".try_into().expect("localhost is valid"));

    let tls_stream = timeout(
        CONNECTION_TIMEOUT,
        connector.connect(server_name, tcp_stream),
    )
    .await
    .map_err(|_| TransferError::ConnectionError)?
    .map_err(|_| TransferError::ConnectionError)?;

    // Verify certificate fingerprint
    let (_, session) = tls_stream.get_ref();
    let fingerprint = crate::network::tls::get_certificate_fingerprint(session)
        .ok_or(TransferError::CertificateMismatch)?;

    if fingerprint != expected_fingerprint {
        return Err(TransferError::CertificateMismatch);
    }

    // Split stream and set up framing
    let (read_half, write_half) = tokio::io::split(tls_stream);
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

/// Read a raw frame with progress-based timeout (for FileData)
///
/// Uses FrameReader::read_frame() which returns a RawFrame containing
/// the raw payload bytes (not parsed as JSON).
async fn read_raw_frame_with_progress_timeout<R>(
    reader: &mut FrameReader<R>,
) -> Result<nexus_common::framing::RawFrame, TransferError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    // For now, use simple timeout. A more sophisticated implementation would
    // track bytes received and reset timeout on each chunk.
    let result = timeout(PROGRESS_TIMEOUT, reader.read_frame()).await;

    match result {
        Ok(Ok(Some(frame))) => Ok(frame),
        Ok(Ok(None)) => Err(TransferError::ConnectionError),
        Ok(Err(_)) => Err(TransferError::ProtocolError),
        Err(_) => Err(TransferError::ConnectionError),
    }
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

/// Compute SHA-256 hash of a file
async fn compute_file_sha256(path: &PathBuf) -> Result<String, TransferError> {
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

        // Reject current directory references at start
        if component == "." && path.starts_with('.') {
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
            error,
            error_kind,
        } => {
            let err_msg = error.unwrap_or_else(|| "Transfer failed".to_string());
            let err_kind = error_kind
                .as_deref()
                .map(TransferError::from_server_error_kind)
                .unwrap_or(TransferError::Unknown);
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: err_msg,
                error_kind: Some(err_kind.clone()),
            });
            Err(err_kind)
        }
        other => {
            eprintln!("Expected TransferComplete, got: {other:?}");
            let _ = event_tx.send(TransferEvent::Failed {
                id,
                error: "Expected TransferComplete message".to_string(),
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
    fn test_is_safe_path_rejects_dot_start() {
        assert!(!is_safe_path("./file.txt"));
        assert!(!is_safe_path(".\\file.txt"));
    }
}
