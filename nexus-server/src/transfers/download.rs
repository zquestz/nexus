//! File download handling for transfers.
//!
//! Single-pass StreamingHasher during streaming; resume verification computes
//! intermediate hashes via clone-and-finalize, with FileHashing keepalives during
//! hash-only phases to prevent timeouts.

use std::io;
use std::path::Path;

use tokio::fs::File;
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufReader, SeekFrom,
};
use tracing::{debug, error, info, warn};

use nexus_common::ERROR_KIND_IO_ERROR;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::hash::StreamingHasher;
use nexus_common::io::read_client_message_with_full_timeout;
use nexus_common::names::fold_name;
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::constants::*;
use crate::db::Permission;
use crate::files::folder_type::{FolderType, parse_folder_type};
use crate::files::path::resolve_path;
use crate::handlers::{
    err_source_busy, err_transfer_access_denied, err_transfer_file_failed, err_transfer_read_failed,
};

use super::hashing::{FALLBACK_FILE_NAME, HashingReader, hash_file_with_keepalives};
use super::helpers::{
    TransferError, build_validated_path, check_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_download_error_and_close, send_download_transfer_error, validate_transfer_path,
};
use super::transfer::{StreamError, Transfer};
use super::types::{DownloadParams, FileInfo};

pub(crate) async fn handle_download<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    params: DownloadParams,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let DownloadParams {
        path: download_path,
        root: use_root,
    } = params;

    let locale = transfer.locale().to_string();
    let username = transfer.user().username.clone();
    let is_admin = transfer.user().is_admin;
    let peer_addr = transfer.peer_addr();

    let resolved_path =
        match validate_and_resolve_download_path(transfer, &locale, &download_path, use_root).await
        {
            Ok(path) => path,
            Err(e) => return send_download_transfer_error(transfer.writer(), &e).await,
        };

    if !can_access_for_download(&resolved_path, &username, is_admin) {
        let err = TransferError::permission(err_transfer_access_denied(&locale));
        return send_download_transfer_error(transfer.writer(), &err).await;
    }

    let _scan_activity_guard = match tokio::fs::metadata(&resolved_path).await {
        Ok(metadata) if metadata.is_dir() => {
            match transfer
                .file_activity()
                .try_enter_descendant_path(transfer.file_root(), &resolved_path)
                .await
            {
                Ok(Ok(g)) => Some(g),
                Ok(Err(_)) => {
                    let err = TransferError::conflict(err_source_busy(&locale));
                    return send_download_transfer_error(transfer.writer(), &err).await;
                }
                Err(e) => {
                    error!(user = %username, ip = %peer_addr, err = %e, "{}", LOG_DOWNLOAD_SCAN_FAILED);
                    let err = TransferError::io_error(err_transfer_read_failed(&locale));
                    return send_download_transfer_error(transfer.writer(), &err).await;
                }
            }
        }
        Ok(_) => None,
        Err(e) => {
            error!(user = %username, ip = %peer_addr, err = %e, "{}", LOG_DOWNLOAD_SCAN_FAILED);
            let err = TransferError::io_error(err_transfer_read_failed(&locale));
            return send_download_transfer_error(transfer.writer(), &err).await;
        }
    };

    let files = match scan_files_for_transfer(&resolved_path, &username, is_admin).await {
        Ok(files) => files,
        Err(e) => {
            error!(user = %username, ip = %peer_addr, err = %e, "{}", LOG_DOWNLOAD_SCAN_FAILED);
            let err_msg = err_transfer_read_failed(&locale);
            return send_download_error_and_close(
                transfer.writer(),
                &err_msg,
                Some(ERROR_KIND_IO_ERROR),
            )
            .await;
        }
    };

    let mut activity_paths = Vec::with_capacity(files.len() * 2);
    for file in &files {
        activity_paths.push(file.absolute_path.clone());
        match tokio::fs::canonicalize(&file.absolute_path).await {
            Ok(canonical_path) if canonical_path != file.absolute_path => {
                activity_paths.push(canonical_path);
            }
            Ok(_) => {}
            Err(e) => {
                error!(user = %username, ip = %peer_addr, err = %e, "{}", LOG_DOWNLOAD_SCAN_FAILED);
                let err = TransferError::io_error(err_transfer_read_failed(&locale));
                return send_download_transfer_error(transfer.writer(), &err).await;
            }
        }
    }
    // Directory downloads are intentionally all-or-nothing: if any scanned
    // file is busy, fail fast rather than streaming a partial snapshot.
    let _activity_guard = match transfer
        .file_activity()
        .try_enter_read_paths(transfer.file_root(), &activity_paths)
        .await
    {
        Ok(Ok(g)) => g,
        Ok(Err(_)) => {
            let err = TransferError::conflict(err_source_busy(&locale));
            return send_download_transfer_error(transfer.writer(), &err).await;
        }
        Err(e) => {
            error!(user = %username, ip = %peer_addr, err = %e, "{}", LOG_DOWNLOAD_SCAN_FAILED);
            let err = TransferError::io_error(err_transfer_read_failed(&locale));
            return send_download_transfer_error(transfer.writer(), &err).await;
        }
    };

    // saturating_add: don't overflow on a huge directory
    let total_size: u64 = files.iter().fold(0u64, |acc, f| acc.saturating_add(f.size));
    let file_count = files.len() as u64;

    // Registry registered with size 0; fill in the real total now.
    transfer.set_total_size(total_size);

    let log_transfer_id = generate_transfer_id();

    debug!(
        id = %log_transfer_id,
        user = %username,
        ip = %peer_addr,
        files = file_count,
        bytes = total_size,
        "{}", LOG_DOWNLOAD_STARTING
    );

    let response = ServerMessage::FileDownloadResponse {
        success: true,
        error: None,
        error_kind: None,
        size: Some(total_size),
        file_count: Some(file_count),
        transfer_id: Some(log_transfer_id.clone()),
    };
    if let Err(e) = transfer.send(&response).await {
        error!(id = %log_transfer_id, user = %username, ip = %peer_addr, err = %e, "{}", LOG_DOWNLOAD_SEND_FAILED);
        return Ok(());
    }

    let mut success = true;
    let mut error: Option<String> = None;
    let mut error_kind: Option<String> = None;

    for file_info in &files {
        // Canonicalize to resolve symlinks before opening.
        let canonical_path = match tokio::fs::canonicalize(&file_info.absolute_path).await {
            Ok(p) => p,
            Err(e) => {
                success = false;
                error = Some(err_transfer_file_failed(
                    &locale,
                    &file_info.relative_path,
                    &e.to_string(),
                ));
                error_kind = Some(ERROR_KIND_IO_ERROR.to_string());
                break;
            }
        };

        match stream_file_with_hash(transfer, file_info, &canonical_path, &log_transfer_id).await {
            Ok(()) => {
                // bytes_transferred updated inside stream_file_with_hash
            }
            Err(StreamFileError::Banned) => {
                // Close the socket; client gets the ban reason on its BBS connection.
                info!(id = %log_transfer_id, user = %username, ip = %peer_addr, "{}", LOG_DOWNLOAD_BANNED);
                let _ = transfer.writer().get_mut().shutdown().await;
                return Ok(());
            }
            Err(StreamFileError::HashMismatch) => {
                warn!(
                    id = %log_transfer_id,
                    user = %username,
                    ip = %peer_addr,
                    path = %file_info.relative_path,
                    "{}", LOG_DOWNLOAD_HASH_MISMATCH
                );
                success = false;
                error = Some(err_transfer_file_failed(
                    &locale,
                    &file_info.relative_path,
                    "resume hash mismatch",
                ));
                error_kind = Some(nexus_common::ERROR_KIND_HASH_MISMATCH.to_string());
                break;
            }
            Err(StreamFileError::Io(e)) => {
                warn!(
                    id = %log_transfer_id,
                    user = %username,
                    ip = %peer_addr,
                    path = %file_info.relative_path,
                    err = %e,
                    "{}", LOG_DOWNLOAD_STREAM_ERROR
                );
                success = false;
                error = Some(err_transfer_file_failed(
                    &locale,
                    &file_info.relative_path,
                    &e.to_string(),
                ));
                error_kind = Some(ERROR_KIND_IO_ERROR.to_string());
                break;
            }
            Err(StreamFileError::FrameStarted(e)) => {
                warn!(
                    id = %log_transfer_id,
                    user = %username,
                    ip = %peer_addr,
                    path = %file_info.relative_path,
                    err = %e,
                    "{}", LOG_DOWNLOAD_STREAM_ERROR
                );
                let _ = transfer.writer().get_mut().shutdown().await;
                return Ok(());
            }
        }
    }

    let complete = ServerMessage::TransferComplete {
        success,
        error,
        error_kind,
    };
    let _ = transfer.send(&complete).await; // Best effort — connection may be closing.

    if success {
        info!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %download_path, "{}", LOG_DOWNLOAD_COMPLETE);
    } else {
        warn!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %download_path, "{}", LOG_DOWNLOAD_FAILED);
    }

    let _ = transfer.writer().get_mut().shutdown().await;

    Ok(())
}

