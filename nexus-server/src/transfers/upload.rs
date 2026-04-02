//! File upload handling for transfers
//!
//! Contains functions for handling upload requests and receiving files
//! from clients with resume support and conflict detection.
//!
//! Uses StreamingHasher for single-pass hashing during file reception.
//! The server independently verifies uploaded data by maintaining its own
//! hasher fed with existing .part content + received FileData chunks.

use std::io;
use std::path::{Path, PathBuf};

use tracing::{debug, error, info, warn};

use crate::constants::*;

use nexus_common::framing::{
    DEFAULT_PROGRESS_TIMEOUT, FrameHeader, FrameReader, FrameWriter, MessageId,
};
use nexus_common::hash::StreamingHasher;
use nexus_common::io::{read_client_message_with_full_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::db::Permission;
use crate::files::path::{allows_upload, validate_and_build_candidate_path};
use crate::handlers::{
    err_upload_conflict, err_upload_connection_lost, err_upload_destination_not_allowed,
    err_upload_empty, err_upload_file_exists, err_upload_hash_mismatch, err_upload_path_invalid,
    err_upload_protocol_error, err_upload_write_failed,
};

use nexus_common::PART_SUFFIX;

use super::hashing::{
    FALLBACK_FILE_NAME, FALLBACK_PART_FILE_NAME, HashingWriter, hash_file_with_keepalives,
};
use super::helpers::{
    TransferError, build_validated_path, check_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_upload_transfer_error, validate_transfer_path,
};
use super::transfer::{StreamError, Transfer};
use super::types::{ReceiveFileParams, UploadParams};

// =============================================================================
// Main Handler
// =============================================================================

/// Handle a file upload request
pub(crate) async fn handle_upload<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    params: UploadParams,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let UploadParams {
        destination,
        file_count,
        total_size,
        root: use_root,
    } = params;

    // Extract values to avoid borrow checker issues
    let locale = transfer.locale().to_string();
    let peer_addr = transfer.peer_addr();
    let username = transfer.user().username.clone();

    // Reject empty uploads - must have at least one file
    if file_count == 0 {
        let err = TransferError::invalid(err_upload_empty(&locale));
        return send_upload_transfer_error(transfer.writer(), &err).await;
    }

    // Validate and resolve destination path
    let (area_root, resolved_destination) =
        match validate_and_resolve_upload_destination(transfer, &destination, use_root, &locale) {
            Ok(result) => result,
            Err(e) => return send_upload_transfer_error(transfer.writer(), &e).await,
        };

    // Generate transfer ID for logging
    let log_transfer_id = generate_transfer_id();

    debug!(
        id = %log_transfer_id,
        user = %username,
        ip = %peer_addr,
        files = file_count,
        bytes = total_size,
        path = %destination,
        "{}", LOG_UPLOAD_STARTING
    );

    // Send FileUploadResponse
    let response = ServerMessage::FileUploadResponse {
        success: true,
        error: None,
        error_kind: None,
        transfer_id: Some(log_transfer_id.clone()),
    };
    if let Err(e) = transfer.send(&response).await {
        error!(id = %log_transfer_id, user = %username, ip = %peer_addr, err = %e, "{}", LOG_UPLOAD_SEND_FAILED);
        return Ok(());
    }

    // Receive each file
    let mut transfer_success = true;
    let mut transfer_error: Option<String> = None;
    let mut transfer_error_kind: Option<String> = None;

    for file_index in 0..file_count {
        let params = ReceiveFileParams {
            area_root: &area_root,
            destination: &resolved_destination,
            locale: &locale,
            transfer_id: &log_transfer_id,
            file_index,
        };
        match receive_file(transfer, params).await {
            Ok(()) => {
                // bytes_transferred is updated inside receive_file
            }
            Err(ReceiveFileError::Banned) => {
                // Just close the socket - client gets ban reason on BBS connection
                info!(id = %log_transfer_id, user = %username, ip = %peer_addr, "{}", LOG_UPLOAD_BANNED);
                let _ = transfer.writer().get_mut().shutdown().await;
                return Ok(());
            }
            Err(ReceiveFileError::Transfer(e)) => {
                warn!(
                    id = %log_transfer_id,
                    user = %username,
                    ip = %peer_addr,
                    file_index = file_index,
                    err = %e.message,
                    "{}", LOG_UPLOAD_ERROR
                );
                transfer_success = false;
                transfer_error = Some(e.message);
                transfer_error_kind = Some(e.kind.to_string());
                break;
            }
        }
    }

    // Send TransferComplete
    let complete = ServerMessage::TransferComplete {
        success: transfer_success,
        error: transfer_error,
        error_kind: transfer_error_kind,
    };
    let _ = transfer.send(&complete).await; // Best effort - connection may be closing

    if transfer_success {
        info!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %destination, "{}", LOG_UPLOAD_COMPLETE);
    } else {
        warn!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %destination, "{}", LOG_UPLOAD_FAILED);
    }

    // Mark file index as dirty on successful upload so it gets rebuilt
    if transfer_success {
        transfer.file_index().mark_dirty();
    }

    // Close connection
    let _ = transfer.writer().get_mut().shutdown().await;

    Ok(())
}

