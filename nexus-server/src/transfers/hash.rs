//! SHA-256 hash computation utilities for file transfers
//!
//! Provides functions for computing full and partial file hashes,
//! used for resume verification and integrity checking.

use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use nexus_common::HASH_BUFFER_SIZE;

/// Compute SHA-256 hash of an entire file
pub(crate) async fn compute_file_sha256(path: &Path) -> io::Result<String> {
    compute_partial_sha256(path, u64::MAX).await
}

/// Compute SHA-256 hash of the first `max_bytes` of a file
///
/// If the file is smaller than `max_bytes`, hashes the entire file.
pub(crate) async fn compute_partial_sha256(path: &Path, max_bytes: u64) -> io::Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];
    let mut remaining = max_bytes;

    while remaining > 0 {
        let to_read = (remaining as usize).min(buffer.len());
        let bytes_read = file.read(&mut buffer[..to_read]).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read as u64;
    }

    let hash = hasher.finalize();

    Ok(hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
