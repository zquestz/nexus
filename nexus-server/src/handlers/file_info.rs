//! FileInfo handler — detailed info about a file or directory.

use std::io;
use std::path::Path;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::{FileInfoDetails, ServerMessage};
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_file_area_not_accessible, err_file_area_not_configured, err_file_not_found,
    err_file_path_invalid, err_file_path_too_long, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    HANDLER_FILE_INFO, LOG_FILE_INFO_NOT_LOGGED_IN, LOG_FILE_INFO_PERMISSION_DENIED,
    LOG_FILE_INFO_ROOT_DENIED,
};
use crate::db::Permission;
use crate::files::{
    build_and_validate_candidate_path, dropbox_entry_visible, is_unreadable_dropbox_dir,
    resolve_path, resolve_user_area,
};

/// Count items in a directory (non-recursive), off the async runtime.
async fn count_directory_items_async(path: &Path) -> Option<u64> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let entries = std::fs::read_dir(&path).ok()?;
        Some(entries.count() as u64)
    })
    .await
    .ok()?
}

/// BLAKE3 of a file via nexus-common's hash module (off the async runtime).
async fn compute_blake3_async(path: &Path) -> Option<String> {
    nexus_common::hash::compute_blake3(path).await.ok()
}

/// Detect MIME type from file content, off the async runtime.
async fn detect_mime_type_async(path: &Path) -> Option<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || detect_mime_type_sync(&path))
        .await
        .ok()?
}

/// Synchronous MIME detection (called from spawn_blocking).
fn detect_mime_type_sync(path: &Path) -> Option<String> {
    // Magic-byte detection first, then fall back to extension.
    if let Some(kind) = infer::get_from_path(path).ok().flatten() {
        return Some(kind.mime_type().to_string());
    }
    detect_mime_from_extension(path)
}

