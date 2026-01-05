//! File download handling for transfers
//!
//! Contains functions for handling download requests, scanning files,
//! checking dropbox access, and streaming files to clients.

use std::io;
use std::path::Path;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, SeekFrom};

use nexus_common::ERROR_KIND_IO_ERROR;
use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::{read_client_message_with_full_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::constants::DEFAULT_FILENAME;
use crate::db::Permission;
use crate::files::folder_type::{FolderType, parse_folder_type};
use crate::files::path::resolve_path;
use crate::handlers::{
    err_transfer_access_denied, err_transfer_file_failed, err_transfer_read_failed,
};

use super::hash::{compute_file_sha256, compute_partial_sha256};
use super::helpers::{
    TransferError, build_validated_path, check_permission, check_root_permission,
    generate_transfer_id, path_error_to_transfer_error, resolve_area_root,
    send_download_error_and_close, send_download_transfer_error, validate_transfer_path,
};
use super::types::{DownloadParams, FileInfo, TransferContext};

/// Handle a file download request
pub(crate) async fn handle_download<R, W>(
    ctx: &mut TransferContext<'_, R, W>,
    params: DownloadParams,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let DownloadParams {
        path: download_path,
        root: use_root,
    } = params;

    // Validate and resolve path using shared helpers
    let resolved_path = match validate_and_resolve_download_path(ctx, &download_path, use_root) {
        Ok(path) => path,
        Err(e) => return send_download_transfer_error(ctx.frame_writer, &e).await,
    };

    // Check dropbox access
    if !can_access_for_download(&resolved_path, &ctx.user.username, ctx.user.is_admin) {
        return send_download_transfer_error(
            ctx.frame_writer,
            &TransferError::permission(err_transfer_access_denied(ctx.locale)),
        )
        .await;
    }

    // Scan files to transfer
    let files = match scan_files_for_transfer(
        &resolved_path,
        &ctx.user.username,
        ctx.user.is_admin,
        ctx.debug,
    )
    .await
    {
        Ok(files) => files,
        Err(e) => {
            if ctx.debug {
                eprintln!("Failed to scan files from {}: {e}", ctx.peer_addr);
            }
            return send_download_error_and_close(
                ctx.frame_writer,
                &err_transfer_read_failed(ctx.locale),
                Some(ERROR_KIND_IO_ERROR),
            )
            .await;
        }
    };

    // Calculate total size using saturating arithmetic to prevent overflow
    let total_size: u64 = files.iter().fold(0u64, |acc, f| acc.saturating_add(f.size));
    let file_count = files.len() as u64;

    // Generate transfer ID for logging
    let transfer_id = generate_transfer_id();

    if ctx.debug {
        eprintln!(
            "Download {transfer_id}: {} files, {} bytes from {}",
            file_count, total_size, ctx.peer_addr
        );
    }

    // Send FileDownloadResponse
    let response = ServerMessage::FileDownloadResponse {
        success: true,
        error: None,
        error_kind: None,
        size: Some(total_size),
        file_count: Some(file_count),
        transfer_id: Some(transfer_id.clone()),
    };
    send_server_message_with_id(ctx.frame_writer, &response, MessageId::new()).await?;

    // Stream each file
    let mut transfer_success = true;
    let mut transfer_error: Option<String> = None;
    let mut transfer_error_kind: Option<String> = None;

    for file_info in &files {
        match stream_file(
            ctx.frame_reader,
            ctx.frame_writer,
            file_info,
            ctx.debug,
            &transfer_id,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                if ctx.debug {
                    eprintln!(
                        "Download {transfer_id}: Error streaming {}: {e}",
                        file_info.relative_path
                    );
                }
                transfer_success = false;
                transfer_error = Some(err_transfer_file_failed(
                    ctx.locale,
                    &file_info.relative_path,
                    &e.to_string(),
                ));
                transfer_error_kind = Some(ERROR_KIND_IO_ERROR.to_string());
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
            eprintln!("Download {transfer_id}: Complete");
        } else {
            eprintln!("Download {transfer_id}: Failed");
        }
    }

    // Close connection
    let _ = ctx.frame_writer.get_mut().shutdown().await;

    Ok(())
}

/// Validate and resolve a download path
///
/// This helper consolidates path validation, permission checks, and resolution
/// into a single function to reduce code duplication.
fn validate_and_resolve_download_path<R, W>(
    ctx: &TransferContext<'_, R, W>,
    download_path: &str,
    use_root: bool,
) -> Result<std::path::PathBuf, TransferError>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // Validate path
    validate_transfer_path(download_path, ctx.locale)?;

    // Check download permission
    check_permission(ctx.user, Permission::FileDownload, ctx.locale)?;

    // Check file_root permission if using root mode
    check_root_permission(ctx.user, use_root, ctx.locale)?;

    // Resolve area root
    let area_root = resolve_area_root(ctx.file_root, &ctx.user.username, use_root, ctx.locale)?;

    // Build candidate path
    let candidate = build_validated_path(&area_root, download_path, ctx.locale)?;

    // Resolve to canonical path
    resolve_path(&area_root, &candidate).map_err(|e| path_error_to_transfer_error(e, ctx.locale))
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
    debug: bool,
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
        scan_directory_recursive(resolved_path, "", &mut files, username, is_admin, debug).await?;
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
    debug: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        if debug {
            eprintln!("Scanning directory: {:?} (prefix: {:?})", dir, prefix);
        }

        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if debug {
                eprintln!("  Processing entry: {:?}", path);
            }
            // Use tokio::fs::metadata instead of entry.metadata() to follow symlinks.
            // entry.metadata() uses lstat which returns symlink metadata, not target metadata.
            let metadata = match tokio::fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    if debug {
                        eprintln!("  Skipping {:?} - metadata failed: {}", path, e);
                    }
                    continue;
                }
            };
            // Skip files with non-UTF-8 names
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                if debug {
                    eprintln!("  Skipping non-UTF-8 filename: {:?}", entry.file_name());
                }
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
                if debug {
                    eprintln!("  Skipping {} - dropbox access denied", file_name);
                }
                continue;
            }

            // Build relative path, handling empty prefix for top-level files
            let relative = if prefix.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", prefix, file_name)
            };

            if metadata.is_file() {
                if debug {
                    eprintln!("  Adding file: {} (size: {})", relative, metadata.len());
                }
                files.push(FileInfo {
                    relative_path: relative,
                    absolute_path: path,
                    size: metadata.len(),
                });
            } else if metadata.is_dir() {
                if debug {
                    eprintln!("  Recursing into directory: {}", relative);
                }
                // For subdirectories, use the relative path as the new prefix
                scan_directory_recursive(&path, &relative, files, username, is_admin, debug)
                    .await?;
            } else if debug {
                eprintln!(
                    "  Skipping {} - special file (not a regular file or directory)",
                    file_name
                );
            }
        }

        if debug {
            eprintln!(
                "Done scanning directory: {:?} (found {} files so far)",
                dir,
                files.len()
            );
        }

        Ok(())
    })
}