// =============================================================================
// File Reception Errors
// =============================================================================

/// Error type for receive_file
enum ReceiveFileError {
    Transfer(TransferError),
    Banned,
}

impl From<TransferError> for ReceiveFileError {
    fn from(e: TransferError) -> Self {
        ReceiveFileError::Transfer(e)
    }
}

impl From<StreamError> for ReceiveFileError {
    fn from(e: StreamError) -> Self {
        match e {
            StreamError::Banned => ReceiveFileError::Banned,
            StreamError::Io(e) => {
                ReceiveFileError::Transfer(TransferError::io_error(e.to_string()))
            }
            StreamError::ConnectionClosed => {
                ReceiveFileError::Transfer(TransferError::io_error("Connection closed"))
            }
        }
    }
}

// =============================================================================
// File Reception
// =============================================================================

/// Receive a single file from the client
///
/// Returns `Ok(())` on success, or `Err(ReceiveFileError)` on failure.
async fn receive_file<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    params: ReceiveFileParams<'_>,
) -> Result<(), ReceiveFileError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let ReceiveFileParams {
        area_root,
        destination,
        locale,
        transfer_id,
        file_index,
    } = params;

    // Read FileStart from client (no sha256 — hash arrives later in FileHash)
    let (relative_path, file_size) = read_client_file_start(transfer.reader(), locale).await?;

    debug!(
        id = %transfer_id,
        file_index = file_index,
        path = %relative_path,
        bytes = file_size,
        "{}", LOG_UPLOAD_RECEIVING
    );

    // Validate the relative path and build target paths
    let (target_path, part_path) =
        validate_and_build_upload_paths(&relative_path, destination, area_root, locale)?;

    // Check for conflicts and get existing file state with StreamingHasher
    // The hasher is pre-fed with existing .part content for single-pass hashing.
    // Sends FileHashing keepalives to client while hashing large existing files.
    let (existing_size, existing_hash, mut hasher, complete_file_exists) =
        check_upload_conflicts_and_get_state(
            transfer.writer(),
            &target_path,
            &part_path,
            file_size,
            locale,
        )
        .await?;

    // Send FileStartResponse with our current state
    send_file_start_response(transfer.writer(), existing_size, existing_hash, locale).await?;

    // Read next frame from client: FileData (data transfer) or FileHash (skip)
    // FileHashing keepalives are skipped automatically
    let client_frame = read_file_data_or_file_hash(transfer.reader(), locale).await?;

    match client_frame {
        ClientFileFrame::FileHash {
            sha256: client_hash,
        } => {
            // Client says: zero-byte or already complete — no FileData
            if file_size == 0 {
                // Zero-byte file
                create_empty_file(&target_path, locale).await?;
                debug!(id = %transfer_id, path = %relative_path, "{}", LOG_UPLOAD_EMPTY_FILE);
            } else {
                // Already complete — verify hashes match
                let server_hash = hasher.finalize();
                if server_hash != client_hash {
                    return Err(ReceiveFileError::Transfer(TransferError::hash_mismatch(
                        err_upload_hash_mismatch(locale),
                    )));
                }
                // No-op when complete file already exists at target_path (no .part to rename).
                // Handles the edge case where a .part file was left behind alongside the complete file.
                finalize_part_file_if_exists(&part_path, &target_path, locale).await?;
                debug!(id = %transfer_id, path = %relative_path, "{}", LOG_UPLOAD_ALREADY_COMPLETE);
            }
        }
        ClientFileFrame::FileData(header) => {
            // Client is sending file data

            // If a complete file already exists, reject — client should have sent FileHash
            if complete_file_exists {
                return Err(ReceiveFileError::Transfer(TransferError::exists(
                    err_upload_file_exists(locale),
                )));
            }

            // Calculate offset from FileData payload size
            let incoming_bytes = header.payload_length;
            if incoming_bytes > file_size {
                return Err(ReceiveFileError::Transfer(TransferError::protocol_error(
                    err_upload_protocol_error(locale),
                )));
            }
            let offset = file_size - incoming_bytes;

            // Check for concurrent upload conflict
            check_resume_conflict(offset, existing_size, locale)?;

            if offset > 0 {
                debug!(
                    id = %transfer_id,
                    path = %relative_path,
                    offset = offset,
                    percent = (offset * 100) / file_size,
                    "{}", LOG_UPLOAD_RESUMING
                );
            }

            // Stream file data to .part file with ban checking and hasher
            let bytes_written = stream_to_part_file(
                transfer,
                &header,
                &target_path,
                &part_path,
                offset,
                &mut hasher,
                locale,
            )
            .await?;

            debug!(
                id = %transfer_id,
                bytes = bytes_written,
                path = %relative_path,
                "{}", LOG_UPLOAD_RECEIVED
            );

            // Read FileHash from client (sent after FileData)
            let client_frame = read_file_data_or_file_hash(transfer.reader(), locale).await?;
            let client_hash = match client_frame {
                ClientFileFrame::FileHash { sha256 } => sha256,
                _ => {
                    return Err(ReceiveFileError::Transfer(TransferError::protocol_error(
                        err_upload_protocol_error(locale),
                    )));
                }
            };

            // Verify: server-computed hash must match client's FileHash
            let server_hash = hasher.finalize();
            if server_hash != client_hash {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(ReceiveFileError::Transfer(TransferError::hash_mismatch(
                    err_upload_hash_mismatch(locale),
                )));
            }

            // Rename .part to final destination
            tokio::fs::rename(&part_path, &target_path)
                .await
                .map_err(|_| {
                    ReceiveFileError::Transfer(TransferError::io_error(err_upload_write_failed(
                        locale,
                    )))
                })?;

            debug!(
                id = %transfer_id,
                path = %relative_path,
                bytes = file_size,
                "{}", LOG_UPLOAD_HASH_VERIFIED
            );
        }
    }

    Ok(())
}

