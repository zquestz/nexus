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
use crate::files::{self, PathLockMode};
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
    TransferError, build_validated_path, check_any_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_upload_transfer_error, validate_transfer_path,
};
use super::transfer::{StreamError, Transfer};
use super::types::{AuthenticatedUser, ReceiveFileParams, UploadParams};

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

    let locale = transfer.locale().to_string();
    let peer_addr = transfer.peer_addr();
    let username = transfer.user().username.clone();

    if file_count == 0 {
        let err = TransferError::invalid(err_upload_empty(&locale));
        return send_upload_transfer_error(transfer.writer(), &err).await;
    }

    let (area_root, resolved_destination) =
        match validate_and_resolve_upload_destination(transfer, &destination, use_root, &locale)
            .await
        {
            Ok(result) => result,
            Err(e) => return send_upload_transfer_error(transfer.writer(), &e).await,
        };

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

    let mut transfer_success = true;
    let mut transfer_error: Option<String> = None;
    let mut transfer_error_kind: Option<String> = None;
    let mut uploaded_files: Vec<String> = Vec::new();

    for file_index in 0..file_count {
        let params = ReceiveFileParams {
            area_root: &area_root,
            destination: &resolved_destination,
            locale: &locale,
            transfer_id: &log_transfer_id,
            file_index,
        };
        match receive_file(transfer, params).await {
            Ok(relative_path) => {
                uploaded_files.push(relative_path);
            }
            Err(ReceiveFileError::Banned) => {
                // Client gets the ban reason on the BBS connection.
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

    let complete = ServerMessage::TransferComplete {
        success: transfer_success,
        error: transfer_error,
        error_kind: transfer_error_kind,
    };
    // Best effort — connection may be closing.
    let _ = transfer.send(&complete).await;

    if transfer_success {
        info!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %destination, files = ?uploaded_files, "{}", LOG_UPLOAD_COMPLETE);
    } else {
        warn!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %destination, files = ?uploaded_files, "{}", LOG_UPLOAD_FAILED);
    }

    if transfer_success {
        transfer.file_index().mark_dirty();
    }

    let _ = transfer.writer().get_mut().shutdown().await;

    Ok(())
}

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

/// Returns the relative path of the received file on success.
async fn receive_file<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    params: ReceiveFileParams<'_>,
) -> Result<String, ReceiveFileError>
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

    // sha256 arrives later in a separate FileHash frame.
    let (relative_path, file_size) = read_client_file_start(transfer.reader(), locale).await?;

    debug!(
        id = %transfer_id,
        file_index = file_index,
        path = %relative_path,
        bytes = file_size,
        "{}", LOG_UPLOAD_RECEIVING
    );

    let (target_path, part_path) =
        validate_and_build_upload_paths(&relative_path, destination, area_root, locale)?;

    // Uploads use `Fail` mode — BBS handlers and other uploads bounce
    // immediately rather than block on a multi-hour transfer. Lock both
    // target AND `.part` so a rename of `.part` can't be promoted to the
    // final target mid-upload.
    let target_key = files::lock_key(&target_path).await.map_err(|_| {
        ReceiveFileError::Transfer(TransferError::invalid(err_upload_path_invalid(locale)))
    })?;
    let part_key = files::lock_key(&part_path).await.map_err(|_| {
        ReceiveFileError::Transfer(TransferError::invalid(err_upload_path_invalid(locale)))
    })?;
    let _lock_guards = transfer
        .file_mutation_locks()
        .acquire_many(vec![target_key, part_key], PathLockMode::Fail)
        .await
        .map_err(|_| {
            ReceiveFileError::Transfer(TransferError::conflict(err_upload_conflict(locale)))
        })?;

    // Hasher is pre-fed with existing .part content for single-pass hashing.
    // Sends FileHashing keepalives while hashing large existing files.
    let (existing_size, existing_hash, mut hasher, complete_file_exists) =
        check_upload_conflicts_and_get_state(
            transfer.writer(),
            &target_path,
            &part_path,
            file_size,
            locale,
        )
        .await?;

    send_file_start_response(transfer.writer(), existing_size, existing_hash, locale).await?;

    // FileHashing keepalives are skipped automatically.
    let client_frame = read_file_data_or_file_hash(transfer.reader(), locale).await?;

    match client_frame {
        ClientFileFrame::FileHash {
            sha256: client_hash,
        } => {
            // Client says: zero-byte or already complete — no FileData coming.
            if file_size == 0 {
                create_empty_file(&target_path, locale).await?;
                debug!(id = %transfer_id, path = %relative_path, "{}", LOG_UPLOAD_EMPTY_FILE);
            } else {
                let server_hash = hasher.finalize();
                if server_hash != client_hash {
                    return Err(ReceiveFileError::Transfer(TransferError::hash_mismatch(
                        err_upload_hash_mismatch(locale),
                    )));
                }
                // Handles the edge case of a leftover .part alongside the complete file.
                finalize_part_file_if_exists(&part_path, &target_path, locale).await?;
                debug!(id = %transfer_id, path = %relative_path, "{}", LOG_UPLOAD_ALREADY_COMPLETE);
            }
        }
        ClientFileFrame::FileData(header) => {
            // If a complete file already exists, client should have sent FileHash.
            if complete_file_exists {
                return Err(ReceiveFileError::Transfer(TransferError::exists(
                    err_upload_file_exists(locale),
                )));
            }

            let incoming_bytes = header.payload_length;
            if incoming_bytes > file_size {
                return Err(ReceiveFileError::Transfer(TransferError::protocol_error(
                    err_upload_protocol_error(locale),
                )));
            }
            let offset = file_size - incoming_bytes;

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

            let client_frame = read_file_data_or_file_hash(transfer.reader(), locale).await?;
            let client_hash = match client_frame {
                ClientFileFrame::FileHash { sha256 } => sha256,
                _ => {
                    return Err(ReceiveFileError::Transfer(TransferError::protocol_error(
                        err_upload_protocol_error(locale),
                    )));
                }
            };

            let server_hash = hasher.finalize();
            if server_hash != client_hash {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(ReceiveFileError::Transfer(TransferError::hash_mismatch(
                    err_upload_hash_mismatch(locale),
                )));
            }

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

    Ok(relative_path)
}

/// Result of an upload folder-access check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadAccess {
    /// Path is inside an upload/dropbox folder — normal case, no bypass needed.
    Allowed,
    /// Path is outside upload folders but the user has FileUploadAnywhere (or is admin).
    Bypassed,
    /// Path is outside upload folders and the user has no bypass — reject.
    Denied,
}

