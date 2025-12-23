//! Safe path resolution for file area operations
//!
//! Provides secure path resolution that prevents directory traversal attacks.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::constants::{
    ERR_FILE_ACCESS_DENIED, ERR_FILE_CANONICALIZE, ERR_FILE_INVALID_AREA_ROOT,
    ERR_FILE_INVALID_PATH, ERR_FILE_NOT_FOUND,
};
use crate::files::folder_type::{FolderType, parse_folder_type};

/// Type alias for file operation errors (for API consistency)
pub type FileError = PathError;

/// Error type for path resolution failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// Path contains invalid components (e.g., `..`, absolute paths)
    InvalidPath,
    /// Path escapes the allowed root directory
    AccessDenied,
    /// Path does not exist on the filesystem
    NotFound,
    /// Failed to canonicalize the path
    CanonicalizeFailed(String),
    /// The area root is not an absolute/canonical path
    InvalidAreaRoot,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath => write!(f, "{}", ERR_FILE_INVALID_PATH),
            Self::AccessDenied => write!(f, "{}", ERR_FILE_ACCESS_DENIED),
            Self::NotFound => write!(f, "{}", ERR_FILE_NOT_FOUND),
            Self::CanonicalizeFailed(e) => write!(f, "{}: {}", ERR_FILE_CANONICALIZE, e),
            Self::InvalidAreaRoot => write!(f, "{}", ERR_FILE_INVALID_AREA_ROOT),
        }
    }
}

impl std::error::Error for PathError {}

impl From<PathError> for io::Error {
    fn from(e: PathError) -> Self {
        match e {
            PathError::InvalidPath => io::Error::new(io::ErrorKind::InvalidInput, e.to_string()),
            PathError::AccessDenied => {
                io::Error::new(io::ErrorKind::PermissionDenied, e.to_string())
            }
            PathError::NotFound => io::Error::new(io::ErrorKind::NotFound, e.to_string()),
            PathError::CanonicalizeFailed(_) => io::Error::other(e.to_string()),
            PathError::InvalidAreaRoot => {
                io::Error::new(io::ErrorKind::InvalidInput, e.to_string())
            }
        }
    }
}

/// Safely resolve a relative path within an area root directory
///
/// This function provides three layers of defense against directory traversal:
///
/// 1. **Component validation**: Rejects `..`, absolute paths, and Windows drive prefixes
/// 2. **Canonicalization**: Resolves symlinks to detect escape attempts
/// 3. **Prefix check**: Verifies the final path is under the allowed root
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area. This **must** be an
///   absolute, canonical path (e.g., from `fs::canonicalize()`). The function will
///   return `InvalidAreaRoot` if this is not absolute.
/// * `relative_path` - The user-provided relative path to resolve
///
/// # Returns
///
/// Returns the canonicalized absolute path if valid, or an error if:
/// - The area_root is not absolute
/// - The path contains `..` or other disallowed components
/// - The path escapes the area root (via symlinks or otherwise)
/// - The path does not exist
///
/// # Security
///
/// The caller is responsible for ensuring `area_root` is canonical. While this
/// function checks that it's absolute, it cannot verify canonicalization (e.g.,
/// that symlinks are resolved). Always obtain `area_root` from `fs::canonicalize()`.
///
/// # Example
///
/// ```ignore
/// let root = std::fs::canonicalize("/data/files/users/alice")?;
/// let path = resolve_path(&root, "documents/readme.txt")?;
/// // path is now /data/files/users/alice/documents/readme.txt
/// ```
#[must_use = "path resolution result should be used"]
pub fn resolve_path(area_root: &Path, relative_path: &str) -> Result<PathBuf, PathError> {
    // Verify area_root is absolute (we can't verify it's canonical, but absolute is required)
    if !area_root.is_absolute() {
        return Err(PathError::InvalidAreaRoot);
    }

    // Layer 1: Validate path components before touching filesystem
    validate_path_components(relative_path)?;

    // Construct the candidate path
    let candidate = area_root.join(relative_path);

    // Layer 2: Canonicalize to resolve symlinks and get absolute path
    let canonical = candidate.canonicalize().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathError::NotFound
        } else {
            PathError::CanonicalizeFailed(e.to_string())
        }
    })?;

    // Layer 3: Verify the canonical path is still under the area root
    if !canonical.starts_with(area_root) {
        return Err(PathError::AccessDenied);
    }

    Ok(canonical)
}

/// Validate path components without touching the filesystem
///
/// Rejects paths containing:
/// - Parent directory references (`..`)
/// - Absolute path indicators (leading `/`, `\`, or Windows drive letters)
///
/// Allows:
/// - Empty paths (refers to root itself)
/// - Normal path components
/// - Current directory (`.`)
fn validate_path_components(path: &str) -> Result<(), PathError> {
    // Empty path is valid (refers to the root itself)
    if path.is_empty() {
        return Ok(());
    }

    let path_ref = Path::new(path);

    for component in path_ref.components() {
        match component {
            // Normal path segment - allowed
            Component::Normal(_) => {}
            // Current directory (.) - allowed (harmless)
            Component::CurDir => {}
            // Parent directory (..) - REJECTED
            Component::ParentDir => return Err(PathError::InvalidPath),
            // Root directory (/) - REJECTED (absolute path)
            Component::RootDir => return Err(PathError::InvalidPath),
            // Windows prefix (C:, \\server) - REJECTED
            Component::Prefix(_) => return Err(PathError::InvalidPath),
        }
    }

    Ok(())
}

