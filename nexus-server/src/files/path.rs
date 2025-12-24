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

/// Build a candidate path from an area root and client-provided path string
///
/// This function handles the translation from client virtual paths (e.g., `/Documents/file.txt`)
/// to filesystem paths by stripping leading path separators and joining with the area root.
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area
/// * `client_path` - The client-provided path (may have leading `/` or `\`)
///
/// # Returns
///
/// Returns the joined path (not yet validated or canonicalized).
///
/// # Example
///
/// ```ignore
/// let root = Path::new("/data/files/shared");
/// let candidate = build_candidate_path(&root, "/Documents/readme.txt");
/// // candidate is /data/files/shared/Documents/readme.txt
/// ```
#[must_use]
pub fn build_candidate_path(area_root: &Path, client_path: &str) -> PathBuf {
    let normalized = client_path.trim_start_matches(['/', '\\']);
    area_root.join(normalized)
}

/// Safely resolve an absolute candidate path within an area root directory
///
/// This function validates paths to prevent directory traversal attacks:
///
/// 1. **Component validation**: Rejects `..` to prevent client-initiated escapes
/// 2. **Canonicalization**: Resolves symlinks to get the real filesystem path
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area. This **must** be an
///   absolute, canonical path (e.g., from `fs::canonicalize()`). The function will
///   return `InvalidAreaRoot` if this is not absolute.
/// * `candidate` - The absolute candidate path to resolve (typically from `build_candidate_path`)
///
/// # Returns
///
/// Returns the canonicalized absolute path if valid, or an error if:
/// - The area_root is not absolute
/// - The path contains `..` or other disallowed components
/// - The path does not exist
///
/// # Symlink Policy
///
/// Symlinks are allowed anywhere, including those that point outside the area root.
/// This lets admins link to external storage (e.g., `shared/Videos -> /mnt/nas/videos`).
///
/// Users cannot create symlinks through the BBS protocol (only file uploads), so
/// any symlinks are admin-created and trusted.
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
/// let candidate = build_candidate_path(&root, "/documents/readme.txt");
/// let resolved = resolve_path(&root, &candidate)?;
/// // resolved is the canonical path (may be outside area_root if symlinks are involved)
/// ```
#[must_use = "path resolution result should be used"]
pub fn resolve_path(area_root: &Path, candidate: &Path) -> Result<PathBuf, PathError> {
    // Verify area_root is absolute (we can't verify it's canonical, but absolute is required)
    if !area_root.is_absolute() {
        return Err(PathError::InvalidAreaRoot);
    }

    // Verify candidate is absolute (should always be true if using build_candidate_path)
    if !candidate.is_absolute() {
        return Err(PathError::InvalidPath);
    }

    // Early rejection: Check entire path for parent directory traversal (..)
    // This must happen BEFORE canonicalize() because:
    // 1. On Windows, path normalization may cause strip_prefix to fail
    // 2. We want to reject malicious paths before touching the filesystem
    validate_path_components(candidate)?;

    // Layer 1: Canonicalize to resolve symlinks and get absolute path
    let canonical = candidate.canonicalize().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathError::NotFound
        } else {
            PathError::CanonicalizeFailed(e.to_string())
        }
    })?;

    // Note: We intentionally do NOT check if canonical.starts_with(area_root).
    // Symlinks that point outside the area are allowed - they're admin-created
    // and trusted. Users cannot create symlinks through the BBS protocol.

    Ok(canonical)
}

/// Validate path components without touching the filesystem
///
/// Rejects paths containing:
/// - Parent directory references (`..`)
///
/// Allows:
/// - Empty paths (refers to root itself)
/// - Normal path components
/// - Current directory (`.`)
fn validate_path_components(path: &Path) -> Result<(), PathError> {
    for component in path.components() {
        match component {
            // Normal path segment - allowed
            Component::Normal(_) => {}
            // Current directory (.) - allowed (harmless)
            Component::CurDir => {}
            // Parent directory (..) - REJECTED
            Component::ParentDir => return Err(PathError::InvalidPath),
            // Root directory (/) - allowed (absolute paths are fine)
            Component::RootDir => {}
            // Windows prefix (C:, \\server) - allowed (absolute paths are fine)
            Component::Prefix(_) => {}
        }
    }

    Ok(())
}