// =============================================================================
// Validation Helpers
// =============================================================================

/// Validate and resolve upload destination path
fn validate_and_resolve_upload_destination<R, W>(
    transfer: &Transfer<'_, R, W>,
    destination: &str,
    use_root: bool,
    locale: &str,
) -> Result<(PathBuf, PathBuf), TransferError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    use crate::files::path::resolve_path;

    let user = transfer.user();
    let file_root = transfer.file_root();

    // Validate destination path
    validate_transfer_path(destination, locale)?;

    // Check upload permission
    check_permission(user, Permission::FileUpload, locale)?;

    // Check file_root permission if using root mode
    check_root_permission(user, use_root, locale)?;

    // Resolve area root
    let area_root = resolve_area_root(file_root, &user.username, use_root, locale)?;

    // Build candidate path
    let candidate = build_validated_path(&area_root, destination, locale)?;

    // Try to resolve the destination - it may or may not exist yet
    let resolved_destination = match resolve_path(&area_root, &candidate) {
        Ok(path) => {
            // Destination exists - verify it's a directory
            if !path.is_dir() {
                return Err(TransferError::invalid(err_upload_path_invalid(locale)));
            }

            // Check that existing destination allows uploads
            if !allows_upload(&area_root, &path) {
                return Err(TransferError::permission(
                    err_upload_destination_not_allowed(locale),
                ));
            }

            path
        }
        Err(crate::files::path::PathError::NotFound) => {
            // Destination doesn't exist - find nearest existing ancestor and check if it allows uploads
            let mut ancestor = candidate.as_path();
            let resolved_ancestor = loop {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| TransferError::invalid(err_upload_path_invalid(locale)))?;

                match resolve_path(&area_root, ancestor) {
                    Ok(resolved) => break resolved,
                    Err(crate::files::path::PathError::NotFound) => continue,
                    Err(e) => return Err(path_error_to_transfer_error(e, locale)),
                }
            };

            // Verify ancestor is a directory
            if !resolved_ancestor.is_dir() {
                return Err(TransferError::invalid(err_upload_path_invalid(locale)));
            }

            // Check that ancestor allows uploads (new directories inherit this)
            if !allows_upload(&area_root, &resolved_ancestor) {
                return Err(TransferError::permission(
                    err_upload_destination_not_allowed(locale),
                ));
            }

            // Create the destination directory and any intermediate directories
            std::fs::create_dir_all(&candidate)
                .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;

            // Return the candidate path (now exists)
            candidate
        }
        Err(e) => {
            return Err(path_error_to_transfer_error(e, locale));
        }
    };

    Ok((area_root, resolved_destination))
}