enum StreamFileError {
    Io(io::Error),
    FrameStarted(io::Error),
    Banned,
    HashMismatch,
}

impl From<io::Error> for StreamFileError {
    fn from(e: io::Error) -> Self {
        StreamFileError::Io(e)
    }
}

impl From<StreamError> for StreamFileError {
    fn from(e: StreamError) -> Self {
        match e {
            StreamError::Banned => StreamFileError::Banned,
            StreamError::Io(e) => StreamFileError::Io(e),
            StreamError::FrameStarted(e) => StreamFileError::FrameStarted(e),
            StreamError::ConnectionClosed => {
                StreamFileError::Io(io::Error::other("Connection closed"))
            }
        }
    }
}

async fn validate_and_resolve_download_path(
    transfer: &Transfer<'_, impl AsyncRead + Unpin, impl AsyncWrite + Unpin>,
    locale: &str,
    download_path: &str,
    use_root: bool,
) -> Result<std::path::PathBuf, TransferError> {
    let user = transfer.user();
    validate_transfer_path(download_path, locale)?;
    check_permission(user, Permission::FileDownload, locale)?;
    check_root_permission(user, use_root, locale)?;

    let area_root = resolve_area_root(
        transfer.file_root(),
        transfer.user_area_root(),
        use_root,
        locale,
    )
    .await?;
    let candidate = build_validated_path(&area_root, download_path, locale).await?;

    resolve_path(&area_root, &candidate)
        .await
        .map_err(|e| path_error_to_transfer_error(e, locale))
}