/// Stream a single file to the client
async fn stream_file<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    file_info: &FileInfo,
    debug: bool,
    transfer_id: &str,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // Re-canonicalize to get the current real path (handles symlinks)
    // Note: Admin-created symlinks pointing outside the file area are allowed
    // (e.g., shared/Videos -> /mnt/nas/videos). Users cannot create symlinks
    // through the BBS protocol, so all symlinks are trusted.
    let canonical_path = std::fs::canonicalize(&file_info.absolute_path)?;

    // Compute SHA-256 of the file
    let sha256 = compute_file_sha256(&canonical_path).await?;

    // Send FileStart
    let file_start = ServerMessage::FileStart {
        path: file_info.relative_path.clone(),
        size: file_info.size,
        sha256: sha256.clone(),
    };
    let file_start_id = MessageId::new();
    send_server_message_with_id(frame_writer, &file_start, file_start_id).await?;

    // Read FileStartResponse to determine resume offset
    let offset =
        read_file_start_response(frame_reader, &sha256, file_info.size, &canonical_path).await?;

    if debug {
        if offset > 0 {
            eprintln!(
                "Transfer {transfer_id}: Resuming {} from offset {} ({}%)",
                file_info.relative_path,
                offset,
                (offset * 100) / file_info.size.max(1)
            );
        } else if file_info.size > 0 {
            eprintln!(
                "Transfer {transfer_id}: Sending {} ({} bytes)",
                file_info.relative_path, file_info.size
            );
        }
    }

    // If offset equals file size, file is already complete - skip streaming
    if offset >= file_info.size {
        if debug && file_info.size > 0 {
            eprintln!(
                "Transfer {transfer_id}: {} already complete",
                file_info.relative_path
            );
        }
        return Ok(());
    }

    // Calculate bytes to send
    let bytes_to_send = file_info.size - offset;

    // Open file and seek to offset (use canonical path for safety)
    let file = File::open(&canonical_path).await?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset)).await?;
    }

    // Stream file data using the framing helper
    frame_writer
        .write_streaming_frame(MessageId::new(), "FileData", &mut reader, bytes_to_send)
        .await
        .map_err(|e| io::Error::other(format!("Failed to stream file: {e}")))?;

    Ok(())
}

/// Read FileStartResponse and calculate resume offset
///
/// Verifies that the client's reported partial file hash matches the hash of
/// the first N bytes of the server's file before allowing resume.
async fn read_file_start_response<R>(
    frame_reader: &mut FrameReader<R>,
    server_sha256: &str,
    server_size: u64,
    file_path: &Path,
) -> io::Result<u64>
where
    R: AsyncReadExt + Unpin,
{
    // With idle timeout - client must respond promptly to FileStart
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
            // If client has no local file, start from beginning
            if size == 0 {
                return Ok(0);
            }

            // If client reports size > server size, start from beginning
            if size > server_size {
                return Ok(0);
            }

            // Client must provide hash for resume
            let Some(client_hash) = sha256 else {
                // No hash provided - start from beginning
                return Ok(0);
            };

            // If sizes match, verify against complete file hash
            if size == server_size {
                if client_hash == server_sha256 {
                    // File is already complete
                    return Ok(server_size);
                }
                // Hash mismatch - start from beginning
                return Ok(0);
            }

            // Client has partial file - verify hash of first N bytes
            let partial_hash = compute_partial_sha256(file_path, size).await?;
            if client_hash == partial_hash {
                // Hash matches - resume from client's position
                Ok(size)
            } else {
                // Hash mismatch - start from beginning
                Ok(0)
            }
        }
        _ => Err(io::Error::other("Expected FileStartResponse message")),
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
