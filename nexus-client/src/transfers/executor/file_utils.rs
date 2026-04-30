//! File utility functions for the transfer executor
//!
//! Provides helpers for generating unique paths, scanning directories,
//! validating paths, and hashing files with keepalive support.
//!
//! Uses `StreamingHasher` from nexus-common for single-pass hashing
//! during file transfers, with periodic `FileHashing` keepalive messages
//! to prevent server timeouts on large files.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::fs::File;
use tokio::io::{AsyncSeekExt, SeekFrom};

use nexus_common::FALLBACK_FILE_NAME;
use nexus_common::framing::FrameWriter;
use nexus_common::hash::StreamingHasher;
use nexus_common::io::send_client_message;
use nexus_common::protocol::ClientMessage;

use super::{PART_SUFFIX, TransferError};
use crate::constants::ERR_HASH_KEEPALIVE_PREFIX;

// =============================================================================
// Local File Info (for uploads)
// =============================================================================

/// Information about a local file to upload
#[derive(Debug, Clone)]
pub struct LocalFileInfo {
    /// Relative path (e.g., "subdir/file.txt")
    pub relative_path: String,
    /// Absolute path on local filesystem (for reading during upload)
    pub absolute_path: std::path::PathBuf,
    /// File size in bytes
    pub size: u64,
}

/// Generate a unique file path by appending (1), (2), etc.
///
/// Given "/path/to/file.txt", tries:
/// - /path/to/file (1).txt
/// - /path/to/file (2).txt
/// - etc.
///
/// Returns an error if no unique path can be found after 1000 attempts.
pub async fn generate_unique_path(original: &Path) -> Result<PathBuf, TransferError> {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(FALLBACK_FILE_NAME);
    let extension = original.extension().and_then(|s| s.to_str());
    let parent = original.parent();

    for i in 1..1000 {
        let new_name = if let Some(ext) = extension {
            format!("{} ({}).{}", stem, i, ext)
        } else {
            format!("{} ({})", stem, i)
        };

        let new_path = if let Some(parent) = parent {
            parent.join(&new_name)
        } else {
            PathBuf::from(&new_name)
        };

        // Check if this path is available (no file and no .part file)
        if tokio::fs::metadata(&new_path).await.is_err()
            && tokio::fs::metadata(format!("{}{}", new_path.display(), PART_SUFFIX))
                .await
                .is_err()
        {
            return Ok(new_path);
        }
    }

    // Could not find a unique path after 1000 attempts
    Err(TransferError::IoError)
}

/// Hash a file into a StreamingHasher, sending FileHashing keepalives periodically
///
/// Reads the file from the beginning up to `byte_count` bytes, feeding each chunk
/// into the hasher. Sends ClientMessage::FileHashing keepalive messages to the server
/// periodically to prevent timeout during large file hashing.
///
/// Supports cancellation via the optional `cancel_flag`.
pub async fn hash_file_with_keepalives<W>(
    path: &Path,
    byte_count: u64,
    file_name: String,
    writer: &mut FrameWriter<W>,
    cancel_flag: &Option<Arc<AtomicBool>>,
) -> Result<StreamingHasher, TransferError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use nexus_common::{HASH_BUFFER_SIZE, KEEPALIVE_INTERVAL};
    use std::time::Instant;
    use tokio::io::AsyncReadExt;

    let mut hasher = StreamingHasher::new();
    let file = File::open(path).await.map_err(|_| TransferError::IoError)?;
    let mut reader = tokio::io::BufReader::new(file);
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];
    let mut remaining = byte_count;
    let mut last_keepalive = Instant::now();

    while remaining > 0 {
        // Check for cancellation
        if is_cancelled(cancel_flag) {
            return Err(TransferError::Cancelled);
        }

        let to_read = (remaining as usize).min(buffer.len());
        let bytes_read = reader
            .read(&mut buffer[..to_read])
            .await
            .map_err(|_| TransferError::IoError)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read as u64;

        // Send keepalive periodically to prevent server timeout
        if last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            let msg = ClientMessage::FileHashing {
                file: file_name.clone(),
            };
            if let Err(e) = send_client_message(writer, &msg).await {
                eprintln!("{}{:?}", ERR_HASH_KEEPALIVE_PREFIX, e);
            }
            last_keepalive = Instant::now();
        }
    }

    Ok(hasher)
}

// =============================================================================
// File Scanning (for uploads)
// =============================================================================

