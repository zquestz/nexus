//! FileInfo message handler - Returns detailed information about a file or directory

use std::io;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWrite;

use nexus_common::protocol::{FileInfoDetails, ServerMessage};
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_file_not_found, err_file_path_invalid, err_file_path_too_long,
    err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;
use crate::files::{build_and_validate_candidate_path, resolve_path, resolve_user_area};

/// Detect MIME type from file content using magic bytes
///
/// Falls back to extension-based detection if magic bytes don't match
fn detect_mime_type(path: &Path) -> Option<String> {
    // Try magic byte detection first
    if let Some(kind) = infer::get_from_path(path).ok().flatten() {
        return Some(kind.mime_type().to_string());
    }

    // Fall back to extension-based detection for text files and others
    // that infer doesn't detect well
    let extension = path.extension()?.to_str()?.to_lowercase();
    let mime = match extension.as_str() {
        // Text files
        "txt" | "log" | "nfo" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "json" => "application/json",
        "yaml" | "yml" => "application/x-yaml",
        "toml" => "application/toml",

        // Source code
        "rs" => "text/x-rust",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "py" => "text/x-python",
        "rb" => "text/x-ruby",
        "go" => "text/x-go",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" | "cxx" => "text/x-c++",
        "java" => "text/x-java",
        "swift" => "text/x-swift",
        "kt" | "kts" => "text/x-kotlin",
        "sh" | "bash" => "text/x-shellscript",
        "ps1" => "text/x-powershell",
        "sql" => "text/x-sql",
        "php" => "text/x-php",

        // Config files
        "ini" | "cfg" | "conf" | "env" => "text/plain",

        // Documents (if not detected by magic)
        "rtf" => "application/rtf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "odt" => "application/vnd.oasis.opendocument.text",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odp" => "application/vnd.oasis.opendocument.presentation",

        _ => return None,
    };

    Some(mime.to_string())
}

/// Count items in a directory (non-recursive)
fn count_directory_items(path: &Path) -> Option<u64> {
    let entries = std::fs::read_dir(path).ok()?;
    Some(entries.count() as u64)
}

/// Compute SHA-256 hash of a file
fn compute_sha256(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer).ok()?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Some(hex::encode(result))
}

