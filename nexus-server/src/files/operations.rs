//! Common file operations for move/copy handlers
//!
//! This module provides shared utilities for file manipulation operations
//! including path relationship checks and recursive operations.

use std::io;
use std::path::Path;

/// Check if `child` path is a subpath of (starts with) `parent` path
///
/// This is used to prevent moving/copying a directory into itself,
/// which would cause infinite recursion or data loss.
///
/// # Arguments
///
/// * `child` - The potential child path
/// * `parent` - The potential parent path
///
/// # Returns
///
/// `true` if `child` starts with `parent`, `false` otherwise.
///
/// # Example
///
/// ```ignore
/// assert!(is_subpath(Path::new("/a/b/c"), Path::new("/a/b")));
/// assert!(!is_subpath(Path::new("/a/b"), Path::new("/a/b/c")));
/// ```
pub fn is_subpath(child: &Path, parent: &Path) -> bool {
    child.starts_with(parent)
}

/// Remove a path (file, symlink, or directory)
///
/// Handles different path types appropriately:
/// - Files and symlinks: removed with `remove_file`
/// - Directories: removed recursively with `remove_dir_all`
///
/// Uses `symlink_metadata` to check the path type without following symlinks,
/// ensuring that symlinks are removed as symlinks (not their targets).
///
/// # Arguments
///
/// * `path` - The path to remove
///
/// # Errors
///
/// Returns an error if:
/// - The path doesn't exist
/// - Permission is denied
/// - The path is a directory and contains files that can't be removed
pub fn remove_path(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Recursively copy a path (file, symlink, or directory)
///
/// Handles different path types:
/// - Files: copied with `std::fs::copy`
/// - Symlinks: recreated as symlinks pointing to the same target
/// - Directories: created and contents copied recursively
///
/// # Arguments
///
/// * `source` - The source path to copy from
/// * `target` - The target path to copy to
///
/// # Errors
///
/// Returns an error if:
/// - The source doesn't exist
/// - Permission is denied
/// - The target already exists
/// - Disk is full
///
/// # Note
///
/// On failure during directory copy, partial results may be left behind.
/// The caller is responsible for cleanup if needed.
pub fn copy_path_recursive(source: &Path, target: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(source)?;

    if meta.file_type().is_symlink() {
        copy_symlink(source, target)?;
    } else if meta.is_dir() {
        copy_directory_recursive(source, target)?;
    } else {
        // Copy file
        std::fs::copy(source, target)?;
    }
    Ok(())
}

/// Copy a symlink without following it
fn copy_symlink(source: &Path, target: &Path) -> io::Result<()> {
    let link_target = std::fs::read_link(source)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_target, target)?;
    }

    #[cfg(windows)]
    {
        // On Windows, we need to determine if the symlink points to a directory or file.
        // We check the original symlink's metadata (not the target) to handle broken symlinks.
        // If the symlink is broken or we can't determine, default to file symlink.
        let source_meta = std::fs::metadata(source);
        let is_dir = source_meta.map(|m| m.is_dir()).unwrap_or(false);

        if is_dir {
            std::os::windows::fs::symlink_dir(&link_target, target)?;
        } else {
            std::os::windows::fs::symlink_file(&link_target, target)?;
        }
    }

    Ok(())
}

/// Recursively copy a directory and its contents
fn copy_directory_recursive(source: &Path, target: &Path) -> io::Result<()> {
    std::fs::create_dir(target)?;

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let child_source = entry.path();
        let child_target = target.join(entry.file_name());
        copy_path_recursive(&child_source, &child_target)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_subpath_true() {
        assert!(is_subpath(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_subpath(Path::new("/a/b"), Path::new("/a/b")));
        assert!(is_subpath(Path::new("/a/b/c/d"), Path::new("/a")));
    }

    #[test]
    fn test_is_subpath_false() {
        assert!(!is_subpath(Path::new("/a/b"), Path::new("/a/b/c")));
        assert!(!is_subpath(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(!is_subpath(Path::new("/x/y"), Path::new("/a/b")));
    }

    #[test]
    fn test_remove_path_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        assert!(file_path.exists());
        remove_path(&file_path).unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_remove_path_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        assert!(dir_path.exists());
        remove_path(&dir_path).unwrap();
        assert!(!dir_path.exists());
    }

    #[test]
    fn test_remove_path_non_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();
        fs::write(dir_path.join("file.txt"), "content").unwrap();

        assert!(dir_path.exists());
        remove_path(&dir_path).unwrap();
        assert!(!dir_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_remove_path_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let link_path = temp_dir.path().join("link.txt");

        fs::write(&target_path, "content").unwrap();
        std::os::unix::fs::symlink(&target_path, &link_path).unwrap();

        assert!(link_path.symlink_metadata().is_ok());
        remove_path(&link_path).unwrap();
        assert!(link_path.symlink_metadata().is_err());
        // Target should still exist
        assert!(target_path.exists());
    }

    #[test]
    fn test_copy_path_recursive_file() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source.txt");
        let target = temp_dir.path().join("target.txt");

        fs::write(&source, "content").unwrap();
        copy_path_recursive(&source, &target).unwrap();

        assert!(source.exists());
        assert!(target.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "content");
    }

    #[test]
    fn test_copy_path_recursive_directory() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        let target_dir = temp_dir.path().join("target");

        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("file1.txt"), "content1").unwrap();
        fs::create_dir(source_dir.join("subdir")).unwrap();
        fs::write(source_dir.join("subdir/file2.txt"), "content2").unwrap();

        copy_path_recursive(&source_dir, &target_dir).unwrap();

        assert!(source_dir.exists());
        assert!(target_dir.exists());
        assert_eq!(
            fs::read_to_string(target_dir.join("file1.txt")).unwrap(),
            "content1"
        );
        assert!(target_dir.join("subdir").is_dir());
        assert_eq!(
            fs::read_to_string(target_dir.join("subdir/file2.txt")).unwrap(),
            "content2"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_copy_path_recursive_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target_file = temp_dir.path().join("target.txt");
        let source_link = temp_dir.path().join("source_link");
        let copied_link = temp_dir.path().join("copied_link");

        fs::write(&target_file, "content").unwrap();
        std::os::unix::fs::symlink(&target_file, &source_link).unwrap();

        copy_path_recursive(&source_link, &copied_link).unwrap();

        // Copied should be a symlink
        assert!(
            copied_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        // Should point to the same target
        assert_eq!(fs::read_link(&copied_link).unwrap(), target_file);
    }

    #[test]
    fn test_copy_path_recursive_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        let target_dir = temp_dir.path().join("target");

        fs::create_dir(&source_dir).unwrap();

        copy_path_recursive(&source_dir, &target_dir).unwrap();

        assert!(source_dir.is_dir());
        assert!(target_dir.is_dir());
    }

    #[test]
    fn test_remove_path_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let missing = temp_dir.path().join("missing.txt");

        let result = remove_path(&missing);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_copy_path_recursive_source_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let missing = temp_dir.path().join("missing.txt");
        let target = temp_dir.path().join("target.txt");

        let result = copy_path_recursive(&missing, &target);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
