//! FileDelete message handler - Deletes a file or empty directory in the file area

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_delete_failed, err_dir_not_empty, err_file_not_found,
    err_file_path_invalid, err_file_path_too_long, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;
use crate::files::path::PathError;
use crate::files::{build_and_validate_candidate_path, resolve_path, resolve_user_area};

/// Handle a file delete request
pub async fn handle_file_delete<W>(
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
        eprintln!("FileDelete request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileDelete"))
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
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check file root (cheap check, should always be set in production)
    let Some(file_root) = ctx.file_root else {
        // File area not configured
        let response = ServerMessage::FileDeleteResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    };

    // Check FileDelete permission
    if !requesting_user.has_permission(Permission::FileDelete) {
        eprintln!(
            "FileDelete from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Check FileRoot permission if root browsing requested
    if root && !requesting_user.has_permission(Permission::FileRoot) {
        eprintln!(
            "FileDelete (root) from {} (user: {}) without file_root permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileDeleteResponse {
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
        let response = ServerMessage::FileDeleteResponse {
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
            let response = ServerMessage::FileDeleteResponse {
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
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check if the candidate path is a symlink (without following it)
    // We use symlink_metadata to avoid following the final symlink component
    let symlink_meta = std::fs::symlink_metadata(&candidate);

    // Determine what to delete and whether it's a directory
    let (path_to_delete, is_dir) = match &symlink_meta {
        Ok(meta) if meta.file_type().is_symlink() => {
            // It's a symlink - delete the symlink itself, not the target
            // Symlinks are treated as files for deletion purposes
            (candidate.clone(), false)
        }
        Ok(_) => {
            // Not a symlink - resolve and use the canonical path
            let resolved = match resolve_path(&area_root, &candidate) {
                Ok(p) => p,
                Err(PathError::NotFound) => {
                    let response = ServerMessage::FileDeleteResponse {
                        success: false,
                        error: Some(err_file_not_found(ctx.locale)),
                    };
                    return ctx.send_message(&response).await;
                }
                Err(_) => {
                    let response = ServerMessage::FileDeleteResponse {
                        success: false,
                        error: Some(err_file_path_invalid(ctx.locale)),
                    };
                    return ctx.send_message(&response).await;
                }
            };

            // Prevent deleting the area root itself
            if resolved == area_root {
                let response = ServerMessage::FileDeleteResponse {
                    success: false,
                    error: Some(err_permission_denied(ctx.locale)),
                };
                return ctx.send_message(&response).await;
            }

            let is_dir = resolved.is_dir();
            (resolved, is_dir)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
        Err(_) => {
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Prevent deleting the area root itself (also check candidate for symlink case)
    if candidate == area_root {
        let response = ServerMessage::FileDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Try to delete - file, symlink, or empty directory
    let result = if is_dir {
        std::fs::remove_dir(&path_to_delete)
    } else {
        std::fs::remove_file(&path_to_delete)
    };

    match result {
        Ok(()) => {
            let response = ServerMessage::FileDeleteResponse {
                success: true,
                error: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            // Check for "directory not empty" error
            let error_msg = if is_dir && e.kind() == std::io::ErrorKind::DirectoryNotEmpty {
                err_dir_not_empty(ctx.locale)
            } else {
                eprintln!(
                    "FileDelete failed for {} (user: {}): {}",
                    ctx.peer_addr, requesting_user.username, e
                );
                err_delete_failed(ctx.locale)
            };
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(error_msg),
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

        // Create shared and users directories
        fs::create_dir_all(temp_dir.path().join("shared")).expect("Failed to create shared");
        fs::create_dir_all(temp_dir.path().join("users")).expect("Failed to create users dir");

        temp_dir
    }

    #[tokio::test]
    async fn test_delete_requires_auth() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_delete(
            "test.txt".to_string(),
            false,
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err()); // Should disconnect
    }

    #[tokio::test]
    async fn test_delete_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Create a file to delete
        fs::write(file_area.path().join("shared/test.txt"), "content")
            .expect("Failed to create test file");

        // User without file_delete permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should still exist
        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_file_success() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file to delete
        fs::write(file_root.join("shared/test.txt"), "content").expect("Failed to create file");
        assert!(file_root.join("shared/test.txt").exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone
        assert!(!file_root.join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_empty_directory_success() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create an empty directory
        fs::create_dir(file_root.join("shared/empty_dir")).expect("Failed to create dir");
        assert!(file_root.join("shared/empty_dir").exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "empty_dir".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // Directory should be gone
        assert!(!file_root.join("shared/empty_dir").exists());
    }

    #[tokio::test]
    async fn test_delete_non_empty_directory_fails() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a directory with a file inside
        fs::create_dir(file_root.join("shared/non_empty")).expect("Failed to create dir");
        fs::write(file_root.join("shared/non_empty/file.txt"), "content")
            .expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "non_empty".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_dir_not_empty(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // Directory should still exist
        assert!(file_root.join("shared/non_empty").exists());
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "nonexistent.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_file_not_found(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_delete_path_traversal_blocked() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "../users/alice/secret.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_file_path_invalid(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_delete_cannot_delete_area_root() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Try to delete the root itself
        handle_file_delete(
            "/".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_delete_root_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file in shared
        fs::write(file_root.join("shared/test.txt"), "content").expect("Failed to create file");

        // User with file_delete but not file_root
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Try to delete with root=true
        handle_file_delete(
            "shared/test.txt".to_string(),
            true, // root = true
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should still exist
        assert!(file_root.join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_admin_can_delete() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file to delete
        fs::write(file_root.join("shared/admin_test.txt"), "content")
            .expect("Failed to create file");

        // Admin (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        handle_file_delete(
            "admin_test.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Admin delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone
        assert!(!file_root.join("shared/admin_test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_nested_file() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a nested file
        fs::create_dir_all(file_root.join("shared/docs/archive")).expect("Failed to create dirs");
        fs::write(file_root.join("shared/docs/archive/old.txt"), "content")
            .expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "docs/archive/old.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone but directories remain
        assert!(!file_root.join("shared/docs/archive/old.txt").exists());
        assert!(file_root.join("shared/docs/archive").exists());
    }

    #[tokio::test]
    async fn test_delete_root_mode_success() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file in shared (accessible via root mode)
        fs::write(file_root.join("shared/root_test.txt"), "content")
            .expect("Failed to create file");

        // User with both file_delete and file_root permissions
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[
                Permission::FileList,
                Permission::FileDelete,
                Permission::FileRoot,
            ],
            false,
        )
        .await;

        handle_file_delete(
            "shared/root_test.txt".to_string(),
            true, // root = true
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete with root mode should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone
        assert!(!file_root.join("shared/root_test.txt").exists());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_delete_symlink() {
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
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Delete the symlink
        handle_file_delete(
            "link.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Symlink delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // Symlink should be gone but target remains
        assert!(!link.exists());
        assert!(target.exists(), "Target file should not be deleted");
    }

    #[tokio::test]
    async fn test_delete_in_user_personal_area() {
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
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Delete from personal area (user sees it as root)
        handle_file_delete(
            "myfile.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(
                    success,
                    "Delete from personal area should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone
        assert!(!file_root.join("users/testuser/myfile.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_empty_path() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Try to delete with empty path (should be treated as root, which is protected)
        handle_file_delete(
            "".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_delete_symlink_to_directory() {
        use std::os::unix::fs::symlink;

        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a target directory with contents and a symlink to it
        let target_dir = file_root.join("shared/target_dir");
        let link_dir = file_root.join("shared/link_dir");
        fs::create_dir(&target_dir).expect("Failed to create target dir");
        fs::write(target_dir.join("file.txt"), "content").expect("Failed to create file");
        symlink(&target_dir, &link_dir).expect("Failed to create symlink");

        assert!(target_dir.exists());
        assert!(link_dir.exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Delete the symlink to directory
        handle_file_delete(
            "link_dir".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Symlink to dir delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // Symlink should be gone but target directory and its contents remain
        assert!(!link_dir.exists());
        assert!(
            target_dir.exists(),
            "Target directory should not be deleted"
        );
        assert!(
            target_dir.join("file.txt").exists(),
            "File in target should not be deleted"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_delete_file_through_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a target directory with a file, and a symlink to the directory
        let target_dir = file_root.join("shared/real_docs");
        let link_dir = file_root.join("shared/docs");
        fs::create_dir(&target_dir).expect("Failed to create target dir");
        fs::write(target_dir.join("readme.txt"), "content").expect("Failed to create file");
        symlink(&target_dir, &link_dir).expect("Failed to create symlink");

        assert!(target_dir.join("readme.txt").exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        // Delete file through the symlinked parent directory
        handle_file_delete(
            "docs/readme.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(
                    success,
                    "Delete through symlinked parent should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // The actual file should be deleted
        assert!(!target_dir.join("readme.txt").exists());
        // But the directories remain
        assert!(target_dir.exists());
        assert!(link_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_unicode_filename() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a file with unicode characters in the name
        let unicode_filename = "文档_ドキュメント_документ.txt";
        fs::write(
            file_root.join("shared").join(unicode_filename),
            "unicode content",
        )
        .expect("Failed to create unicode file");
        assert!(file_root.join("shared").join(unicode_filename).exists());

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            unicode_filename.to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(
                    success,
                    "Unicode filename delete should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone
        assert!(!file_root.join("shared").join(unicode_filename).exists());
    }
}
