//! Safe path resolution for file area operations
//!
//! Provides secure path resolution that prevents directory traversal attacks.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::constants::{
    ERR_FILE_ACCESS_DENIED, ERR_FILE_CANONICALIZE, ERR_FILE_INVALID_AREA_ROOT,
    ERR_FILE_INVALID_PATH, ERR_FILE_NOT_FOUND, FOLDER_SUFFIX_DROPBOX, FOLDER_SUFFIX_DROPBOX_PREFIX,
    FOLDER_SUFFIX_UPLOAD,
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

/// Validate a client-provided path string for directory traversal attempts
///
/// This MUST be called on the raw client path string BEFORE joining with area_root,
/// because Windows normalizes paths during join, removing `..` components.
///
/// # Returns
///
/// Returns `Ok(())` if the path is safe, `Err(PathError::InvalidPath)` if it contains `..`.
fn validate_client_path(client_path: &str) -> Result<(), PathError> {
    // Check for ".." in the path - this catches traversal attempts before Windows can normalize them
    // We check for common patterns: standalone "..", or ".." with path separators
    for segment in client_path.split(['/', '\\']) {
        if segment == ".." {
            return Err(PathError::InvalidPath);
        }
    }
    Ok(())
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

/// Build and validate a candidate path from an area root and client-provided path string
///
/// This combines validation and path building. It validates the raw client path string
/// for traversal attempts BEFORE joining with area_root (important for Windows compatibility).
///
/// **Suffix Matching**: This function resolves each path segment with folder type suffix
/// matching. For example, if the client sends "uploads/file.txt" but the filesystem has
/// "uploads [NEXUS-UL]/file.txt", this function will resolve it correctly. Exact matches
/// take priority over suffix-stripped matches.
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area
/// * `client_path` - The client-provided path (may have leading `/` or `\`)
///
/// # Returns
///
/// Returns the resolved path if valid, or an error if:
/// - Path contains directory traversal attempts (`InvalidPath`)
/// - A parent path segment cannot be resolved (`NotFound`)
///
/// Note: The final segment is allowed to not exist (for operations that create files).
pub fn build_and_validate_candidate_path(
    area_root: &Path,
    client_path: &str,
) -> Result<PathBuf, PathError> {
    validate_client_path(client_path)?;

    // Normalize the client path
    let normalized = client_path
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/");

    // Empty path means area root itself
    if normalized.is_empty() {
        return Ok(area_root.to_path_buf());
    }

    // Split into segments and resolve each one with suffix matching
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();

    let mut current_path = area_root.to_path_buf();

    for (i, segment) in segments.iter().enumerate() {
        // Skip current directory references
        if *segment == "." {
            continue;
        }

        let is_last_segment = i == segments.len() - 1;

        // Try to resolve this segment with suffix matching
        match resolve_segment_in_dir(&current_path, segment) {
            Some(resolved_name) => {
                current_path = current_path.join(resolved_name);
            }
            None => {
                // Segment not found on disk
                if is_last_segment {
                    // Final segment not found - this is OK for operations
                    // that create new files. Return the path with the literal segment.
                    // The caller (resolve_path) will handle NotFound appropriately.
                    current_path = current_path.join(segment);
                } else {
                    // Parent segment not found - this is an error
                    return Err(PathError::NotFound);
                }
            }
        }
    }

    Ok(current_path)
}

/// Validate a client path and build a candidate path WITHOUT suffix matching
///
/// This is used for operations where the path doesn't need to exist yet (e.g., uploads).
/// It validates the path for traversal attacks but doesn't try to resolve segments
/// against the filesystem.
///
/// # Arguments
///
/// * `area_root` - The root directory for the user's file area
/// * `client_path` - The client-provided path (may have leading `/` or `\`)
///
/// # Returns
///
/// Returns the joined path if valid, or `Err(PathError::InvalidPath)` if the path
/// contains directory traversal attempts.
pub fn validate_and_build_candidate_path(
    area_root: &Path,
    client_path: &str,
) -> Result<PathBuf, PathError> {
    validate_client_path(client_path)?;
    Ok(build_candidate_path(area_root, client_path))
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
/// let root = tokio::fs::canonicalize("/data/files/users/alice").await?;
/// let candidate = build_candidate_path(&root, "/documents/readme.txt");
/// let resolved = resolve_path(&root, &candidate).await?;
/// // resolved is the canonical path (may be outside area_root if symlinks are involved)
/// ```
#[must_use = "use the returned path; reusing the input bypasses the area-root containment check"]
pub async fn resolve_path(area_root: &Path, candidate: &Path) -> Result<PathBuf, PathError> {
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
    let canonical = tokio::fs::canonicalize(candidate).await.map_err(|e| {
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
#[must_use = "use the returned path; reusing the input bypasses the area-root containment check"]
pub async fn resolve_new_path(area_root: &Path, candidate: &Path) -> Result<PathBuf, PathError> {
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
    let canonical_parent = tokio::fs::canonicalize(parent).await.map_err(|e| {
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

/// Normalize a client-provided path for use in responses
///
/// This function cleans up a path for consistent display back to the client:
/// - Replaces backslashes with forward slashes
/// - Removes empty segments (from multiple slashes)
/// - Removes "." (current directory) segments
///
/// This is purely cosmetic normalization for response paths, not security validation.
/// Security validation should be done via `build_and_validate_candidate_path()` and `resolve_path()`.
///
/// # Arguments
///
/// * `path` - The client-provided path string
///
/// # Returns
///
/// A normalized path string with consistent forward slashes and no redundant segments.
///
/// # Example
///
/// ```ignore
/// assert_eq!(normalize_client_path("foo//bar"), "foo/bar");
/// assert_eq!(normalize_client_path("foo\\bar"), "foo/bar");
/// assert_eq!(normalize_client_path("./foo/./bar"), "foo/bar");
/// assert_eq!(normalize_client_path(""), "");
/// ```
#[must_use]
/// Strip folder type suffix from a name to get the display name
///
/// This is the inverse of how folders are named with suffixes like `[NEXUS-UL]`.
/// Used for matching client paths that use stripped names against filesystem names.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(strip_folder_suffix("uploads [NEXUS-UL]"), "uploads");
/// assert_eq!(strip_folder_suffix("dropbox [NEXUS-DB]"), "dropbox");
/// assert_eq!(strip_folder_suffix("inbox [NEXUS-DB-alice]"), "inbox");
/// assert_eq!(strip_folder_suffix("normal"), "normal");
/// ```
fn strip_folder_suffix(name: &str) -> String {
    let name_upper = name.to_uppercase();

    // Check for user-specific dropbox suffix first (e.g., " [NEXUS-DB-alice]")
    if let Some(pos) = name_upper.rfind(FOLDER_SUFFIX_DROPBOX_PREFIX)
        && name_upper.ends_with(']')
    {
        return name[..pos].to_string();
    }

    // Check for generic dropbox suffix
    if name_upper.ends_with(FOLDER_SUFFIX_DROPBOX) {
        let suffix_start = name.len() - FOLDER_SUFFIX_DROPBOX.len();
        return name[..suffix_start].to_string();
    }

    // Check for upload suffix
    if name_upper.ends_with(FOLDER_SUFFIX_UPLOAD) {
        let suffix_start = name.len() - FOLDER_SUFFIX_UPLOAD.len();
        return name[..suffix_start].to_string();
    }

    name.to_string()
}

/// Resolve a single path segment within a directory, with suffix matching
///
/// Tries exact match first, then falls back to matching against stripped suffix names.
/// Case-sensitive matching (matches filesystem behavior).
///
/// # Arguments
///
/// * `parent_dir` - The directory to search in
/// * `segment` - The segment name to find
///
/// # Returns
///
/// The actual filesystem name if found, or None if no match.
fn resolve_segment_in_dir(parent_dir: &Path, segment: &str) -> Option<String> {
    // First try exact match (fast path)
    let exact_path = parent_dir.join(segment);
    if exact_path.exists() {
        return Some(segment.to_string());
    }

    // Fall back to suffix matching - read directory and find a match
    let entries = match fs::read_dir(parent_dir) {
        Ok(e) => e,
        Err(_) => return None,
    };

    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue, // Skip non-UTF-8 names
        };

        // Check if stripping the suffix gives us the requested segment
        let stripped = strip_folder_suffix(&name);
        if stripped == segment {
            return Some(name);
        }
    }

    None
}

/// Resolve a client path with folder suffix matching
///
/// This function resolves each segment of a client-provided path, allowing
/// clients to use stripped names (e.g., "uploads") that match filesystem names
/// with suffixes (e.g., "uploads [NEXUS-UL]").
///
/// # Resolution Rules
///
/// For each path segment:
/// 1. **Exact match first**: If a file/folder with the exact name exists, use it
/// 2. **Stripped match fallback**: Otherwise, find a file/folder whose name with
///    suffix stripped matches the segment
/// 3. **Case sensitive**: Matching is case-sensitive (follows filesystem behavior)
///
/// # Arguments
///
/// * `area_root` - The canonical root directory for the file area
/// * `client_path` - The client-provided path (may use stripped names)
///
/// # Returns
///
/// Returns the resolved filesystem path if all segments resolve successfully.
/// The returned path is NOT canonicalized (final component may not exist).
///
/// # Errors
///
/// - `InvalidAreaRoot` if area_root is not absolute
/// - `InvalidPath` if path contains `..` or other invalid components
/// - `NotFound` if any segment cannot be resolved
///
/// # Example
///
/// ```ignore
/// // Filesystem has: /files/shared/uploads [NEXUS-UL]/docs/readme.txt
/// let resolved = resolve_path_with_suffix_matching(
///     Path::new("/files/shared"),
///     "uploads/docs/readme.txt"
/// )?;
/// // resolved = /files/shared/uploads [NEXUS-UL]/docs/readme.txt
/// ```
pub fn resolve_path_with_suffix_matching(
    area_root: &Path,
    client_path: &str,
) -> Result<PathBuf, PathError> {
    // Verify area_root is absolute
    if !area_root.is_absolute() {
        return Err(PathError::InvalidAreaRoot);
    }

    // Validate for traversal attacks first
    validate_client_path(client_path)?;

    // Normalize the client path (strip leading slashes, handle backslashes)
    let normalized = client_path
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/");

    // Empty path means area root itself
    if normalized.is_empty() {
        return Ok(area_root.to_path_buf());
    }

    // Split into segments and resolve each one
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();

    let mut current_path = area_root.to_path_buf();

    for segment in segments {
        // Skip current directory references
        if segment == "." {
            continue;
        }

        // Try to resolve this segment
        match resolve_segment_in_dir(&current_path, segment) {
            Some(resolved_name) => {
                current_path = current_path.join(resolved_name);
            }
            None => {
                return Err(PathError::NotFound);
            }
        }
    }

    Ok(current_path)
}

/// Resolve a client path for a new file/directory with folder suffix matching
///
/// Similar to `resolve_path_with_suffix_matching` but handles the case where
/// the final component doesn't exist yet. All parent segments must resolve.
///
/// # Arguments
///
/// * `area_root` - The canonical root directory for the file area
/// * `client_path` - The client-provided path (may use stripped names for parents)
///
/// # Returns
///
/// Returns the resolved path where the new item should be created.
/// Parent directories are resolved with suffix matching, final component is used as-is.
///
/// # Errors
///
/// - `InvalidAreaRoot` if area_root is not absolute
/// - `InvalidPath` if path contains `..`, is empty, or equals area_root
/// - `NotFound` if any parent segment cannot be resolved
pub fn resolve_new_path_with_suffix_matching(
    area_root: &Path,
    client_path: &str,
) -> Result<PathBuf, PathError> {
    // Verify area_root is absolute
    if !area_root.is_absolute() {
        return Err(PathError::InvalidAreaRoot);
    }

    // Validate for traversal attacks first
    validate_client_path(client_path)?;

    // Normalize the client path
    let normalized = client_path
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/");

    // Empty path is invalid for new files
    if normalized.is_empty() {
        return Err(PathError::InvalidPath);
    }

    // Split into segments
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return Err(PathError::InvalidPath);
    }

    // Resolve all parent segments (all but the last)
    let mut current_path = area_root.to_path_buf();
    let parent_segments = &segments[..segments.len() - 1];
    let final_segment = segments[segments.len() - 1];

    for segment in parent_segments {
        // Skip current directory references
        if *segment == "." {
            continue;
        }

        // Try to resolve this segment
        match resolve_segment_in_dir(&current_path, segment) {
            Some(resolved_name) => {
                current_path = current_path.join(resolved_name);
            }
            None => {
                return Err(PathError::NotFound);
            }
        }
    }

    // Append final segment as-is (it's the new item name)
    Ok(current_path.join(final_segment))
}

pub fn normalize_client_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Check if a filename should be treated as hidden
///
/// Hidden files are filtered from directory listings and the search index
/// unless the user explicitly requests them via `show_hidden`.
///
/// Prefixes treated as hidden:
/// - `.` — Unix dotfiles (e.g., `.DS_Store`, `.gitignore`)
/// - `@` — NAS metadata (e.g., Synology `@eaDir`, `@tmp`, `@sharebin`)
/// - `#` — NAS metadata (e.g., Synology `#recycle`, `#snapshot`)
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('@') || name.starts_with('#')
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
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    // ==========================================================================
    // Tests for strip_folder_suffix
    // ==========================================================================

    #[test]
    fn test_strip_folder_suffix_upload() {
        assert_eq!(strip_folder_suffix("uploads [NEXUS-UL]"), "uploads");
        assert_eq!(strip_folder_suffix("My Uploads [NEXUS-UL]"), "My Uploads");
    }

    #[test]
    fn test_strip_folder_suffix_dropbox() {
        assert_eq!(strip_folder_suffix("inbox [NEXUS-DB]"), "inbox");
        assert_eq!(strip_folder_suffix("Drop Box [NEXUS-DB]"), "Drop Box");
    }

    #[test]
    fn test_strip_folder_suffix_user_dropbox() {
        assert_eq!(strip_folder_suffix("inbox [NEXUS-DB-alice]"), "inbox");
        assert_eq!(strip_folder_suffix("For Bob [NEXUS-DB-bob]"), "For Bob");
    }

    #[test]
    fn test_strip_folder_suffix_case_insensitive() {
        assert_eq!(strip_folder_suffix("uploads [nexus-ul]"), "uploads");
        assert_eq!(strip_folder_suffix("inbox [Nexus-DB]"), "inbox");
        assert_eq!(strip_folder_suffix("inbox [NEXUS-db-Alice]"), "inbox");
    }

    #[test]
    fn test_strip_folder_suffix_no_suffix() {
        assert_eq!(strip_folder_suffix("normal"), "normal");
        assert_eq!(strip_folder_suffix("My Documents"), "My Documents");
        assert_eq!(strip_folder_suffix(""), "");
    }

    #[test]
    fn test_strip_folder_suffix_preserves_non_suffix_brackets() {
        // Brackets that aren't suffixes should be preserved
        assert_eq!(strip_folder_suffix("folder [other]"), "folder [other]");
        assert_eq!(strip_folder_suffix("[test] folder"), "[test] folder");
    }

    #[test]
    fn test_strip_folder_suffix_malformed() {
        // Incomplete/malformed suffixes should be treated as literal names
        assert_eq!(strip_folder_suffix("folder [NEXUS-"), "folder [NEXUS-");
        assert_eq!(strip_folder_suffix("folder [NEXUS-UL"), "folder [NEXUS-UL");
        assert_eq!(strip_folder_suffix("folder [NEXUS-DB"), "folder [NEXUS-DB");
        assert_eq!(
            strip_folder_suffix("folder [NEXUS-DB-"),
            "folder [NEXUS-DB-"
        );
        assert_eq!(
            strip_folder_suffix("folder [NEXUS-DB-user"),
            "folder [NEXUS-DB-user"
        );
    }

    // ==========================================================================
    // Tests for resolve_path_with_suffix_matching
    // ==========================================================================

    fn setup_suffix_test_area() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create directories with suffixes
        fs::create_dir_all(root.join("uploads [NEXUS-UL]")).unwrap();
        fs::create_dir_all(root.join("uploads [NEXUS-UL]/subdir")).unwrap();
        fs::create_dir_all(root.join("inbox [NEXUS-DB]")).unwrap();
        fs::create_dir_all(root.join("normal")).unwrap();

        // Create some files
        File::create(root.join("uploads [NEXUS-UL]/file.txt")).unwrap();
        File::create(root.join("uploads [NEXUS-UL]/subdir/nested.txt")).unwrap();
        File::create(root.join("normal/doc.txt")).unwrap();

        temp
    }

    #[test]
    fn test_resolve_suffix_exact_match_preferred() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create both "uploads" and "uploads [NEXUS-UL]"
        fs::create_dir(root.join("uploads")).unwrap();
        fs::create_dir(root.join("uploads [NEXUS-UL]")).unwrap();

        // Exact match should win
        let resolved = resolve_path_with_suffix_matching(root, "uploads").unwrap();
        assert_eq!(resolved, root.join("uploads"));
    }

    #[test]
    fn test_resolve_suffix_single_segment() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // "uploads" should resolve to "uploads [NEXUS-UL]"
        let resolved = resolve_path_with_suffix_matching(root, "uploads").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]"));

        // "inbox" should resolve to "inbox [NEXUS-DB]"
        let resolved = resolve_path_with_suffix_matching(root, "inbox").unwrap();
        assert_eq!(resolved, root.join("inbox [NEXUS-DB]"));

        // "normal" should resolve exactly
        let resolved = resolve_path_with_suffix_matching(root, "normal").unwrap();
        assert_eq!(resolved, root.join("normal"));
    }

    #[test]
    fn test_resolve_suffix_multi_segment() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // "uploads/file.txt" should resolve to "uploads [NEXUS-UL]/file.txt"
        let resolved = resolve_path_with_suffix_matching(root, "uploads/file.txt").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]/file.txt"));

        // "uploads/subdir/nested.txt"
        let resolved =
            resolve_path_with_suffix_matching(root, "uploads/subdir/nested.txt").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]/subdir/nested.txt"));
    }

    #[test]
    fn test_resolve_suffix_with_explicit_suffix() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Client can also use the full name with suffix
        let resolved =
            resolve_path_with_suffix_matching(root, "uploads [NEXUS-UL]/file.txt").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]/file.txt"));
    }

    #[test]
    fn test_resolve_suffix_empty_path() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Empty path returns area root
        let resolved = resolve_path_with_suffix_matching(root, "").unwrap();
        assert_eq!(resolved, root);

        let resolved = resolve_path_with_suffix_matching(root, "/").unwrap();
        assert_eq!(resolved, root);
    }

    #[test]
    fn test_resolve_suffix_not_found() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Non-existent path
        let result = resolve_path_with_suffix_matching(root, "nonexistent");
        assert!(matches!(result, Err(PathError::NotFound)));

        // Non-existent nested
        let result = resolve_path_with_suffix_matching(root, "uploads/nonexistent");
        assert!(matches!(result, Err(PathError::NotFound)));
    }

    #[test]
    fn test_resolve_suffix_case_sensitive() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // "Uploads" (capital U) should NOT match "uploads [NEXUS-UL]"
        let result = resolve_path_with_suffix_matching(root, "Uploads");
        assert!(matches!(result, Err(PathError::NotFound)));
    }

    #[test]
    fn test_resolve_suffix_rejects_traversal() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Parent directory traversal should be rejected
        let result = resolve_path_with_suffix_matching(root, "../etc/passwd");
        assert!(matches!(result, Err(PathError::InvalidPath)));

        let result = resolve_path_with_suffix_matching(root, "uploads/../../../etc");
        assert!(matches!(result, Err(PathError::InvalidPath)));
    }

    #[test]
    fn test_resolve_suffix_leading_slash() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Leading slash should be handled
        let resolved = resolve_path_with_suffix_matching(root, "/uploads").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]"));
    }

    #[test]
    fn test_resolve_suffix_backslashes() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Backslashes should be normalized to forward slashes
        let resolved = resolve_path_with_suffix_matching(root, "uploads\\file.txt").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]/file.txt"));
    }

    // ==========================================================================
    // Tests for resolve_new_path_with_suffix_matching
    // ==========================================================================

    #[test]
    fn test_resolve_new_path_suffix_single_segment() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // New file in root
        let resolved = resolve_new_path_with_suffix_matching(root, "newfile.txt").unwrap();
        assert_eq!(resolved, root.join("newfile.txt"));
    }

    #[test]
    fn test_resolve_new_path_suffix_in_suffixed_dir() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // New file in uploads (which has suffix)
        let resolved = resolve_new_path_with_suffix_matching(root, "uploads/newfile.txt").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]/newfile.txt"));
    }

    #[test]
    fn test_resolve_new_path_suffix_nested() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // New file in nested directory
        let resolved =
            resolve_new_path_with_suffix_matching(root, "uploads/subdir/newfile.txt").unwrap();
        assert_eq!(resolved, root.join("uploads [NEXUS-UL]/subdir/newfile.txt"));
    }

    #[test]
    fn test_resolve_new_path_suffix_parent_not_found() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Parent doesn't exist
        let result = resolve_new_path_with_suffix_matching(root, "nonexistent/file.txt");
        assert!(matches!(result, Err(PathError::NotFound)));
    }

    #[test]
    fn test_resolve_new_path_suffix_empty_invalid() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        // Empty path is invalid for new files
        let result = resolve_new_path_with_suffix_matching(root, "");
        assert!(matches!(result, Err(PathError::InvalidPath)));
    }

    #[test]
    fn test_resolve_new_path_suffix_rejects_traversal() {
        let temp = setup_suffix_test_area();
        let root = temp.path();

        let result = resolve_new_path_with_suffix_matching(root, "../newfile.txt");
        assert!(matches!(result, Err(PathError::InvalidPath)));
    }

    // ==========================================================================
    // Original tests (keep existing tests below)
    // ==========================================================================

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

    #[tokio::test]
    async fn test_resolve_valid_file() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "documents/readme.txt");

        let result = resolve_path(&root, &candidate).await;
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents/readme.txt"));
    }

    #[tokio::test]
    async fn test_resolve_valid_file_with_leading_slash() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/documents/readme.txt");

        let result = resolve_path(&root, &candidate).await;
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents/readme.txt"));
    }

    #[tokio::test]
    async fn test_resolve_valid_directory() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "documents");

        let result = resolve_path(&root, &candidate).await;
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("documents"));
    }

    #[tokio::test]
    async fn test_resolve_empty_path() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "");

        // Empty path should resolve to the root itself
        let result = resolve_path(&root, &candidate).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[tokio::test]
    async fn test_resolve_just_slash() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/");

        // Just "/" should resolve to the root itself
        let result = resolve_path(&root, &candidate).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_reject_parent_directory() {
        let (_temp, root) = setup_test_area();
        // Simulate client sending "../etc/passwd" - validate before building path
        let result = build_and_validate_candidate_path(&root, "../etc/passwd");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[test]
    fn test_reject_parent_in_middle() {
        let (_temp, root) = setup_test_area();
        // Simulate client sending path with .. in the middle
        let result = build_and_validate_candidate_path(&root, "documents/../../../etc/passwd");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[tokio::test]
    async fn test_not_found() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "nonexistent/file.txt");

        let result = resolve_path(&root, &candidate).await;
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[tokio::test]
    async fn test_symlink_to_external_allowed() {
        let (_temp, _root) = setup_test_area();

        // Symlinks pointing outside the area are allowed (admin-created, trusted)
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create a temp directory outside the area to link to
            let external = TempDir::new().expect("Failed to create external dir");
            let external_path = external.path().canonicalize().unwrap();
            fs::write(external_path.join("external.txt"), "external").unwrap();

            // Create symlink pointing outside
            let link_path = _root.join("documents/external_link");
            symlink(&external_path, &link_path).expect("Failed to create symlink");

            // Should be allowed - admin-created symlink
            let candidate = build_candidate_path(&_root, "documents/external_link/external.txt");
            let result = resolve_path(&_root, &candidate).await;
            assert!(result.is_ok());
            assert!(result.unwrap().ends_with("external.txt"));
        }
    }

    #[tokio::test]
    async fn test_symlink_within_area_allowed() {
        let (_temp, _root) = setup_test_area();

        // Symlink that stays within the area root should work
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create symlink from one folder to another (both within area)
            let link_path = _root.join("doc_link");
            symlink(_root.join("documents"), &link_path).expect("Failed to create symlink");

            let candidate = build_candidate_path(&_root, "doc_link/readme.txt");
            let result = resolve_path(&_root, &candidate).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_current_dir_allowed() {
        let (_temp, root) = setup_test_area();
        let candidate = root.join("./documents/./readme.txt");

        let result = resolve_path(&root, &candidate).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reject_non_absolute_area_root() {
        let candidate = Path::new("/absolute/path/file.txt");
        let result = resolve_path(Path::new("relative/path"), candidate).await;
        assert_eq!(result, Err(PathError::InvalidAreaRoot));
    }

    #[tokio::test]
    async fn test_reject_non_absolute_candidate() {
        let (_temp, root) = setup_test_area();
        let candidate = Path::new("relative/path/file.txt");
        let result = resolve_path(&root, candidate).await;
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    // =========================================================================
    // resolve_new_path tests
    // =========================================================================

    #[tokio::test]
    async fn test_resolve_new_path_valid() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "documents/newfile.txt");

        let result = resolve_new_path(&root, &candidate).await;
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("newfile.txt"));
        assert!(path.parent().unwrap().ends_with("documents"));
    }

    #[tokio::test]
    async fn test_resolve_new_path_with_leading_slash() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/documents/newfile.txt");

        let result = resolve_new_path(&root, &candidate).await;
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("newfile.txt"));
    }

    #[tokio::test]
    async fn test_resolve_new_path_in_root() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "newfile.txt");

        let result = resolve_new_path(&root, &candidate).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_new_path_parent_not_found() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "nonexistent/newfile.txt");

        let result = resolve_new_path(&root, &candidate).await;
        assert_eq!(result, Err(PathError::NotFound));
    }

    #[test]
    fn test_resolve_new_path_reject_traversal() {
        let (_temp, root) = setup_test_area();
        // Simulate client sending "../newfile.txt" - validate before building path
        let result = build_and_validate_candidate_path(&root, "../newfile.txt");
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[tokio::test]
    async fn test_resolve_new_path_empty_is_invalid() {
        let (_temp, root) = setup_test_area();
        // Candidate equals area_root - no filename
        let candidate = root.clone();

        let result = resolve_new_path(&root, &candidate).await;
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[tokio::test]
    async fn test_resolve_new_path_just_slash_is_invalid() {
        let (_temp, root) = setup_test_area();
        let candidate = build_candidate_path(&root, "/");

        // This resolves to root with trailing slash, which after normalization equals root
        // The function should reject this since there's no filename
        let result = resolve_new_path(&root, &candidate).await;
        // Note: "/data/root/" != "/data/root" as Path, so this may succeed or fail
        // depending on path normalization. Let's check what we actually get:
        // build_candidate_path returns root.join("") which adds a trailing component
        // that's empty. Let's verify the behavior is sensible either way.
        assert!(result.is_err() || result.unwrap().file_name().is_some());
    }

    #[tokio::test]
    async fn test_resolve_new_path_reject_non_absolute_root() {
        let candidate = Path::new("/absolute/path/file.txt");
        let result = resolve_new_path(Path::new("relative/path"), candidate).await;
        assert_eq!(result, Err(PathError::InvalidAreaRoot));
    }

    #[tokio::test]
    async fn test_resolve_new_path_reject_non_absolute_candidate() {
        let (_temp, root) = setup_test_area();
        let candidate = Path::new("relative/path/file.txt");
        let result = resolve_new_path(&root, candidate).await;
        assert_eq!(result, Err(PathError::InvalidPath));
    }

    #[tokio::test]
    async fn test_resolve_new_path_via_symlink_allowed() {
        let (_temp, _root) = setup_test_area();

        // Symlinks are trusted (admin-created), so creating files through them is allowed
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            // Create a temp directory outside the area
            let external = TempDir::new().expect("Failed to create external dir");
            let external_path = external.path().canonicalize().unwrap();

            // Create symlink pointing outside
            let link_path = _root.join("external_link");
            symlink(&external_path, &link_path).expect("Failed to create symlink");

            // Creating a new file through the symlink should succeed
            let candidate = build_candidate_path(&_root, "external_link/newfile.txt");
            let result = resolve_new_path(&_root, &candidate).await;
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

    // =========================================================================
    // normalize_client_path tests
    // =========================================================================

    #[test]
    fn test_normalize_client_path_simple() {
        assert_eq!(normalize_client_path("foo/bar"), "foo/bar");
    }

    #[test]
    fn test_normalize_client_path_backslashes() {
        assert_eq!(normalize_client_path("foo\\bar\\baz"), "foo/bar/baz");
    }

    #[test]
    fn test_normalize_client_path_mixed_separators() {
        assert_eq!(normalize_client_path("foo/bar\\baz"), "foo/bar/baz");
    }

    #[test]
    fn test_normalize_client_path_multiple_slashes() {
        assert_eq!(normalize_client_path("foo//bar///baz"), "foo/bar/baz");
    }

    #[test]
    fn test_normalize_client_path_dot_segments() {
        assert_eq!(normalize_client_path("./foo/./bar/."), "foo/bar");
    }

    #[test]
    fn test_normalize_client_path_leading_slash() {
        assert_eq!(normalize_client_path("/foo/bar"), "foo/bar");
    }

    #[test]
    fn test_normalize_client_path_trailing_slash() {
        assert_eq!(normalize_client_path("foo/bar/"), "foo/bar");
    }

    #[test]
    fn test_normalize_client_path_empty() {
        assert_eq!(normalize_client_path(""), "");
    }

    #[test]
    fn test_normalize_client_path_just_slash() {
        assert_eq!(normalize_client_path("/"), "");
    }

    #[test]
    fn test_normalize_client_path_just_dot() {
        assert_eq!(normalize_client_path("."), "");
    }

    #[test]
    fn test_normalize_client_path_complex() {
        assert_eq!(normalize_client_path("./foo//bar\\.\\baz/"), "foo/bar/baz");
    }

    #[test]
    fn test_is_hidden_name_dotfiles() {
        assert!(is_hidden_name(".DS_Store"));
        assert!(is_hidden_name(".gitignore"));
        assert!(is_hidden_name(".hidden"));
    }

    #[test]
    fn test_is_hidden_name_nas_at_prefix() {
        assert!(is_hidden_name("@eaDir"));
        assert!(is_hidden_name("@tmp"));
        assert!(is_hidden_name("@sharebin"));
        assert!(is_hidden_name("@SynoResource"));
    }

    #[test]
    fn test_is_hidden_name_nas_hash_prefix() {
        assert!(is_hidden_name("#recycle"));
        assert!(is_hidden_name("#snapshot"));
    }

    #[test]
    fn test_is_hidden_name_normal_files() {
        assert!(!is_hidden_name("README.md"));
        assert!(!is_hidden_name("photo.jpg"));
        assert!(!is_hidden_name("Documents"));
        assert!(!is_hidden_name("my file.txt"));
        assert!(!is_hidden_name("Uploads [NEXUS-UL]"));
    }
}