/// Validate relative path and build target/part paths
fn validate_and_build_upload_paths(
    relative_path: &str,
    destination: &Path,
    area_root: &Path,
    locale: &str,
) -> Result<(PathBuf, PathBuf), TransferError> {
    // Validate the relative path (security critical!)
    // - Empty paths are invalid (no filename)
    // - Absolute paths are invalid (must be relative)
    // - validate_file_path checks for null bytes, control chars, Windows drive letters
    // - Path traversal (..) is rejected by build_and_validate_candidate_path below
    if relative_path.is_empty()
        || relative_path.starts_with('/')
        || relative_path.starts_with('\\')
        || validators::validate_file_path(relative_path).is_err()
    {
        return Err(TransferError::invalid(err_upload_path_invalid(locale)));
    }

    // Build the target path
    let target_path = destination.join(relative_path);
    let part_path = PathBuf::from(format!("{}{}", target_path.display(), PART_SUFFIX));

    // Compute the path relative to area_root for validation
    // destination is already validated to be under area_root, so we can strip_prefix
    let relative_to_root = match destination.strip_prefix(area_root) {
        Ok(rel) => rel.join(relative_path),
        Err(_) => {
            // destination is not under area_root - this shouldn't happen
            // if validate_and_resolve_upload_destination was called first
            return Err(TransferError::invalid(err_upload_path_invalid(locale)));
        }
    };

    // Validate the path doesn't contain traversal attempts
    // Note: We don't call resolve_new_path here because parent directories may not exist yet
    // when uploading a directory structure. The security validation in build_and_validate_candidate_path
    // is sufficient (it rejects ".." traversal), and stream_to_part_file/create_empty_file
    // will create parent directories as needed.
    if validate_and_build_candidate_path(area_root, &relative_to_root.to_string_lossy()).is_err() {
        return Err(TransferError::invalid(err_upload_path_invalid(locale)));
    }

    Ok((target_path, part_path))
}

// =============================================================================
// Conflict Detection
// =============================================================================

/// Check for upload conflicts and get existing file state
///
/// Sends FileHashing keepalive messages to the client while hashing large
/// existing files to prevent client timeout.
async fn check_upload_conflicts_and_get_state<W>(
    frame_writer: &mut FrameWriter<W>,
    target_path: &Path,
    part_path: &Path,
    file_size: u64,
    locale: &str,
) -> Result<(u64, Option<String>, StreamingHasher, bool), TransferError>
where
    W: AsyncWriteExt + Unpin,
{
    // Check if complete file already exists (no .part)
    if target_path.exists() && !part_path.exists() {
        let existing_metadata = tokio::fs::metadata(target_path).await.ok();
        let existing_len = existing_metadata.map(|m| m.len()).unwrap_or(0);

        if existing_len == 0 && file_size == 0 {
            // Both are empty files — "already complete"
            return Ok((0, None, StreamingHasher::new(), true));
        }

        if existing_len != file_size {
            // Different sizes — definitely different file, reject
            return Err(TransferError::exists(err_upload_file_exists(locale)));
        }

        // Same size — hash the existing file into a StreamingHasher
        // Client will compare and decide (already complete or conflict)
        let file_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(FALLBACK_FILE_NAME)
            .to_string();
        let hasher = hash_file_with_keepalives(target_path, file_size, &file_name, frame_writer)
            .await
            .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
        let hash = hasher.partial_hash();
        return Ok((file_size, Some(hash), hasher, true));
    }

    // Check for existing .part file for resume
    if part_path.exists() {
        let metadata = tokio::fs::metadata(part_path)
            .await
            .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
        let part_size = metadata.len();
        let file_name = part_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(FALLBACK_PART_FILE_NAME)
            .to_string();
        match hash_file_with_keepalives(part_path, part_size, &file_name, frame_writer).await {
            Ok(hasher) => {
                let hash = hasher.partial_hash();
                return Ok((part_size, Some(hash), hasher, false));
            }
            Err(_) => {
                return Ok((0, None, StreamingHasher::new(), false));
            }
        }
    }

    Ok((0, None, StreamingHasher::new(), false))
}

/// Check for concurrent upload conflict (different uploader) and offset mismatch
fn check_resume_conflict(
    offset: u64,
    existing_size: u64,
    locale: &str,
) -> Result<(), TransferError> {
    // CONFLICT DETECTION: If client is sending full file (offset == 0) but a .part file
    // already existed with data (existing_size > 0), this is a DIFFERENT uploader.
    // Return an error instead of overwriting - the original uploader can still resume.
    if offset == 0 && existing_size > 0 {
        return Err(TransferError::conflict(err_upload_conflict(locale)));
    }

    // SECURITY: Verify client's claimed offset matches actual .part file size.
    // A malicious client could claim offset=1000 when .part is only 500 bytes,
    // causing corrupt data (server would append, resulting in wrong file content).
    // The offset is derived from: offset = file_size - incoming_bytes
    // For a valid resume, this must equal the existing .part file size.
    if offset > 0 && offset != existing_size {
        return Err(TransferError::protocol_error(err_upload_protocol_error(
            locale,
        )));
    }
    Ok(())
}

// =============================================================================
// Protocol Helpers
// =============================================================================

/// Read FileStart message from client
async fn read_client_file_start<R>(
    frame_reader: &mut FrameReader<R>,
    locale: &str,
) -> Result<(String, u64), TransferError>
where
    R: AsyncReadExt + Unpin,
{
    // Loop to skip any FileHashing keepalive messages
    loop {
        let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                return Err(TransferError::io_error(err_upload_connection_lost(locale)));
            }
            Err(_) => {
                return Err(TransferError::protocol_error(err_upload_protocol_error(
                    locale,
                )));
            }
        };

        match received.message {
            ClientMessage::FileStart { path, size } => {
                return Ok((path, size));
            }
            ClientMessage::FileHashing { .. } => {
                // Keepalive message - ignore and continue waiting for FileStart
                continue;
            }
            _ => {
                return Err(TransferError::protocol_error(err_upload_protocol_error(
                    locale,
                )));
            }
        }
    }
}

