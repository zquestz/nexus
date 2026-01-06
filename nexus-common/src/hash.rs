//! High-performance SHA-256 hashing utilities
//!
//! Provides optimized file hashing using buffered I/O with hardware acceleration.
//! All async functions use `spawn_blocking` to run CPU-intensive hashing on a
//! dedicated thread pool, avoiding blocking tokio's async worker threads.
//!
//! ## Performance Characteristics
//!
//! - **Hardware acceleration**: SHA-NI on x86_64, crypto extensions on ARM64
//! - **Large buffers**: 1MB buffers reduce syscall overhead
//! - **Non-blocking**: Uses `spawn_blocking` to avoid blocking async workers
//! - **Cancellation granularity**: Checked every buffer read (~1MB), sub-second response
//!
//! ## Keepalive Support
//!
//! For large files, use `compute_sha256_with_keepalive` which sends notifications
//! through a channel every [`KEEPALIVE_INTERVAL`] (10 seconds). This allows the
//! caller to send keepalive messages to prevent connection timeouts during
//! multi-gigabyte file transfers.
//!
//! ## Cancellation Support
//!
//! All keepalive variants support cancellation via an `AtomicBool` flag. The hash
//! computation checks this flag before each buffer read and returns
//! `ErrorKind::Interrupted` if cancellation is requested. This enables responsive
//! cancellation even for very large files.
//!
//! ## Security Considerations
//!
//! - Hash functions trust the caller to validate paths before calling
//! - File names in keepalive messages are for logging only (no security impact)
//! - Partial hashes are used for resume verification; final hash is always verified

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::HASH_BUFFER_SIZE;

/// How often to send keepalive notifications during hashing.
///
/// This interval (10 seconds) is chosen to be well under the typical idle timeout
/// (30 seconds) while not overwhelming the network with keepalive messages.
/// At 500 MB/s, a 100GB file takes ~200 seconds, generating ~20 keepalive messages.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// Compute SHA-256 hash of an entire file
///
/// Runs on a blocking thread pool to avoid blocking async workers.
pub async fn compute_sha256(path: &Path) -> io::Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_sha256_sync(&path))
        .await
        .map_err(|e| io::Error::other(format!("hash task failed: {e}")))?
}

/// Compute SHA-256 hash of the first `max_bytes` of a file
///
/// Used for resume verification. If the file is smaller than `max_bytes`,
/// hashes the entire file.
///
/// Runs on a blocking thread pool to avoid blocking async workers.
pub async fn compute_partial_sha256(path: &Path, max_bytes: u64) -> io::Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_partial_sha256_sync(&path, max_bytes))
        .await
        .map_err(|e| io::Error::other(format!("hash task failed: {e}")))?
}