/// Scan local files for upload
///
/// For a single file, returns one entry with the filename as the relative path.
/// For a directory, recursively scans and returns all files with relative paths.
/// Returns paths and sizes only — hashes are computed lazily during transfer.
pub async fn scan_local_files(
    local_path: &Path,
    is_directory: bool,
) -> Result<Vec<LocalFileInfo>, TransferError> {
    if is_directory {
        scan_directory(local_path, local_path).await
    } else {
        // Single file
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|_| TransferError::NotFound)?;

        if !metadata.is_file() {
            return Err(TransferError::Invalid);
        }

        let filename = local_path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(TransferError::Invalid)?
            .to_string();

        Ok(vec![LocalFileInfo {
            relative_path: filename,
            absolute_path: local_path.to_path_buf(),
            size: metadata.len(),
        }])
    }
}

/// Recursively scan a directory for files
///
/// Uses Box::pin to handle the recursive async call.
fn scan_directory<'a>(
    base_path: &'a Path,
    current_path: &'a Path,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<LocalFileInfo>, TransferError>> + Send + 'a>,
> {
    Box::pin(async move {
        let mut files = Vec::new();
        let mut entries = tokio::fs::read_dir(current_path)
            .await
            .map_err(|_| TransferError::IoError)?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|_| TransferError::IoError)?
        {
            let path = entry.path();
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|_| TransferError::IoError)?;

            if metadata.is_dir() {
                // Recurse into subdirectory
                let mut subdir_files = scan_directory(base_path, &path).await?;
                files.append(&mut subdir_files);
            } else if metadata.is_file() {
                // Compute relative path from base
                let relative_path = path
                    .strip_prefix(base_path)
                    .map_err(|_| TransferError::Invalid)?
                    .to_str()
                    .ok_or(TransferError::Invalid)?
                    .to_string();

                // Normalize path separators to forward slashes (for cross-platform compatibility)
                let relative_path = relative_path.replace('\\', "/");

                files.push(LocalFileInfo {
                    relative_path,
                    absolute_path: path,
                    size: metadata.len(),
                });
            }
            // Skip special file types (sockets, pipes, etc.)
            // Note: symlinks are followed automatically by metadata()
        }

        Ok(files)
    })
}

/// Open a file and seek to a specific offset for resume
pub async fn open_file_for_upload(path: &Path, offset: u64) -> Result<File, TransferError> {
    let mut file = File::open(path).await.map_err(|_| TransferError::IoError)?;

    if offset > 0 {
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|_| TransferError::IoError)?;
    }

    Ok(file)
}

/// Check if the transfer has been cancelled
pub fn is_cancelled(cancel_flag: &Option<Arc<AtomicBool>>) -> bool {
    cancel_flag
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
}