/// Check if a path can be accessed for download (dropbox restrictions)
pub(crate) fn can_access_for_download(path: &Path, username: &str, is_admin: bool) -> bool {
    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            match parse_folder_type(name) {
                FolderType::DropBox => {
                    // Generic dropbox: admins only.
                    if !is_admin {
                        return false;
                    }
                }
                FolderType::UserDropBox(owner) => {
                    // User dropbox: the named owner or an admin.
                    if !is_admin && fold_name(&owner) != fold_name(username) {
                        return false;
                    }
                }
                FolderType::Upload | FolderType::Default => {
                    // Open to all.
                }
            }
        }
    }
    true
}

async fn scan_files_for_transfer(
    resolved_path: &Path,
    username: &str,
    is_admin: bool,
) -> io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    let metadata = tokio::fs::metadata(resolved_path).await?;

    if metadata.is_file() {
        let file_name = resolved_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(DEFAULT_FILENAME);

        files.push(FileInfo {
            relative_path: file_name.to_string(),
            absolute_path: resolved_path.to_path_buf(),
            size: metadata.len(),
        });
    } else if metadata.is_dir() {
        // Empty prefix: the client already includes the directory name in local_path,
        // so paths are relative to inside the directory ("song.mp3", "Jazz/tune.mp3").
        scan_directory_recursive(resolved_path, "", &mut files, username, is_admin).await?;
    }

    Ok(files)
}

