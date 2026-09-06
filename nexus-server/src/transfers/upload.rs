//! File upload handling for transfers (resume support, conflict detection).
//!
//! The server independently verifies uploaded data via its own StreamingHasher,
//! fed with existing .part content + received FileData chunks.

use std::collections::HashSet;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use tracing::{debug, error, info, warn};

use crate::constants::*;
use crate::files::path::resolve_path;

use nexus_common::framing::{FrameHeader, FrameReader, FrameWriter, MessageId};
use nexus_common::hash::StreamingHasher;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::{Instant, timeout_at};

use crate::db::Permission;
use crate::files::activity_key;
use crate::files::path::{allows_upload, validate_and_build_candidate_path};
use crate::handlers::{
    err_upload_conflict, err_upload_connection_lost, err_upload_destination_not_allowed,
    err_upload_empty, err_upload_file_exists, err_upload_hash_mismatch,
    err_upload_insufficient_space, err_upload_path_invalid, err_upload_protocol_error,
    err_upload_write_failed,
};

use nexus_common::{PART_SUFFIX, TRANSFER_CONTROL_FRAME_TIMEOUT, TRANSFER_IO_PROGRESS_TIMEOUT};

use super::hashing::{
    FALLBACK_FILE_NAME, FALLBACK_PART_FILE_NAME, HashingWriter, hash_file_with_keepalives,
};
use super::helpers::{
    TransferError, build_validated_path, check_any_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, read_transfer_control_message,
    resolve_area_root, send_upload_transfer_error, shutdown_transfer_writer,
    validate_transfer_path,
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
            Err(ReceiveFileError::FrameStarted(e)) => {
                warn!(id = %log_transfer_id, user = %username, ip = %peer_addr, err = %e, "{}", LOG_UPLOAD_ERROR);
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

#[derive(Debug)]
enum ReceiveFileError {
    Transfer(TransferError),
    FrameStarted(io::Error),
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
            StreamError::FrameStarted(e) => ReceiveFileError::FrameStarted(e),
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

    // Hasher pre-fed with the existing completed file or .part for single-pass hashing; sends
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
            // Without FileData, all declared bytes must already be available.
            if existing_size != file_size {
                return Err(ReceiveFileError::Transfer(TransferError::protocol_error(
                    err_upload_protocol_error(locale),
                )));
            }
            let server_hash = hasher.finalize();
            if server_hash != client_hash {
                return Err(ReceiveFileError::Transfer(TransferError::hash_mismatch(
                    err_upload_hash_mismatch(locale),
                )));
            }

            if file_size == 0 {
                if !complete_file_exists {
                    create_empty_file(&target_path, locale).await?;
                }
                debug!(id = %transfer_id, path = %relative_path, "{}", LOG_UPLOAD_EMPTY_FILE);
            } else {
                // Never replace a verified completed file with an unrelated leftover .part.
                if !complete_file_exists {
                    finalize_part_file_if_exists(&part_path, &target_path, locale).await?;
                }
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
                discard_failed_upload_bytes(&part_path, offset)
                    .await
                    .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
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

async fn upload_metadata_if_exists(
    path: &Path,
    locale: &str,
) -> Result<Option<Metadata>, TransferError> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(Some(metadata)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(TransferError::io_error(err_upload_write_failed(locale))),
    }
}

/// Sends FileHashing keepalives while hashing large existing files to prevent
/// client timeout.
async fn check_upload_conflicts_and_get_state<W>(
    frame_writer: &mut FrameWriter<W>,
    target_path: &Path,
    part_path: &Path,
    file_size: u64,
    locale: &str,
) -> Result<(u64, Option<String>, StreamingHasher, bool), ReceiveFileError>
where
    W: AsyncWriteExt + Unpin,
{
    // A completed destination takes precedence over any leftover .part.
    if let Some(metadata) = upload_metadata_if_exists(target_path, locale).await? {
        let existing_len = metadata.len();

        if existing_len == 0 && file_size == 0 {
            return Ok((0, None, StreamingHasher::new(), true));
        }

        if existing_len != file_size {
            return Err(TransferError::exists(err_upload_file_exists(locale)).into());
        }

        // Same size — hash and let the client decide (already complete or conflict).
        let file_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(FALLBACK_FILE_NAME)
            .to_string();
        let hasher = hash_file_with_keepalives(target_path, file_size, &file_name, frame_writer)
            .await
            .map_err(|e| match e {
                StreamError::Io(_) => ReceiveFileError::Transfer(TransferError::io_error(
                    err_upload_write_failed(locale),
                )),
                e => e.into(),
            })?;
        let hash = hasher.partial_hash();
        return Ok((file_size, Some(hash), hasher, true));
    }

    if let Some(metadata) = upload_metadata_if_exists(part_path, locale).await? {
        let part_size = metadata.len();
        if part_size > file_size {
            return Err(TransferError::conflict(err_upload_conflict(locale)).into());
        }

        let file_name = part_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(FALLBACK_PART_FILE_NAME)
            .to_string();
        let hasher = hash_file_with_keepalives(part_path, part_size, &file_name, frame_writer)
            .await
            .map_err(|e| match e {
                StreamError::Io(_) => ReceiveFileError::Transfer(TransferError::io_error(
                    err_upload_write_failed(locale),
                )),
                e => e.into(),
            })?;
        let hash = hasher.partial_hash();
        return Ok((part_size, Some(hash), hasher, false));
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
        let received = match read_transfer_control_message(frame_reader).await {
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
            StreamError::FrameStarted(e) => ReceiveFileError::FrameStarted(e),
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
        // Control payloads share the header deadline; only a valid keepalive renews it.
        let deadline = Instant::now() + TRANSFER_CONTROL_FRAME_TIMEOUT;
        let h = match timeout_at(deadline, frame_reader.read_frame_header()).await {
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
            "FileData" => {
                return Ok(ClientFileFrame::FileData(h));
            }
            "FileHashing" | "FileHash" => {
                let payload = timeout_at(deadline, frame_reader.read_payload_into_vec(&h))
                    .await
                    .map_err(|_| TransferError::io_error(err_upload_connection_lost(locale)))?
                    .map_err(|_| {
                        TransferError::protocol_error(err_upload_protocol_error(locale))
                    })?;
                let msg: ClientMessage = serde_json::from_slice(&payload).map_err(|_| {
                    TransferError::protocol_error(err_upload_protocol_error(locale))
                })?;
                match (h.message_type.as_str(), msg) {
                    ("FileHashing", ClientMessage::FileHashing { .. }) => continue,
                    ("FileHash", ClientMessage::FileHash { blake3 }) => {
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
    if upload_metadata_if_exists(part_path, locale)
        .await?
        .is_some()
    {
        tokio::fs::rename(part_path, target_path)
            .await
            .map_err(|_| TransferError::io_error(err_upload_write_failed(locale)))?;
    }
    Ok(())
}

/// Discard a failed upload's bytes without destroying an existing resume prefix.
async fn discard_failed_upload_bytes(part_path: &Path, offset: u64) -> io::Result<()> {
    if offset == 0 {
        return tokio::fs::remove_file(part_path).await;
    }

    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(part_path)
        .await?;
    file.set_len(offset).await?;
    file.sync_all().await
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
        .stream_file_from_client(header, &mut hashing_writer, TRANSFER_IO_PROGRESS_TIMEOUT)
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
    use super::super::test_helpers::{
        FailingWriter, WriteFailure, make_authenticated_user, with_slow_polls,
    };
    use super::*;
    use std::fs::File as StdFile;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use nexus_common::hash::StreamingHasher;
    use nexus_common::io::{
        read_transfer_server_message as read_server_message, send_client_message,
    };
    use nexus_common::{
        ERROR_KIND_CAPACITY, ERROR_KIND_CONFLICT, ERROR_KIND_EXISTS, ERROR_KIND_HASH_MISMATCH,
        ERROR_KIND_INVALID, ERROR_KIND_IO_ERROR, ERROR_KIND_PROTOCOL_ERROR,
    };
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::{AsyncWriteExt, BufReader, DuplexStream, ReadHalf, duplex};
    use tokio::sync::mpsc;

    use crate::egress::EGRESS_DISPATCH_QUEUE_CAPACITY;
    use crate::egress::task::{DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY, EgressHandle};
    use crate::files::{FileActivityMap, FileIndex};
    use crate::scheduler::ConnectionId;
    use crate::transfers::registry::{TransferDirection, TransferRegistration, TransferRegistry};
    use crate::transfers::transfer::{TransferContext, TransferEgress};

    const TEST_LOCALE: &str = "en";
    const TEST_CONTROL_PAYLOADS: [(&str, &[u8]); 2] = [
        (
            "FileHash",
            br#"{"type":"FileHash","blake3":"abc123def456"}"#,
        ),
        (
            "FileHashing",
            br#"{"type":"FileHashing","file":"test.txt"}"#,
        ),
    ];

    fn mock_writer() -> FrameWriter<Vec<u8>> {
        FrameWriter::new(Vec::new())
    }

    #[tokio::test(start_paused = true)]
    async fn test_file_start_keepalives_renew_shared_deadline() {
        let (sender, receiver) = duplex(8192);
        let mut reader = FrameReader::new(BufReader::new(receiver));
        let started = Instant::now();
        let read = read_client_file_start(&mut reader, TEST_LOCALE);
        let send = async {
            let mut sender = FrameWriter::new(sender);
            for _ in 0..3 {
                tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
                send_client_message(
                    &mut sender,
                    &ClientMessage::FileHashing {
                        file: "test.txt".into(),
                    },
                )
                .await
                .unwrap();
            }
            tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
            send_client_message(
                &mut sender,
                &ClientMessage::FileStart {
                    path: "test.txt".into(),
                    size: 0,
                },
            )
            .await
            .unwrap();
        };
        let (result, ()) = tokio::time::timeout(TRANSFER_CONTROL_FRAME_TIMEOUT * 5, async {
            tokio::join!(read, send)
        })
        .await
        .expect("complete keepalives must renew the FileStart deadline");
        assert_eq!(result.unwrap(), ("test.txt".into(), 0));
        assert!(started.elapsed() > TRANSFER_CONTROL_FRAME_TIMEOUT);
    }

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7501)
    }

    fn make_upload_transfer<'a, W: AsyncWrite + Unpin>(
        server_read: ReadHalf<DuplexStream>,
        server_write: W,
        file_root: &'a Path,
        shared_root: &'a Path,
        file_index: &'a Arc<FileIndex>,
        file_activity: &'a Arc<FileActivityMap>,
        registry: &'a TransferRegistry,
    ) -> Transfer<'a, BufReader<ReadHalf<DuplexStream>>, W> {
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

    struct UploadTestResult {
        start_state: Option<(u64, Option<String>)>,
        completion: Result<(), String>,
        target: Option<Vec<u8>>,
        part: Option<Vec<u8>>,
    }

    #[tokio::test(start_paused = true)]
    async fn test_upload_hashing_write_failure_preserves_files_and_releases_reservations() {
        for completed in [false, true] {
            for failure in [
                WriteFailure::Error,
                WriteFailure::Zero,
                WriteFailure::Stall,
                WriteFailure::FlushError,
                WriteFailure::FlushStall,
            ] {
                let temp_dir = TempDir::new().unwrap();
                let file_root = temp_dir.path().canonicalize().unwrap();
                let shared_root = file_root.join("shared");
                let destination = shared_root.join("uploads [NEXUS-UL]");
                fs::create_dir_all(&destination).await.unwrap();
                let target = destination.join("file.txt");
                let part = destination.join("file.txt.part");
                if completed {
                    fs::write(&target, b"original").await.unwrap();
                }
                fs::write(&part, b"partial").await.unwrap();
                let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
                let file_activity = Arc::new(FileActivityMap::new());
                let registry = TransferRegistry::new();
                let (client, server) = duplex(8192);
                let (server_read, _server_write) = tokio::io::split(server);
                let mut sender = FrameWriter::new(client);
                for path in ["file.txt", "next.txt"] {
                    send_client_message(
                        &mut sender,
                        &ClientMessage::FileStart {
                            path: path.into(),
                            size: 8,
                        },
                    )
                    .await
                    .unwrap();
                }
                let mut transfer = make_upload_transfer(
                    server_read,
                    FailingWriter::new(Vec::new(), 1, 8, failure),
                    &file_root,
                    &shared_root,
                    &file_index,
                    &file_activity,
                    &registry,
                );
                let delay_enabled = Arc::clone(&transfer.writer().get_ref().failure_armed);
                tokio::time::timeout(
                    TRANSFER_IO_PROGRESS_TIMEOUT * 20,
                    with_slow_polls(
                        handle_upload(
                            &mut transfer,
                            UploadParams {
                                destination: "uploads [NEXUS-UL]".into(),
                                file_count: 2,
                                total_size: 16,
                                root: false,
                            },
                        ),
                        delay_enabled,
                    ),
                )
                .await
                .expect("hashing write failure must terminate the handler")
                .unwrap();
                let writer = transfer.writer().get_ref();
                assert!(writer.shutdown, "{failure:?} / completed={completed}");
                let mut output = FrameReader::new(writer.inner.as_slice());
                assert!(matches!(
                    read_server_message(&mut output)
                        .await
                        .unwrap()
                        .unwrap()
                        .message,
                    ServerMessage::FileUploadResponse { success: true, .. }
                ));
                if matches!(failure, WriteFailure::FlushError | WriteFailure::FlushStall) {
                    let expected_name = if completed {
                        "file.txt"
                    } else {
                        "file.txt.part"
                    };
                    assert!(matches!(
                        read_server_message(&mut output).await.unwrap().unwrap().message,
                        ServerMessage::FileHashing { file } if file == expected_name
                    ));
                    assert!(
                        output.get_ref().is_empty(),
                        "no frame after failed keepalive"
                    );
                } else {
                    // The type-length field distinguishes this from FileStartResponse.
                    assert_eq!(*output.get_ref(), b"NX|11|Fi", "{failure:?}");
                }
                assert_eq!(fs::try_exists(&target).await.unwrap(), completed);
                if completed {
                    assert_eq!(fs::read(&target).await.unwrap(), b"original");
                }
                assert_eq!(fs::read(&part).await.unwrap(), b"partial");
                assert!(!fs::try_exists(destination.join("next.txt")).await.unwrap());
                assert!(
                    !fs::try_exists(destination.join("next.txt.part"))
                        .await
                        .unwrap()
                );
                let _guard = file_activity
                    .try_enter_directory_path(&file_root, &destination)
                    .await
                    .unwrap()
                    .unwrap();
                drop(transfer);
                assert!(registry.snapshot().is_empty());
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_upload_control_write_failure_preserves_files_and_closes() {
        for failure in [
            WriteFailure::Error,
            WriteFailure::Zero,
            WriteFailure::Stall,
            WriteFailure::FlushError,
            WriteFailure::FlushStall,
        ] {
            let temp_dir = TempDir::new().unwrap();
            let file_root = temp_dir.path().canonicalize().unwrap();
            let shared_root = file_root.join("shared");
            let destination = shared_root.join("uploads [NEXUS-UL]");
            fs::create_dir_all(&destination).await.unwrap();
            let target = destination.join("file.txt");
            let part = destination.join("file.txt.part");
            fs::write(&target, b"original").await.unwrap();
            fs::write(&part, b"partial").await.unwrap();
            let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
            let file_activity = Arc::new(FileActivityMap::new());
            let registry = TransferRegistry::new();
            let (client, server) = duplex(8192);
            let (server_read, _server_write) = tokio::io::split(server);
            let mut sender = FrameWriter::new(client);
            send_client_message(
                &mut sender,
                &ClientMessage::FileStart {
                    path: "file.txt".into(),
                    size: 8,
                },
            )
            .await
            .unwrap();
            let mut transfer = make_upload_transfer(
                server_read,
                FailingWriter::new(Vec::new(), 1, 8, failure),
                &file_root,
                &shared_root,
                &file_index,
                &file_activity,
                &registry,
            );
            tokio::time::timeout(
                TRANSFER_IO_PROGRESS_TIMEOUT * 2,
                handle_upload(
                    &mut transfer,
                    UploadParams {
                        destination: "uploads [NEXUS-UL]".into(),
                        file_count: 1,
                        total_size: 8,
                        root: false,
                    },
                ),
            )
            .await
            .expect("must close after the first write timeout")
            .unwrap();
            let writer = transfer.writer().get_ref();
            assert!(writer.shutdown, "{failure:?}");
            let mut output = FrameReader::new(writer.inner.as_slice());
            assert!(matches!(
                read_server_message(&mut output)
                    .await
                    .unwrap()
                    .unwrap()
                    .message,
                ServerMessage::FileUploadResponse { success: true, .. }
            ));
            if matches!(failure, WriteFailure::FlushError | WriteFailure::FlushStall) {
                assert!(matches!(
                    read_server_message(&mut output)
                        .await
                        .unwrap()
                        .unwrap()
                        .message,
                    ServerMessage::FileStartResponse { .. }
                ));
                assert!(
                    output.get_ref().is_empty(),
                    "no TransferComplete after {failure:?}"
                );
            } else {
                assert_eq!(
                    output.get_ref().len(),
                    8,
                    "no bytes after the failed prefix: {failure:?}"
                );
            }
            assert_eq!(fs::read(&target).await.unwrap(), b"original");
            assert_eq!(fs::read(&part).await.unwrap(), b"partial");
            let _guard = file_activity
                .try_enter_directory_path(&file_root, &destination)
                .await
                .unwrap()
                .unwrap();
            drop(transfer);
            assert!(registry.snapshot().is_empty());
        }
    }

    fn hash_bytes(data: &[u8]) -> String {
        let mut hasher = StreamingHasher::new();
        hasher.update(data);
        hasher.finalize()
    }

    // None sends only FileHash; Some(offset) sends FileData from that offset first.
    async fn upload_with_existing_files(
        existing_target: Option<&[u8]>,
        existing_part: Option<&[u8]>,
        upload_data: &[u8],
        data_offset: Option<usize>,
    ) -> UploadTestResult {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let target_path = destination.join("file.txt");
        let part_path = destination.join("file.txt.part");
        let target_modified = if let Some(data) = existing_target {
            fs::write(&target_path, data).await.unwrap();
            // An old timestamp makes even an empty-to-empty rewrite observable.
            let file = StdFile::options().write(true).open(&target_path).unwrap();
            file.set_modified(SystemTime::UNIX_EPOCH).unwrap();
            Some(file.metadata().unwrap().modified().unwrap())
        } else {
            None
        };
        if let Some(data) = existing_part {
            fs::write(&part_path, data).await.unwrap();
        }

        let result = upload_to_test_directory(
            &file_root,
            upload_data,
            data_offset,
            hash_bytes(upload_data),
        )
        .await;

        if let Some(modified) = target_modified {
            assert_eq!(
                fs::metadata(&target_path)
                    .await
                    .unwrap()
                    .modified()
                    .unwrap(),
                modified,
                "existing completed destination must not be rewritten"
            );
        }

        result
    }

    async fn upload_to_test_directory(
        file_root: &Path,
        upload_data: &[u8],
        data_offset: Option<usize>,
        client_hash: String,
    ) -> UploadTestResult {
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let target_path = destination.join("file.txt");
        let part_path = destination.join("file.txt.part");
        let file_index = Arc::new(FileIndex::new(file_root, file_root));
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
            file_root,
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
                    total_size: upload_data.len() as u64,
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
            assert!(matches!(
                initial,
                ServerMessage::FileUploadResponse { success: true, .. }
            ));

            send_client_message(
                &mut client_writer,
                &ClientMessage::FileStart {
                    path: "file.txt".to_string(),
                    size: upload_data.len() as u64,
                },
            )
            .await
            .unwrap();

            let mut start_state = None;
            loop {
                let response = read_server_message(&mut client_reader)
                    .await
                    .unwrap()
                    .unwrap()
                    .message;
                match response {
                    ServerMessage::FileHashing { .. } => continue,
                    ServerMessage::FileStartResponse { size, blake3 } => {
                        assert!(start_state.is_none(), "unexpected second FileStartResponse");
                        start_state = Some((size, blake3));
                        if let Some(offset) = data_offset {
                            client_writer
                                .get_mut()
                                .write_all(&build_frame("FileData", &upload_data[offset..]))
                                .await
                                .unwrap();
                        }
                        send_client_message(
                            &mut client_writer,
                            &ClientMessage::FileHash {
                                blake3: client_hash.clone(),
                            },
                        )
                        .await
                        .unwrap();
                    }
                    ServerMessage::TransferComplete {
                        success,
                        error,
                        error_kind,
                    } => {
                        assert_eq!(success, error.is_none());
                        let completion = if success {
                            assert!(error_kind.is_none());
                            Ok(())
                        } else {
                            Err(error_kind.expect("failed upload must report an error kind"))
                        };
                        break (start_state, completion);
                    }
                    other => panic!("Unexpected upload response: {other:?}"),
                }
            }
        };

        let ((), (start_state, completion)) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(server, client)
        })
        .await
        .expect("upload should complete");

        UploadTestResult {
            start_state,
            completion,
            target: if fs::try_exists(&target_path).await.unwrap() {
                Some(fs::read(&target_path).await.unwrap())
            } else {
                None
            },
            part: if fs::try_exists(&part_path).await.unwrap() {
                Some(fs::read(&part_path).await.unwrap())
            } else {
                None
            },
        }
    }

    #[tokio::test]
    async fn test_upload_rejects_hash_only_replacement_with_part_collision() {
        let original = b"original";
        let replacement = b"replaced";
        assert_eq!(original.len(), replacement.len());

        let result =
            upload_with_existing_files(Some(original), Some(replacement), replacement, None).await;

        assert_eq!(
            result.start_state,
            Some((original.len() as u64, Some(hash_bytes(original))))
        );
        assert_eq!(result.completion, Err(ERROR_KIND_HASH_MISMATCH.to_string()));
        assert_eq!(result.target.as_deref(), Some(original.as_slice()));
        assert_eq!(result.part.as_deref(), Some(replacement.as_slice()));
    }

    #[tokio::test]
    async fn test_upload_rejects_streamed_replacement_with_part_collision() {
        let original = b"original";
        let replacement = b"replaced";
        let part = &replacement[..3];
        assert_eq!(original.len(), replacement.len());

        for offset in [0, part.len()] {
            let result =
                upload_with_existing_files(Some(original), Some(part), replacement, Some(offset))
                    .await;

            assert_eq!(
                result.start_state,
                Some((original.len() as u64, Some(hash_bytes(original))))
            );
            assert_eq!(result.completion, Err(ERROR_KIND_EXISTS.to_string()));
            assert_eq!(result.target.as_deref(), Some(original.as_slice()));
            assert_eq!(result.part.as_deref(), Some(part));
        }
    }

    #[tokio::test]
    async fn test_upload_rejects_zero_byte_truncation_with_part_collision() {
        let original = b"original";
        let part = b"leftover";

        let result = upload_with_existing_files(Some(original), Some(part), b"", None).await;

        assert!(result.start_state.is_none());
        assert_eq!(result.completion, Err(ERROR_KIND_EXISTS.to_string()));
        assert_eq!(result.target.as_deref(), Some(original.as_slice()));
        assert_eq!(result.part.as_deref(), Some(part.as_slice()));
    }

    #[tokio::test]
    async fn test_upload_rejects_different_size_with_part_collision() {
        let original = b"original";
        let replacement = b"a longer replacement";

        let result =
            upload_with_existing_files(Some(original), Some(replacement), replacement, None).await;

        assert!(result.start_state.is_none());
        assert_eq!(result.completion, Err(ERROR_KIND_EXISTS.to_string()));
        assert_eq!(result.target.as_deref(), Some(original.as_slice()));
        assert_eq!(result.part.as_deref(), Some(replacement.as_slice()));
    }

    #[tokio::test]
    async fn test_upload_duplicate_preserves_completed_file_and_leftover_part() {
        let original = b"original";
        let part = b"leftover";

        let result = upload_with_existing_files(Some(original), Some(part), original, None).await;

        assert_eq!(
            result.start_state,
            Some((original.len() as u64, Some(hash_bytes(original))))
        );
        assert_eq!(result.completion, Ok(()));
        assert_eq!(result.target.as_deref(), Some(original.as_slice()));
        assert_eq!(result.part.as_deref(), Some(part.as_slice()));
    }

    #[tokio::test]
    async fn test_upload_empty_duplicate_preserves_leftover_part() {
        let part = b"leftover";

        let result = upload_with_existing_files(Some(b""), Some(part), b"", None).await;

        assert_eq!(result.start_state, Some((0, None)));
        assert_eq!(result.completion, Ok(()));
        assert_eq!(result.target.as_deref(), Some(b"".as_slice()));
        assert_eq!(result.part.as_deref(), Some(part.as_slice()));
    }

    #[tokio::test]
    async fn test_upload_resumes_partial_file_without_completed_destination() {
        let data = b"resumed file content";
        let part = &data[..7];

        let result = upload_with_existing_files(None, Some(part), data, Some(part.len())).await;

        assert_eq!(
            result.start_state,
            Some((part.len() as u64, Some(hash_bytes(part))))
        );
        assert_eq!(result.completion, Ok(()));
        assert_eq!(result.target.as_deref(), Some(data.as_slice()));
        assert!(result.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_finalizes_complete_part_without_completed_destination() {
        let data = b"completed partial file";

        let result = upload_with_existing_files(None, Some(data), data, None).await;

        assert_eq!(
            result.start_state,
            Some((data.len() as u64, Some(hash_bytes(data))))
        );
        assert_eq!(result.completion, Ok(()));
        assert_eq!(result.target.as_deref(), Some(data.as_slice()));
        assert!(result.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_hash_only_incomplete_part_rejects_and_allows_resume() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let destination = file_root.join("shared/uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let data = b"original prefix and remaining content";
        let prefix = &data[..15];
        fs::write(destination.join("file.txt.part"), prefix)
            .await
            .unwrap();

        // Missing data is a protocol error regardless of whether the hash matches the prefix.
        for client_hash in [hash_bytes(prefix), hash_bytes(data)] {
            let rejected = upload_to_test_directory(&file_root, data, None, client_hash).await;

            assert_eq!(
                rejected.start_state,
                Some((prefix.len() as u64, Some(hash_bytes(prefix))))
            );
            assert_eq!(
                rejected.completion,
                Err(ERROR_KIND_PROTOCOL_ERROR.to_string())
            );
            assert!(rejected.target.is_none());
            assert_eq!(rejected.part.as_deref(), Some(prefix));
        }

        let retried =
            upload_to_test_directory(&file_root, data, Some(prefix.len()), hash_bytes(data)).await;

        assert_eq!(
            retried.start_state,
            Some((prefix.len() as u64, Some(hash_bytes(prefix))))
        );
        assert_eq!(retried.completion, Ok(()));
        assert_eq!(retried.target.as_deref(), Some(data.as_slice()));
        assert!(retried.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_hash_only_nonempty_file_rejects_missing_or_empty_part() {
        for existing_part in [None, Some(b"".as_slice())] {
            let temp_dir = TempDir::new().unwrap();
            let file_root = temp_dir.path().canonicalize().unwrap();
            let destination = file_root.join("shared/uploads [NEXUS-UL]");
            fs::create_dir_all(&destination).await.unwrap();
            if let Some(part) = existing_part {
                fs::write(destination.join("file.txt.part"), part)
                    .await
                    .unwrap();
            }

            let rejected =
                upload_to_test_directory(&file_root, b"declared content", None, hash_bytes(b""))
                    .await;

            assert_eq!(
                rejected.start_state,
                Some((0, existing_part.map(hash_bytes)))
            );
            assert_eq!(
                rejected.completion,
                Err(ERROR_KIND_PROTOCOL_ERROR.to_string())
            );
            assert!(rejected.target.is_none());
            assert_eq!(rejected.part.as_deref(), existing_part);
        }
    }

    #[tokio::test]
    async fn test_upload_hash_only_complete_part_hash_mismatch_allows_retry() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let destination = file_root.join("shared/uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let data = b"completed partial file";
        fs::write(destination.join("file.txt.part"), data)
            .await
            .unwrap();

        let rejected =
            upload_to_test_directory(&file_root, data, None, hash_bytes(b"different content"))
                .await;

        assert_eq!(
            rejected.start_state,
            Some((data.len() as u64, Some(hash_bytes(data))))
        );
        assert_eq!(
            rejected.completion,
            Err(ERROR_KIND_HASH_MISMATCH.to_string())
        );
        assert!(rejected.target.is_none());
        assert_eq!(rejected.part.as_deref(), Some(data.as_slice()));

        let retried = upload_to_test_directory(&file_root, data, None, hash_bytes(data)).await;

        assert_eq!(retried.start_state, rejected.start_state);
        assert_eq!(retried.completion, Ok(()));
        assert_eq!(retried.target.as_deref(), Some(data.as_slice()));
        assert!(retried.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_hash_only_empty_hash_mismatch_preserves_state_and_allows_retry() {
        let empty = b"".as_slice();
        for (existing_target, existing_part) in [
            (None, None),
            (None, Some(empty)),
            (Some(empty), None),
            (Some(empty), Some(b"leftover".as_slice())),
        ] {
            let temp_dir = TempDir::new().unwrap();
            let file_root = temp_dir.path().canonicalize().unwrap();
            let destination = file_root.join("shared/uploads [NEXUS-UL]");
            fs::create_dir_all(&destination).await.unwrap();
            if let Some(target) = existing_target {
                fs::write(destination.join("file.txt"), target)
                    .await
                    .unwrap();
            }
            if let Some(part) = existing_part {
                fs::write(destination.join("file.txt.part"), part)
                    .await
                    .unwrap();
            }
            let expected_hash = if existing_target.is_some() {
                None
            } else {
                existing_part.map(hash_bytes)
            };

            let rejected =
                upload_to_test_directory(&file_root, empty, None, hash_bytes(b"nonempty content"))
                    .await;

            assert_eq!(rejected.start_state, Some((0, expected_hash)));
            assert_eq!(
                rejected.completion,
                Err(ERROR_KIND_HASH_MISMATCH.to_string())
            );
            assert_eq!(rejected.target.as_deref(), existing_target);
            assert_eq!(rejected.part.as_deref(), existing_part);

            let retried =
                upload_to_test_directory(&file_root, empty, None, hash_bytes(empty)).await;

            assert_eq!(retried.start_state, rejected.start_state);
            assert_eq!(retried.completion, Ok(()));
            assert_eq!(retried.target.as_deref(), Some(empty));
            assert_eq!(retried.part.as_deref(), existing_part);
        }
    }

    #[tokio::test]
    async fn test_upload_creates_new_empty_file() {
        let result = upload_with_existing_files(None, None, b"", None).await;

        assert_eq!(result.start_state, Some((0, None)));
        assert_eq!(result.completion, Ok(()));
        assert_eq!(result.target.as_deref(), Some(b"".as_slice()));
        assert!(result.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_partial_hash_failure_reports_io_error_before_start() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let destination = shared_root.join("uploads [NEXUS-UL]");
        let target = destination.join("file.txt");
        let part = destination.join("file.txt.part");
        // Metadata succeeds, but hashing a directory fails without relying on permissions.
        fs::create_dir_all(&part).await.unwrap();
        let preserved = part.join("existing-data");
        fs::write(&preserved, b"preserve me").await.unwrap();
        let file_size = fs::metadata(&part).await.unwrap().len();
        assert!(
            hash_file_with_keepalives(&part, file_size, "file.txt.part", &mut mock_writer())
                .await
                .is_err(),
            "fixture must fail during hashing, not metadata lookup"
        );

        let file_index = Arc::new(FileIndex::new(&file_root, &file_root));
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

        send_client_message(
            &mut client_writer,
            &ClientMessage::FileStart {
                path: "file.txt".to_string(),
                size: file_size,
            },
        )
        .await
        .unwrap();
        tokio::time::timeout(
            Duration::from_secs(2),
            handle_upload(
                &mut transfer,
                UploadParams {
                    destination: "uploads [NEXUS-UL]".to_string(),
                    file_count: 1,
                    total_size: file_size,
                    root: false,
                },
            ),
        )
        .await
        .expect("upload must fail without waiting for file data")
        .unwrap();

        let initial = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        assert!(matches!(
            initial,
            ServerMessage::FileUploadResponse { success: true, .. }
        ));
        let response = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        assert!(matches!(
            response,
            ServerMessage::TransferComplete {
                success: false,
                error: Some(_),
                error_kind: Some(kind),
            } if kind == ERROR_KIND_IO_ERROR
        ));
        assert!(!fs::try_exists(&target).await.unwrap());
        assert_eq!(fs::read(&preserved).await.unwrap(), b"preserve me");
    }

    #[tokio::test]
    async fn test_upload_smaller_than_partial_file_rejects_and_allows_retry() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let destination = file_root.join("shared/uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let data = b"original upload content";
        let prefix = &data[..9];
        fs::write(destination.join("file.txt.part"), prefix)
            .await
            .unwrap();

        for file_size in [0, prefix.len() - 1] {
            // Even echoing the partial file's hash cannot complete a smaller upload.
            let rejected =
                upload_to_test_directory(&file_root, &data[..file_size], None, hash_bytes(prefix))
                    .await;

            assert!(rejected.start_state.is_none());
            assert_eq!(rejected.completion, Err(ERROR_KIND_CONFLICT.to_string()));
            assert!(rejected.target.is_none());
            assert_eq!(rejected.part.as_deref(), Some(prefix));
        }

        let retried =
            upload_to_test_directory(&file_root, data, Some(prefix.len()), hash_bytes(data)).await;

        assert_eq!(
            retried.start_state,
            Some((prefix.len() as u64, Some(hash_bytes(prefix))))
        );
        assert_eq!(retried.completion, Ok(()));
        assert_eq!(retried.target.as_deref(), Some(data.as_slice()));
        assert!(retried.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_empty_file_with_empty_part_succeeds() {
        let result = upload_with_existing_files(None, Some(b""), b"", None).await;

        assert_eq!(result.start_state, Some((0, Some(hash_bytes(b"")))));
        assert_eq!(result.completion, Ok(()));
        assert_eq!(result.target.as_deref(), Some(b"".as_slice()));
        assert_eq!(result.part.as_deref(), Some(b"".as_slice()));
    }

    #[tokio::test]
    async fn test_upload_hash_mismatch_restores_prefix_and_allows_retry() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let destination = file_root.join("shared/uploads [NEXUS-UL]");
        fs::create_dir_all(&destination).await.unwrap();
        let data = b"original prefix and valid suffix";
        let prefix = &data[..16];
        fs::write(destination.join("file.txt.part"), prefix)
            .await
            .unwrap();

        // The prefix matches, but bytes appended by this attempt do not match its final hash.
        let mut corrupt_data = data.to_vec();
        *corrupt_data.last_mut().unwrap() ^= 1;
        assert_eq!(&corrupt_data[..prefix.len()], prefix);
        assert_ne!(hash_bytes(&corrupt_data), hash_bytes(data));

        let failed = upload_to_test_directory(
            &file_root,
            &corrupt_data,
            Some(prefix.len()),
            hash_bytes(data),
        )
        .await;

        assert_eq!(
            failed.start_state,
            Some((prefix.len() as u64, Some(hash_bytes(prefix))))
        );
        assert_eq!(failed.completion, Err(ERROR_KIND_HASH_MISMATCH.to_string()));
        assert!(failed.target.is_none());
        assert_eq!(failed.part.as_deref(), Some(prefix));

        let retried =
            upload_to_test_directory(&file_root, data, Some(prefix.len()), hash_bytes(data)).await;

        assert_eq!(retried.start_state, failed.start_state);
        assert_eq!(retried.completion, Ok(()));
        assert_eq!(retried.target.as_deref(), Some(data.as_slice()));
        assert!(retried.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_hash_mismatch_removes_fresh_partial_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let data = b"new upload content";

        let result =
            upload_to_test_directory(&file_root, data, Some(0), hash_bytes(b"different content"))
                .await;

        assert_eq!(result.start_state, Some((0, None)));
        assert_eq!(result.completion, Err(ERROR_KIND_HASH_MISMATCH.to_string()));
        assert!(result.target.is_none());
        assert!(result.part.is_none());
    }

    #[tokio::test]
    async fn test_upload_restart_conflict_preserves_partial_file() {
        let prefix = b"original prefix";
        let data = b"different upload content";
        assert_ne!(hash_bytes(prefix), hash_bytes(&data[..prefix.len()]));

        let result = upload_with_existing_files(None, Some(prefix), data, Some(0)).await;

        assert_eq!(
            result.start_state,
            Some((prefix.len() as u64, Some(hash_bytes(prefix))))
        );
        assert_eq!(result.completion, Err(ERROR_KIND_CONFLICT.to_string()));
        assert!(result.target.is_none());
        assert_eq!(result.part.as_deref(), Some(prefix.as_slice()));
    }

    #[tokio::test]
    async fn test_discard_failed_upload_bytes_missing_file_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let part_path = temp_dir.path().join("missing.part");

        for offset in [0, 4] {
            let err = discard_failed_upload_bytes(&part_path, offset)
                .await
                .unwrap_err();

            assert_eq!(err.kind(), io::ErrorKind::NotFound);
            assert!(!fs::try_exists(&part_path).await.unwrap());
        }
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
                Err(ReceiveFileError::FrameStarted(err)) => {
                    panic!("upload receive should succeed, got write failure: {err}")
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
                blake3: hash_bytes(b""),
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
    async fn test_conflicts_target_metadata_error_does_not_fall_back_to_part() {
        let temp_dir = TempDir::new().unwrap();
        // An embedded NUL causes an error even when tests run with elevated privileges.
        let target = temp_dir.path().join("invalid\0.txt");
        let part = temp_dir.path().join("existing.txt.part");
        fs::write(&part, b"partial data").await.unwrap();
        let mut writer = mock_writer();

        for file_size in [0, 100] {
            let err = check_upload_conflicts_and_get_state(
                &mut writer,
                &target,
                &part,
                file_size,
                TEST_LOCALE,
            )
            .await
            .unwrap_err();

            assert!(
                matches!(err, ReceiveFileError::Transfer(err) if err.kind == ERROR_KIND_IO_ERROR)
            );
        }
        assert_eq!(fs::read(&part).await.unwrap(), b"partial data");
        assert!(writer.get_mut().is_empty());
    }

    #[tokio::test]
    async fn test_conflicts_part_metadata_error_does_not_report_empty_state() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("newfile.txt");
        let part = temp_dir.path().join("invalid\0.part");
        let mut writer = mock_writer();

        for file_size in [0, 100] {
            let err = check_upload_conflicts_and_get_state(
                &mut writer,
                &target,
                &part,
                file_size,
                TEST_LOCALE,
            )
            .await
            .unwrap_err();

            assert!(
                matches!(err, ReceiveFileError::Transfer(err) if err.kind == ERROR_KIND_IO_ERROR)
            );
        }
        assert!(!fs::try_exists(&target).await.unwrap());
        assert!(writer.get_mut().is_empty());
    }

    #[tokio::test]
    async fn test_conflicts_completed_file_does_not_inspect_leftover_part() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("existing.txt");
        let part = temp_dir.path().join("invalid\0.part");
        let content = b"completed data";
        fs::write(&target, content).await.unwrap();
        let mut writer = mock_writer();

        let (size, hash, hasher, complete_exists) = check_upload_conflicts_and_get_state(
            &mut writer,
            &target,
            &part,
            content.len() as u64,
            TEST_LOCALE,
        )
        .await
        .unwrap();

        assert_eq!(size, content.len() as u64);
        assert_eq!(hash, Some(hash_bytes(content)));
        assert_eq!(hasher.finalize(), hash_bytes(content));
        assert!(complete_exists);
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
        assert!(
            matches!(err, ReceiveFileError::Transfer(err) if err.kind == nexus_common::ERROR_KIND_EXISTS)
        );
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
    async fn test_finalize_part_file_metadata_error_does_not_report_success() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("final.txt");
        let part = temp_dir.path().join("invalid\0.part");

        let err = finalize_part_file_if_exists(&part, &target, TEST_LOCALE)
            .await
            .unwrap_err();

        assert_eq!(err.kind, ERROR_KIND_IO_ERROR);
        assert!(!fs::try_exists(&target).await.unwrap());
    }

    #[tokio::test]
    async fn test_finalize_part_file_rename_error_preserves_partial_data() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("final.txt");
        let part = temp_dir.path().join("final.txt.part");
        fs::create_dir(&target).await.unwrap();
        fs::write(&part, b"complete content").await.unwrap();

        let err = finalize_part_file_if_exists(&part, &target, TEST_LOCALE)
            .await
            .unwrap_err();

        assert_eq!(err.kind, ERROR_KIND_IO_ERROR);
        assert!(fs::metadata(&target).await.unwrap().is_dir());
        assert_eq!(fs::read(&part).await.unwrap(), b"complete content");
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

    #[tokio::test(start_paused = true)]
    async fn test_read_control_frame_stalls_time_out() {
        for (type_name, payload) in TEST_CONTROL_PAYLOADS {
            let frame = build_frame(type_name, payload);
            let header_len = frame.len() - payload.len() - 1;
            for sent_len in [
                0,
                1,
                header_len,
                header_len + payload.len() / 2,
                frame.len() - 1,
            ] {
                let (mut sender, receiver) = duplex(8192);
                sender.write_all(&frame[..sent_len]).await.unwrap();
                let mut reader = FrameReader::new(BufReader::new(receiver));
                let started = Instant::now();

                let err = tokio::time::timeout(
                    TRANSFER_CONTROL_FRAME_TIMEOUT * 2,
                    read_file_data_or_file_hash(&mut reader, TEST_LOCALE),
                )
                .await
                .expect("control frame must time out even with its connection still open")
                .unwrap_err();

                assert_eq!(
                    err.kind, ERROR_KIND_IO_ERROR,
                    "{type_name}, {sent_len} bytes"
                );
                assert!(started.elapsed() >= TRANSFER_CONTROL_FRAME_TIMEOUT);
                assert!(started.elapsed() < TRANSFER_CONTROL_FRAME_TIMEOUT * 2);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_read_control_frame_shares_deadline_between_header_and_payload() {
        for (type_name, payload) in TEST_CONTROL_PAYLOADS {
            let frame = build_frame(type_name, payload);
            let header_len = frame.len() - payload.len() - 1;
            let (mut sender, receiver) = duplex(8192);
            let mut reader = FrameReader::new(BufReader::new(receiver));

            let read = async {
                let started = Instant::now();
                let result = tokio::time::timeout(
                    TRANSFER_CONTROL_FRAME_TIMEOUT * 2,
                    read_file_data_or_file_hash(&mut reader, TEST_LOCALE),
                )
                .await
                .expect("partial payload progress must not renew the frame deadline");
                (result, started.elapsed())
            };
            let send = async {
                tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT / 2).await;
                sender.write_all(&frame[..header_len]).await.unwrap();
                tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT / 4).await;
                sender
                    .write_all(&frame[header_len..header_len + 1])
                    .await
                    .unwrap();
                tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT / 2).await;
                sender.write_all(&frame[header_len + 1..]).await.unwrap();
            };

            let ((result, elapsed), ()) = tokio::join!(read, send);

            assert_eq!(result.unwrap_err().kind, ERROR_KIND_IO_ERROR, "{type_name}");
            assert!(elapsed >= TRANSFER_CONTROL_FRAME_TIMEOUT);
            assert!(elapsed < TRANSFER_CONTROL_FRAME_TIMEOUT + TRANSFER_CONTROL_FRAME_TIMEOUT / 4);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_read_complete_keepalives_renew_frame_deadline() {
        let (mut sender, receiver) = duplex(8192);
        let mut reader = FrameReader::new(BufReader::new(receiver));
        let keepalive = build_frame("FileHashing", TEST_CONTROL_PAYLOADS[1].1);
        let hash = build_frame("FileHash", TEST_CONTROL_PAYLOADS[0].1);
        let started = Instant::now();

        let read = read_file_data_or_file_hash(&mut reader, TEST_LOCALE);
        let send = async {
            for _ in 0..3 {
                tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
                sender.write_all(&keepalive).await.unwrap();
            }
            tokio::time::sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
            sender.write_all(&hash).await.unwrap();
        };
        let (result, ()) = tokio::time::timeout(TRANSFER_CONTROL_FRAME_TIMEOUT * 5, async {
            tokio::join!(read, send)
        })
        .await
        .expect("complete keepalives must allow a long hashing phase");

        assert!(matches!(
            result.unwrap(),
            ClientFileFrame::FileHash { blake3 } if blake3 == "abc123def456"
        ));
        assert!(started.elapsed() > TRANSFER_CONTROL_FRAME_TIMEOUT);
    }

    #[tokio::test]
    async fn test_read_control_frame_errors_remain_protocol_errors() {
        for (type_name, payload) in TEST_CONTROL_PAYLOADS {
            for terminator in [None, Some(b'!')] {
                let mut frame = build_frame(type_name, payload);
                frame.pop();
                if let Some(terminator) = terminator {
                    frame.push(terminator);
                }
                let mut reader = FrameReader::new(std::io::Cursor::new(frame));

                let err = read_file_data_or_file_hash(&mut reader, TEST_LOCALE)
                    .await
                    .unwrap_err();

                assert_eq!(err.kind, ERROR_KIND_PROTOCOL_ERROR);
            }
        }

        let frame = build_frame("FileHash", b"not JSON");
        let mut reader = FrameReader::new(std::io::Cursor::new(frame));
        let err = read_file_data_or_file_hash(&mut reader, TEST_LOCALE)
            .await
            .unwrap_err();
        assert_eq!(err.kind, ERROR_KIND_PROTOCOL_ERROR);
    }

    #[tokio::test]
    async fn test_read_keepalive_rejects_invalid_payload_without_consuming_next_frame() {
        for (type_name, payload) in [
            ("FileHashing", &b"not JSON"[..]),
            ("FileHashing", &b""[..]),
            ("FileHashing", &br#"{"type":"FileHashing"}"#[..]),
            ("FileHashing", &br#"{"type":"FileHashing","file":42}"#[..]),
            ("FileHashing", &br#"{"type":"Ping"}"#[..]),
            ("FileHashing", TEST_CONTROL_PAYLOADS[0].1),
            ("FileHash", TEST_CONTROL_PAYLOADS[1].1),
        ] {
            let mut frames = build_frame(type_name, payload);
            frames.extend_from_slice(&build_frame("FileHash", TEST_CONTROL_PAYLOADS[0].1));
            let mut reader = FrameReader::new(std::io::Cursor::new(frames));
            let err = read_file_data_or_file_hash(&mut reader, TEST_LOCALE)
                .await
                .unwrap_err();
            assert_eq!(err.kind, ERROR_KIND_PROTOCOL_ERROR, "{type_name}");
            assert!(matches!(
                read_file_data_or_file_hash(&mut reader, TEST_LOCALE).await.unwrap(),
                ClientFileFrame::FileHash { blake3 } if blake3 == "abc123def456"
            ));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_read_malformed_keepalive_does_not_renew_deadline() {
        let (mut sender, receiver) = duplex(8192);
        let mut reader = FrameReader::new(BufReader::new(receiver));
        let delay = TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4;
        let started = Instant::now();
        let read = read_file_data_or_file_hash(&mut reader, TEST_LOCALE);
        let send = async {
            tokio::time::sleep(delay).await;
            sender
                .write_all(&build_frame("FileHashing", b"not JSON"))
                .await
                .unwrap();
            // Keep the connection open so skipping the payload would renew the wait.
            sender
        };
        let (result, _sender) = tokio::time::timeout(TRANSFER_CONTROL_FRAME_TIMEOUT * 2, async {
            tokio::join!(read, send)
        })
        .await
        .expect("malformed keepalive must be rejected immediately");
        assert_eq!(result.unwrap_err().kind, ERROR_KIND_PROTOCOL_ERROR);
        assert_eq!(started.elapsed(), delay);
    }

    #[tokio::test(start_paused = true)]
    async fn test_read_file_data_keeps_progress_timeout_for_long_streams() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = Arc::new(FileIndex::new(file_root, file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let (mut client, server) = duplex(8192);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_upload_transfer(
            server_read,
            server_write,
            file_root,
            file_root,
            &file_index,
            &file_activity,
            &registry,
        );
        let data = b"hello";
        let frame = build_frame("FileData", data);
        let header_len = frame.len() - data.len() - 1;
        client.write_all(&frame[..header_len]).await.unwrap();

        let header = tokio::time::timeout(
            TRANSFER_CONTROL_FRAME_TIMEOUT / 4,
            read_file_data_or_file_hash(transfer.reader(), TEST_LOCALE),
        )
        .await
        .expect("FileData dispatch must not wait for payload bytes")
        .unwrap();
        let ClientFileFrame::FileData(header) = header else {
            panic!("expected FileData header");
        };

        let mut output = Vec::new();
        let started = Instant::now();
        let read =
            transfer.stream_file_from_client(&header, &mut output, TRANSFER_IO_PROGRESS_TIMEOUT);
        let send = async {
            for byte in data {
                tokio::time::sleep(TRANSFER_IO_PROGRESS_TIMEOUT * 3 / 4).await;
                client.write_all(&[*byte]).await.unwrap();
            }
            client.write_all(b"\n").await.unwrap();
        };
        let (result, ()) = tokio::time::timeout(TRANSFER_IO_PROGRESS_TIMEOUT * 6, async {
            tokio::join!(read, send)
        })
        .await
        .expect("streaming must continue while each chunk makes progress");

        assert_eq!(result.unwrap(), data.len() as u64);
        assert_eq!(output, data);
        assert!(started.elapsed() > TRANSFER_CONTROL_FRAME_TIMEOUT);
    }

    #[tokio::test]
    async fn test_upload_control_timeout_preserves_part_and_releases_reservations() {
        for (type_name, payload) in TEST_CONTROL_PAYLOADS {
            let temp_dir = TempDir::new().unwrap();
            let file_root = temp_dir.path().canonicalize().unwrap();
            let shared_root = file_root.join("shared");
            let destination = shared_root.join("uploads [NEXUS-UL]");
            fs::create_dir_all(&destination).await.unwrap();
            let target = destination.join("file.txt");
            let part = destination.join("file.txt.part");
            let data = b"original prefix and remaining content";
            let prefix = &data[..15];
            fs::write(&part, prefix).await.unwrap();
            let file_index = Arc::new(FileIndex::new(&file_root, &file_root));
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

            let server = handle_upload(
                &mut transfer,
                UploadParams {
                    destination: "uploads [NEXUS-UL]".to_string(),
                    file_count: 1,
                    total_size: data.len() as u64,
                    root: false,
                },
            );
            let client = async {
                let initial = read_server_message(&mut client_reader)
                    .await
                    .unwrap()
                    .unwrap()
                    .message;
                assert!(matches!(
                    initial,
                    ServerMessage::FileUploadResponse { success: true, .. }
                ));
                send_client_message(
                    &mut client_writer,
                    &ClientMessage::FileStart {
                        path: "file.txt".to_string(),
                        size: data.len() as u64,
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
                    ServerMessage::FileStartResponse { size, blake3 }
                        if size == prefix.len() as u64 && blake3 == Some(hash_bytes(prefix))
                ));
                assert_eq!(registry.snapshot().len(), 1);
                for path in [&target, &part] {
                    assert!(
                        file_activity
                            .try_enter_child_path(&file_root, path)
                            .await
                            .unwrap()
                            .is_err()
                    );
                }

                // Pause only after filesystem setup, so blocking file work cannot advance time.
                tokio::time::pause();
                let frame = build_frame(type_name, payload);
                client_writer
                    .get_mut()
                    .write_all(&frame[..frame.len() - 1])
                    .await
                    .unwrap();
                let response = read_server_message(&mut client_reader)
                    .await
                    .unwrap()
                    .unwrap()
                    .message;
                tokio::time::resume();
                assert!(matches!(
                    response,
                    ServerMessage::TransferComplete {
                        success: false,
                        error: Some(_),
                        error_kind: Some(kind),
                    } if kind == ERROR_KIND_IO_ERROR
                ));
                assert!(
                    read_server_message(&mut client_reader)
                        .await
                        .unwrap()
                        .is_none()
                );
            };
            let (result, ()) = tokio::time::timeout(TRANSFER_CONTROL_FRAME_TIMEOUT * 3, async {
                tokio::join!(server, client)
            })
            .await
            .expect("upload must close after an incomplete control frame");
            result.unwrap();
            drop(transfer);

            assert!(registry.snapshot().is_empty());
            assert!(!fs::try_exists(&target).await.unwrap());
            assert_eq!(fs::read(&part).await.unwrap(), prefix);
            let _reservation = file_activity
                .try_enter_directory_path(&file_root, &destination)
                .await
                .unwrap()
                .expect("upload must release its file and directory reservations");
        }
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