/// Validate that a path from the server is safe
///
/// Rejects absolute paths, paths with "..", and other dangerous patterns
pub fn is_safe_path(path: &str) -> bool {
    // Reject empty paths
    if path.is_empty() {
        return false;
    }

    // Reject null bytes (defense-in-depth - server also validates)
    if path.contains('\0') {
        return false;
    }

    // Reject control characters (0x00-0x1F and 0x7F)
    if path.chars().any(|c| c.is_ascii_control()) {
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

        // Reject current directory references anywhere (e.g., "./foo", "foo/./bar")
        // These serve no purpose and could be used to confuse path matching/logging
        if component == "." {
            return false;
        }
    }

    true
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::framing::FrameWriter;
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tokio::io::AsyncReadExt;

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
    fn test_is_safe_path_rejects_null_bytes() {
        assert!(!is_safe_path("foo\0bar"));
        assert!(!is_safe_path("dir/file\0.txt"));
        assert!(!is_safe_path("\0"));
    }

    #[test]
    fn test_is_safe_path_rejects_control_chars() {
        assert!(!is_safe_path("foo\x01bar"));
        assert!(!is_safe_path("dir\x1f/file.txt"));
        assert!(!is_safe_path("file\x7f.txt"));
        assert!(!is_safe_path("\t"));
        assert!(!is_safe_path("\n"));
        assert!(!is_safe_path("\r"));
    }

    #[test]
    fn test_is_safe_path_allows_unicode() {
        assert!(is_safe_path("文件/test.txt"));
        assert!(is_safe_path("données/fichier.pdf"));
        assert!(is_safe_path("ファイル.txt"));
    }

    #[test]
    fn test_is_safe_path_rejects_dot_components() {
        // Reject "." anywhere in path - serves no purpose and could confuse logging
        assert!(!is_safe_path("./file.txt"));
        assert!(!is_safe_path(".\\file.txt"));
        assert!(!is_safe_path("foo/./bar"));
        assert!(!is_safe_path("dir/./subdir/file.txt"));
        assert!(!is_safe_path("."));
    }

    #[tokio::test]
    async fn test_scan_local_files_single_file() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        let content = "hello world";
        tokio::fs::write(&file_path, content)
            .await
            .expect("write file");

        let files = scan_local_files(&file_path, false)
            .await
            .expect("scan files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "test.txt");
        assert_eq!(files[0].size, content.len() as u64);
    }

    #[tokio::test]
    async fn test_scan_local_files_directory() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");

        // Create some files
        tokio::fs::write(temp_dir.path().join("file1.txt"), "content1")
            .await
            .expect("write file1");

        tokio::fs::create_dir(temp_dir.path().join("subdir"))
            .await
            .expect("create subdir");
        tokio::fs::write(temp_dir.path().join("subdir/file2.txt"), "content2")
            .await
            .expect("write file2");

        let files = scan_local_files(temp_dir.path(), true)
            .await
            .expect("scan files");
        assert_eq!(files.len(), 2);

        // Sort by path for predictable ordering
        let mut paths: Vec<_> = files.iter().map(|f| f.relative_path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["file1.txt", "subdir/file2.txt"]);
    }

    #[tokio::test]
    async fn test_scan_local_files_empty_directory() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");

        let files = scan_local_files(temp_dir.path(), true)
            .await
            .expect("scan files");
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_open_file_for_upload() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, "hello world")
            .await
            .expect("write file");

        // Open at offset 0
        let mut file = open_file_for_upload(&file_path, 0)
            .await
            .expect("open file");
        let mut buf = vec![0u8; 5];
        file.read_exact(&mut buf).await.expect("read");
        assert_eq!(&buf, b"hello");

        // Open at offset 6
        let mut file = open_file_for_upload(&file_path, 6)
            .await
            .expect("open file");
        let mut buf = vec![0u8; 5];
        file.read_exact(&mut buf).await.expect("read");
        assert_eq!(&buf, b"world");
    }

    #[tokio::test]
    async fn test_hash_file_with_keepalives_basic() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("hello.txt");
        tokio::fs::write(&file_path, "Hello, World!")
            .await
            .expect("write file");

        let mut writer = FrameWriter::new(Cursor::new(Vec::new()));
        let hasher =
            hash_file_with_keepalives(&file_path, 13, "hello.txt".to_string(), &mut writer, &None)
                .await
                .expect("hash file");

        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hash_file_with_keepalives_partial_then_continue() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("hello.txt");
        tokio::fs::write(&file_path, "Hello, World!")
            .await
            .expect("write file");

        let mut writer = FrameWriter::new(Cursor::new(Vec::new()));
        let mut hasher =
            hash_file_with_keepalives(&file_path, 5, "hello.txt".to_string(), &mut writer, &None)
                .await
                .expect("hash file");

        // Partial hash should equal SHA-256 of "Hello"
        let partial = hasher.partial_hash();
        assert_eq!(
            partial,
            "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
        );

        // Feed the remaining bytes and verify the full hash
        hasher.update(b", World!");
        let full_hash = hasher.finalize();
        assert_eq!(
            full_hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hash_file_with_keepalives_empty() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("empty.txt");
        tokio::fs::write(&file_path, "").await.expect("write file");

        let mut writer = FrameWriter::new(Cursor::new(Vec::new()));
        let hasher =
            hash_file_with_keepalives(&file_path, 0, "empty.txt".to_string(), &mut writer, &None)
                .await
                .expect("hash file");

        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_hash_file_with_keepalives_cancellation() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let file_path = temp_dir.path().join("cancel.txt");
        tokio::fs::write(&file_path, "some content to hash")
            .await
            .expect("write file");

        let cancel_flag = Arc::new(AtomicBool::new(true));
        let mut writer = FrameWriter::new(Cursor::new(Vec::new()));
        let result = hash_file_with_keepalives(
            &file_path,
            20,
            "cancel.txt".to_string(),
            &mut writer,
            &Some(cancel_flag),
        )
        .await;

        assert_eq!(result.unwrap_err(), TransferError::Cancelled);
    }
}
