//! File download handling for transfers
//!
//! Contains functions for handling download requests, scanning files,
//! checking dropbox access, and streaming files to clients.
//!
//! Uses StreamingHasher for single-pass hashing during file transfers.
//! For resume verification, computes intermediate hashes via clone-and-finalize.
//! Sends FileHashing keepalive messages during hash-only phases to prevent timeouts.

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
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::constants::*;
use crate::db::Permission;
use crate::files::folder_type::{FolderType, parse_folder_type};
use crate::files::path::resolve_path;
use crate::handlers::{
    err_transfer_access_denied, err_transfer_file_failed, err_transfer_read_failed,
};

use super::hashing::{FALLBACK_FILE_NAME, HashingReader, hash_file_with_keepalives};
use super::helpers::{
    TransferError, build_validated_path, check_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_download_error_and_close, send_download_transfer_error, validate_transfer_path,
};
use super::transfer::{StreamError, Transfer};
use super::types::{AuthenticatedUser, DownloadParams, FileInfo};

/// Handle a file download request
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

    // Extract values to avoid borrow checker issues
    let locale = transfer.locale().to_string();
    let username = transfer.user().username.clone();
    let is_admin = transfer.user().is_admin;
    let peer_addr = transfer.peer_addr();
    let file_root = transfer.file_root();

    // Validate and resolve path using shared helpers
    let resolved_path = match validate_and_resolve_download_path(
        transfer.user(),
        file_root,
        &locale,
        &download_path,
        use_root,
    )
    .await
    {
        Ok(path) => path,
        Err(e) => return send_download_transfer_error(transfer.writer(), &e).await,
    };

    // Check dropbox access
    if !can_access_for_download(&resolved_path, &username, is_admin) {
        let err = TransferError::permission(err_transfer_access_denied(&locale));
        return send_download_transfer_error(transfer.writer(), &err).await;
    }

    // Scan files to transfer
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

    // Calculate total size using saturating arithmetic to prevent overflow
    let total_size: u64 = files.iter().fold(0u64, |acc, f| acc.saturating_add(f.size));
    let file_count = files.len() as u64;

    // Update transfer registry with actual size (was 0 at registration time)
    transfer.set_total_size(total_size);

    // Generate transfer ID for logging
    let log_transfer_id = generate_transfer_id();

    debug!(
        id = %log_transfer_id,
        user = %username,
        ip = %peer_addr,
        files = file_count,
        bytes = total_size,
        "{}", LOG_DOWNLOAD_STARTING
    );

    // Send FileDownloadResponse
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

    // Stream each file
    let mut success = true;
    let mut error: Option<String> = None;
    let mut error_kind: Option<String> = None;

    for file_info in &files {
        // Canonicalize path to handle symlinks
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
                // bytes_transferred is updated inside stream_file_with_hash
            }
            Err(StreamFileError::Banned) => {
                // Just close the socket - client gets ban reason on BBS connection
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
        }
    }

    // Send TransferComplete
    let complete = ServerMessage::TransferComplete {
        success,
        error,
        error_kind,
    };
    let _ = transfer.send(&complete).await; // Best effort - connection may be closing

    if success {
        info!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %download_path, "{}", LOG_DOWNLOAD_COMPLETE);
    } else {
        warn!(id = %log_transfer_id, user = %username, ip = %peer_addr, path = %download_path, "{}", LOG_DOWNLOAD_FAILED);
    }

    // Close connection
    let _ = transfer.writer().get_mut().shutdown().await;

    Ok(())
}

/// Error type for stream_file_with_hash
enum StreamFileError {
    Io(io::Error),
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
            StreamError::ConnectionClosed => {
                StreamFileError::Io(io::Error::other("Connection closed"))
            }
        }
    }
}