/// Detect MIME type from file extension only (no I/O)
fn detect_mime_from_extension(path: &Path) -> Option<String> {
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

pub async fn handle_file_info<W>(
    path: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_INFO_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_FILE_INFO))
            .await;
    };

    let Some(file_root) = ctx.file_root else {
        let response = ServerMessage::FileInfoResponse {
            success: false,
            error: Some(err_file_area_not_configured(ctx.locale)),
            info: None,
        };
        return ctx.send_message(&response).await;
    };

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

    let user_area_result = 'user_area: {
        let _state_guard = ctx.user_manager.read_user_state().await;
        let requesting_user = match ctx
            .user_manager
            .get_user_by_session_id(requesting_session_id)
            .await
        {
            Some(u) => u,
            None => {
                // Session gone — a race, not a security event.
                let response = ServerMessage::FileInfoResponse {
                    success: false,
                    error: Some(err_not_logged_in(ctx.locale)),
                    info: None,
                };
                break 'user_area Err(response);
            }
        };

        if !requesting_user.has_permission(Permission::FileInfo) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_INFO_PERMISSION_DENIED);
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                info: None,
            };
            break 'user_area Err(response);
        }

        // Root browsing additionally requires FileRoot.
        if root && !requesting_user.has_permission(Permission::FileRoot) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_INFO_ROOT_DENIED);
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                info: None,
            };
            break 'user_area Err(response);
        }

        let area_root_path = if root {
            file_root.to_path_buf()
        } else {
            resolve_user_area(file_root, &requesting_user.username).await
        };
        Ok((
            area_root_path,
            requesting_user.is_admin,
            requesting_user.username,
        ))
    };
    let (area_root_path, is_admin, username) = match user_area_result {
        Ok(user_area) => user_area,
        Err(response) => return ctx.send_message(&response).await,
    };

    let area_root = match tokio::fs::canonicalize(&area_root_path).await {
        Ok(p) => p,
        Err(_) => {
            // Root mode: admin's file_root is broken. Non-root: the user's
            // personal area dir doesn't exist yet, so the file can't either.
            let error_msg = if root {
                err_file_area_not_accessible(ctx.locale)
            } else {
                err_file_not_found(ctx.locale)
            };
            let response = ServerMessage::FileInfoResponse {
                success: false,
                error: Some(error_msg),
                info: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let candidate = match build_and_validate_candidate_path(&area_root, &path).await {
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

    // Check symlink-ness before resolve_path follows it.
    let is_symlink = tokio::fs::symlink_metadata(&candidate)
        .await
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    // Resolve symlinks and confirm the result stays within the area.
    let resolved = match resolve_path(&area_root, &candidate).await {
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

    // Metadata from the resolved path (symlink target's size, timestamps, …).
    let metadata = match tokio::fs::metadata(&resolved).await {
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

    // Drop-box read-gate, now that we know whether the entry is a directory (so a
    // regular file that merely *looks* like a drop box isn't treated as one, and
    // the owner of a nested drop box still sees their own box). An item the
    // requester can't see is indistinguishable from missing; the box folder
    // itself stays viewable, with its child count masked below.
    if !dropbox_entry_visible(&candidate, &area_root, &username, is_admin, is_directory) {
        let response = ServerMessage::FileInfoResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
            info: None,
        };
        return ctx.send_message(&response).await;
    }
    let contents_readable =
        !is_unreadable_dropbox_dir(&candidate, &area_root, &username, is_admin, is_directory);

    let size = if is_directory { 0 } else { metadata.len() };

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Not available on all filesystems.
    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    // Name from the candidate (not resolved) so symlink names are preserved.
    let name = candidate
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let mime_type = if is_directory {
        None
    } else {
        detect_mime_type_async(&resolved).await
    };

    // Mask the child count of a drop box folder the requester may not read.
    let item_count = if !is_directory {
        None
    } else if contents_readable {
        count_directory_items_async(&resolved).await
    } else {
        Some(0)
    };

    let blake3 = if is_directory {
        None
    } else {
        compute_blake3_async(&resolved).await
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
        blake3,
    };

    let response = ServerMessage::FileInfoResponse {
        success: true,
        error: None,
        info: Some(info),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
        setup_file_area_basic,
    };

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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create a test file
        let shared_dir = file_area.path().join("shared");
        let test_file = shared_dir.join("test.txt");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"Hello, world!").unwrap();

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
                // BLAKE3 of "Hello, world!"
                assert_eq!(
                    info.blake3.as_deref(),
                    Some("ede5c0b10f2ec4979c69b52f61e42ff5b413519ce09be0f14d098dcfe5f6f98d")
                );
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_directory_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create a test directory with some files
        let shared_dir = file_area.path().join("shared");
        let test_dir = shared_dir.join("testdir");
        fs::create_dir(&test_dir).unwrap();
        fs::File::create(test_dir.join("file1.txt")).unwrap();
        fs::File::create(test_dir.join("file2.txt")).unwrap();
        fs::create_dir(test_dir.join("subdir")).unwrap();

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
                assert!(info.blake3.is_none()); // Directories don't have BLAKE3
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_not_found() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create a test file
        let shared_dir = file_area.path().join("shared");
        let test_file = shared_dir.join("admin_test.txt");
        fs::File::create(&test_file).unwrap();

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileInfoResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_info_root_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create a test file and a symlink to it
        let shared_dir = file_area.path().join("shared");
        let test_file = shared_dir.join("original.txt");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"Original content").unwrap();

        let symlink_path = shared_dir.join("link.txt");
        symlink(&test_file, &symlink_path).unwrap();

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);
        let shared_dir = file_area.path().join("shared");

        // Create a text file
        let file_path = shared_dir.join("test.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"Hello world").unwrap();

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create user's personal area with a file
        let user_dir = file_area.path().join("users").join("testuser");
        fs::create_dir_all(&user_dir).unwrap();
        let test_file = user_dir.join("myfile.txt");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"My personal file").unwrap();

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
        let response: ServerMessage = read_server_message(&mut test_ctx).await;
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

    fn assert_info_not_found(response: ServerMessage) {
        match response {
            ServerMessage::FileInfoResponse {
                success,
                error,
                info,
            } => {
                assert!(!success);
                assert!(info.is_none());
                assert_eq!(error, Some(err_file_not_found(DEFAULT_TEST_LOCALE)));
            }
            other => panic!("Expected FileInfoResponse, got: {:?}", other),
        }
    }

    fn assert_info_ok(response: ServerMessage) -> nexus_common::protocol::FileInfoDetails {
        match response {
            ServerMessage::FileInfoResponse { success, info, .. } => {
                assert!(success);
                info.expect("info")
            }
            other => panic!("Expected FileInfoResponse, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_file_info_dropbox_gating() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // alice's drop box in /shared (users without a personal dir share /shared).
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir_all(&dropbox).unwrap();
        fs::write(dropbox.join("secret.txt"), "hi").unwrap();

        let file = "For Alice [NEXUS-DB-alice]/secret.txt";
        let folder = "For Alice [NEXUS-DB-alice]";

        // bob (non-owner): the file inside is indistinguishable from missing; the
        // folder itself is viewable, but its child count is masked to 0.
        let bob = login_user(&mut test_ctx, "bob", "pass", &[Permission::FileInfo], false).await;
        let r = handle_file_info(
            file.to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert_info_not_found(read_server_message(&mut test_ctx).await);
        let r = handle_file_info(
            folder.to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        let info = assert_info_ok(read_server_message(&mut test_ctx).await);
        assert!(info.is_directory);
        assert_eq!(info.item_count, Some(0));

        // alice (owner): real child count, and the file inside is readable.
        let alice = login_user(
            &mut test_ctx,
            "alice",
            "pass",
            &[Permission::FileInfo],
            false,
        )
        .await;
        let r = handle_file_info(
            folder.to_string(),
            false,
            Some(alice),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert_eq!(
            assert_info_ok(read_server_message(&mut test_ctx).await).item_count,
            Some(1)
        );
        let r = handle_file_info(
            file.to_string(),
            false,
            Some(alice),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert_eq!(
            assert_info_ok(read_server_message(&mut test_ctx).await).name,
            "secret.txt"
        );

        // admin: real child count.
        let admin = login_user(&mut test_ctx, "admin", "pass", &[], true).await;
        let r = handle_file_info(
            folder.to_string(),
            false,
            Some(admin),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert_eq!(
            assert_info_ok(read_server_message(&mut test_ctx).await).item_count,
            Some(1)
        );
    }

    #[tokio::test]
    async fn test_file_info_nested_owned_and_suffix_named_file() {
        let mut test_ctx = create_test_context().await;
        let area = setup_file_area_basic(&mut test_ctx);
        let shared = area.path().join("shared");
        fs::create_dir_all(shared.join("Outer [NEXUS-DB-alice]/Inner [NEXUS-DB-bob]")).unwrap();
        fs::write(
            shared.join("Outer [NEXUS-DB-alice]/Inner [NEXUS-DB-bob]/data.txt"),
            "x",
        )
        .unwrap();
        // A regular FILE whose name merely carries a drop-box suffix.
        fs::write(shared.join("Budget [NEXUS-DB-alice]"), "y").unwrap();

        let inner = "Outer [NEXUS-DB-alice]/Inner [NEXUS-DB-bob]";

        // M1: bob (owner of the inner box) sees the folder with its REAL child
        // count, even though it sits inside alice's box.
        let bob = login_user(&mut test_ctx, "bob", "pw", &[Permission::FileInfo], false).await;
        let r = handle_file_info(
            inner.to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        let info = assert_info_ok(read_server_message(&mut test_ctx).await);
        assert!(info.is_directory);
        assert_eq!(info.item_count, Some(1));

        // L1: a regular file that only looks like a box returns normal info.
        let r = handle_file_info(
            "Budget [NEXUS-DB-alice]".to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert!(!assert_info_ok(read_server_message(&mut test_ctx).await).is_directory);

        // An unrelated user can't see the nested box at all — it's not-found.
        let carol = login_user(&mut test_ctx, "carol", "pw", &[Permission::FileInfo], false).await;
        let r = handle_file_info(
            inner.to_string(),
            false,
            Some(carol),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert_info_not_found(read_server_message(&mut test_ctx).await);
    }

    #[tokio::test]
    async fn test_file_info_missing_permission_is_path_independent() {
        let mut test_ctx = create_test_context().await;
        let area = setup_file_area_basic(&mut test_ctx);
        let shared = area.path().join("shared");
        fs::write(shared.join("public.txt"), "x").unwrap();
        fs::create_dir_all(shared.join("For Alice [NEXUS-DB-alice]")).unwrap();
        fs::write(shared.join("For Alice [NEXUS-DB-alice]/secret.txt"), "y").unwrap();

        // bob has NO FileInfo permission. A normal file and a hidden drop-box path
        // both return the same permission-denied — the base-permission check runs
        // first, so the response leaks nothing about which paths exist.
        let bob = login_user(&mut test_ctx, "bob", "pw", &[], false).await;
        for path in ["public.txt", "For Alice [NEXUS-DB-alice]/secret.txt"] {
            let r = handle_file_info(
                path.to_string(),
                false,
                Some(bob),
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(r.is_ok());
            match read_server_message(&mut test_ctx).await {
                ServerMessage::FileInfoResponse { success, error, .. } => {
                    assert!(!success);
                    assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
                }
                other => panic!("Expected FileInfoResponse, got: {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_file_info_root_mode_non_admin_fileroot_still_gated() {
        let mut test_ctx = create_test_context().await;
        let area = setup_file_area_basic(&mut test_ctx);
        // A blind drop box somewhere in the tree (contents are admin-only).
        let blind = area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir_all(&blind).unwrap();
        fs::write(blind.join("secret.txt"), "x").unwrap();

        let path = "shared/Inbox [NEXUS-DB]/secret.txt";

        // A non-admin FileRoot holder browsing root mode is still gated: FileRoot
        // grants tree-wide *navigation*, not the admin override the drop-box gate
        // keys on. The blind box contents stay indistinguishable from missing.
        let mgr = login_user(
            &mut test_ctx,
            "mgr",
            "pw",
            &[Permission::FileInfo, Permission::FileRoot],
            false,
        )
        .await;
        let r = handle_file_info(
            path.to_string(),
            true,
            Some(mgr),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert_info_not_found(read_server_message(&mut test_ctx).await);

        // An admin (real override) reads it.
        let admin = login_user(&mut test_ctx, "admin", "pw", &[], true).await;
        let r = handle_file_info(
            path.to_string(),
            true,
            Some(admin),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(r.is_ok());
        assert!(!assert_info_ok(read_server_message(&mut test_ctx).await).is_directory);
    }
}
