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
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio::time::Instant;

use nexus_common::framing::FrameWriter;
use nexus_common::hash::StreamingHasher;
use nexus_common::protocol::ClientMessage;
use nexus_common::{FALLBACK_FILE_NAME, HASH_BUFFER_SIZE, KEEPALIVE_INTERVAL};

use super::streaming::send_transfer_control_message;
use super::{PART_SUFFIX, TransferError};
use crate::transfers::is_safe_download_name;

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
    let file = File::open(path).await.map_err(|_| TransferError::IoError)?;
    let mut reader = tokio::io::BufReader::new(file);
    hash_reader_with_keepalives(&mut reader, byte_count, file_name, writer, cancel_flag).await
}

async fn hash_reader_with_keepalives<R, W>(
    reader: &mut R,
    byte_count: u64,
    file_name: String,
    writer: &mut FrameWriter<W>,
    cancel_flag: &Option<Arc<AtomicBool>>,
) -> Result<StreamingHasher, TransferError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut hasher = StreamingHasher::new();
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

        // Send keepalive periodically to prevent server timeout.
        if last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            let msg = ClientMessage::FileHashing {
                file: file_name.clone(),
            };
            send_transfer_control_message(writer, &msg).await?;
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

/// Validate a relative FileStart path using native filename rules.
pub fn is_safe_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') {
        return false;
    }

    // Protocol paths use '/'. A backslash is a literal filename character on Unix,
    // and is rejected by the component validator on Windows. Preserve tolerance
    // for repeated/trailing separators without normalizing away '.' or '..'.
    path.split('/')
        .filter(|component| !component.is_empty())
        .all(is_safe_download_name)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::framing::FrameWriter;
    use std::future::Future;
    use std::io::{self, Cursor};
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::task::{Context, Poll, ready};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
    use tokio::time::{Sleep, sleep, timeout};

    use super::super::test_helpers::{FailingWriter, WriteFailure};

    struct SlowHashReader {
        delay: Pin<Box<Sleep>>,
        reads: usize,
        is_failed: bool,
    }

    impl SlowHashReader {
        fn new(is_failed: bool) -> Self {
            Self {
                delay: Box::pin(sleep(KEEPALIVE_INTERVAL + Duration::from_secs(1))),
                reads: 0,
                is_failed,
            }
        }
    }

    impl AsyncRead for SlowHashReader {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            ready!(this.delay.as_mut().poll(cx));
            this.reads += 1;
            if this.is_failed {
                return Poll::Ready(Err(io::Error::other("injected file read failure")));
            }
            buf.put_slice(b"x");
            this.delay = Box::pin(sleep(KEEPALIVE_INTERVAL + Duration::from_secs(1)));
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_hashing_stops_after_failed_keepalive() {
        for failure in [
            WriteFailure::Error,
            WriteFailure::Zero,
            WriteFailure::Stall,
            WriteFailure::FlushError,
            WriteFailure::FlushStall,
        ] {
            let mut reader = SlowHashReader::new(false);
            let mut writer = FrameWriter::new(FailingWriter::new(0, 8, failure));
            let result = timeout(
                nexus_common::TRANSFER_IO_PROGRESS_TIMEOUT * 2,
                hash_reader_with_keepalives(&mut reader, 3, "test.txt".into(), &mut writer, &None),
            )
            .await
            .unwrap();
            assert!(
                matches!(result, Err(TransferError::ConnectionError)),
                "{failure:?}"
            );
            assert_eq!(
                reader.reads, 1,
                "must stop reading file bytes after {failure:?}"
            );
            if matches!(
                failure,
                WriteFailure::Error | WriteFailure::Zero | WriteFailure::Stall
            ) {
                assert_eq!(writer.get_ref().bytes.len(), 8);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_hashing_preserves_file_error_classification() {
        let mut reader = SlowHashReader::new(true);
        let mut writer = FrameWriter::new(Vec::new());
        let result =
            hash_reader_with_keepalives(&mut reader, 3, "test.txt".into(), &mut writer, &None)
                .await;
        assert!(matches!(result, Err(TransferError::IoError)));
        assert!(writer.get_ref().is_empty());
    }

    #[test]
    fn test_is_safe_path_valid() {
        assert!(is_safe_path("file.txt"));
        assert!(is_safe_path("dir/file.txt"));
        assert!(is_safe_path("dir/subdir/file.txt"));
        assert!(is_safe_path("Games/app.zip"));
        assert!(is_safe_path("Documents/report.pdf"));
        assert!(is_safe_path("dir//file.txt"));
        assert!(is_safe_path("dir/file.txt/"));
        assert!(is_safe_path("file\x7f.txt"));
    }

    #[test]
    fn test_is_safe_path_rejects_absolute() {
        assert!(!is_safe_path("/etc/passwd"));
        assert!(!is_safe_path("/home/user/file.txt"));
        assert!(!is_safe_path("//server/share"));
    }

    #[test]
    fn test_is_safe_path_rejects_parent_refs() {
        assert!(!is_safe_path(".."));
        assert!(!is_safe_path("../file.txt"));
        assert!(!is_safe_path("dir/../file.txt"));
        assert!(!is_safe_path("dir/subdir/../../file.txt"));
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
    fn test_is_safe_path_windows_filename_rules_only_apply_on_windows() {
        for path in [
            "\\Windows\\System32",
            "C:\\Windows\\System32",
            "D:file.txt",
            "dir/C:track.mp3",
            "dir\\..\\file.txt",
            ".\\file.txt",
            "dir/\\rooted/file.txt",
            "dir/file:stream",
            "dir/.. /file.txt",
            "dir/.../file.txt",
            "dir/file.",
            "dir/file ",
            "dir/file?name",
            "foo\x01bar",
            "dir\x1f/file.txt",
            "\t",
            "\n",
            "\r",
        ] {
            assert_eq!(is_safe_path(path), !cfg!(windows), "{path:?}");
        }
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
        assert!(!is_safe_path("foo/./bar"));
        assert!(!is_safe_path("dir/./subdir/file.txt"));
        assert!(!is_safe_path("."));
    }

    #[test]
    fn test_is_safe_path_windows_reserved_names_are_platform_specific() {
        for path in [
            "CON", "CON ", "CON.", "con", "PRN", "AUX", "NUL", "NUL ", "COM1", "COM1 ", "COM9",
            "LPT1", "LPT9",
        ] {
            assert_eq!(is_safe_path(path), !cfg!(windows), "{path:?}");
        }
    }

    #[test]
    fn test_is_safe_path_windows_reserved_extensions_are_platform_specific() {
        for path in [
            "CON.txt",
            "dir/NUL.log",
            "dir/subdir/LPT1.tar.gz",
            "com1.backup",
            "COM1 .txt",
            "dir/NUL .log",
        ] {
            assert_eq!(is_safe_path(path), !cfg!(windows), "{path:?}");
        }
    }

    #[test]
    fn test_is_safe_path_checks_reserved_names_at_every_depth() {
        for name in [
            "CON",
            "NUL",
            "PRN",
            "AUX",
            "COM1",
            "LPT9",
            "COM\u{b9}",
            "COM\u{b2}",
            "COM\u{b3}",
            "LPT\u{b9}",
            "LPT\u{b2}",
            "LPT\u{b3}",
        ] {
            for path in [
                name.to_string(),
                format!("artist/album/{name}.txt"),
                format!("artist/{name}/track.mp3"),
                format!("artist/{name} .ext/track.mp3"),
                format!("artist\\{name}\\track.mp3"),
            ] {
                assert_eq!(is_safe_path(&path), !cfg!(windows), "{path:?}");
            }
        }
        assert!(is_safe_path("artist/album/track.mp3"));
    }

    #[test]
    fn test_is_safe_path_allows_reserved_name_prefixes() {
        assert!(is_safe_path("CONSOLE.txt"));
        assert!(is_safe_path("COM10.txt"));
        assert!(is_safe_path("LPT10.log"));
        assert!(is_safe_path("NULLED"));
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
            "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
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

        // Partial hash should equal BLAKE3 of "Hello"
        let partial = hasher.partial_hash();
        assert_eq!(
            partial,
            "fbc2b0516ee8744d293b980779178a3508850fdcfe965985782c39601b65794f"
        );

        // Feed the remaining bytes and verify the full hash
        hasher.update(b", World!");
        let full_hash = hasher.finalize();
        assert_eq!(
            full_hash,
            "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
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
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
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