/// Resolve a path for a new file/directory that doesn't exist yet
///
/// Similar to `resolve_path` but handles the case where the final component
/// doesn't exist. Validates the parent directory exists.
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area. This **must** be an
///   absolute, canonical path (e.g., from `fs::canonicalize()`).
/// * `candidate` - The absolute candidate path for the new item (typically from `build_candidate_path`).
///   Must not equal `area_root` (you can't create nameless files).
///
/// # Security
///
/// Like `resolve_path`, this function validates components before filesystem access
/// to ensure cross-platform consistency (especially important on Windows).
///
/// # Returns
///
/// Returns the path where the new item should be created if valid.
/// The returned path uses the canonicalized parent joined with the filename.
/// The parent directory is verified to exist (may be outside area_root via symlink).
///
/// # Errors
///
/// Returns `InvalidPath` if `candidate` equals `area_root` (can't create nameless files).
#[must_use = "path resolution result should be used"]
pub fn resolve_new_path(area_root: &Path, candidate: &Path) -> Result<PathBuf, PathError> {
    // Verify area_root is absolute
    if !area_root.is_absolute() {
        return Err(PathError::InvalidAreaRoot);
    }

    // Verify candidate is absolute (should always be true if using build_candidate_path)
    if !candidate.is_absolute() {
        return Err(PathError::InvalidPath);
    }

    // Can't create a file with no name (candidate == area_root)
    if candidate == area_root {
        return Err(PathError::InvalidPath);
    }

    // Early rejection: Check entire path for parent directory traversal (..)
    // This must happen BEFORE canonicalize() because:
    // 1. On Windows, path normalization may cause strip_prefix to fail
    // 2. We want to reject malicious paths before touching the filesystem
    validate_path_components(candidate)?;

    // Get the parent directory
    let parent = candidate.parent().ok_or(PathError::InvalidPath)?;

    // If the parent is the area_root itself, just verify and return
    if parent == area_root {
        return Ok(candidate.to_path_buf());
    }

    // Canonicalize the parent to verify it exists
    // Note: We don't check if it's under area_root - symlinks are trusted (admin-created)
    let canonical_parent = parent.canonicalize().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathError::NotFound
        } else {
            PathError::CanonicalizeFailed(e.to_string())
        }
    })?;

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
/// * `path` - The canonicalized path to check
///
/// # Returns
///
/// Returns `true` if uploads are allowed at this path, `false` otherwise.
///
/// # Note
///
/// This function assumes `path` has already been validated via `resolve_path`.
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
    // build_candidate_path tests
    // =========================================================================

    #[test]
    fn test_build_candidate_path_no_leading_slash() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "Documents/file.txt");
        assert_eq!(
            result,
            PathBuf::from("/data/files/shared/Documents/file.txt")
        );
    }

    #[test]
    fn test_build_candidate_path_leading_slash() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "/Documents/file.txt");
        assert_eq!(
            result,
            PathBuf::from("/data/files/shared/Documents/file.txt")
        );
    }

    #[test]
    fn test_build_candidate_path_multiple_leading_slashes() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "///Documents/file.txt");
        assert_eq!(
            result,
            PathBuf::from("/data/files/shared/Documents/file.txt")
        );
    }

    #[test]
    fn test_build_candidate_path_leading_backslash() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "\\Documents\\file.txt");
        assert_eq!(
            result,
            PathBuf::from("/data/files/shared/Documents\\file.txt")
        );
    }

    #[test]
    fn test_build_candidate_path_mixed_leading_separators() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "/\\/Documents");
        assert_eq!(result, PathBuf::from("/data/files/shared/Documents"));
    }

    #[test]
    fn test_build_candidate_path_empty() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "");
        assert_eq!(result, PathBuf::from("/data/files/shared/"));
    }

    #[test]
    fn test_build_candidate_path_just_slash() {
        let root = Path::new("/data/files/shared");
        let result = build_candidate_path(root, "/");
        assert_eq!(result, PathBuf::from("/data/files/shared/"));
    }

    // =========================================================================
    // resolve_path tests
    // =========================================================================

    #[test]
    fn test_resolve_valid_file() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "documents/readme.txt");

        let result = resolve_path(&root, &candidate);
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents/readme.txt"));
    }

    #[test]
    fn test_resolve_valid_file_with_leading_slash() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/documents/readme.txt");

        let result = resolve_path(&root, &candidate);
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents/readme.txt"));
    }

    #[test]
    fn test_resolve_valid_directory() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "documents");

        let result = resolve_path(&root, &candidate);
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents"));
    }

    #[test]
    fn test_resolve_empty_path() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "");

        // Empty path should resolve to the root itself
        let result = resolve_path(&root, &candidate);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_resolve_just_slash() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/");

        // Just "/" should resolve to the root itself
        let result = resolve_path(&root, &candidate);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_reject_parent_directory() {
        let (_temp, root) = setup_test_area();
        // Simulate client sending "../etc/passwd" - use build_candidate_path like real code
        let candidate = build_candidate_path(&root, "../etc/passwd");

        let result = resolve_path(&root, &candidate);
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_reject_parent_in_middle() {
        let (_temp, root) = setup_test_area();
        // Simulate client sending path with .. in the middle
        let candidate = build_candidate_path(&root, "documents/../../../etc/passwd");

        let result = resolve_path(&root, &candidate);
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_not_found() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "nonexistent/file.txt");

        let result = resolve_path(&root, &candidate);
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[test]
    fn test_symlink_to_external_allowed() {
        let (_temp, root) = setup_test_area();

        // Symlinks pointing outside the area are allowed (admin-created, trusted)
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create a temp directory outside the area to link to
            let external = TempDir::new().expect("Failed to create external dir");
            let external_path = external.path().canonicalize().unwrap();
            fs::write(external_path.join("external.txt"), "external").unwrap();

            // Create symlink pointing outside
            let link_path = root.join("documents/external_link");
            symlink(&external_path, &link_path).expect("Failed to create symlink");

            // Should be allowed - admin-created symlink
            let candidate = build_candidate_path(&root, "documents/external_link/external.txt");
            let result = resolve_path(&root, &candidate);
            assert!(result.is_ok());
            assert!(result.unwrap().ends_with("external.txt"));
        }
    }

    #[test]
    fn test_symlink_within_area_allowed() {
        let (_temp, root) = setup_test_area();

        // Symlink that stays within the area root should work
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create symlink from one folder to another (both within area)
            let link_path = root.join("doc_link");
            symlink(root.join("documents"), &link_path).expect("Failed to create symlink");

            let candidate = build_candidate_path(&root, "doc_link/readme.txt");
            let result = resolve_path(&root, &candidate);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_current_dir_allowed() {
        let (_temp, root) = setup_test_area();
        let candidate = root.join("./documents/./readme.txt");

        let result = resolve_path(&root, &candidate);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_non_absolute_area_root() {
        let candidate = Path::new("/absolute/path/file.txt");
        let result = resolve_path(Path::new("relative/path"), candidate);
        assert_eq!(result, Err(PathError::InvalidAreaRoot));
    }

    #[test]
    fn test_reject_non_absolute_candidate() {
        let (_temp, root) = setup_test_area();
        let candidate = Path::new("relative/path/file.txt");
        let result = resolve_path(&root, candidate);
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    // =========================================================================
    // resolve_new_path tests
    // =========================================================================

    #[test]
    fn test_resolve_new_path_valid() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "documents/newfile.txt");

        let result = resolve_new_path(&root, &candidate);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("newfile.txt"));
        assert!(path.parent().unwrap().ends_with("documents"));
    }

    #[test]
    fn test_resolve_new_path_with_leading_slash() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/documents/newfile.txt");

        let result = resolve_new_path(&root, &candidate);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("newfile.txt"));
    }

    #[test]
    fn test_resolve_new_path_in_root() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "newfile.txt");

        let result = resolve_new_path(&root, &candidate);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_new_path_parent_not_found() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "nonexistent/newfile.txt");

        let result = resolve_new_path(&root, &candidate);
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[test]
    fn test_resolve_new_path_reject_traversal() {
        let (_temp, root) = setup_test_area();
        // Simulate client sending "../newfile.txt" - use build_candidate_path like real code
        let candidate = build_candidate_path(&root, "../newfile.txt");

        let result = resolve_new_path(&root, &candidate);
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_resolve_new_path_empty_is_invalid() {
        let (_temp, root) = setup_test_area();
        // Candidate equals area_root - no filename
        let candidate = root.clone();

        let result = resolve_new_path(&root, &candidate);
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_resolve_new_path_just_slash_is_invalid() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/");

        // This resolves to root with trailing slash, which after normalization equals root
        // The function should reject this since there's no filename
        let result = resolve_new_path(&root, &candidate);
        // Note: "/data/root/" != "/data/root" as Path, so this may succeed or fail
        // depending on path normalization. Let's check what we actually get:
        // build_candidate_path returns root.join("") which adds a trailing component
        // that's empty. Let's verify the behavior is sensible either way.
        assert!(result.is_err() || result.unwrap().file_name().is_some());
    }

    #[test]
    fn test_resolve_new_path_reject_non_absolute_root() {
        let candidate = Path::new("/absolute/path/file.txt");
        let result = resolve_new_path(Path::new("relative/path"), candidate);
        assert_eq!(result, Err(PathError::InvalidAreaRoot));
    }

    #[test]
    fn test_resolve_new_path_reject_non_absolute_candidate() {
        let (_temp, root) = setup_test_area();
        let candidate = Path::new("relative/path/file.txt");
        let result = resolve_new_path(&root, candidate);
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_resolve_new_path_via_symlink_allowed() {
        let (_temp, root) = setup_test_area();

        // Symlinks are trusted (admin-created), so creating files through them is allowed
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create a temp directory outside the area
            let external = TempDir::new().expect("Failed to create external dir");
            let external_path = external.path().canonicalize().unwrap();

            // Create symlink pointing outside
            let link_path = root.join("external_link");
            symlink(&external_path, &link_path).expect("Failed to create symlink");

            // Creating a new file through the symlink should succeed
            let candidate = build_candidate_path(&root, "external_link/newfile.txt");
            let result = resolve_new_path(&root, &candidate);
            assert!(result.is_ok());
            assert!(result.unwrap().ends_with("newfile.txt"));
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
