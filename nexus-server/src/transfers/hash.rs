//! SHA-256 hash computation utilities for file transfers
//!
//! Re-exports the high-performance hashing functions from nexus-common,
//! which use hardware acceleration. For large files, use the `_with_keepalive`
//! variant to send periodic keepalive messages and prevent client timeouts.

use std::io;
use std::path::Path;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWriteExt;

pub use nexus_common::hash::compute_sha256;

// Note: compute_partial_sha256 is not re-exported because callers should use
// compute_partial_sha256_with_keepalive to prevent client timeouts.

/// Compute SHA-256 hash of an entire file
///
/// This is an alias for `compute_sha256` to maintain backward compatibility
/// with existing code that uses `compute_file_sha256`.
pub(crate) async fn compute_file_sha256(path: &Path) -> io::Result<String> {
    compute_sha256(path).await
}

/// Compute SHA-256 hash of a file, sending FileHashing keepalives periodically
///
/// This prevents client timeouts when hashing large files. Keepalives are sent
/// periodically during hashing (see `KEEPALIVE_INTERVAL` in nexus-common).
pub(crate) async fn compute_file_sha256_with_keepalive<W>(
    path: &Path,
    file_name: String,
    writer: &mut FrameWriter<W>,
) -> io::Result<String>
where
    W: AsyncWriteExt + Unpin,
{
    // Server-side hashing doesn't support cancellation (no cancel flag)
    let (handle, keepalive_rx) =
        nexus_common::hash::compute_sha256_with_keepalive(path, file_name, None).await;
    poll_hash_with_keepalives(handle, keepalive_rx, writer).await
}

/// Compute SHA-256 hash of the first N bytes of a file, sending FileHashing keepalives periodically
///
/// This prevents client timeouts when hashing large partial files for resume verification.
/// Keepalives are sent periodically during hashing (see `KEEPALIVE_INTERVAL` in nexus-common).
pub(crate) async fn compute_partial_sha256_with_keepalive<W>(
    path: &Path,
    max_bytes: u64,
    file_name: String,
    writer: &mut FrameWriter<W>,
) -> io::Result<String>
where
    W: AsyncWriteExt + Unpin,
{
    // Server-side hashing doesn't support cancellation (no cancel flag)
    let (handle, keepalive_rx) =
        nexus_common::hash::compute_partial_sha256_with_keepalive(path, max_bytes, file_name, None)
            .await;
    poll_hash_with_keepalives(handle, keepalive_rx, writer).await
}

/// Poll a hash computation task while sending keepalive messages to the client
///
/// This is the common implementation for both full and partial hash computation.
/// Sends `ServerMessage::FileHashing` keepalives when notified by the hash task.
async fn poll_hash_with_keepalives<W>(
    handle: tokio::task::JoinHandle<io::Result<String>>,
    mut keepalive_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    writer: &mut FrameWriter<W>,
) -> io::Result<String>
where
    W: AsyncWriteExt + Unpin,
{
    // Use tokio::pin! to allow polling the handle multiple times
    tokio::pin!(handle);

    // Poll for keepalive notifications while waiting for hash to complete
    loop {
        tokio::select! {
            biased;
            // Send keepalive when notified
            Some(file) = keepalive_rx.recv() => {
                let msg = ServerMessage::FileHashing { file };
                if let Err(e) = send_server_message_with_id(writer, &msg, MessageId::new()).await {
                    eprintln!("[HASH] Failed to send keepalive: {:?}", e);
                    // Continue anyway - hash might complete before timeout
                }
            }
            // Check if hash task is done
            result = &mut handle => {
                return result
                    .map_err(|e| io::Error::other(format!("hash task failed: {e}")))?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::hash::compute_partial_sha256;
    use std::io::Write;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_compute_file_sha256() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Known SHA-256 of "Hello, World!"
        fs::write(&file_path, "Hello, World!").await.unwrap();
        let hash = compute_file_sha256(&file_path).await.unwrap();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_compute_file_sha256_empty() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");

        // Known SHA-256 of empty file
        fs::write(&file_path, "").await.unwrap();
        let hash = compute_file_sha256(&file_path).await.unwrap();
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_compute_partial_sha256() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("partial.txt");

        // Write "Hello, World!" but only hash first 5 bytes ("Hello")
        fs::write(&file_path, "Hello, World!").await.unwrap();
        let partial_hash = compute_partial_sha256(&file_path, 5).await.unwrap();

        // Create another file with just "Hello" to verify
        let hello_path = temp_dir.path().join("hello.txt");
        fs::write(&hello_path, "Hello").await.unwrap();
        let full_hash = compute_file_sha256(&hello_path).await.unwrap();

        assert_eq!(partial_hash, full_hash);
    }

    #[tokio::test]
    async fn test_compute_partial_sha256_larger_than_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("small.txt");

        // Request more bytes than file contains
        fs::write(&file_path, "Hi").await.unwrap();
        let partial_hash = compute_partial_sha256(&file_path, 1000).await.unwrap();
        let full_hash = compute_file_sha256(&file_path).await.unwrap();

        // Should hash entire file when max_bytes exceeds file size
        assert_eq!(partial_hash, full_hash);
    }

    #[tokio::test]
    async fn test_large_file_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("large.bin");

        // Create a file larger than hash buffer size (1MB+)
        let data = vec![0x42u8; 1024 * 1024 + 1000];
        {
            let mut file = std::fs::File::create(&file_path).unwrap();
            file.write_all(&data).unwrap();
        }

        let hash = compute_file_sha256(&file_path).await.unwrap();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA-256 hex is 64 chars
    }
}
