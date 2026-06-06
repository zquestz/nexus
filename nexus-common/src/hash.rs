//! High-performance BLAKE3 hashing utilities
//!
//! Provides optimized file hashing using buffered I/O with hardware acceleration.
//! All async functions use `spawn_blocking` to run CPU-intensive hashing on a
//! dedicated thread pool, avoiding blocking tokio's async worker threads.
//!
//! ## Performance Characteristics
//!
//! - **Hardware acceleration**: SIMD-optimized BLAKE3 implementation
//! - **Large buffers**: 1MB buffers reduce syscall overhead
//! - **Non-blocking**: Uses `spawn_blocking` to avoid blocking async workers
//! - **Cancellation granularity**: Checked every buffer read (~1MB), sub-second response
//!
//! ## Cancellation Support
//!
//! The core hash function supports cancellation via an `AtomicBool` flag. The hash
//! computation checks this flag before each buffer read and returns
//! `ErrorKind::Interrupted` if cancellation is requested. This enables responsive
//! cancellation even for very large files.
//!
//! ## Streaming Support
//!
//! [`StreamingHasher`] provides incremental hashing for streaming transfers.
//! It supports computing intermediate hashes without consuming state (`partial_hash`)
//! without losing state, enabling single-pass hashing across resume verification
//! and streaming phases.
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
use std::time::Instant;

use blake3::Hasher;

use crate::{HASH_BUFFER_SIZE, KEEPALIVE_INTERVAL};

/// Compute BLAKE3 hash of an entire file
///
/// Runs on a blocking thread pool to avoid blocking async workers.
pub async fn compute_blake3(path: &Path) -> io::Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_blake3_sync(&path))
        .await
        .map_err(|e| io::Error::other(format!("hash task failed: {e}")))?
}

/// Compute BLAKE3 hash of the first `max_bytes` of a file
///
/// Used for resume verification. If the file is smaller than `max_bytes`,
/// hashes the entire file.
///
/// Runs on a blocking thread pool to avoid blocking async workers.
pub async fn compute_partial_blake3(path: &Path, max_bytes: u64) -> io::Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_partial_blake3_sync(&path, max_bytes))
        .await
        .map_err(|e| io::Error::other(format!("hash task failed: {e}")))?
}

/// Synchronous BLAKE3 computation (full file)
pub fn compute_blake3_sync(path: &Path) -> io::Result<String> {
    compute_partial_blake3_sync(path, u64::MAX)
}

/// Synchronous partial BLAKE3 computation
pub fn compute_partial_blake3_sync(path: &Path, max_bytes: u64) -> io::Result<String> {
    compute_partial_blake3_sync_with_keepalive_cancellable(path, max_bytes, None, || {})
}

/// Incremental BLAKE3 hasher for streaming transfers.
///
/// Supports computing intermediate hashes without losing
/// state, enabling single-pass hashing across resume verification and streaming
/// phases.
pub struct StreamingHasher {
    hasher: Hasher,
}

impl std::fmt::Debug for StreamingHasher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingHasher").finish_non_exhaustive()
    }
}

impl Default for StreamingHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingHasher {
    /// Creates a new incremental BLAKE3 hasher.
    pub fn new() -> Self {
        Self {
            hasher: Hasher::new(),
        }
    }

    /// Feeds data into the hasher.
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Returns the current hex string without consuming the hasher.
    pub fn partial_hash(&self) -> String {
        self.hasher.finalize().to_hex().to_string()
    }

    /// Consumes the hasher and returns the final hex-encoded BLAKE3 hash.
    pub fn finalize(self) -> String {
        self.hasher.finalize().to_hex().to_string()
    }
}

/// Synchronous partial BLAKE3 with periodic keepalive callback and cancellation support
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
/// * `Ok(hash)` - The hex-encoded BLAKE3 hash
/// * `Err` with `ErrorKind::Interrupted` - If cancelled via `cancel_flag`
/// * `Err` with other kinds - For I/O errors
fn compute_partial_blake3_sync_with_keepalive_cancellable<F>(
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
        return Ok(blake3::hash(b"").to_hex().to_string());
    }

    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
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

    Ok(hasher.finalize().to_hex().to_string())
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
        let result = compute_partial_blake3_sync(Path::new("/dev/null"), 0).unwrap();
        // BLAKE3 of empty string
        assert_eq!(
            result,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn test_small_file_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let result = compute_blake3_sync(file.path()).unwrap();
        // BLAKE3 of "hello world"
        assert_eq!(
            result,
            "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );
    }

    #[test]
    fn test_partial_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        // Hash of first 5 bytes ("hello")
        let result = compute_partial_blake3_sync(file.path(), 5).unwrap();
        assert_eq!(
            result,
            "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f"
        );
    }

    #[test]
    fn test_partial_larger_than_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"short").unwrap();
        file.flush().unwrap();

        // Request more bytes than file contains
        let partial = compute_partial_blake3_sync(file.path(), 1000).unwrap();
        let full = compute_blake3_sync(file.path()).unwrap();

        // Should be the same (hashes entire file)
        assert_eq!(partial, full);
    }

    #[tokio::test]
    async fn test_async_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"async test").unwrap();
        file.flush().unwrap();

        let result = compute_blake3(file.path()).await.unwrap();
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

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
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

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
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
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn test_keepalive_hash_matches_non_keepalive() {
        // Hash result should be identical whether using keepalive or not
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xAB; 1024 * 1024]; // 1MB of data
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let hash_without_keepalive = compute_blake3_sync(file.path()).unwrap();

        let hash_with_keepalive = compute_partial_blake3_sync_with_keepalive_cancellable(
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
            compute_partial_blake3_sync(file.path(), partial_bytes).unwrap();

        let hash_with_keepalive = compute_partial_blake3_sync_with_keepalive_cancellable(
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

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
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
        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
            Path::new("/nonexistent/path/to/file.txt"),
            u64::MAX,
            None,
            || {},
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_keepalive_interval_constant() {
        use std::time::Duration;
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

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
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

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
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

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            Some(&cancel_flag),
            || {},
        );

        assert!(result.is_ok());

        // Verify hash is correct
        let expected = compute_blake3_sync(file.path()).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_none_cancel_flag_completes_normally() {
        // With None cancel flag, hash should complete normally
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();
        file.flush().unwrap();

        let result = compute_partial_blake3_sync_with_keepalive_cancellable(
            file.path(),
            u64::MAX,
            None,
            || {},
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_streaming_hasher_basic() {
        let mut hasher = StreamingHasher::new();
        hasher.update(b"Hello, World!");
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
        );
    }

    #[test]
    fn test_streaming_hasher_incremental() {
        let mut hasher = StreamingHasher::new();
        hasher.update(b"Hello");
        hasher.update(b", ");
        hasher.update(b"World!");
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
        );
    }

    #[test]
    fn test_streaming_hasher_partial_hash() {
        let mut hasher = StreamingHasher::new();
        hasher.update(b"Hello");
        let partial = hasher.partial_hash();
        // partial_hash should not consume the hasher
        hasher.update(b", World!");
        let full = hasher.finalize();
        // Partial should be hash of "Hello"
        assert_eq!(
            partial,
            "fbc2b0516ee8744d293b980779178a3508850fdcfe965985782c39601b65794f"
        );
        // Full should be hash of "Hello, World!"
        assert_eq!(
            full,
            "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
        );
    }

    #[test]
    fn test_streaming_hasher_empty() {
        let hasher = StreamingHasher::new();
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }
}
