//! FileRename message handler - Renames a file or directory in the file area

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, DirNameError, FilePathError};

use super::{
    HandlerContext, err_dir_name_empty, err_dir_name_invalid, err_dir_name_too_long,
    err_file_not_found, err_file_path_invalid, err_file_path_too_long, err_not_logged_in,
    err_permission_denied, err_rename_failed, err_rename_target_exists,
};
use crate::db::Permission;
use crate::files::{build_and_validate_candidate_path, resolve_path, resolve_user_area};

/// Handle a file rename request
pub async fn handle_file_rename<W>(
    path: String,
    new_name: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("FileRename request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileRename"))
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
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check file root (cheap check, should always be set in production)
    let Some(file_root) = ctx.file_root else {
        // File area not configured
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    };

    // Check FileRename permission
    if !requesting_user.has_permission(Permission::FileRename) {
        eprintln!(
            "FileRename from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Check FileRoot permission if root browsing requested
    if root && !requesting_user.has_permission(Permission::FileRoot) {
        eprintln!(
            "FileRename (root) from {} (user: {}) without file_root permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
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
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Validate new name (same rules as directory names - no path separators, no .., etc.)
    if let Err(e) = validators::validate_dir_name(&new_name) {
        let error_msg = match e {
            DirNameError::Empty => err_dir_name_empty(ctx.locale),
            DirNameError::TooLong => {
                err_dir_name_too_long(ctx.locale, validators::MAX_DIR_NAME_LENGTH)
            }
            DirNameError::ContainsPathSeparator
            | DirNameError::ContainsParentRef
            | DirNameError::ContainsNull
            | DirNameError::InvalidCharacters => err_dir_name_invalid(ctx.locale),
        };
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(error_msg),
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
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build and validate candidate path
    let candidate = match build_and_validate_candidate_path(&area_root, &path) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check if the candidate path exists (using symlink_metadata to not follow symlinks)
    let symlink_meta = std::fs::symlink_metadata(&candidate);

    // Determine the source path
    let source_path = match &symlink_meta {
        Ok(meta) if meta.file_type().is_symlink() => {
            // It's a symlink - rename the symlink itself, not the target
            candidate.clone()
        }
        Ok(_) => {
            // Not a symlink - resolve and use the canonical path
            match resolve_path(&area_root, &candidate) {
                Ok(p) => p,
                Err(_) => {
                    let response = ServerMessage::FileRenameResponse {
                        success: false,
                        error: Some(err_file_not_found(ctx.locale)),
                    };
                    return ctx.send_message(&response).await;
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
        Err(_) => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Prevent renaming the area root itself
    if source_path == area_root || candidate == area_root {
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Build the target path (same directory, new name)
    let parent_dir = match source_path.parent() {
        Some(p) => p,
        None => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };
    let target_path = parent_dir.join(&new_name);

    // Check if target already exists
    if target_path.exists() || target_path.symlink_metadata().is_ok() {
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_rename_target_exists(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Perform the rename
    match std::fs::rename(&source_path, &target_path) {
        Ok(()) => {
            let response = ServerMessage::FileRenameResponse {
                success: true,
                error: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!(
                "FileRename failed for {} (user: {}): {}",
                ctx.peer_addr, requesting_user.username, e
            );
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_rename_failed(ctx.locale)),
            };
            ctx.send_message(&response).await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
    };

    fn setup_file_area() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        fs::create_dir_all(temp_dir.path().join("shared")).expect("Failed to create shared");
        fs::create_dir_all(temp_dir.path().join("users")).expect("Failed to create users dir");
        temp_dir
    }

    #[tokio::test]
    async fn test_rename_requires_auth() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_rename(
            "test.txt".to_string(),
            "renamed.txt".to_string(),
            false,
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err()); // Should disconnect
    }

    #[tokio::test]
    async fn test_rename_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Create a file to rename
        fs::write(file_area.path().join("shared/test.txt"), "content")
            .expect("Failed to create test file");

        // User without file_rename permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_rename(
            "test.txt".to_string(),
            "renamed.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // File should still exist
        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_file_success() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file to rename
        fs::write(file_root.join("shared/original.txt"), "content").expect("Failed to create file");
        assert!(file_root.join("shared/original.txt").exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "original.txt".to_string(),
            "renamed.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // Verify file was renamed
        assert!(!file_root.join("shared/original.txt").exists());
        assert!(file_root.join("shared/renamed.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_directory_success() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a directory to rename
        fs::create_dir(file_root.join("shared/original_dir")).expect("Failed to create dir");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "original_dir".to_string(),
            "renamed_dir".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // Verify directory was renamed
        assert!(!file_root.join("shared/original_dir").exists());
        assert!(file_root.join("shared/renamed_dir").exists());
    }

    #[tokio::test]
    async fn test_rename_target_exists() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create two files
        fs::write(file_root.join("shared/file1.txt"), "content1").unwrap();
        fs::write(file_root.join("shared/file2.txt"), "content2").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "file1.txt".to_string(),
            "file2.txt".to_string(), // Already exists
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().contains("exists"));
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_not_found() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "nonexistent.txt".to_string(),
            "renamed.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_path_traversal_blocked() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "../escape.txt".to_string(),
            "renamed.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_new_name_with_path_separator_blocked() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        fs::write(file_root.join("shared/test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "test.txt".to_string(),
            "subdir/renamed.txt".to_string(), // Contains path separator
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_new_name_with_parent_ref_blocked() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        fs::write(file_root.join("shared/test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "test.txt".to_string(),
            "..".to_string(), // Parent ref
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_empty_new_name() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        fs::write(file_root.join("shared/test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "test.txt".to_string(),
            "".to_string(), // Empty new name
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // File should still exist with original name
        assert!(file_root.join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_unicode_filename() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file with unicode name
        fs::write(file_root.join("shared/文件.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "文件.txt".to_string(),
            "新文件.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // Verify file was renamed
        assert!(!file_root.join("shared/文件.txt").exists());
        assert!(file_root.join("shared/新文件.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_root_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file in shared
        fs::write(file_root.join("shared/test.txt"), "content").expect("Failed to create file");

        // User with file_rename but not file_root
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        // Try to rename with root=true
        handle_file_rename(
            "shared/test.txt".to_string(),
            "renamed.txt".to_string(),
            true, // root = true
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // File should still exist with original name
        assert!(file_root.join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_root_mode_success() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file in shared (accessible via root mode)
        fs::write(file_root.join("shared/root_test.txt"), "content")
            .expect("Failed to create file");

        // User with both file_rename and file_root permissions
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[
                Permission::FileList,
                Permission::FileRename,
                Permission::FileRoot,
            ],
            false,
        )
        .await;

        handle_file_rename(
            "shared/root_test.txt".to_string(),
            "renamed_root.txt".to_string(),
            true, // root = true
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success, "Rename with root mode should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // File should be renamed
        assert!(!file_root.join("shared/root_test.txt").exists());
        assert!(file_root.join("shared/renamed_root.txt").exists());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_rename_symlink() {
        use std::os::unix::fs::symlink;

        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a target file and a symlink to it
        let target = file_root.join("shared/target.txt");
        let link = file_root.join("shared/link.txt");
        fs::write(&target, "target content").expect("Failed to create target");
        symlink(&target, &link).expect("Failed to create symlink");

        assert!(target.exists());
        assert!(link.exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        // Rename the symlink
        handle_file_rename(
            "link.txt".to_string(),
            "renamed_link.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success, "Symlink rename should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // Old symlink name should be gone, new name should exist
        assert!(!link.exists());
        assert!(file_root.join("shared/renamed_link.txt").exists());
        // Target should still exist (we renamed the link, not the target)
        assert!(target.exists(), "Target file should not be affected");
    }

    #[tokio::test]
    async fn test_rename_in_user_personal_area() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a user's personal area with a file
        fs::create_dir_all(file_root.join("users/testuser")).expect("Failed to create user dir");
        fs::write(
            file_root.join("users/testuser/myfile.txt"),
            "personal content",
        )
        .expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        // Rename from personal area (user sees it as root)
        handle_file_rename(
            "myfile.txt".to_string(),
            "renamed_personal.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(
                    success,
                    "Rename in personal area should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        // File should be renamed
        assert!(!file_root.join("users/testuser/myfile.txt").exists());
        assert!(
            file_root
                .join("users/testuser/renamed_personal.txt")
                .exists()
        );
    }
}