/// Check whether `user` may upload under `path`.
///
/// Normal case: `path` is under a folder typed as upload/dropbox → `Allowed`.
/// Otherwise: admins and holders of `FileUploadAnywhere` get `Bypassed`;
/// everyone else gets `Denied`. Callers log bypasses for audit.
fn check_upload_access(user: &AuthenticatedUser, area_root: &Path, path: &Path) -> UploadAccess {
    if allows_upload(area_root, path) {
        return UploadAccess::Allowed;
    }
    if user.is_admin || user.permissions.contains(&Permission::FileUploadAnywhere) {
        return UploadAccess::Bypassed;
    }
    UploadAccess::Denied
}

/// Validate and resolve upload destination path
async fn validate_and_resolve_upload_destination<R, W>(
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

    validate_transfer_path(destination, locale)?;

    // FileUploadAnywhere implies upload capability plus folder-restriction
    // bypass, so it stands alone as a complete grant.
    check_any_permission(
        user,
        &[Permission::FileUpload, Permission::FileUploadAnywhere],
        locale,
    )?;

    check_root_permission(user, use_root, locale)?;

    let area_root = resolve_area_root(file_root, &user.username, use_root, locale).await?;

    let candidate = build_validated_path(&area_root, destination, locale).await?;

    // Destination may not exist yet.
    let resolved_destination = match resolve_path(&area_root, &candidate).await {
        Ok(path) => {
            let path_is_dir = tokio::fs::metadata(&path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if !path_is_dir {
                return Err(TransferError::invalid(err_upload_path_invalid(locale)));
            }

            // FileUploadAnywhere bypasses the upload-folder check.
            match check_upload_access(user, &area_root, &path) {
                UploadAccess::Allowed => {}
                UploadAccess::Bypassed => {
                    info!(user = %user.username, destination = %path.display(), "{}", LOG_UPLOAD_BYPASS_FOLDER_RESTRICTION);
                }
                UploadAccess::Denied => {
                    return Err(TransferError::permission(
                        err_upload_destination_not_allowed(locale),
                    ));
                }
            }

            path
        }
        Err(crate::files::path::PathError::NotFound) => {
            // Walk up to the nearest existing ancestor; new dirs inherit its upload-allowed status.
            let mut ancestor = candidate.as_path();
            let resolved_ancestor = loop {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| TransferError::invalid(err_upload_path_invalid(locale)))?;

                match resolve_path(&area_root, ancestor).await {
                    Ok(resolved) => break resolved,
                    Err(crate::files::path::PathError::NotFound) => continue,
                    Err(e) => return Err(path_error_to_transfer_error(e, locale)),
                }
            };

            let ancestor_is_dir = tokio::fs::metadata(&resolved_ancestor)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if !ancestor_is_dir {
                return Err(TransferError::invalid(err_upload_path_invalid(locale)));
            }

            match check_upload_access(user, &area_root, &resolved_ancestor) {
                UploadAccess::Allowed => {}
                UploadAccess::Bypassed => {
                    info!(user = %user.username, destination = %candidate.display(), "{}", LOG_UPLOAD_BYPASS_FOLDER_RESTRICTION);
                }
                UploadAccess::Denied => {
                    return Err(TransferError::permission(
                        err_upload_destination_not_allowed(locale),
                    ));
                }
            }

            tokio::fs::create_dir_all(&candidate)
                .await
                .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;

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
    // Security-critical: reject empty/absolute/null/control/drive-letter/traversal.
    // The `validate_and_build_candidate_path` call below catches `..`.
    if relative_path.is_empty()
        || relative_path.starts_with('/')
        || relative_path.starts_with('\\')
        || validators::validate_file_path(relative_path).is_err()
    {
        return Err(TransferError::invalid(err_upload_path_invalid(locale)));
    }

    let target_path = destination.join(relative_path);
    let part_path = PathBuf::from(format!("{}{}", target_path.display(), PART_SUFFIX));

    // `destination` may be outside area_root via an admin-created symlink
    // (e.g. `shared/Music -> /home/user/Music`), so we validate
    // `relative_path` against area_root directly instead of stripping —
    // strip_prefix would spuriously reject those uploads. The relative_path
    // call below catches `..` traversal; canonical `destination` has none.
    if validate_and_build_candidate_path(area_root, relative_path).is_err() {
        return Err(TransferError::invalid(err_upload_path_invalid(locale)));
    }

    Ok((target_path, part_path))
}

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
    // Complete file already exists, no .part.
    if tokio::fs::try_exists(target_path).await.unwrap_or(false)
        && !tokio::fs::try_exists(part_path).await.unwrap_or(false)
    {
        let existing_metadata = tokio::fs::metadata(target_path).await.ok();
        let existing_len = existing_metadata.map(|m| m.len()).unwrap_or(0);

        if existing_len == 0 && file_size == 0 {
            return Ok((0, None, StreamingHasher::new(), true));
        }

        if existing_len != file_size {
            return Err(TransferError::exists(err_upload_file_exists(locale)));
        }

        // Same size — hash and let the client decide (already complete or conflict).
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

    if tokio::fs::try_exists(part_path).await.unwrap_or(false) {
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
    // Full file with an existing .part = different uploader; don't overwrite,
    // the original uploader can still resume.
    if offset == 0 && existing_size > 0 {
        return Err(TransferError::conflict(err_upload_conflict(locale)));
    }

    // Security: a malicious client could claim offset=1000 when .part is only
    // 500 bytes, causing the server to append corrupt data. Valid resume
    // requires offset == existing .part size.
    if offset > 0 && offset != existing_size {
        return Err(TransferError::protocol_error(err_upload_protocol_error(
            locale,
        )));
    }
    Ok(())
}

/// Read FileStart message from client
async fn read_client_file_start<R>(
    frame_reader: &mut FrameReader<R>,
    locale: &str,
) -> Result<(String, u64), TransferError>
where
    R: AsyncReadExt + Unpin,
{
    // Loop to skip FileHashing keepalives.
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

#[derive(Debug)]
enum ClientFileFrame {
    FileData(FrameHeader),
    /// File was already complete or zero-byte — no FileData coming.
    FileHash {
        sha256: String,
    },
}

/// After FileStartResponse, the client sends either `FileData` then `FileHash`,
/// or `FileHash` alone. `FileHashing` keepalives are skipped.
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
                // Consume keepalive payload and wait for the next frame.
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

/// Create an empty file at the target path
async fn create_empty_file(target_path: &Path, locale: &str) -> Result<(), TransferError> {
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
    if tokio::fs::try_exists(part_path).await.unwrap_or(false) {
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
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|_| {
            ReceiveFileError::Transfer(TransferError::io_error(err_upload_write_failed(locale)))
        })?;
    }

    // Fresh uploads use `create_new` so two uploaders can't race the .part file.
    // Resume opens the existing .part for append.
    let file_result = if offset == 0 {
        tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(part_path)
            .await
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(part_path)
            .await
    };

    let file = match file_result {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another uploader raced us to the .part file.
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

    let file = hashing_writer.into_inner();

    let bytes_written = result?;

    if transfer.is_banned() {
        return Err(ReceiveFileError::Banned);
    }

    file.sync_all().await.map_err(|_| {
        ReceiveFileError::Transfer(TransferError::io_error(err_upload_write_failed(locale)))
    })?;

    Ok(bytes_written)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::make_authenticated_user;
    use super::*;
    use nexus_common::ERROR_KIND_INVALID;
    use tempfile::TempDir;
    use tokio::fs;

    const TEST_LOCALE: &str = "en";

    /// Create a mock writer for tests that discards all output
    fn mock_writer() -> FrameWriter<Vec<u8>> {
        FrameWriter::new(Vec::new())
    }

    #[test]
    fn test_validate_paths_valid_simple() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let destination = area_root.join("uploads");
        std::fs::create_dir_all(&destination).unwrap();
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
        // Parent dirs for nested paths are created during streaming, not validation.
        std::fs::create_dir_all(&destination).unwrap();
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
    fn test_validate_paths_destination_outside_area_root() {
        // Regression: admin-created symlinks may resolve destination outside
        // area_root. Before the fix, this triggered a strip_prefix failure
        // and rejected the upload with err_upload_path_invalid. Now the
        // relative_path is validated independently of area_root, so uploads
        // to symlinked destinations succeed.
        let area_root_dir = TempDir::new().unwrap();
        let external_dir = TempDir::new().unwrap();
        let area_root = area_root_dir.path().canonicalize().unwrap();
        let destination = external_dir.path().canonicalize().unwrap();

        assert!(
            destination.strip_prefix(&area_root).is_err(),
            "test precondition: destination must be outside area_root"
        );

        let result =
            validate_and_build_upload_paths("file.txt", &destination, &area_root, TEST_LOCALE);
        assert!(
            result.is_ok(),
            "uploads to destinations outside area_root (via admin symlinks) should succeed"
        );
        let (target, _) = result.unwrap();
        assert_eq!(target, destination.join("file.txt"));
    }

    #[test]
    fn test_validate_paths_traversal_rejected_with_external_destination() {
        // Even for external (symlinked) destinations, traversal in relative_path
        // must still be rejected.
        let area_root_dir = TempDir::new().unwrap();
        let external_dir = TempDir::new().unwrap();
        let area_root = area_root_dir.path().canonicalize().unwrap();
        let destination = external_dir.path().canonicalize().unwrap();

        let result =
            validate_and_build_upload_paths("../escape.txt", &destination, &area_root, TEST_LOCALE);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_INVALID);
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

    #[test]
    fn test_upload_access_normal_upload_folder() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let upload_dir = area_root.join("uploads [NEXUS-UL]");
        std::fs::create_dir_all(&upload_dir).unwrap();

        let user = make_authenticated_user(false, &[]);
        assert_eq!(
            check_upload_access(&user, &area_root, &upload_dir),
            UploadAccess::Allowed
        );
    }

    #[test]
    fn test_upload_access_regular_folder_no_permission() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let regular_dir = area_root.join("docs");
        std::fs::create_dir_all(&regular_dir).unwrap();

        let user = make_authenticated_user(false, &[]);
        assert_eq!(
            check_upload_access(&user, &area_root, &regular_dir),
            UploadAccess::Denied
        );
    }

    #[test]
    fn test_upload_access_regular_folder_with_bypass_permission() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let regular_dir = area_root.join("docs");
        std::fs::create_dir_all(&regular_dir).unwrap();

        let user = make_authenticated_user(false, &[Permission::FileUploadAnywhere]);
        assert_eq!(
            check_upload_access(&user, &area_root, &regular_dir),
            UploadAccess::Bypassed
        );
    }

    #[test]
    fn test_upload_access_regular_folder_admin() {
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let regular_dir = area_root.join("docs");
        std::fs::create_dir_all(&regular_dir).unwrap();

        let user = make_authenticated_user(true, &[]);
        assert_eq!(
            check_upload_access(&user, &area_root, &regular_dir),
            UploadAccess::Bypassed
        );
    }

    #[test]
    fn test_upload_access_upload_folder_with_bypass_permission_still_normal() {
        // Having FileUploadAnywhere doesn't flip a normal upload folder into
        // the Bypassed state — we only log audit events for genuine bypasses.
        let temp_dir = TempDir::new().unwrap();
        let area_root = temp_dir.path().canonicalize().unwrap();
        let upload_dir = area_root.join("uploads [NEXUS-UL]");
        std::fs::create_dir_all(&upload_dir).unwrap();

        let user = make_authenticated_user(false, &[Permission::FileUploadAnywhere]);
        assert_eq!(
            check_upload_access(&user, &area_root, &upload_dir),
            UploadAccess::Allowed
        );
    }
}
