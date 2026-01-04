//! File utility functions for the transfer executor
//!
//! Provides helpers for checking local files, generating unique paths,
//! computing SHA-256 hashes, and validating server-provided paths.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};

use super::{BUFFER_SIZE, PART_SUFFIX, TransferError};

/// Check for existing local file (complete or .part)
///
/// Returns (size, Option<sha256_hash>)
pub async fn check_local_file(complete_path: &Path, part_path: &Path) -> (u64, Option<String>) {
    // First check for complete file
    if let Ok(metadata) = tokio::fs::metadata(complete_path).await
        && metadata.is_file()
    {
        let size = metadata.len();
        if size > 0
            && let Ok(hash) = compute_file_sha256(complete_path).await
        {
            return (size, Some(hash));
        }
        return (size, None);
    }

    // Check for .part file
    if let Ok(metadata) = tokio::fs::metadata(part_path).await
        && metadata.is_file()
    {
        let size = metadata.len();
        if size > 0
            && let Ok(hash) = compute_file_sha256(part_path).await
        {
            return (size, Some(hash));
        }
        return (size, None);
    }

    (0, None)
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
        .unwrap_or("file");
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

/// Compute SHA-256 hash of a file
pub async fn compute_file_sha256(path: &Path) -> Result<String, TransferError> {
    let file = File::open(path).await.map_err(|_| TransferError::IoError)?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .await
            .map_err(|_| TransferError::IoError)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
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
}