/// Validate and resolve a download path
///
/// This helper consolidates path validation, permission checks, and resolution
/// into a single function to reduce code duplication.
async fn validate_and_resolve_download_path(
    user: &AuthenticatedUser,
    file_root: &Path,
    locale: &str,
    download_path: &str,
    use_root: bool,
) -> Result<std::path::PathBuf, TransferError> {
    // Validate path
    validate_transfer_path(download_path, locale)?;

    // Check download permission
    check_permission(user, Permission::FileDownload, locale)?;

    // Check file_root permission if using root mode
    check_root_permission(user, use_root, locale)?;

    // Resolve area root
    let area_root = resolve_area_root(file_root, &user.username, use_root, locale).await?;

    // Build candidate path
    let candidate = build_validated_path(&area_root, download_path, locale)?;

    // Resolve to canonical path
    resolve_path(&area_root, &candidate)
        .await
        .map_err(|e| path_error_to_transfer_error(e, locale))
}

/// Check if a path can be accessed for download (dropbox restrictions)
pub(crate) fn can_access_for_download(path: &Path, username: &str, is_admin: bool) -> bool {
    // Check each component of the path for dropbox folders
    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            match parse_folder_type(name) {
                FolderType::DropBox => {
                    // Only admins can download from generic dropboxes
                    if !is_admin {
                        return false;
                    }
                }
                FolderType::UserDropBox(owner) => {
                    // Only the named user or admins can download
                    if !is_admin && owner.to_lowercase() != username.to_lowercase() {
                        return false;
                    }
                }
                FolderType::Upload | FolderType::Default => {
                    // Anyone can download from upload folders and default folders
                }
            }
        }
    }
    true
}

/// Scan files to transfer from a path (file or directory)
async fn scan_files_for_transfer(
    resolved_path: &Path,
    username: &str,
    is_admin: bool,
) -> io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    let metadata = tokio::fs::metadata(resolved_path).await?;

    if metadata.is_file() {
        // Single file download
        let file_name = resolved_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(DEFAULT_FILENAME);

        // Use just the filename for single file downloads
        files.push(FileInfo {
            relative_path: file_name.to_string(),
            absolute_path: resolved_path.to_path_buf(),
            size: metadata.len(),
        });
    } else if metadata.is_dir() {
        // Directory download - recursively scan
        // Use empty prefix because the client already includes the directory name in local_path.
        // Files will have paths relative to inside the directory (e.g., "song.mp3", "Jazz/tune.mp3")
        // rather than including the directory name (e.g., "Music/song.mp3", "Music/Jazz/tune.mp3").
        scan_directory_recursive(resolved_path, "", &mut files, username, is_admin).await?;
    }

    Ok(files)
}

/// Recursively scan a directory for files
///
/// Filters out files in dropbox folders that the user doesn't have access to.
/// This prevents information leakage when downloading a parent directory that
/// contains dropbox subfolders.
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
            // Use tokio::fs::metadata instead of entry.metadata() to follow symlinks.
            // entry.metadata() uses lstat which returns symlink metadata, not target metadata.
            let metadata = match tokio::fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    debug!(path = ?path, err = %e, "{}", LOG_SCAN_METADATA_FAILED);
                    continue;
                }
            };
            // Skip files with non-UTF-8 names
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                debug!(name = ?entry.file_name(), "{}", LOG_SCAN_NON_UTF8);
                continue;
            };

            // Note: Hidden files (dotfiles) are included in downloads.
            // The show_hidden setting only affects the file browser UI, not transfers.

            // Check dropbox access on the symlink's location, NOT its target.
            // Symlinks are trusted because only admins can create them (users can't create
            // symlinks through the BBS protocol). If an admin creates a symlink in a public
            // folder pointing into a dropbox, that's intentional - they're choosing to expose
            // that content.
            if !can_access_for_download(&path, username, is_admin) {
                debug!(path = %file_name, "{}", LOG_SCAN_DROPBOX_DENIED);
                continue;
            }

            // Build relative path, handling empty prefix for top-level files
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
                // For subdirectories, use the relative path as the new prefix
                scan_directory_recursive(&path, &relative, files, username, is_admin).await?;
            } else {
                debug!(path = %file_name, "{}", LOG_SCAN_SPECIAL_FILE);
            }
        }

        debug!(path = ?dir, files = files.len(), "{}", LOG_SCAN_DONE);

        Ok(())
    })
}