/// Send FileStartResponse to client
async fn send_file_start_response<W>(
    frame_writer: &mut FrameWriter<W>,
    existing_size: u64,
    existing_hash: Option<String>,
    locale: &str,
) -> Result<(), TransferError>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::FileStartResponse {
        size: existing_size,
        sha256: existing_hash,
    };
    send_server_message_with_id(frame_writer, &response, MessageId::new())
        .await
        .map_err(|_| TransferError::io_error(err_upload_connection_lost(locale)))
}

/// Result of reading the next client frame after FileStartResponse
#[derive(Debug)]
enum ClientFileFrame {
    /// Client is sending file data
    FileData(FrameHeader),
    /// Client says file is already complete (or zero-byte) — no FileData
    FileHash { sha256: String },
}

/// Read the next client frame: FileData or FileHash
///
/// After the FileStartResponse exchange, the client sends one of:
/// - `FileData` then `FileHash` — data transfer with post-hash verification
/// - `FileHash` alone — file was already complete or zero-byte
///
/// Automatically skips `FileHashing` keepalive messages.
async fn read_file_data_or_file_hash<R>(
    frame_reader: &mut FrameReader<R>,
    locale: &str,
) -> Result<ClientFileFrame, TransferError>
where
    R: AsyncReadExt + Unpin,
{
    loop {
        let h = match frame_reader.read_frame_header().await {
            Ok(Some(h)) => h,
            Ok(None) => {
                return Err(TransferError::io_error(err_upload_connection_lost(locale)));
            }
            Err(_) => {
                return Err(TransferError::protocol_error(err_upload_protocol_error(
                    locale,
                )));
            }
        };

        match h.message_type.as_str() {
            "FileHashing" => {
                // Keepalive — consume payload and continue
                if frame_reader.read_payload_into_vec(&h).await.is_err() {
                    return Err(TransferError::protocol_error(err_upload_protocol_error(
                        locale,
                    )));
                }
                continue;
            }
            "FileData" => {
                return Ok(ClientFileFrame::FileData(h));
            }
            "FileHash" => {
                // Read payload and deserialize
                let payload = frame_reader.read_payload_into_vec(&h).await.map_err(|_| {
                    TransferError::protocol_error(err_upload_protocol_error(locale))
                })?;
                let msg: ClientMessage = serde_json::from_slice(&payload).map_err(|_| {
                    TransferError::protocol_error(err_upload_protocol_error(locale))
                })?;
                match msg {
                    ClientMessage::FileHash { sha256 } => {
                        return Ok(ClientFileFrame::FileHash { sha256 });
                    }
                    _ => {
                        return Err(TransferError::protocol_error(err_upload_protocol_error(
                            locale,
                        )));
                    }
                }
            }
            _ => {
                return Err(TransferError::protocol_error(err_upload_protocol_error(
                    locale,
                )));
            }
        }
    }
}

// =============================================================================
// File Operations
// =============================================================================

/// Create an empty file at the target path
async fn create_empty_file(target_path: &Path, locale: &str) -> Result<(), TransferError> {
    // Create parent directories if needed
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
    }

    tokio::fs::write(target_path, &[])
        .await
        .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))
}

/// If a .part file exists, rename it to the final target path
async fn finalize_part_file_if_exists(
    part_path: &Path,
    target_path: &Path,
    locale: &str,
) -> Result<(), TransferError> {
    if part_path.exists() {
        tokio::fs::rename(part_path, target_path)
            .await
            .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
    }
    Ok(())
}