/// Resolve a path for a new file/directory that doesn't exist yet
///
/// Similar to `resolve_path` but handles the case where the final component
/// doesn't exist. Validates the parent directory exists and is under the area root.
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area. This **must** be an
///   absolute, canonical path (e.g., from `fs::canonicalize()`).
/// * `relative_path` - The user-provided relative path to the new item. Must not be empty.
///
/// # Returns
///
/// Returns the path where the new item should be created if valid.
/// The returned path is NOT canonicalized (since the file doesn't exist),
/// but the parent directory is verified to exist and be under the area root.
///
/// # Errors
///
/// Returns `InvalidPath` if `relative_path` is empty (can't create nameless files).
#[must_use = "path resolution result should be used"]
pub fn resolve_new_path(area_root: &Path, relative_path: &str) -> Result<PathBuf, PathError> {
    // Verify area_root is absolute
    if !area_root.is_absolute() {
        return Err(PathError::InvalidAreaRoot);
    }

    // Empty path is invalid for new files - you need a filename
    if relative_path.is_empty() {
        return Err(PathError::InvalidPath);
    }

    // Layer 1: Validate path components
    validate_path_components(relative_path)?;

    let candidate = area_root.join(relative_path);

    // Get the parent directory
    let parent = candidate.parent().ok_or(PathError::InvalidPath)?;

    // If the parent is the area_root itself, just verify and return
    if parent == area_root {
        return Ok(candidate);
    }

    // Canonicalize the parent to verify it exists and is under the root
    let canonical_parent = parent.canonicalize().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathError::NotFound
        } else {
            PathError::CanonicalizeFailed(e.to_string())
        }
    })?;

    if !canonical_parent.starts_with(area_root) {
        return Err(PathError::AccessDenied);
    }

    // Return the non-canonicalized path (file doesn't exist yet)
    // Join the canonical parent with the filename
    let filename = candidate.file_name().ok_or(PathError::InvalidPath)?;

    Ok(canonical_parent.join(filename))
}