/// Recursively scan a directory, skipping inaccessible dropbox subfolders so a
/// parent-directory download can't leak their contents.
fn scan_directory_recursive<'a>(
    dir: &'a Path,
    prefix: &'a str,
    files: &'a mut Vec<FileInfo>,
    username: &'a str,
    is_admin: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        debug!(path = ?dir, prefix = ?prefix, "{}", LOG_SCAN_DIRECTORY);

        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            debug!(path = ?path, "{}", LOG_SCAN_ENTRY);
            // tokio::fs::metadata (stat) follows symlinks; entry.metadata() (lstat) wouldn't.
            let metadata = match tokio::fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    debug!(path = ?path, err = %e, "{}", LOG_SCAN_METADATA_FAILED);
                    continue;
                }
            };
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                debug!(name = ?entry.file_name(), "{}", LOG_SCAN_NON_UTF8);
                continue;
            };

            // Dotfiles are included; show_hidden only affects the browser UI, not transfers.

            // Access-check the symlink's location, not its target: symlinks are
            // admin-created and trusted, so one pointing into a dropbox is an
            // intentional exposure.
            if !can_access_for_download(&path, username, is_admin) {
                debug!(path = %file_name, "{}", LOG_SCAN_DROPBOX_DENIED);
                continue;
            }

            let relative = if prefix.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", prefix, file_name)
            };

            if metadata.is_file() {
                debug!(path = %relative, bytes = metadata.len(), "{}", LOG_SCAN_ADDING_FILE);
                files.push(FileInfo {
                    relative_path: relative,
                    absolute_path: path,
                    size: metadata.len(),
                });
            } else if metadata.is_dir() {
                debug!(path = %relative, "{}", LOG_SCAN_RECURSING);
                scan_directory_recursive(&path, &relative, files, username, is_admin).await?;
            } else {
                debug!(path = %file_name, "{}", LOG_SCAN_SPECIAL_FILE);
            }
        }

        debug!(path = ?dir, files = files.len(), "{}", LOG_SCAN_DONE);

        Ok(())
    })
}

/// Stream a file to the client, hashing in a single pass:
/// FileStart (no hash) → FileStartResponse (resume negotiation) → hash 0..offset for
/// resume verification (hasher keeps state) → stream FileData from offset feeding the
/// hasher → FileHash with the full hash. Streaming is ban-checked for mid-transfer kill.
async fn stream_file_with_hash<R, W>(
    transfer: &mut Transfer<'_, R, W>,
    file_info: &FileInfo,
    canonical_path: &Path,
    transfer_id: &str,
) -> Result<(), StreamFileError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // FileStart carries no sha256; the hash follows in FileHash after the data.
    let file_start = ServerMessage::FileStart {
        path: file_info.relative_path.clone(),
        size: file_info.size,
    };
    transfer.send(&file_start).await?;

    // Returns the resume offset and a hasher pre-fed with 0..offset.
    let (frame_reader, frame_writer) = transfer.reader_writer();
    let (offset, mut hasher) =
        match read_file_start_response(frame_reader, frame_writer, file_info.size, canonical_path)
            .await
        {
            Ok(result) => result,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                return Err(StreamFileError::HashMismatch);
            }
            Err(e) => return Err(StreamFileError::Io(e)),
        };

    if offset > 0 {
        debug!(
            id = %transfer_id,
            path = %file_info.relative_path,
            offset = offset,
            percent = (offset * 100) / file_info.size.max(1),
            "{}", LOG_DOWNLOAD_RESUMING
        );
    } else if file_info.size > 0 {
        debug!(
            id = %transfer_id,
            path = %file_info.relative_path,
            bytes = file_info.size,
            "{}", LOG_DOWNLOAD_SENDING
        );
    }

    // Already complete (offset == size): send FileHash alone, no data.
    if offset >= file_info.size {
        if file_info.size > 0 {
            debug!(
                id = %transfer_id,
                path = %file_info.relative_path,
                "{}", LOG_DOWNLOAD_ALREADY_COMPLETE
            );
        }
        let file_hash = ServerMessage::FileHash {
            sha256: hasher.finalize(),
        };
        transfer.send(&file_hash).await?;
        return Ok(());
    }

    let bytes_to_send = file_info.size - offset;

    let file = File::open(canonical_path).await?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset)).await?;
    }

    // HashingReader hashes each disk chunk before it's sent, extending the hasher
    // (already holding 0..offset) over offset..end in a single pass.
    let mut hashing_reader = HashingReader::new(reader, &mut hasher);

    // stream_file_to_client ban-checks between 64KB chunks for mid-transfer kill.
    let result = transfer
        .stream_file_to_client("FileData", &mut hashing_reader, bytes_to_send)
        .await;

    // Release the hasher borrow before finalizing.
    drop(hashing_reader);

    let bytes_written = result?;

    // A short-but-finished frame means a mid-stream ban.
    if bytes_written < bytes_to_send || transfer.is_banned() {
        return Err(StreamFileError::Banned);
    }

    // Full-file hash: hasher has now consumed 0..offset + offset..end.
    let file_hash = ServerMessage::FileHash {
        sha256: hasher.finalize(),
    };
    transfer.send(&file_hash).await?;

    Ok(())
}