async fn stream_to_part_file<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    header: &FrameHeader,
    target_path: &Path,
    part_path: &Path,
    offset: u64,
    hasher: &mut StreamingHasher,
    locale: &str,
) -> Result<u64, ReceiveFileError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // Create parent directories if needed
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|_| {
            ReceiveFileError::Transfer(TransferError::io_error(err_upload_write_failed(locale)))
        })?;
    }

    // Open .part file for writing
    // For fresh uploads (offset == 0), use create_new(true) to atomically fail if the file
    // already exists. This prevents TOCTOU race conditions.
    let file_result = if offset == 0 {
        tokio::fs::OpenOptions::new()
            .create_new(true) // Atomic: fails if file exists
            .write(true)
            .open(part_path)
            .await
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true) // Resume: append to existing .part file
            .open(part_path)
            .await
    };

    let file = match file_result {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Race condition: another uploader created the .part file
            return Err(ReceiveFileError::Transfer(TransferError::conflict(
                err_upload_conflict(locale),
            )));
        }
        Err(_) => {
            return Err(ReceiveFileError::Transfer(TransferError::io_error(
                err_upload_write_failed(locale),
            )));
        }
    };

    // Wrap file with HashingWriter to feed received bytes to hasher
    let mut hashing_writer = HashingWriter::new(file, hasher);

    // Stream data from client to .part file with ban checking and hashing
    let result = transfer
        .stream_file_from_client(header, &mut hashing_writer, DEFAULT_PROGRESS_TIMEOUT)
        .await;

    // Extract the inner file for flushing/syncing
    let file = hashing_writer.into_inner();

    let bytes_written = result?;

    // Check if we were banned mid-stream
    if transfer.is_banned() {
        return Err(ReceiveFileError::Banned);
    }

    // Ensure all data is flushed to disk
    file.sync_all().await.map_err(|_| {
        ReceiveFileError::Transfer(TransferError::io_error(err_upload_write_failed(locale)))
    })?;

    Ok(bytes_written)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    const TEST_LOCALE: &str = "en";

    /// Create a mock writer for tests that discards all output
    fn mock_writer() -> FrameWriter<Vec<u8>> {
        FrameWriter::new(Vec::new())
    }

    // =========================================================================
    // validate_and_build_upload_paths tests
    // =========================================================================

    #[test]
    fn test_validate_paths_valid_simple() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let destination = area_root.join("uploads");
        // Create destination directory (parent of target file)
        std::fs::create_dir_all(&destination).unwrap();
        // Canonicalize destination after creation for path comparison
        let destination = destination.canonicalize().unwrap();

        let result =
            validate_and_build_upload_paths("file.txt", &destination, &area_root, TEST_LOCALE);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);

        let (target, part) = result.unwrap();
        assert_eq!(target, destination.join("file.txt"));
        assert_eq!(part, destination.join("file.txt.part"));
    }

    #[test]
    fn test_validate_paths_valid_nested() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let destination = area_root.join("uploads");
        // Only create the destination directory - parent directories for nested files
        // don't need to exist during validation (they're created during streaming)
        std::fs::create_dir_all(&destination).unwrap();
        // Canonicalize destination after creation
        let destination = destination.canonicalize().unwrap();

        let result = validate_and_build_upload_paths(
            "subdir/nested/file.txt",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(
            result.is_ok(),
            "Nested paths should validate even if parent dirs don't exist"
        );

        let (target, _) = result.unwrap();
        assert_eq!(target, destination.join("subdir/nested/file.txt"));
    }

    #[test]
    fn test_validate_paths_empty_rejected() {
        use nexus_common::ERROR_KIND_INVALID;

        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let result = validate_and_build_upload_paths("", &area_root, &area_root, TEST_LOCALE);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_INVALID);
    }

    #[test]
    fn test_validate_paths_absolute_rejected() {
        use nexus_common::ERROR_KIND_INVALID;
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let result =
            validate_and_build_upload_paths("/etc/passwd", &area_root, &area_root, TEST_LOCALE);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_INVALID);
    }

    #[test]
    fn test_validate_paths_backslash_absolute_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let result = validate_and_build_upload_paths(
            "\\Windows\\System32",
            &area_root,
            &area_root,
            TEST_LOCALE,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_paths_part_extension_handling() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let destination = area_root.join("uploads");
        std::fs::create_dir_all(&destination).unwrap();
        // Canonicalize destination after creation
        let destination = destination.canonicalize().unwrap();

        // File with extension
        let result = validate_and_build_upload_paths(
            "archive.tar.gz",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        let (_, part) = result.unwrap();
        assert_eq!(part, destination.join("archive.tar.gz.part"));

        // File without extension
        let result =
            validate_and_build_upload_paths("README", &destination, &area_root, TEST_LOCALE);
        let (_, part) = result.unwrap();
        assert_eq!(part, destination.join("README.part"));
    }

    #[test]
    fn test_validate_paths_traversal_rejected() {
        use nexus_common::ERROR_KIND_INVALID;

        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let destination = area_root.join("uploads");
        std::fs::create_dir_all(&destination).unwrap();
        let destination = destination.canonicalize().unwrap();

        // Parent directory traversal
        let result =
            validate_and_build_upload_paths("../escape.txt", &destination, &area_root, TEST_LOCALE);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_INVALID);

        // Nested traversal
        let result = validate_and_build_upload_paths(
            "subdir/../../escape.txt",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(result.is_err());

        // Hidden traversal in middle of path
        let result = validate_and_build_upload_paths(
            "a/b/../../../escape.txt",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_paths_unicode_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let destination = area_root.join("uploads");
        std::fs::create_dir_all(&destination).unwrap();
        let destination = destination.canonicalize().unwrap();

        // Japanese filename
        let result = validate_and_build_upload_paths(
            "日本語ファイル.txt",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(result.is_ok());
        let (target, _) = result.unwrap();
        assert_eq!(target, destination.join("日本語ファイル.txt"));

        // Emoji filename in nested directory - parent doesn't need to exist
        let result = validate_and_build_upload_paths(
            "📁folder/🎵music.mp3",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(
            result.is_ok(),
            "Nested unicode paths should validate even if parent dirs don't exist"
        );
        let (target, _) = result.unwrap();
        assert_eq!(target, destination.join("📁folder/🎵music.mp3"));
    }

    // =========================================================================
    // check_resume_conflict tests
    // =========================================================================

    #[test]
    fn test_resume_conflict_fresh_upload_no_existing() {
        // Fresh upload (offset=0), no existing .part file (existing_size=0)
        let result = check_resume_conflict(0, 0, TEST_LOCALE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_conflict_resume_upload() {
        // Resume upload (offset>0), existing .part file
        let result = check_resume_conflict(500, 500, TEST_LOCALE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_conflict_different_uploader() {
        use nexus_common::ERROR_KIND_CONFLICT;

        // Fresh upload (offset=0) but .part file exists with data - CONFLICT
        let result = check_resume_conflict(0, 500, TEST_LOCALE);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_CONFLICT);
    }

    #[test]
    fn test_resume_conflict_offset_mismatch() {
        use nexus_common::ERROR_KIND_PROTOCOL_ERROR;

        // Resume with mismatched offset - client claims offset 1000 but .part is 500 bytes
        // This could be malicious attempt to corrupt the file
        let result = check_resume_conflict(1000, 500, TEST_LOCALE);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_PROTOCOL_ERROR);
    }

    #[test]
    fn test_resume_conflict_offset_matches() {
        // Valid resume - client's offset matches .part file size
        let result = check_resume_conflict(500, 500, TEST_LOCALE);
        assert!(result.is_ok());
    }

    // =========================================================================
    // check_upload_conflicts_and_get_state tests
    // =========================================================================

    #[tokio::test]
    async fn test_conflicts_no_existing_files() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("newfile.txt");
        let part = temp_dir.path().join("newfile.txt.part");
        let mut writer = mock_writer();

        let result =
            check_upload_conflicts_and_get_state(&mut writer, &target, &part, 100, TEST_LOCALE)
                .await;

        assert!(result.is_ok());
        let (size, hash, _hasher, complete_exists) = result.unwrap();
        assert_eq!(size, 0);
        assert!(hash.is_none());
        assert!(!complete_exists);
    }

    #[tokio::test]
    async fn test_conflicts_existing_complete_file_same_content() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("existing.txt");
        let part = temp_dir.path().join("existing.txt.part");
        let mut writer = mock_writer();

        // Create an empty file (same content as empty upload)
        fs::write(&target, &[]).await.unwrap();

        let result =
            check_upload_conflicts_and_get_state(&mut writer, &target, &part, 0, TEST_LOCALE).await;

        // Empty file uploading empty file = same content, should succeed
        assert!(result.is_ok());
        let (_size, _hash, _hasher, complete_exists) = result.unwrap();
        assert!(complete_exists);
    }

    #[tokio::test]
    async fn test_conflicts_existing_complete_file_different_content() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("existing.txt");
        let part = temp_dir.path().join("existing.txt.part");
        let mut writer = mock_writer();

        // Create an existing file with content
        fs::write(&target, b"existing content").await.unwrap();

        // Try to upload different content (different size)
        let result =
            check_upload_conflicts_and_get_state(&mut writer, &target, &part, 100, TEST_LOCALE)
                .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, nexus_common::ERROR_KIND_EXISTS);
    }

    #[tokio::test]
    async fn test_conflicts_existing_part_file() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("uploading.txt");
        let part = temp_dir.path().join("uploading.txt.part");
        let mut writer = mock_writer();

        // Create a .part file with some content
        fs::write(&part, b"partial data").await.unwrap();

        let result =
            check_upload_conflicts_and_get_state(&mut writer, &target, &part, 1000, TEST_LOCALE)
                .await;

        assert!(result.is_ok());
        let (size, hash, _hasher, complete_exists) = result.unwrap();
        assert_eq!(size, 12); // "partial data".len()
        assert!(hash.is_some());
        assert!(!complete_exists);
    }

    // =========================================================================
    // File operation tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("subdir/empty.txt");

        let result = create_empty_file(&target, TEST_LOCALE).await;
        assert!(result.is_ok());
        assert!(target.exists());

        let content = fs::read(&target).await.unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_finalize_part_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("final.txt");
        let part = temp_dir.path().join("final.txt.part");

        fs::write(&part, b"complete content").await.unwrap();

        let result = finalize_part_file_if_exists(&part, &target, TEST_LOCALE).await;
        assert!(result.is_ok());
        assert!(target.exists());
        assert!(!part.exists());

        let content = fs::read(&target).await.unwrap();
        assert_eq!(content, b"complete content");
    }

    #[tokio::test]
    async fn test_finalize_part_file_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("final.txt");
        let part = temp_dir.path().join("final.txt.part");

        // No .part file exists - should succeed without doing anything
        let result = finalize_part_file_if_exists(&part, &target, TEST_LOCALE).await;
        assert!(result.is_ok());
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn test_hash_file_with_keepalives_basic() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let content = b"Hello, World!";
        fs::write(&file_path, content).await.unwrap();

        let mut writer = mock_writer();
        let hasher =
            hash_file_with_keepalives(&file_path, content.len() as u64, "test.txt", &mut writer)
                .await
                .unwrap();

        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hash_file_with_keepalives_partial_then_finalize() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Write "Hello, World!" but only hash first 5 bytes via hash_file_with_keepalives
        let content = b"Hello, World!";
        fs::write(&file_path, content).await.unwrap();

        let mut writer = mock_writer();
        let hasher = hash_file_with_keepalives(&file_path, 5, "test.txt", &mut writer)
            .await
            .unwrap();

        // partial_hash should be hash of "Hello"
        let partial = hasher.partial_hash();
        assert_eq!(
            partial,
            "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
        );

        // finalize should also be hash of first 5 bytes (that's all we fed)
        let full = hasher.finalize();
        assert_eq!(partial, full);
    }

    // =========================================================================
    // read_file_data_or_file_hash tests
    // =========================================================================

    /// Helper: build a raw protocol frame from type name and payload bytes
    fn build_frame(type_name: &str, payload: &[u8]) -> Vec<u8> {
        let header = format!(
            "NX|{}|{}|a1b2c3d4e5f6|{}|",
            type_name.len(),
            type_name,
            payload.len()
        );
        let mut frame = header.into_bytes();
        frame.extend_from_slice(payload);
        frame.push(b'\n');
        frame
    }

    #[tokio::test]
    async fn test_read_dispatches_file_data() {
        // FileData frame with 5-byte payload
        let frame = build_frame("FileData", b"hello");
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_ok());
        match result.unwrap() {
            ClientFileFrame::FileData(header) => {
                assert_eq!(header.message_type, "FileData");
                assert_eq!(header.payload_length, 5);
            }
            ClientFileFrame::FileHash { .. } => panic!("Expected FileData, got FileHash"),
        }
    }

    #[tokio::test]
    async fn test_read_dispatches_file_hash() {
        // FileHash frame with JSON payload
        let payload = br#"{"type":"FileHash","sha256":"abc123def456"}"#;
        let frame = build_frame("FileHash", payload);
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_ok());
        match result.unwrap() {
            ClientFileFrame::FileHash { sha256 } => {
                assert_eq!(sha256, "abc123def456");
            }
            ClientFileFrame::FileData(_) => panic!("Expected FileHash, got FileData"),
        }
    }

    #[tokio::test]
    async fn test_read_skips_file_hashing_keepalive() {
        // FileHashing frame followed by FileData frame
        // The function should skip the keepalive and return the FileData
        let hashing_payload = br#"{"type":"FileHashing","file":"test.txt"}"#;
        let mut data = build_frame("FileHashing", hashing_payload);
        data.extend_from_slice(&build_frame("FileData", b"world"));

        let cursor = std::io::Cursor::new(data);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_ok());
        match result.unwrap() {
            ClientFileFrame::FileData(header) => {
                assert_eq!(header.message_type, "FileData");
                assert_eq!(header.payload_length, 5);
            }
            ClientFileFrame::FileHash { .. } => panic!("Expected FileData after skip"),
        }
    }

    #[tokio::test]
    async fn test_read_skips_multiple_keepalives() {
        // Two FileHashing keepalives followed by FileHash
        let hashing_payload = br#"{"type":"FileHashing","file":"big.zip"}"#;
        let hash_payload = br#"{"type":"FileHash","sha256":"deadbeef"}"#;
        let mut data = build_frame("FileHashing", hashing_payload);
        data.extend_from_slice(&build_frame("FileHashing", hashing_payload));
        data.extend_from_slice(&build_frame("FileHash", hash_payload));

        let cursor = std::io::Cursor::new(data);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_ok());
        match result.unwrap() {
            ClientFileFrame::FileHash { sha256 } => {
                assert_eq!(sha256, "deadbeef");
            }
            ClientFileFrame::FileData(_) => panic!("Expected FileHash"),
        }
    }

    #[tokio::test]
    async fn test_read_rejects_unexpected_message_type() {
        // A ChatSend frame — valid frame type but wrong context
        let payload = br#"{"type":"ChatSend","message":"hello"}"#;
        let frame = build_frame("ChatSend", payload);
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, nexus_common::ERROR_KIND_PROTOCOL_ERROR);
    }
}
