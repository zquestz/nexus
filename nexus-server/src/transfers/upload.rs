//! File upload handling for transfers
//!
//! Contains functions for handling upload requests and receiving files
//! from clients with resume support and conflict detection.

use std::io;
use std::path::{Path, PathBuf};

use nexus_common::framing::{
    DEFAULT_PROGRESS_TIMEOUT, FrameHeader, FrameReader, FrameWriter, MessageId,
};
use nexus_common::io::{read_client_message_with_full_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::db::Permission;
use crate::files::path::{allows_upload, build_and_validate_candidate_path, resolve_new_path};
use crate::handlers::{
    err_upload_conflict, err_upload_connection_lost, err_upload_destination_not_allowed,
    err_upload_empty, err_upload_file_exists, err_upload_hash_mismatch, err_upload_path_invalid,
    err_upload_protocol_error, err_upload_write_failed,
};

use super::hash::compute_file_sha256;
use super::helpers::{
    TransferError, build_validated_path, check_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_upload_transfer_error, validate_transfer_path,
};
use super::types::{ReceiveFileParams, TransferContext, UploadParams};

// =============================================================================
// Main Handler
// =============================================================================

/// Handle a file upload request
pub(crate) async fn handle_upload<R, W>(
    ctx: &mut TransferContext<'_, R, W>,
    params: UploadParams,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let UploadParams {
        destination,
        file_count,
        total_size,
        root: use_root,
    } = params;

    // Reject empty uploads - must have at least one file
    if file_count == 0 {
        return send_upload_transfer_error(
            ctx.frame_writer,
            &TransferError::invalid(err_upload_empty(ctx.locale)),
        )
        .await;
    }

    // Validate and resolve destination path
    let (area_root, resolved_destination) =
        match validate_and_resolve_upload_destination(ctx, &destination, use_root) {
            Ok(result) => result,
            Err(e) => return send_upload_transfer_error(ctx.frame_writer, &e).await,
        };

    // Generate transfer ID for logging
    let transfer_id = generate_transfer_id();

    if ctx.debug {
        eprintln!(
            "Upload {transfer_id}: {} files, {} bytes to {} from {}",
            file_count, total_size, destination, ctx.peer_addr
        );
    }

    // Send FileUploadResponse
    let response = ServerMessage::FileUploadResponse {
        success: true,
        error: None,
        error_kind: None,
        transfer_id: Some(transfer_id.clone()),
    };
    send_server_message_with_id(ctx.frame_writer, &response, MessageId::new()).await?;

    // Receive each file
    let mut transfer_success = true;
    let mut transfer_error: Option<String> = None;
    let mut transfer_error_kind: Option<String> = None;

    for file_index in 0..file_count {
        let params = ReceiveFileParams {
            area_root: &area_root,
            destination: &resolved_destination,
            locale: ctx.locale,
            debug: ctx.debug,
            transfer_id: &transfer_id,
            file_index,
        };
        match receive_file(ctx.frame_reader, ctx.frame_writer, params).await {
            Ok(()) => {}
            Err(e) => {
                if ctx.debug {
                    eprintln!(
                        "Upload {transfer_id}: Error receiving file {file_index}: {}",
                        e.message
                    );
                }
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
    send_server_message_with_id(ctx.frame_writer, &complete, MessageId::new()).await?;

    if ctx.debug {
        if transfer_success {
            eprintln!("Upload {transfer_id}: Complete");
        } else {
            eprintln!("Upload {transfer_id}: Failed");
        }
    }

    // Close connection
    let _ = ctx.frame_writer.get_mut().shutdown().await;

    Ok(())
}

// =============================================================================
// File Reception
// =============================================================================

/// Receive a single file from the client
///
/// Returns `Ok(())` on success, or `Err((error_message, error_kind))` on failure.
async fn receive_file<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    params: ReceiveFileParams<'_>,
) -> Result<(), TransferError>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let ReceiveFileParams {
        area_root,
        destination,
        locale,
        debug,
        transfer_id,
        file_index,
    } = params;

    // Read FileStart from client
    let (relative_path, file_size, client_sha256) =
        read_client_file_start(frame_reader, locale).await?;

    if debug {
        eprintln!(
            "Upload {transfer_id}: Receiving file {file_index}: {} ({} bytes)",
            relative_path, file_size
        );
    }

    // Validate the relative path and build target paths
    let (target_path, part_path) =
        validate_and_build_upload_paths(&relative_path, destination, area_root, locale)?;

    // Check for conflicts and get existing file state
    let (existing_size, existing_hash) = check_upload_conflicts_and_get_state(
        &target_path,
        &part_path,
        file_size,
        &client_sha256,
        locale,
    )
    .await?;

    // Send FileStartResponse with our current state
    send_file_start_response(frame_writer, existing_size, existing_hash.clone(), locale).await?;

    // Handle zero-byte files - NO FileData frame expected
    if file_size == 0 {
        create_empty_file(&target_path, locale).await?;
        if debug {
            eprintln!("Upload {transfer_id}: Created empty file {}", relative_path);
        }
        return Ok(());
    }

    // Check if file is already complete (sizes and hashes match) - NO FileData expected
    if is_file_already_complete(&existing_hash, &client_sha256, existing_size, file_size) {
        if debug {
            eprintln!("Upload {transfer_id}: {} already complete", relative_path);
        }
        finalize_part_file_if_exists(&part_path, &target_path, locale).await?;
        return Ok(());
    }

    // Read FileData header and calculate offset
    let (header, offset) = read_file_data_header(frame_reader, file_size, locale).await?;

    // Check for concurrent upload conflict
    check_resume_conflict(offset, existing_size, locale)?;

    if debug && offset > 0 {
        eprintln!(
            "Upload {transfer_id}: Resuming {} from offset {} ({}%)",
            relative_path,
            offset,
            (offset * 100) / file_size
        );
    }

    // Stream file data to .part file
    let bytes_written = stream_to_part_file(
        frame_reader,
        &header,
        &target_path,
        &part_path,
        offset,
        locale,
    )
    .await?;

    if debug {
        eprintln!(
            "Upload {transfer_id}: Received {} bytes for {}",
            bytes_written, relative_path
        );
    }

    // Verify hash and finalize
    verify_and_finalize_upload(&part_path, &target_path, &client_sha256, locale).await?;

    if debug {
        eprintln!(
            "Upload {transfer_id}: Completed {} ({} bytes, hash verified)",
            relative_path, file_size
        );
    }

    Ok(())
}

// =============================================================================
// Validation Helpers
// =============================================================================

/// Validate and resolve upload destination path
fn validate_and_resolve_upload_destination<R, W>(
    ctx: &TransferContext<'_, R, W>,
    destination: &str,
    use_root: bool,
) -> Result<(PathBuf, PathBuf), TransferError>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    use crate::files::path::resolve_path;

    // Validate destination path
    validate_transfer_path(destination, ctx.locale)?;

    // Check upload permission
    check_permission(ctx.user, Permission::FileUpload, ctx.locale)?;

    // Check file_root permission if using root mode
    check_root_permission(ctx.user, use_root, ctx.locale)?;

    // Resolve area root
    let area_root = resolve_area_root(ctx.file_root, &ctx.user.username, use_root, ctx.locale)?;

    // Build candidate path
    let candidate = build_validated_path(&area_root, destination, ctx.locale)?;

    // Resolve to canonical path
    let resolved_destination = resolve_path(&area_root, &candidate)
        .map_err(|e| path_error_to_transfer_error(e, ctx.locale))?;

    // Verify destination is a directory
    if !resolved_destination.is_dir() {
        return Err(TransferError::invalid(err_upload_path_invalid(ctx.locale)));
    }

    // Check that destination allows uploads
    if !allows_upload(&area_root, &resolved_destination) {
        return Err(TransferError::permission(
            err_upload_destination_not_allowed(ctx.locale),
        ));
    }

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
    let part_path = target_path.with_extension(
        target_path
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

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
    let candidate_path =
        match build_and_validate_candidate_path(area_root, &relative_to_root.to_string_lossy()) {
            Ok(p) => p,
            Err(_) => {
                return Err(TransferError::invalid(err_upload_path_invalid(locale)));
            }
        };

    // Use resolve_new_path which allows non-existent final component but requires valid parent
    if resolve_new_path(area_root, &candidate_path).is_err() {
        return Err(TransferError::invalid(err_upload_path_invalid(locale)));
    }

    Ok((target_path, part_path))
}

// =============================================================================
// Conflict Detection
// =============================================================================

/// Check for conflicts with existing files and get current state for resume
async fn check_upload_conflicts_and_get_state(
    target_path: &Path,
    part_path: &Path,
    file_size: u64,
    client_sha256: &str,
    locale: &str,
) -> Result<(u64, Option<String>), TransferError> {
    // Check if complete file already exists (no .part)
    if target_path.exists() && !part_path.exists() {
        let existing_metadata = tokio::fs::metadata(target_path).await.ok();
        let existing_len = existing_metadata.map(|m| m.len()).unwrap_or(0);

        let same_content = if existing_len == file_size && file_size > 0 {
            if let Ok(existing_hash) = compute_file_sha256(target_path).await {
                existing_hash == client_sha256
            } else {
                false
            }
        } else if existing_len == 0 && file_size == 0 {
            // Both are empty files - same content
            true
        } else {
            false
        };

        if !same_content {
            // Different content - return error, don't auto-rename
            return Err(TransferError::exists(err_upload_file_exists(locale)));
        }
        // Same content - will be handled as "already complete" by caller
    }

    // Check for existing .part file for resume
    let (existing_size, existing_hash) = if part_path.exists() {
        match compute_file_sha256(part_path).await {
            Ok(hash) => {
                let metadata = tokio::fs::metadata(part_path)
                    .await
                    .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
                (metadata.len(), Some(hash))
            }
            Err(_) => (0, None),
        }
    } else {
        (0, None)
    };

    Ok((existing_size, existing_hash))
}

/// Check if file is already complete based on hash and size
fn is_file_already_complete(
    existing_hash: &Option<String>,
    client_sha256: &str,
    existing_size: u64,
    file_size: u64,
) -> bool {
    if let Some(hash) = existing_hash {
        hash == client_sha256 && existing_size == file_size
    } else {
        false
    }
}

/// Check for concurrent upload conflict (different uploader)
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
    Ok(())
}

// =============================================================================
// Protocol Helpers
// =============================================================================

/// Read FileStart message from client
async fn read_client_file_start<R>(
    frame_reader: &mut FrameReader<R>,
    locale: &str,
) -> Result<(String, u64, String), TransferError>
where
    R: AsyncReadExt + Unpin,
{
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
        ClientMessage::FileStart { path, size, sha256 } => Ok((path, size, sha256)),
        _ => Err(TransferError::protocol_error(err_upload_protocol_error(
            locale,
        ))),
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

/// Read FileData header and calculate the resume offset
async fn read_file_data_header<R>(
    frame_reader: &mut FrameReader<R>,
    file_size: u64,
    locale: &str,
) -> Result<(FrameHeader, u64), TransferError>
where
    R: AsyncReadExt + Unpin,
{
    let header = match frame_reader.read_frame_header().await {
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

    // Verify it's a FileData message
    if header.message_type != "FileData" {
        return Err(TransferError::protocol_error(err_upload_protocol_error(
            locale,
        )));
    }

    let incoming_bytes = header.payload_length;

    // Reject if client is sending more data than declared file size
    if incoming_bytes > file_size {
        return Err(TransferError::protocol_error(err_upload_protocol_error(
            locale,
        )));
    }

    // Calculate offset: offset = total_size - incoming_bytes
    // Safe now since we verified incoming_bytes <= file_size
    let offset = file_size - incoming_bytes;

    Ok((header, offset))
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

/// Stream file data from client to .part file
async fn stream_to_part_file<R>(
    frame_reader: &mut FrameReader<R>,
    header: &FrameHeader,
    target_path: &Path,
    part_path: &Path,
    offset: u64,
    locale: &str,
) -> Result<u64, TransferError>
where
    R: AsyncReadExt + Unpin,
{
    // Create parent directories if needed
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
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

    let mut file = match file_result {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Race condition: another uploader created the .part file
            return Err(TransferError::conflict(err_upload_conflict(locale)));
        }
        Err(_) => {
            return Err(TransferError::io_error(err_upload_write_failed(locale)));
        }
    };

    // Stream data from client to .part file
    let bytes_written = frame_reader
        .stream_payload_to_writer(header, &mut file, DEFAULT_PROGRESS_TIMEOUT)
        .await
        .map_err(|_| TransferError::io_error(err_upload_connection_lost(locale)))?;

    // Ensure all data is flushed to disk
    file.sync_all()
        .await
        .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;

    Ok(bytes_written)
}

/// Verify the completed file hash and rename from .part to final destination
async fn verify_and_finalize_upload(
    part_path: &Path,
    target_path: &Path,
    expected_sha256: &str,
    locale: &str,
) -> Result<(), TransferError> {
    // Verify the complete file hash
    let actual_hash = compute_file_sha256(part_path)
        .await
        .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;

    if actual_hash != expected_sha256 {
        // Hash mismatch - delete the .part file
        let _ = tokio::fs::remove_file(part_path).await;
        return Err(TransferError::hash_mismatch(err_upload_hash_mismatch(
            locale,
        )));
    }

    // Rename .part to final destination
    tokio::fs::rename(part_path, target_path)
        .await
        .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))
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
        // Create the destination AND the parent directories for the nested file
        std::fs::create_dir_all(destination.join("subdir/nested")).unwrap();
        // Canonicalize destination after creation
        let destination = destination.canonicalize().unwrap();

        let result = validate_and_build_upload_paths(
            "subdir/nested/file.txt",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(result.is_ok());

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

        // Emoji filename
        let result = validate_and_build_upload_paths(
            "📁folder/🎵music.mp3",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        // This should fail because the parent directory doesn't exist
        assert!(result.is_err());

        // Create the parent and try again
        std::fs::create_dir_all(destination.join("📁folder")).unwrap();
        let result = validate_and_build_upload_paths(
            "📁folder/🎵music.mp3",
            &destination,
            &area_root,
            TEST_LOCALE,
        );
        assert!(result.is_ok());
    }

    // =========================================================================
    // is_file_already_complete tests
    // =========================================================================

    #[test]
    fn test_file_complete_matching_hash_and_size() {
        let hash = Some("abc123".to_string());
        assert!(is_file_already_complete(&hash, "abc123", 1000, 1000));
    }

    #[test]
    fn test_file_not_complete_different_hash() {
        let hash = Some("abc123".to_string());
        assert!(!is_file_already_complete(&hash, "def456", 1000, 1000));
    }

    #[test]
    fn test_file_not_complete_different_size() {
        let hash = Some("abc123".to_string());
        assert!(!is_file_already_complete(&hash, "abc123", 500, 1000));
    }

    #[test]
    fn test_file_not_complete_no_existing_hash() {
        assert!(!is_file_already_complete(&None, "abc123", 0, 1000));
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

    // =========================================================================
    // check_upload_conflicts_and_get_state tests
    // =========================================================================

    #[tokio::test]
    async fn test_conflicts_no_existing_files() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("new_file.txt");
        let part = temp_dir.path().join("new_file.txt.part");

        let result =
            check_upload_conflicts_and_get_state(&target, &part, 100, "somehash", TEST_LOCALE)
                .await;

        assert!(result.is_ok());
        let (size, hash) = result.unwrap();
        assert_eq!(size, 0);
        assert!(hash.is_none());
    }

    #[tokio::test]
    async fn test_conflicts_existing_complete_file_same_content() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("existing.txt");
        let part = temp_dir.path().join("existing.txt.part");

        // Create an empty file (same content as empty upload)
        fs::write(&target, &[]).await.unwrap();

        let result =
            check_upload_conflicts_and_get_state(&target, &part, 0, "anyhash", TEST_LOCALE).await;

        // Empty file uploading empty file = same content, should succeed
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_conflicts_existing_complete_file_different_content() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("existing.txt");
        let part = temp_dir.path().join("existing.txt.part");

        // Create an existing file with content
        fs::write(&target, b"existing content").await.unwrap();

        // Try to upload different content (different size)
        let result =
            check_upload_conflicts_and_get_state(&target, &part, 100, "newhash", TEST_LOCALE).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, nexus_common::ERROR_KIND_EXISTS);
    }

    #[tokio::test]
    async fn test_conflicts_existing_part_file() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("uploading.txt");
        let part = temp_dir.path().join("uploading.txt.part");

        // Create a .part file with some content
        fs::write(&part, b"partial data").await.unwrap();

        let result =
            check_upload_conflicts_and_get_state(&target, &part, 1000, "somehash", TEST_LOCALE)
                .await;

        assert!(result.is_ok());
        let (size, hash) = result.unwrap();
        assert_eq!(size, 12); // "partial data".len()
        assert!(hash.is_some());
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
    async fn test_verify_and_finalize_success() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("verified.txt");
        let part = temp_dir.path().join("verified.txt.part");

        let content = b"test content for hashing";
        fs::write(&part, content).await.unwrap();

        // Compute the actual hash
        let expected_hash = super::super::hash::compute_file_sha256(&part)
            .await
            .unwrap();

        let result = verify_and_finalize_upload(&part, &target, &expected_hash, TEST_LOCALE).await;
        assert!(result.is_ok());
        assert!(target.exists());
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn test_verify_and_finalize_hash_mismatch() {
        use nexus_common::ERROR_KIND_HASH_MISMATCH;

        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("verified.txt");
        let part = temp_dir.path().join("verified.txt.part");

        fs::write(&part, b"some content").await.unwrap();

        let result =
            verify_and_finalize_upload(&part, &target, "wrong_hash_value", TEST_LOCALE).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_HASH_MISMATCH);

        // .part file should be deleted on hash mismatch
        assert!(!part.exists());
        assert!(!target.exists());
    }
}