/// Stream a file to the client with single-pass hashing
///
/// For each file:
/// 1. Send FileStart (path, size — no hash)
/// 2. Read FileStartResponse (for resume negotiation)
/// 3. Hash 0..offset for resume verification (returns hasher with state)
/// 4. Stream FileData from offset, feeding bytes to hasher
/// 5. Send FileHash with full file hash
///
/// Uses Transfer's ban-checked streaming to allow mid-transfer termination.
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
    // Send FileStart — no sha256, hash will be sent in FileHash after data
    let file_start = ServerMessage::FileStart {
        path: file_info.relative_path.clone(),
        size: file_info.size,
    };
    transfer.send(&file_start).await?;

    // Read FileStartResponse and determine resume offset
    // Returns hasher pre-fed with 0..offset bytes for single-pass hashing
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

    // If offset equals file size, file is already complete — send FileHash only
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

    // Calculate bytes to send
    let bytes_to_send = file_info.size - offset;

    // Open file and seek to offset (use canonical path for safety)
    let file = File::open(canonical_path).await?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset)).await?;
    }

    // Wrap reader with HashingReader to feed bytes to hasher during streaming.
    // Each chunk read from disk is hashed before being sent to the client.
    let mut hashing_reader = HashingReader::new(reader, &mut hasher);

    // Stream file data with ban checking between chunks
    let result = transfer
        .stream_file_to_client("FileData", &mut hashing_reader, bytes_to_send)
        .await;

    // Drop hashing_reader to release the borrow on hasher before finalizing
    drop(hashing_reader);

    let bytes_written = result?;

    // Check if we were banned mid-stream (frame was finished but short)
    if bytes_written < bytes_to_send || transfer.is_banned() {
        return Err(StreamFileError::Banned);
    }

    // Send FileHash with the full file hash (hasher consumed 0..offset + offset..end)
    let file_hash = ServerMessage::FileHash {
        sha256: hasher.finalize(),
    };
    transfer.send(&file_hash).await?;

    Ok(())
}

/// Read FileStartResponse and compute resume offset with single-pass hasher
///
/// If the client reports an existing file (partial or complete), this function:
/// 1. Creates a StreamingHasher
/// 2. Reads 0..client_size bytes of the server's file into the hasher
/// 3. Verifies the hash against the client's reported hash
/// 4. Returns (offset, hasher) — the hasher retains its state for continued use
///
/// The caller continues feeding offset..end into the returned hasher during
/// streaming, then finalizes it to get the full file hash for FileHash.
///
/// Sends FileHashing keepalives during hash computation to prevent client timeout.
/// Automatically skips FileHashing keepalives from the client.
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
    // Loop to skip any FileHashing keepalive messages from client
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
                // Keepalive — client is hashing a large local file
                continue;
            }
            _ => {
                return Err(io::Error::other("Expected FileStartResponse message"));
            }
        }
    };

    // If client has no local file, start from beginning
    if size == 0 {
        return Ok((0, StreamingHasher::new()));
    }

    // If client reports size > server size, start from beginning
    if size > server_size {
        return Ok((0, StreamingHasher::new()));
    }

    // Client must provide hash for resume verification
    let Some(client_hash) = sha256 else {
        return Ok((0, StreamingHasher::new()));
    };

    // Hash 0..size bytes of the server's file using StreamingHasher
    // Sends FileHashing keepalives to prevent client timeout during large file hashing
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(FALLBACK_FILE_NAME)
        .to_string();

    let hasher = hash_file_with_keepalives(file_path, size, &file_name, frame_writer).await?;

    // Verify: clone+finalize to get hash of 0..size without consuming hasher
    let server_hash = hasher.partial_hash();

    if client_hash == server_hash {
        // Hash matches — resume from client's position (hasher retains 0..size state)
        Ok((size, hasher))
    } else {
        // Hash mismatch — client's partial file doesn't match server's file
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "resume hash mismatch",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // can_access_for_download tests
    // ==========================================================================

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
