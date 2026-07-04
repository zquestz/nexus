//! File upload handling for transfers (resume support, conflict detection).
//!
//! The server independently verifies uploaded data via its own StreamingHasher,
//! fed with existing .part content + received FileData chunks.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use tracing::{debug, error, info, warn};

use crate::constants::*;
use crate::files::path::resolve_path;

use nexus_common::framing::{
    DEFAULT_PROGRESS_TIMEOUT, FrameHeader, FrameReader, FrameWriter, MessageId,
};
use nexus_common::hash::StreamingHasher;
use nexus_common::io::read_transfer_client_message_with_full_timeout;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::db::Permission;
use crate::files::activity_key;
use crate::files::path::{allows_upload, validate_and_build_candidate_path};
use crate::handlers::{
    err_upload_conflict, err_upload_connection_lost, err_upload_destination_not_allowed,
    err_upload_empty, err_upload_file_exists, err_upload_hash_mismatch,
    err_upload_insufficient_space, err_upload_path_invalid, err_upload_protocol_error,
    err_upload_write_failed,
};

use nexus_common::PART_SUFFIX;

use super::hashing::{
    FALLBACK_FILE_NAME, FALLBACK_PART_FILE_NAME, HashingWriter, hash_file_with_keepalives,
};
use super::helpers::{
    TransferError, build_validated_path, check_any_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_upload_transfer_error, shutdown_transfer_writer, validate_transfer_path,
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

    let upload_destination =
        match validate_and_resolve_upload_destination(transfer, &destination, use_root, &locale)
            .await
        {
            Ok(result) => result,
            Err(e) => return send_upload_transfer_error(transfer.writer(), &e).await,
        };
    let area_root = upload_destination.area_root;
    let resolved_destination = upload_destination.destination;
    let create_destination = upload_destination.create_destination;

    let log_transfer_id = generate_transfer_id();

    // Whole-upload descendant activity prevents the destination directory from
    // being renamed or deleted between files, without blocking sibling uploads.
    let _upload_activity_guard = match transfer
        .file_activity()
        .try_enter_descendant_path(transfer.file_root(), &resolved_destination)
        .await
    {
        Ok(Ok(g)) => g,
        Ok(Err(_)) => {
            let err = TransferError::conflict(err_upload_conflict(&locale));
            return send_upload_transfer_error(transfer.writer(), &err).await;
        }
        Err(_) => {
            let err = TransferError::invalid(err_upload_path_invalid(&locale));
            return send_upload_transfer_error(transfer.writer(), &err).await;
        }
    };

    if create_destination
        && tokio::fs::create_dir_all(&resolved_destination)
            .await
            .is_err()
    {
        let err = TransferError::io_error(err_upload_write_failed(&locale));
        return send_upload_transfer_error(transfer.writer(), &err).await;
    }

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
    let mut upload_targets = HashSet::new();

    for file_index in 0..file_count {
        let params = ReceiveFileParams {
            area_root: &area_root,
            destination: &resolved_destination,
            upload_targets: &mut upload_targets,
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
                let _ = shutdown_transfer_writer(transfer.writer()).await;
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

    let _ = shutdown_transfer_writer(transfer.writer()).await;

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
            StreamError::Io(e) | StreamError::FrameStarted(e) => {
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
        upload_targets,
    } = params;

    // BLAKE3 arrives later in a separate FileHash frame.
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

    let target_activity_key = activity_key(&target_path).await.map_err(|_| {
        ReceiveFileError::Transfer(TransferError::invalid(err_upload_path_invalid(locale)))
    })?;
    let part_activity_key = activity_key(&part_path).await.map_err(|_| {
        ReceiveFileError::Transfer(TransferError::invalid(err_upload_path_invalid(locale)))
    })?;

    // Claim before reserving. If reservation or later upload work fails, the
    // transfer aborts, so this per-transfer set does not need rollback.
    if !upload_targets.insert(target_activity_key.clone()) {
        return Err(ReceiveFileError::Transfer(TransferError::conflict(
            err_upload_conflict(locale),
        )));
    }

    // Long uploads reserve both target AND `.part` so a `.part` rename can't
    // be promoted to the final target mid-upload.
    let _activity_guard = transfer
        .file_activity()
        .try_enter_child_keys(
            transfer.file_root(),
            vec![target_activity_key, part_activity_key],
        )
        .await
        .map_err(|_| {
            ReceiveFileError::Transfer(TransferError::invalid(err_upload_path_invalid(locale)))
        })?
        .map_err(|_| {
            ReceiveFileError::Transfer(TransferError::conflict(err_upload_conflict(locale)))
        })?;

    // Hasher pre-fed with existing .part for single-pass hashing; sends
    // FileHashing keepalives while hashing large existing files.
    let (existing_size, existing_hash, mut hasher, complete_file_exists) =
        check_upload_conflicts_and_get_state(
            transfer.writer(),
            &target_path,
            &part_path,
            file_size,
            locale,
        )
        .await?;

    ensure_upload_has_available_space(destination, file_size, existing_size, locale).await?;

    send_file_start_response(transfer, existing_size, existing_hash, locale).await?;

    // FileHashing keepalives are skipped automatically.
    let client_frame = read_file_data_or_file_hash(transfer.reader(), locale).await?;

    match client_frame {
        ClientFileFrame::FileHash {
            blake3: client_hash,
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
                // Edge case: leftover .part alongside the complete file.
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
                ClientFileFrame::FileHash { blake3 } => blake3,
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

struct UploadDestinationResolution {
    area_root: PathBuf,
    destination: PathBuf,
    create_destination: bool,
}

/// Upload-folder gate: under an upload/dropbox folder → `Allowed`; else admins
/// and `FileUploadAnywhere` holders get `Bypassed` (callers log for audit),
/// everyone else `Denied`.
fn check_upload_access(user: &AuthenticatedUser, area_root: &Path, path: &Path) -> UploadAccess {
    if allows_upload(area_root, path) {
        return UploadAccess::Allowed;
    }
    if user.is_admin || user.permissions.contains(&Permission::FileUploadAnywhere) {
        return UploadAccess::Bypassed;
    }
    UploadAccess::Denied
}

async fn validate_and_resolve_upload_destination<R, W>(
    transfer: &Transfer<'_, R, W>,
    destination: &str,
    use_root: bool,
    locale: &str,
) -> Result<UploadDestinationResolution, TransferError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let user = transfer.user();
    validate_transfer_path(destination, locale)?;

    // FileUploadAnywhere implies upload capability plus folder-restriction
    // bypass, so it stands alone as a complete grant.
    check_any_permission(
        user,
        &[Permission::FileUpload, Permission::FileUploadAnywhere],
        locale,
    )?;

    check_root_permission(user, use_root, locale)?;

    let area_root = resolve_area_root(
        transfer.file_root(),
        transfer.user_area_root(),
        use_root,
        locale,
    )
    .await?;

    let candidate = build_validated_path(&area_root, destination, locale).await?;

    // Destination may not exist yet.
    let (resolved_destination, create_destination) = match resolve_path(&area_root, &candidate)
        .await
    {
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

            (path, false)
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

            (candidate, true)
        }
        Err(e) => {
            return Err(path_error_to_transfer_error(e, locale));
        }
    };

    Ok(UploadDestinationResolution {
        area_root,
        destination: resolved_destination,
        create_destination,
    })
}

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
    // (e.g. `shared/Music -> /home/user/Music`), so validate `relative_path`
    // against area_root directly — strip_prefix would spuriously reject those.
    // This catches `..` traversal; canonical `destination` has none.
    if validate_and_build_candidate_path(area_root, relative_path).is_err() {
        return Err(TransferError::invalid(err_upload_path_invalid(locale)));
    }

    Ok((target_path, part_path))
}

fn bytes_needed_for_upload(file_size: u64, existing_size: u64) -> u64 {
    file_size.saturating_sub(existing_size)
}

async fn ensure_upload_has_available_space(
    filesystem_path: &Path,
    file_size: u64,
    existing_size: u64,
    locale: &str,
) -> Result<(), TransferError> {
    let bytes_needed = bytes_needed_for_upload(file_size, existing_size);
    if bytes_needed == 0 {
        return Ok(());
    }

    let path_for_task = filesystem_path.to_path_buf();
    let available = tokio::task::spawn_blocking(move || fs4::available_space(path_for_task))
        .await
        .map_err(|e| {
            error!(err = %e, path = %filesystem_path.display(), "{}", LOG_UPLOAD_SPACE_CHECK_FAILED);
            TransferError::io_error(err_upload_write_failed(locale))
        })?
        .map_err(|e| {
            error!(err = %e, path = %filesystem_path.display(), "{}", LOG_UPLOAD_SPACE_CHECK_FAILED);
            TransferError::io_error(err_upload_write_failed(locale))
        })?;

    if available < bytes_needed {
        warn!(
            path = %filesystem_path.display(),
            available,
            needed = bytes_needed,
            "{}", LOG_UPLOAD_INSUFFICIENT_SPACE
        );
        return Err(TransferError::capacity(err_upload_insufficient_space(
            locale,
        )));
    }

    Ok(())
}

/// Sends FileHashing keepalives while hashing large existing files to prevent
/// client timeout.
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

    // Security: a client claiming offset > .part size would append corrupt
    // data. Valid resume requires offset == existing .part size.
    if offset > 0 && offset != existing_size {
        return Err(TransferError::protocol_error(err_upload_protocol_error(
            locale,
        )));
    }
    Ok(())
}

async fn read_client_file_start<R>(
    frame_reader: &mut FrameReader<R>,
    locale: &str,
) -> Result<(String, u64), TransferError>
where
    R: AsyncReadExt + Unpin,
{
    // Loop to skip FileHashing keepalives.
    loop {
        let received =
            match read_transfer_client_message_with_full_timeout(frame_reader, None, None).await {
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

async fn send_file_start_response<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    existing_size: u64,
    existing_hash: Option<String>,
    locale: &str,
) -> Result<(), ReceiveFileError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let response = ServerMessage::FileStartResponse {
        size: existing_size,
        blake3: existing_hash,
    };
    transfer
        .send_direct_with_id(&response, MessageId::new())
        .await
        .map_err(|e| match e {
            StreamError::Banned => ReceiveFileError::Banned,
            _ => ReceiveFileError::Transfer(TransferError::io_error(err_upload_connection_lost(
                locale,
            ))),
        })
}

#[derive(Debug)]
enum ClientFileFrame {
    FileData(FrameHeader),
    /// File was already complete or zero-byte — no FileData coming.
    FileHash {
        blake3: String,
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
        let h =
            match tokio::time::timeout(DEFAULT_PROGRESS_TIMEOUT, frame_reader.read_frame_header())
                .await
            {
                Ok(Ok(Some(h))) => h,
                Ok(Ok(None)) => {
                    return Err(TransferError::io_error(err_upload_connection_lost(locale)));
                }
                Ok(Err(_)) => {
                    return Err(TransferError::protocol_error(err_upload_protocol_error(
                        locale,
                    )));
                }
                Err(_) => {
                    return Err(TransferError::io_error(err_upload_connection_lost(locale)));
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
                    ClientMessage::FileHash { blake3 } => {
                        return Ok(ClientFileFrame::FileHash { blake3 });
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

/// Promotes a leftover `.part` to the final target, if present.
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

    // Fresh uploads use `create_new` so two uploaders can't race the .part
    // file; resume opens the existing .part for append.
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

    let mut hashing_writer = HashingWriter::new(file, hasher);

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
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::time::Duration;

    use nexus_common::hash::StreamingHasher;
    use nexus_common::io::{
        read_transfer_server_message as read_server_message, send_client_message,
    };
    use nexus_common::{ERROR_KIND_CAPACITY, ERROR_KIND_CONFLICT, ERROR_KIND_INVALID};
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::{AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf, duplex};
    use tokio::sync::mpsc;

    use crate::egress::EGRESS_DISPATCH_QUEUE_CAPACITY;
    use crate::egress::task::{DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY, EgressHandle};
    use crate::files::{FileActivityMap, FileIndex};
    use crate::scheduler::ConnectionId;
    use crate::transfers::registry::{TransferDirection, TransferRegistration, TransferRegistry};
    use crate::transfers::transfer::{TransferContext, TransferEgress};

    const TEST_LOCALE: &str = "en";

    fn mock_writer() -> FrameWriter<Vec<u8>> {
        FrameWriter::new(Vec::new())
    }

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7501)
    }

    fn make_upload_transfer<'a>(
        server_read: ReadHalf<DuplexStream>,
        server_write: WriteHalf<DuplexStream>,
        file_root: &'a Path,
        shared_root: &'a Path,
        file_index: &'a Arc<FileIndex>,
        file_activity: &'a Arc<FileActivityMap>,
        registry: &'a TransferRegistry,
    ) -> Transfer<'a, BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>> {
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr: test_addr(),
            nickname: "tester".to_string(),
            username: "tester".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "uploads [NEXUS-UL]".to_string(),
            total_size: 0,
        });

        Transfer::new(
            FrameReader::new(BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_authenticated_user(false, &[Permission::FileUpload]),
                locale: TEST_LOCALE.to_string(),
                file_root,
                file_index,
                file_activity,
                user_area_root: Some(shared_root.to_path_buf()),
                registry,
                egress: None,
            },
        )
    }

    #[tokio::test]
    async fn receive_file_with_egress_handle_does_not_stage_upload_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();

        let (command_tx, mut command_rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        let (settings_tx, _settings_rx) = mpsc::unbounded_channel();
        let egress = EgressHandle::new(command_tx, settings_tx);
        let (_dispatch_tx, dispatch_rx) = mpsc::channel(EGRESS_DISPATCH_QUEUE_CAPACITY);
        let transfer_egress = TransferEgress::new(egress, ConnectionId::new(77_001), dispatch_rx);

        let (client, server) = duplex(8192);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr: test_addr(),
            nickname: "tester".to_string(),
            username: "tester".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "uploads [NEXUS-UL]".to_string(),
            total_size: 0,
        });
        let mut transfer = Transfer::new(
            FrameReader::new(BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_authenticated_user(false, &[Permission::FileUpload]),
                locale: TEST_LOCALE.to_string(),
                file_root: &file_root,
                file_index: &file_index,
                file_activity: &file_activity,
                user_area_root: Some(shared_root.to_path_buf()),
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );
        let mut upload_targets = HashSet::new();
        let file_data = b"uploaded through inbound path".to_vec();
        let mut hasher = StreamingHasher::new();
        hasher.update(&file_data);
        let file_hash = hasher.finalize();

        let server = async {
            match receive_file(
                &mut transfer,
                ReceiveFileParams {
                    area_root: &shared_root,
                    destination: &destination,
                    upload_targets: &mut upload_targets,
                    locale: TEST_LOCALE,
                    transfer_id: "test",
                    file_index: 0,
                },
            )
            .await
            {
                Ok(relative_path) => relative_path,
                Err(ReceiveFileError::Transfer(err)) => {
                    panic!("upload receive should succeed, got transfer error: {err:?}")
                }
                Err(ReceiveFileError::Banned) => {
                    panic!("upload receive should succeed, got banned")
                }
            }
        };

        let client = async {
            send_client_message(
                &mut client_writer,
                &ClientMessage::FileStart {
                    path: "file.txt".to_string(),
                    size: file_data.len() as u64,
                },
            )
            .await
            .unwrap();

            let response = read_server_message(&mut client_reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(
                response,
                ServerMessage::FileStartResponse { size: 0, .. }
            ));

            client_writer
                .get_mut()
                .write_all(&build_frame("FileData", &file_data))
                .await
                .unwrap();
            send_client_message(
                &mut client_writer,
                &ClientMessage::FileHash { blake3: file_hash },
            )
            .await
            .unwrap();
        };

        let (relative_path, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(server, client)
        })
        .await
        .expect("upload receive should not wait on egress");
        assert_eq!(relative_path, "file.txt");
        assert!(
            command_rx.try_recv().is_err(),
            "inbound upload FileData must not stage egress commands"
        );
        assert_eq!(
            fs::read(destination.join("file.txt")).await.unwrap(),
            file_data
        );
    }

    #[tokio::test]
    async fn test_upload_rejects_file_start_that_exceeds_available_space() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();

        let (client, server) = duplex(8192);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );

        let server = async {
            handle_upload(
                &mut transfer,
                UploadParams {
                    destination: "uploads [NEXUS-UL]".to_string(),
                    file_count: 1,
                    total_size: u64::MAX,
                    root: false,
                },
            )
            .await
            .unwrap();
        };

        let client = async {
            let initial = read_server_message(&mut client_reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match initial {
                ServerMessage::FileUploadResponse { success, .. } => assert!(success),
                other => panic!("Expected FileUploadResponse, got {other:?}"),
            }

            send_client_message(
                &mut client_writer,
                &ClientMessage::FileStart {
                    path: "huge.bin".to_string(),
                    size: u64::MAX,
                },
            )
            .await
            .unwrap();

            let complete = read_server_message(&mut client_reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match complete {
                ServerMessage::TransferComplete {
                    success,
                    error,
                    error_kind,
                } => {
                    assert!(!success);
                    assert_eq!(error.as_deref(), Some("Not enough free space for upload"));
                    assert_eq!(error_kind.as_deref(), Some(ERROR_KIND_CAPACITY));
                }
                other => panic!("Expected TransferComplete, got {other:?}"),
            }
        };

        tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(server, client)
        })
        .await
        .expect("capacity rejection should complete");

        assert!(!destination.join("huge.bin").exists());
        assert!(!destination.join("huge.bin.part").exists());
    }

    #[tokio::test]
    async fn test_upload_resume_rejects_remaining_bytes_that_exceed_available_space() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let part_path = destination.join("huge.bin.part");
        fs::write(&part_path, b"partial data").await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();

        let (client, server) = duplex(8192);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );

        let server = async {
            handle_upload(
                &mut transfer,
                UploadParams {
                    destination: "uploads [NEXUS-UL]".to_string(),
                    file_count: 1,
                    total_size: u64::MAX,
                    root: false,
                },
            )
            .await
            .unwrap();
        };

        let client = async {
            let initial = read_server_message(&mut client_reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match initial {
                ServerMessage::FileUploadResponse { success, .. } => assert!(success),
                other => panic!("Expected FileUploadResponse, got {other:?}"),
            }

            send_client_message(
                &mut client_writer,
                &ClientMessage::FileStart {
                    path: "huge.bin".to_string(),
                    size: u64::MAX,
                },
            )
            .await
            .unwrap();

            let complete = read_server_message(&mut client_reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match complete {
                ServerMessage::TransferComplete {
                    success,
                    error_kind,
                    ..
                } => {
                    assert!(!success);
                    assert_eq!(error_kind.as_deref(), Some(ERROR_KIND_CAPACITY));
                }
                other => panic!("Expected TransferComplete, got {other:?}"),
            }
        };

        tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(server, client)
        })
        .await
        .expect("capacity rejection should complete");

        assert_eq!(fs::read(&part_path).await.unwrap(), b"partial data");
        assert!(!destination.join("huge.bin").exists());
    }

    #[tokio::test]
    async fn test_upload_destination_activity_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let _directory_guard = file_activity
            .try_enter_directory_path(&file_root, &destination)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );

        handle_upload(
            &mut transfer,
            UploadParams {
                destination: "uploads [NEXUS-UL]".to_string(),
                file_count: 1,
                total_size: 1,
                root: false,
            },
        )
        .await
        .unwrap();

        let mut reader = FrameReader::new(BufReader::new(client_read));
        let response = read_server_message(&mut reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        match response {
            ServerMessage::FileUploadResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind.as_deref(), Some(ERROR_KIND_CONFLICT));
            }
            _ => panic!("Expected FileUploadResponse"),
        }
    }

    #[tokio::test]
    async fn test_upload_missing_destination_conflict_does_not_create_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let upload_parent = shared_root.join("uploads [NEXUS-UL]");
        let destination = upload_parent.join("new");
        fs::create_dir_all(&upload_parent).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let _directory_guard = file_activity
            .try_enter_directory_path(&file_root, &upload_parent)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );

        handle_upload(
            &mut transfer,
            UploadParams {
                destination: "uploads [NEXUS-UL]/new".to_string(),
                file_count: 1,
                total_size: 1,
                root: false,
            },
        )
        .await
        .unwrap();

        let mut reader = FrameReader::new(BufReader::new(client_read));
        let response = read_server_message(&mut reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        match response {
            ServerMessage::FileUploadResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind.as_deref(), Some(ERROR_KIND_CONFLICT));
            }
            _ => panic!("Expected FileUploadResponse"),
        }
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn test_receive_file_conflicts_when_target_active() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let target = destination.join("file.txt");
        let _target_guard = file_activity
            .try_enter_child_path(&file_root, &target)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (_client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_writer = FrameWriter::new(client_write);
        send_client_message(
            &mut client_writer,
            &ClientMessage::FileStart {
                path: "file.txt".to_string(),
                size: 1,
            },
        )
        .await
        .unwrap();

        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );
        let mut upload_targets = HashSet::new();
        let result = receive_file(
            &mut transfer,
            ReceiveFileParams {
                area_root: &shared_root,
                destination: &destination,
                upload_targets: &mut upload_targets,
                locale: TEST_LOCALE,
                transfer_id: "test",
                file_index: 0,
            },
        )
        .await;

        match result {
            Err(ReceiveFileError::Transfer(err)) => {
                assert_eq!(err.kind, ERROR_KIND_CONFLICT);
            }
            _ => panic!("Expected upload conflict"),
        }
    }

    #[tokio::test]
    async fn test_receive_file_conflicts_when_part_active() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let part = destination.join("file.txt.part");
        let _part_guard = file_activity
            .try_enter_child_path(&file_root, &part)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (_client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_writer = FrameWriter::new(client_write);
        send_client_message(
            &mut client_writer,
            &ClientMessage::FileStart {
                path: "file.txt".to_string(),
                size: 1,
            },
        )
        .await
        .unwrap();

        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );
        let mut upload_targets = HashSet::new();
        let result = receive_file(
            &mut transfer,
            ReceiveFileParams {
                area_root: &shared_root,
                destination: &destination,
                upload_targets: &mut upload_targets,
                locale: TEST_LOCALE,
                transfer_id: "test",
                file_index: 0,
            },
        )
        .await;

        match result {
            Err(ReceiveFileError::Transfer(err)) => {
                assert_eq!(err.kind, ERROR_KIND_CONFLICT);
            }
            _ => panic!("Expected upload conflict"),
        }
    }

    #[tokio::test]
    async fn test_receive_file_conflicts_when_transfer_reuses_target() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();

        let (client, server) = duplex(8192);
        let (_client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_writer = FrameWriter::new(client_write);
        send_client_message(
            &mut client_writer,
            &ClientMessage::FileStart {
                path: "file.txt".to_string(),
                size: 0,
            },
        )
        .await
        .unwrap();
        send_client_message(
            &mut client_writer,
            &ClientMessage::FileHash {
                blake3: String::new(),
            },
        )
        .await
        .unwrap();
        send_client_message(
            &mut client_writer,
            &ClientMessage::FileStart {
                path: "file.txt".to_string(),
                size: 0,
            },
        )
        .await
        .unwrap();

        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            &file_root,
            &shared_root,
            &file_index,
            &file_activity,
            &registry,
        );
        let mut upload_targets = HashSet::new();

        let first = receive_file(
            &mut transfer,
            ReceiveFileParams {
                area_root: &shared_root,
                destination: &destination,
                upload_targets: &mut upload_targets,
                locale: TEST_LOCALE,
                transfer_id: "test",
                file_index: 0,
            },
        )
        .await;
        match first {
            Ok(path) => assert_eq!(path, "file.txt"),
            Err(_) => panic!("Expected first upload to succeed"),
        }

        let second = receive_file(
            &mut transfer,
            ReceiveFileParams {
                area_root: &shared_root,
                destination: &destination,
                upload_targets: &mut upload_targets,
                locale: TEST_LOCALE,
                transfer_id: "test",
                file_index: 1,
            },
        )
        .await;

        match second {
            Err(ReceiveFileError::Transfer(err)) => {
                assert_eq!(err.kind, ERROR_KIND_CONFLICT);
            }
            _ => panic!("Expected duplicate target upload conflict"),
        }
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
        let result = check_resume_conflict(0, 0, TEST_LOCALE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_conflict_resume_upload() {
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
        let result = check_resume_conflict(500, 500, TEST_LOCALE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_bytes_needed_for_upload() {
        assert_eq!(bytes_needed_for_upload(100, 0), 100);
        assert_eq!(bytes_needed_for_upload(100, 40), 60);
        assert_eq!(bytes_needed_for_upload(100, 100), 0);
        assert_eq!(bytes_needed_for_upload(100, 150), 0);
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
            "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
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
            "fbc2b0516ee8744d293b980779178a3508850fdcfe965985782c39601b65794f"
        );

        // finalize should also be hash of first 5 bytes (that's all we fed)
        let full = hasher.finalize();
        assert_eq!(partial, full);
    }

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
        let payload = br#"{"type":"FileHash","blake3":"abc123def456"}"#;
        let frame = build_frame("FileHash", payload);
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_ok());
        match result.unwrap() {
            ClientFileFrame::FileHash { blake3 } => {
                assert_eq!(blake3, "abc123def456");
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
        let hash_payload = br#"{"type":"FileHash","blake3":"deadbeef"}"#;
        let mut data = build_frame("FileHashing", hashing_payload);
        data.extend_from_slice(&build_frame("FileHashing", hashing_payload));
        data.extend_from_slice(&build_frame("FileHash", hash_payload));

        let cursor = std::io::Cursor::new(data);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader, "en").await;
        assert!(result.is_ok());
        match result.unwrap() {
            ClientFileFrame::FileHash { blake3 } => {
                assert_eq!(blake3, "deadbeef");
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