/// Read FileStartResponse and compute the resume offset with a single-pass hasher.
///
/// If the client reports an existing file, hash the server's 0..client_size into a
/// StreamingHasher and compare against the client's hash; on match return
/// (offset, hasher) with the hasher retaining 0..offset state for the caller to extend
/// and finalize. FileHashing keepalives are sent (and skipped on input) during hashing.
async fn read_file_start_response<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    server_size: u64,
    file_path: &Path,
) -> io::Result<(u64, StreamingHasher)>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // Loop so we skip any FileHashing keepalives the client sends while hashing.
    let (size, sha256) = loop {
        let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                return Err(io::Error::other(
                    "Connection closed waiting for FileStartResponse",
                ));
            }
            Err(e) => {
                return Err(io::Error::other(format!(
                    "Failed to read FileStartResponse: {e}"
                )));
            }
        };

        match received.message {
            ClientMessage::FileStartResponse { size, sha256 } => {
                break (size, sha256);
            }
            ClientMessage::FileHashing { .. } => {
                // Keepalive while the client hashes a large local file.
                continue;
            }
            _ => {
                return Err(io::Error::other("Expected FileStartResponse message"));
            }
        }
    };

    // No local file, larger-than-server, or no hash to verify against → start over.
    if size == 0 {
        return Ok((0, StreamingHasher::new()));
    }
    if size > server_size {
        return Ok((0, StreamingHasher::new()));
    }
    let Some(client_hash) = sha256 else {
        return Ok((0, StreamingHasher::new()));
    };

    // Hash the server's 0..size (keepalives prevent client timeout on large files).
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(FALLBACK_FILE_NAME)
        .to_string();

    let hasher = hash_file_with_keepalives(file_path, size, &file_name, frame_writer).await?;

    // partial_hash() clones and finalizes, so the hasher keeps its 0..size state.
    let server_hash = hasher.partial_hash();

    if client_hash == server_hash {
        // Match: resume from `size` reusing the retained hasher state.
        Ok((size, hasher))
    } else {
        // Mismatch: client's partial file diverges from ours.
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "resume hash mismatch",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::make_authenticated_user;
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroUsize;
    use std::sync::Arc;

    use nexus_common::ERROR_KIND_CONFLICT;
    use nexus_common::io::{read_server_message, send_client_message};
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::{BufReader, DuplexStream, ReadHalf, WriteHalf, duplex};
    use tokio::sync::mpsc;

    use crate::egress::task::{EgressHandle, EgressTask};
    use crate::egress::{EGRESS_DISPATCH_QUEUE_CAPACITY, EgressManager};
    use crate::files::{FileActivityMap, FileIndex};
    use crate::scheduler::{ConnectionClass, ConnectionId};
    use crate::transfers::registry::{TransferDirection, TransferRegistration, TransferRegistry};
    use crate::transfers::transfer::{TransferContext, TransferEgress};

    const TEST_LOCALE: &str = "en";

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7501)
    }

    struct DownloadTransferFixture<'a> {
        file_root: &'a Path,
        shared_root: &'a Path,
        file_index: &'a Arc<FileIndex>,
        file_activity: &'a Arc<FileActivityMap>,
        registry: &'a TransferRegistry,
        egress: Option<TransferEgress>,
    }

    fn make_download_transfer<'a>(
        server_read: ReadHalf<DuplexStream>,
        server_write: WriteHalf<DuplexStream>,
        fixture: DownloadTransferFixture<'a>,
    ) -> Transfer<'a, BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>> {
        let (info, ban_rx) = fixture.registry.register(TransferRegistration {
            user_id: 1,
            peer_addr: test_addr(),
            nickname: "tester".to_string(),
            username: "tester".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "file.txt".to_string(),
            total_size: 0,
        });

        Transfer::new(
            FrameReader::new(BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_authenticated_user(false, &[Permission::FileDownload]),
                locale: TEST_LOCALE.to_string(),
                file_root: fixture.file_root,
                file_index: fixture.file_index,
                file_activity: fixture.file_activity,
                user_area_root: Some(fixture.shared_root.to_path_buf()),
                registry: fixture.registry,
                egress: fixture.egress,
            },
        )
    }

    async fn registered_download_egress(
        chunk_size: usize,
    ) -> (
        TransferEgress,
        EgressHandle,
        tokio::task::JoinHandle<()>,
        ConnectionId,
    ) {
        let (egress, egress_task) =
            EgressTask::channel(EgressManager::new(NonZeroUsize::new(chunk_size).unwrap()));
        let egress_task = tokio::spawn(egress_task.run());
        let connection_id = ConnectionId::new(9001);
        let (dispatch_tx, dispatch_rx) = mpsc::channel(EGRESS_DISPATCH_QUEUE_CAPACITY);
        assert!(
            egress
                .register_anon(connection_id, ConnectionClass::Bulk, dispatch_tx)
                .await
                .unwrap()
        );

        (
            TransferEgress::new(egress.clone(), connection_id, dispatch_rx),
            egress,
            egress_task,
            connection_id,
        )
    }

    async fn assert_download_conflict_response(
        client_read: ReadHalf<DuplexStream>,
        transfer: &mut Transfer<'_, BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>>,
        path: &str,
    ) {
        handle_download(
            transfer,
            DownloadParams {
                path: path.to_string(),
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
            ServerMessage::FileDownloadResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind.as_deref(), Some(ERROR_KIND_CONFLICT));
            }
            _ => panic!("Expected FileDownloadResponse"),
        }
    }

    #[tokio::test]
    async fn test_download_conflicts_when_scanned_file_active() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        fs::create_dir_all(&shared_root).await.unwrap();
        let target = shared_root.join("file.txt");
        fs::write(&target, b"content").await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let _target_guard = file_activity
            .try_enter_child_path(&file_root, &target)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_download_transfer(
            server_read,
            server_write,
            DownloadTransferFixture {
                file_root: &file_root,
                shared_root: &shared_root,
                file_index: &file_index,
                file_activity: &file_activity,
                registry: &registry,
                egress: None,
            },
        );

        assert_download_conflict_response(client_read, &mut transfer, "file.txt").await;
    }

    #[tokio::test]
    async fn test_download_empty_directory_conflicts_when_directory_active() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let bundle = shared_root.join("bundle");
        fs::create_dir_all(&bundle).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let _directory_guard = file_activity
            .try_enter_directory_path(&file_root, &bundle)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_download_transfer(
            server_read,
            server_write,
            DownloadTransferFixture {
                file_root: &file_root,
                shared_root: &shared_root,
                file_index: &file_index,
                file_activity: &file_activity,
                registry: &registry,
                egress: None,
            },
        );

        assert_download_conflict_response(client_read, &mut transfer, "bundle").await;
    }

    #[tokio::test]
    async fn test_download_allows_same_file_active_read() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        fs::create_dir_all(&shared_root).await.unwrap();
        let target = shared_root.join("file.txt");
        fs::write(&target, b"").await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let _read_guard = file_activity
            .try_enter_read_paths(&file_root, std::slice::from_ref(&target))
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_download_transfer(
            server_read,
            server_write,
            DownloadTransferFixture {
                file_root: &file_root,
                shared_root: &shared_root,
                file_index: &file_index,
                file_activity: &file_activity,
                registry: &registry,
                egress: None,
            },
        );
        let mut reader = FrameReader::new(BufReader::new(client_read));
        let mut writer = FrameWriter::new(client_write);

        let server = async {
            handle_download(
                &mut transfer,
                DownloadParams {
                    path: "file.txt".to_string(),
                    root: false,
                },
            )
            .await
            .unwrap();
        };

        let client = async {
            let response = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match response {
                ServerMessage::FileDownloadResponse {
                    success,
                    error_kind,
                    ..
                } => {
                    assert!(success);
                    assert_eq!(error_kind, None);
                }
                _ => panic!("Expected FileDownloadResponse"),
            }

            let file_start = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match file_start {
                ServerMessage::FileStart { path, size } => {
                    assert_eq!(path, "file.txt");
                    assert_eq!(size, 0);
                }
                _ => panic!("Expected FileStart"),
            }

            send_client_message(
                &mut writer,
                &ClientMessage::FileStartResponse {
                    size: 0,
                    sha256: None,
                },
            )
            .await
            .unwrap();

            let file_hash = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(file_hash, ServerMessage::FileHash { .. }));
        };

        tokio::join!(server, client);
    }

    #[tokio::test]
    async fn test_download_registered_egress_zero_byte_sends_file_hash_without_file_data() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        fs::create_dir_all(&shared_root).await.unwrap();
        let target = shared_root.join("file.txt");
        fs::write(&target, b"").await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_download_egress(8192).await;

        let (client, server) = duplex(8192);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_download_transfer(
            server_read,
            server_write,
            DownloadTransferFixture {
                file_root: &file_root,
                shared_root: &shared_root,
                file_index: &file_index,
                file_activity: &file_activity,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );
        let mut reader = FrameReader::new(BufReader::new(client_read));
        let mut writer = FrameWriter::new(client_write);

        let server = async {
            handle_download(
                &mut transfer,
                DownloadParams {
                    path: "file.txt".to_string(),
                    root: false,
                },
            )
            .await
            .unwrap();
        };

        let client = async {
            let response = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(
                response,
                ServerMessage::FileDownloadResponse { success: true, .. }
            ));

            let file_start = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match file_start {
                ServerMessage::FileStart { path, size } => {
                    assert_eq!(path, "file.txt");
                    assert_eq!(size, 0);
                }
                _ => panic!("Expected FileStart"),
            }

            send_client_message(
                &mut writer,
                &ClientMessage::FileStartResponse {
                    size: 0,
                    sha256: None,
                },
            )
            .await
            .unwrap();

            let next = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(
                matches!(next, ServerMessage::FileHash { .. }),
                "zero-byte egress download must not emit FileData"
            );

            let complete = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(
                complete,
                ServerMessage::TransferComplete { success: true, .. }
            ));
        };

        tokio::join!(server, client);
        egress.unregister(connection_id).await.unwrap();
        drop(transfer);
        drop(egress);
        egress_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_download_egress_file_data_is_single_frame_followed_by_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        fs::create_dir_all(&shared_root).await.unwrap();
        let file_data = {
            let mut data = vec![0xAB; 64 * 1024];
            data.extend_from_slice(b"final");
            data
        };
        let target = shared_root.join("file.bin");
        fs::write(&target, &file_data).await.unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_download_egress(8192).await;

        let (client, server) = duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_download_transfer(
            server_read,
            server_write,
            DownloadTransferFixture {
                file_root: &file_root,
                shared_root: &shared_root,
                file_index: &file_index,
                file_activity: &file_activity,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );
        let mut reader = FrameReader::new(BufReader::new(client_read));
        let mut writer = FrameWriter::new(client_write);

        let server = async {
            handle_download(
                &mut transfer,
                DownloadParams {
                    path: "file.bin".to_string(),
                    root: false,
                },
            )
            .await
            .unwrap();
        };

        let client = async {
            let response = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(
                response,
                ServerMessage::FileDownloadResponse { success: true, .. }
            ));

            let file_start = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            match file_start {
                ServerMessage::FileStart { path, size } => {
                    assert_eq!(path, "file.bin");
                    assert_eq!(size, file_data.len() as u64);
                }
                _ => panic!("Expected FileStart"),
            }

            send_client_message(
                &mut writer,
                &ClientMessage::FileStartResponse {
                    size: 0,
                    sha256: None,
                },
            )
            .await
            .unwrap();

            let frame = reader.read_frame_header().await.unwrap().unwrap();
            assert_eq!(frame.message_type, "FileData");
            assert_eq!(frame.payload_length, file_data.len() as u64);
            let payload = reader.read_payload_into_vec(&frame).await.unwrap();
            assert_eq!(payload, file_data);

            let file_hash = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(file_hash, ServerMessage::FileHash { .. }));

            let complete = read_server_message(&mut reader)
                .await
                .unwrap()
                .unwrap()
                .message;
            assert!(matches!(
                complete,
                ServerMessage::TransferComplete { success: true, .. }
            ));
        };

        tokio::join!(server, client);
        egress.unregister(connection_id).await.unwrap();
        drop(transfer);
        drop(egress);
        egress_task.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_download_directory_conflicts_when_symlink_target_active() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path().canonicalize().unwrap();
        let shared_root = file_root.join("shared");
        let bundle = shared_root.join("bundle");
        fs::create_dir_all(&bundle).await.unwrap();
        let target = shared_root.join("target.txt");
        let link = bundle.join("link.txt");
        fs::write(&target, b"content").await.unwrap();
        symlink(&target, &link).unwrap();

        let file_index = Arc::new(FileIndex::new(temp_dir.path(), &file_root));
        let file_activity = Arc::new(FileActivityMap::new());
        let registry = TransferRegistry::new();
        let _target_guard = file_activity
            .try_enter_child_path(&file_root, &target)
            .await
            .unwrap()
            .unwrap();

        let (client, server) = duplex(8192);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut transfer = make_download_transfer(
            server_read,
            server_write,
            DownloadTransferFixture {
                file_root: &file_root,
                shared_root: &shared_root,
                file_index: &file_index,
                file_activity: &file_activity,
                registry: &registry,
                egress: None,
            },
        );

        assert_download_conflict_response(client_read, &mut transfer, "bundle").await;
    }

    #[test]
    fn test_can_access_default_folder() {
        let path = Path::new("/files/shared/Documents/readme.txt");
        assert!(can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "bob", false));
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_can_access_upload_folder() {
        let path = Path::new("/files/shared/Uploads [NEXUS-UL]/file.zip");
        assert!(can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "bob", false));
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_cannot_access_dropbox_non_admin() {
        let path = Path::new("/files/shared/Submissions [NEXUS-DB]/secret.txt");
        assert!(!can_access_for_download(path, "alice", false));
        assert!(!can_access_for_download(path, "bob", false));
    }

    #[test]
    fn test_admin_can_access_dropbox() {
        let path = Path::new("/files/shared/Submissions [NEXUS-DB]/secret.txt");
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_user_can_access_own_user_dropbox() {
        let path = Path::new("/files/shared/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "ALICE", false)); // case insensitive
    }

    #[test]
    fn test_user_cannot_access_other_user_dropbox() {
        let path = Path::new("/files/shared/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(!can_access_for_download(path, "bob", false));
        assert!(!can_access_for_download(path, "charlie", false));
    }

    #[test]
    fn test_admin_can_access_any_user_dropbox() {
        let path = Path::new("/files/shared/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_nested_dropbox_blocks_access() {
        // File is in a regular folder, but parent is a dropbox
        let path = Path::new("/files/shared/Submissions [NEXUS-DB]/subfolder/file.txt");
        assert!(!can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "admin", true));
    }
}