/// Check if a path allows file uploads
///
/// Uploads are allowed if the path is within a folder that has:
/// - `[NEXUS-UL]` suffix (upload folder)
/// - `[NEXUS-DB]` or `[NEXUS-DB-username]` suffix (drop box)
///
/// Upload permission is inherited - if any ancestor folder has an upload
/// or dropbox suffix, uploads are allowed.
///
/// # Arguments
///
/// * `area_root` - The canonicalized root directory for the user's file area
/// * `path` - The canonicalized path to check (must be under area_root)
///
/// # Returns
///
/// Returns `true` if uploads are allowed at this path, `false` otherwise.
///
/// # Note
///
/// This function assumes `path` has already been validated to be under `area_root`.
/// It does not perform security checks - use `resolve_path` first.
#[must_use]
pub fn allows_upload(area_root: &Path, path: &Path) -> bool {
    // Start from the path and walk up to (but not including) the area root
    let mut current = path;

    while current != area_root {
        // Get the folder name
        if let Some(name) = current.file_name()
            && let Some(name_str) = name.to_str()
        {
            match parse_folder_type(name_str) {
                FolderType::Upload | FolderType::DropBox | FolderType::UserDropBox(_) => {
                    return true;
                }
                FolderType::Default => {}
            }
        }

        // Move up to parent
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn setup_test_area() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir
            .path()
            .canonicalize()
            .expect("Failed to canonicalize");

        // Create some test directories
        fs::create_dir_all(root.join("documents")).expect("Failed to create documents");
        fs::create_dir_all(root.join("uploads")).expect("Failed to create uploads");

        // Create a test file
        fs::write(root.join("documents/readme.txt"), "test").expect("Failed to create file");

        (temp_dir, root)
    }

    // =========================================================================
    // resolve_path tests
    // =========================================================================

    #[test]
    fn test_resolve_valid_file() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "documents/readme.txt");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents/readme.txt"));
    }

    #[test]
    fn test_resolve_valid_directory() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "documents");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents"));
    }

    #[test]
    fn test_resolve_empty_path() {
        let (_temp, root) = setup_test_area();

        // Empty path should resolve to the root itself
        let result = resolve_path(&root, "");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_reject_parent_directory() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "../etc/passwd");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_reject_parent_in_middle() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "documents/../../../etc/passwd");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_reject_absolute_unix() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "/etc/passwd");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_reject_windows_absolute() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "C:\\Windows\\System32");
        // On Windows, Component::Prefix catches drive letters -> InvalidPath
        // On Linux, "C:\Windows\System32" is a valid filename that doesn't exist -> NotFound
        #[cfg(windows)]
        assert_eq!(result, Err(PathError::InvalidPath));
        #[cfg(not(windows))]
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[test]
    fn test_not_found() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "nonexistent/file.txt");
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[test]
    fn test_symlink_escape() {
        let (_temp, root) = setup_test_area();

        // Create a symlink that points outside the area
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link_path = root.join("escape");
            symlink("/tmp", &link_path).expect("Failed to create symlink");

            let result = resolve_path(&root, "escape");
            assert_eq!(result, Err(PathError::AccessDenied));
        }
    }

    #[test]
    fn test_current_dir_allowed() {
        let (_temp, root) = setup_test_area();

        let result = resolve_path(&root, "./documents/./readme.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_non_absolute_area_root() {
        let result = resolve_path(Path::new("relative/path"), "file.txt");
        assert_eq!(result, Err(PathError::InvalidAreaRoot));
    }

    // =========================================================================
    // resolve_new_path tests
    // =========================================================================

    #[test]
    fn test_resolve_new_path_valid() {
        let (_temp, root) = setup_test_area();

        let result = resolve_new_path(&root, "documents/newfile.txt");
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("newfile.txt"));
        assert!(path.parent().unwrap().ends_with("documents"));
    }

    #[test]
    fn test_resolve_new_path_in_root() {
        let (_temp, root) = setup_test_area();

        let result = resolve_new_path(&root, "newfile.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_new_path_parent_not_found() {
        let (_temp, root) = setup_test_area();

        let result = resolve_new_path(&root, "nonexistent/newfile.txt");
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[test]
    fn test_resolve_new_path_reject_traversal() {
        let (_temp, root) = setup_test_area();

        let result = resolve_new_path(&root, "../newfile.txt");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_resolve_new_path_empty_is_invalid() {
        let (_temp, root) = setup_test_area();

        // Empty path should be rejected for new files
        let result = resolve_new_path(&root, "");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_resolve_new_path_reject_non_absolute_root() {
        let result = resolve_new_path(Path::new("relative/path"), "file.txt");
        assert_eq!(result, Err(PathError::InvalidAreaRoot));
    }

    #[test]
    fn test_resolve_new_path_symlink_escape() {
        let (_temp, root) = setup_test_area();

        // Create a symlink that points outside the area
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link_path = root.join("escape_link");
            symlink("/tmp", &link_path).expect("Failed to create symlink");

            // Trying to create a new file through the symlink should fail
            let result = resolve_new_path(&root, "escape_link/newfile.txt");
            assert_eq!(result, Err(PathError::AccessDenied));
        }
    }

    // =========================================================================
    // allows_upload tests
    // =========================================================================

    #[test]
    fn test_upload_not_allowed_in_default_folder() {
        let (_temp, root) = setup_test_area();

        let path = root.join("documents");
        assert!(!allows_upload(&root, &path));
    }

    #[test]
    fn test_upload_allowed_in_upload_folder() {
        let (_temp, root) = setup_test_area();

        // Create an upload folder
        let upload_dir = root.join("Uploads [NEXUS-UL]");
        fs::create_dir(&upload_dir).expect("Failed to create upload dir");

        assert!(allows_upload(&root, &upload_dir));
    }

    #[test]
    fn test_upload_allowed_in_nested_under_upload_folder() {
        let (_temp, root) = setup_test_area();

        // Create an upload folder with a subfolder
        let upload_dir = root.join("Uploads [NEXUS-UL]");
        let nested_dir = upload_dir.join("subfolder");
        fs::create_dir_all(&nested_dir).expect("Failed to create dirs");

        // Subfolder should inherit upload permission
        assert!(allows_upload(&root, &nested_dir));
    }

    #[test]
    fn test_upload_allowed_in_deeply_nested_under_upload_folder() {
        let (_temp, root) = setup_test_area();

        // Create an upload folder with deeply nested subfolders
        let upload_dir = root.join("Uploads [NEXUS-UL]");
        let deeply_nested = upload_dir.join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deeply_nested).expect("Failed to create dirs");

        // Deeply nested subfolder should inherit upload permission
        assert!(allows_upload(&root, &deeply_nested));
    }

    #[test]
    fn test_upload_allowed_in_dropbox() {
        let (_temp, root) = setup_test_area();

        let dropbox_dir = root.join("Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox_dir).expect("Failed to create dropbox dir");

        assert!(allows_upload(&root, &dropbox_dir));
    }

    #[test]
    fn test_upload_allowed_in_user_dropbox() {
        let (_temp, root) = setup_test_area();

        let dropbox_dir = root.join("For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox_dir).expect("Failed to create user dropbox dir");

        assert!(allows_upload(&root, &dropbox_dir));
    }

    #[test]
    fn test_upload_case_insensitive_suffix() {
        let (_temp, root) = setup_test_area();

        let upload_dir = root.join("Uploads [nexus-ul]");
        fs::create_dir(&upload_dir).expect("Failed to create upload dir");

        assert!(allows_upload(&root, &upload_dir));
    }

    #[test]
    fn test_upload_not_allowed_at_root() {
        let (_temp, root) = setup_test_area();

        // The area root itself should not allow uploads
        assert!(!allows_upload(&root, &root));
    }
}