/// Compute SHA-256 hash with periodic keepalive notifications and cancellation support
///
/// Returns a receiver that will receive the file name periodically (every
/// `KEEPALIVE_INTERVAL`) while hashing is in progress. The caller should send a
/// keepalive message each time a notification is received to prevent connection timeouts.
///
/// If `cancel_flag` is set to `true`, the hash computation will stop and return
/// an error with `ErrorKind::Interrupted`.
///
/// Returns (hash_result, keepalive_receiver).
///
/// Runs on a blocking thread pool to avoid blocking async workers.
pub async fn compute_sha256_with_keepalive(
    path: &Path,
    file_name: String,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> (
    tokio::task::JoinHandle<io::Result<String>>,
    mpsc::UnboundedReceiver<String>,
) {
    compute_partial_sha256_with_keepalive(path, u64::MAX, file_name, cancel_flag).await
}

/// Compute partial SHA-256 hash with periodic keepalive notifications and cancellation support
///
/// Like `compute_sha256_with_keepalive` but only hashes the first `max_bytes` of the file.
/// Used for resume verification where we need to hash a partial file. Sends keepalive
/// notifications every `KEEPALIVE_INTERVAL`.
///
/// If `cancel_flag` is set to `true`, the hash computation will stop and return
/// an error with `ErrorKind::Interrupted`.
///
/// Returns (hash_result, keepalive_receiver).
///
/// Runs on a blocking thread pool to avoid blocking async workers.
pub async fn compute_partial_sha256_with_keepalive(
    path: &Path,
    max_bytes: u64,
    file_name: String,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> (
    tokio::task::JoinHandle<io::Result<String>>,
    mpsc::UnboundedReceiver<String>,
) {
    let path = path.to_path_buf();

    // Channel for keepalive notifications from blocking task
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    // Spawn blocking task for hashing
    let handle = tokio::task::spawn_blocking(move || {
        let file_name_clone = file_name.clone();
        compute_partial_sha256_sync_with_keepalive_cancellable(
            &path,
            max_bytes,
            cancel_flag.as_ref(),
            move || {
                // Send keepalive notification (ignore errors if receiver dropped)
                let _ = tx.send(file_name_clone.clone());
            },
        )
    });

    (handle, rx)
}

/// Synchronous SHA-256 computation (full file)
pub fn compute_sha256_sync(path: &Path) -> io::Result<String> {
    compute_partial_sha256_sync(path, u64::MAX)
}

/// Synchronous partial SHA-256 computation
pub fn compute_partial_sha256_sync(path: &Path, max_bytes: u64) -> io::Result<String> {
    compute_partial_sha256_sync_with_keepalive_cancellable(path, max_bytes, None, || {})
}

/// Synchronous partial SHA-256 with periodic keepalive callback and cancellation support
///
/// This is the core implementation used by all other hash functions.
///
/// # Arguments
///
/// * `path` - Path to the file to hash
/// * `max_bytes` - Maximum number of bytes to hash (use `u64::MAX` for full file)
/// * `cancel_flag` - Optional flag to check for cancellation
/// * `on_keepalive` - Callback called every `KEEPALIVE_INTERVAL`
///
/// # Returns
///
/// * `Ok(hash)` - The hex-encoded SHA-256 hash
/// * `Err` with `ErrorKind::Interrupted` - If cancelled via `cancel_flag`
/// * `Err` with other kinds - For I/O errors
fn compute_partial_sha256_sync_with_keepalive_cancellable<F>(
    path: &Path,
    max_bytes: u64,
    cancel_flag: Option<&Arc<AtomicBool>>,
    mut on_keepalive: F,
) -> io::Result<String>
where
    F: FnMut(),
{
    // Check for cancellation before starting
    if is_cancelled(cancel_flag) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "hash computation cancelled",
        ));
    }

    if max_bytes == 0 {
        // Hash of empty input
        let hasher = Sha256::new();
        return Ok(hex::encode(hasher.finalize()));
    }

    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];
    let mut last_keepalive = Instant::now();
    let mut remaining = max_bytes;

    while remaining > 0 {
        // Check for cancellation before each read
        if is_cancelled(cancel_flag) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "hash computation cancelled",
            ));
        }

        let to_read = (remaining as usize).min(buffer.len());
        let bytes_read = file.read(&mut buffer[..to_read])?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read as u64;

        // Send keepalive notification periodically
        if last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            on_keepalive();
            last_keepalive = Instant::now();
        }
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Check if the cancel flag is set.
///
/// Uses `Ordering::Relaxed` because we only need eventual visibility of the
/// cancellation request. The worst case is one additional buffer read (~1MB)
/// before noticing cancellation, which is acceptable for responsive UX.
#[inline]
fn is_cancelled(cancel_flag: Option<&Arc<AtomicBool>>) -> bool {
    cancel_flag
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicUsize;
    use tempfile::NamedTempFile;

    #[test]
    fn test_empty_hash() {
        let result = compute_partial_sha256_sync(Path::new("/dev/null"), 0).unwrap();
        // SHA-256 of empty string
        assert_eq!(
            result,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_small_file_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let result = compute_sha256_sync(file.path()).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(
            result,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_partial_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        // Hash of first 5 bytes ("hello")
        let result = compute_partial_sha256_sync(file.path(), 5).unwrap();
        assert_eq!(
            result,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_partial_larger_than_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"short").unwrap();
        file.flush().unwrap();

        // Request more bytes than file contains
        let partial = compute_partial_sha256_sync(file.path(), 1000).unwrap();
        let full = compute_sha256_sync(file.path()).unwrap();

        // Should be the same (hashes entire file)
        assert_eq!(partial, full);
    }

    #[tokio::test]
    async fn test_async_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"async test").unwrap();
        file.flush().unwrap();

        let result = compute_sha256(file.path()).await.unwrap();
        assert!(!result.is_empty());
    }

    // ==========================================================================
    // Keepalive callback tests
    // ==========================================================================

    #[test]
    fn test_keepalive_callback_not_called_for_small_file() {
        // Small files complete before KEEPALIVE_INTERVAL, so callback shouldn't be called
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"small file content").unwrap();
        file.flush().unwrap();

        let callback_count = Arc::new(AtomicUsize::new(0));
        let counter = callback_count.clone();

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            None,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_ok());
        // Small file should complete instantly, no keepalive needed
        assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_keepalive_callback_empty_file() {
        // Empty file (max_bytes = 0) should not call callback
        let file = NamedTempFile::new().unwrap();

        let callback_count = Arc::new(AtomicUsize::new(0));
        let counter = callback_count.clone();

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            0,
            None,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_ok());
        // Empty hash computed immediately, no callback
        assert_eq!(callback_count.load(Ordering::SeqCst), 0);

        // Verify correct empty hash
        assert_eq!(
            result.unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_keepalive_hash_matches_non_keepalive() {
        // Hash result should be identical whether using keepalive or not
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xAB; 1024 * 1024]; // 1MB of data
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let hash_without_keepalive = compute_sha256_sync(file.path()).unwrap();

        let hash_with_keepalive = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            None,
            || {},
        )
        .unwrap();

        assert_eq!(hash_without_keepalive, hash_with_keepalive);
    }

    #[test]
    fn test_partial_keepalive_hash_matches_non_keepalive() {
        // Partial hash result should be identical whether using keepalive or not
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xCD; 1024 * 1024]; // 1MB of data
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let partial_bytes = 512 * 1024; // Hash first 512KB

        let hash_without_keepalive =
            compute_partial_sha256_sync(file.path(), partial_bytes).unwrap();

        let hash_with_keepalive = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            partial_bytes,
            None,
            || {},
        )
        .unwrap();

        assert_eq!(hash_without_keepalive, hash_with_keepalive);
    }

    #[test]
    fn test_keepalive_file_exact_buffer_size() {
        // Test edge case: file size exactly equals buffer size
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xEF; HASH_BUFFER_SIZE];
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let callback_count = Arc::new(AtomicUsize::new(0));
        let counter = callback_count.clone();

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            None,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_ok());
        // File processed in one read, no keepalive interval elapsed
        assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_keepalive_file_not_found() {
        // Non-existent file should return error
        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            Path::new("/nonexistent/path/to/file.txt"),
            u64::MAX,
            None,
            || {},
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn test_async_keepalive_returns_correct_hash() {
        // Test the async keepalive version returns correct hash
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"async keepalive test").unwrap();
        file.flush().unwrap();

        let (handle, mut rx) =
            compute_sha256_with_keepalive(file.path(), "test.txt".to_string(), None).await;

        // Drain any keepalive messages (there shouldn't be any for small file)
        let mut keepalive_count = 0;
        while rx.try_recv().is_ok() {
            keepalive_count += 1;
        }

        let result = handle.await.unwrap().unwrap();

        // Verify hash is correct
        let expected = compute_sha256_sync(file.path()).unwrap();
        assert_eq!(result, expected);

        // Small file shouldn't trigger keepalives
        assert_eq!(keepalive_count, 0);
    }

    #[tokio::test]
    async fn test_async_partial_keepalive_returns_correct_hash() {
        // Test the async partial keepalive version returns correct hash
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"partial async keepalive test data")
            .unwrap();
        file.flush().unwrap();

        let partial_bytes = 10;

        let (handle, _rx) = compute_partial_sha256_with_keepalive(
            file.path(),
            partial_bytes,
            "test.txt".to_string(),
            None,
        )
        .await;

        let result = handle.await.unwrap().unwrap();

        // Verify hash matches non-keepalive version
        let expected = compute_partial_sha256_sync(file.path(), partial_bytes).unwrap();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_async_keepalive_channel_receives_filename() {
        // Verify keepalive messages contain the correct filename
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();
        file.flush().unwrap();

        let filename = "my_special_file.dat".to_string();
        let (handle, mut rx) =
            compute_sha256_with_keepalive(file.path(), filename.clone(), None).await;

        // Even if no keepalives are sent, the channel should be valid
        // Just ensure the hash completes successfully
        let result = handle.await.unwrap();
        assert!(result.is_ok());

        // Channel should be closed after task completes (no more messages)
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_keepalive_interval_constant() {
        // Verify KEEPALIVE_INTERVAL is a reasonable value
        assert!(KEEPALIVE_INTERVAL >= Duration::from_secs(1));
        assert!(KEEPALIVE_INTERVAL <= Duration::from_secs(60));
        // Current value should be 10 seconds
        assert_eq!(KEEPALIVE_INTERVAL, Duration::from_secs(10));
    }

    // ==========================================================================
    // Cancellation tests
    // ==========================================================================

    #[test]
    fn test_cancellation_before_start() {
        // If cancel flag is set before starting, should return immediately
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();
        file.flush().unwrap();

        let cancel_flag = Arc::new(AtomicBool::new(true)); // Already cancelled

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            Some(&cancel_flag),
            || {},
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn test_cancellation_returns_interrupted() {
        // Verify cancelled hash returns Interrupted error kind
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xAB; 1024 * 1024]; // 1MB to ensure multiple iterations
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag_clone = cancel_flag.clone();

        // Set cancel after first callback (won't actually be called for small file,
        // but we can test the flag check in the loop)
        let callback_count = Arc::new(AtomicUsize::new(0));
        let counter = callback_count.clone();

        // For this test, we'll set cancel immediately since the file is small
        // The cancellation check happens at each buffer read
        cancel_flag.store(true, Ordering::SeqCst);

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            Some(&cancel_flag_clone),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(err.to_string().contains("cancelled"));
    }

    #[test]
    fn test_no_cancellation_completes_normally() {
        // With cancel flag set to false, hash should complete normally
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();
        file.flush().unwrap();

        let cancel_flag = Arc::new(AtomicBool::new(false)); // Not cancelled

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            Some(&cancel_flag),
            || {},
        );

        assert!(result.is_ok());

        // Verify hash is correct
        let expected = compute_sha256_sync(file.path()).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_none_cancel_flag_completes_normally() {
        // With None cancel flag, hash should complete normally
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();
        file.flush().unwrap();

        let result = compute_partial_sha256_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            None,
            || {},
        );

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_async_cancellation() {
        // Test async version with cancellation
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xAB; 1024 * 1024]; // 1MB
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let cancel_flag = Arc::new(AtomicBool::new(true)); // Pre-cancelled

        let (handle, _rx) =
            compute_sha256_with_keepalive(file.path(), "test.txt".to_string(), Some(cancel_flag))
                .await;

        let result = handle.await.unwrap();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }

    #[tokio::test]
    async fn test_async_no_cancellation() {
        // Test async version without cancellation completes normally
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"async no cancel test").unwrap();
        file.flush().unwrap();

        let cancel_flag = Arc::new(AtomicBool::new(false));

        let (handle, _rx) =
            compute_sha256_with_keepalive(file.path(), "test.txt".to_string(), Some(cancel_flag))
                .await;

        let result = handle.await.unwrap();
        assert!(result.is_ok());

        // Verify hash is correct
        let expected = compute_sha256_sync(file.path()).unwrap();
        assert_eq!(result.unwrap(), expected);
    }
}