/// Handle a file info request
pub async fn handle_file_info<W>(
    path: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("FileInfo request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileInfo"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            // Session not found - likely a race condition, not a security event
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                info: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check file root (cheap check, should always be set in production)
    let Some(file_root) = ctx.file_root else {
        // File area not configured
        let response = ServerMessage::FileInfoResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
            info: None,
        };
        return ctx.send_message(&response).await;
    };

    // Check FileInfo permission
    if !requesting_user.has_permission(Permission::FileInfo) {
        eprintln!(
            "FileInfo from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileInfoResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            info: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check FileRoot permission if root browsing requested
    if root && !requesting_user.has_permission(Permission::FileRoot) {
        eprintln!(
            "FileInfo (root) from {} (user: {}) without file_root permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileInfoResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            info: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate path
    if let Err(e) = validators::validate_file_path(&path) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_file_path_invalid(ctx.locale),
        };
        let response = ServerMessage::FileInfoResponse {
            success: false,
            error: Some(error_msg),
            info: None,
        };
        return ctx.send_message(&response).await;
    }

    // Resolve area root - either file root (if root browsing) or user's area
    let area_root_path = if root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
    };

    // Canonicalize area_root (it might not exist yet for new users)
    let area_root = match area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // User's area doesn't exist
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                info: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build and validate candidate path
    let candidate = match build_and_validate_candidate_path(&area_root, &path) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                info: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check if the candidate path is a symlink BEFORE resolving
    // (resolve_path follows symlinks, so we'd lose this info)
    let is_symlink = std::fs::symlink_metadata(&candidate)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    // Resolve and validate the path (follows symlinks, checks it's within area)
    let resolved = match resolve_path(&area_root, &candidate) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                info: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Get file metadata from resolved path (follows symlinks for size, timestamps, etc.)
    let metadata = match std::fs::metadata(&resolved) {
        Ok(m) => m,
        Err(_) => {
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                info: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let is_directory = metadata.is_dir();
    let size = if is_directory { 0 } else { metadata.len() };

    // Get timestamps
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Created time (not available on all filesystems)
    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    // Get file name from the candidate path (not resolved) to preserve symlink names
    let name = candidate
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    // MIME type (only for files)
    let mime_type = if is_directory {
        None
    } else {
        detect_mime_type(&resolved)
    };

    // Item count (only for directories)
    let item_count = if is_directory {
        count_directory_items(&resolved)
    } else {
        None
    };

    // SHA-256 hash (only for files)
    let sha256 = if is_directory {
        None
    } else {
        compute_sha256(&resolved)
    };

    let info = FileInfoDetails {
        name,
        size,
        created,
        modified,
        is_directory,
        is_symlink,
        mime_type,
        item_count,
        sha256,
    };

    let response = ServerMessage::FileInfoResponse {
        success: true,
        error: None,
        info: Some(info),
    };
    ctx.send_message(&response).await
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    fn setup_file_area() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        fs::create_dir_all(temp_dir.path().join("shared")).expect("Failed to create shared");
        fs::create_dir_all(temp_dir.path().join("users")).expect("Failed to create users dir");
        temp_dir
    }

    #[tokio::test]
    async fn test_file_info_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_info(
            "test.txt".to_string(),
            false,
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err()); // Should disconnect
    }

    #[tokio::test]
    async fn test_file_info_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // User without file_list permission
        let session_id = login_user(&mut test_ctx, "testuser", "pass", &[], false).await;

        let result = handle_file_info(
            "test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_file_success() {
        let file_area = setup_file_area();

        // Create a test file
        let shared_dir = file_area.path().join("shared");
        let test_file = shared_dir.join("test.txt");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"Hello, world!").unwrap();

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse {
                success,
                error,
                info,
            } => {
                assert!(success);
                assert!(error.is_none());
                let info = info.expect("Expected info");
                assert_eq!(info.name, "test.txt");
                assert_eq!(info.size, 13); // "Hello, world!" is 13 bytes
                assert!(!info.is_directory);
                assert!(!info.is_symlink);
                assert_eq!(info.mime_type.as_deref(), Some("text/plain"));
                assert!(info.item_count.is_none());
                // SHA-256 of "Hello, world!"
                assert_eq!(
                    info.sha256.as_deref(),
                    Some("315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3")
                );
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_directory_success() {
        let file_area = setup_file_area();

        // Create a test directory with some files
        let shared_dir = file_area.path().join("shared");
        let test_dir = shared_dir.join("testdir");
        fs::create_dir(&test_dir).unwrap();
        fs::File::create(test_dir.join("file1.txt")).unwrap();
        fs::File::create(test_dir.join("file2.txt")).unwrap();
        fs::create_dir(test_dir.join("subdir")).unwrap();

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "testdir".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse {
                success,
                error,
                info,
            } => {
                assert!(success);
                assert!(error.is_none());
                let info = info.expect("Expected info");
                assert_eq!(info.name, "testdir");
                assert_eq!(info.size, 0);
                assert!(info.is_directory);
                assert!(!info.is_symlink);
                assert!(info.mime_type.is_none());
                assert_eq!(info.item_count, Some(3)); // 2 files + 1 subdir
                assert!(info.sha256.is_none()); // Directories don't have SHA256
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_not_found() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "nonexistent.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_path_traversal_blocked() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "../../../etc/passwd".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_admin_has_permission() {
        let file_area = setup_file_area();

        // Create a test file
        let shared_dir = file_area.path().join("shared");
        let test_file = shared_dir.join("admin_test.txt");
        fs::File::create(&test_file).unwrap();

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Admin has all permissions implicitly
        let session_id = login_user(&mut test_ctx, "admin", "pass", &[], true).await;

        let result = handle_file_info(
            "admin_test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_root_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // User with file_list but not file_root
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        // Try to browse with root=true
        let result = handle_file_info(
            "shared".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some()); // Permission denied
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_root_with_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // User with both file_info and file_root
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo, Permission::FileRoot],
            false,
        )
        .await;

        let result = handle_file_info(
            "shared".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, info, .. } => {
                assert!(success);
                let info = info.expect("Expected info");
                assert_eq!(info.name, "shared");
                assert!(info.is_directory);
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_info_symlink_detected() {
        use std::os::unix::fs::symlink;

        let file_area = setup_file_area();

        // Create a test file and a symlink to it
        let shared_dir = file_area.path().join("shared");
        let test_file = shared_dir.join("original.txt");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"Original content").unwrap();

        let symlink_path = shared_dir.join("link.txt");
        symlink(&test_file, &symlink_path).unwrap();

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "link.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, info, .. } => {
                assert!(success);
                let info = info.expect("Expected info");
                assert_eq!(info.name, "link.txt");
                assert!(info.is_symlink);
                assert!(!info.is_directory);
                // Size should be of the target file
                assert_eq!(info.size, 16); // "Original content"
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_mime_type_detection() {
        let file_area = setup_file_area();
        let shared_dir = file_area.path().join("shared");

        // Create a text file
        let file_path = shared_dir.join("test.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"Hello world").unwrap();

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, info, .. } => {
                assert!(success);
                let info = info.expect("Expected info");
                assert_eq!(info.mime_type.as_deref(), Some("text/plain"));
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_personal_area() {
        let file_area = setup_file_area();

        // Create user's personal area with a file
        let user_dir = file_area.path().join("users").join("testuser");
        fs::create_dir_all(&user_dir).unwrap();
        let test_file = user_dir.join("myfile.txt");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"My personal file").unwrap();

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;

        let result = handle_file_info(
            "myfile.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response: ServerMessage = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileInfoResponse { success, info, .. } => {
                assert!(success);
                let info = info.expect("Expected info");
                assert_eq!(info.name, "myfile.txt");
                assert_eq!(info.size, 16); // "My personal file"
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }
}
