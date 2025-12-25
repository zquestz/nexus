//! FileCreateDir message handler - Creates a new directory in the file area

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, DirNameError, FilePathError};

use super::{
    HandlerContext, err_dir_already_exists, err_dir_create_failed, err_dir_name_empty,
    err_dir_name_invalid, err_dir_name_too_long, err_file_not_directory, err_file_not_found,
    err_file_path_invalid, err_file_path_too_long, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;
use crate::files::path::PathError;
use crate::files::{
    allows_upload, build_and_validate_candidate_path, resolve_new_path, resolve_path,
    resolve_user_area,
};

/// Handle a file create directory request
pub async fn handle_file_create_dir<W>(
    path: String,
    name: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("FileCreateDir request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileCreateDir"))
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
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check file root (cheap check, should always be set in production)
    let Some(file_root) = ctx.file_root else {
        // File area not configured
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    };

    // Check FileRoot permission if root browsing requested
    if root && !requesting_user.has_permission(Permission::FileRoot) {
        eprintln!(
            "FileCreateDir (root) from {} (user: {}) without file_root permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate parent path
    if let Err(e) = validators::validate_file_path(&path) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_file_path_invalid(ctx.locale),
        };
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(error_msg),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate directory name
    if let Err(e) = validators::validate_dir_name(&name) {
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
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(error_msg),
            path: None,
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
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build and validate candidate path for the parent directory
    let parent_candidate = match build_and_validate_candidate_path(&area_root, &path) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Resolve the parent directory (must exist)
    let parent_resolved = match resolve_path(&area_root, &parent_candidate) {
        Ok(p) => p,
        Err(PathError::NotFound) => {
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(_) => {
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Verify parent is a directory
    if !parent_resolved.is_dir() {
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_file_not_directory(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Build the full path for the new directory
    let new_dir_candidate = parent_resolved.join(&name);

    // Check if the new directory already exists
    if new_dir_candidate.exists() {
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_dir_already_exists(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check permission: either file_create_dir OR parent allows upload
    let has_create_permission = requesting_user.has_permission(Permission::FileCreateDir);
    let parent_allows_upload = allows_upload(&area_root, &parent_resolved);

    if !has_create_permission && !parent_allows_upload {
        eprintln!(
            "FileCreateDir from {} (user: {}) without permission (path: {})",
            ctx.peer_addr, requesting_user.username, path
        );
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate the new path using resolve_new_path (ensures parent is valid)
    let new_dir_path = match resolve_new_path(&area_root, &new_dir_candidate) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Create the directory
    if let Err(e) = std::fs::create_dir(&new_dir_path) {
        eprintln!(
            "FileCreateDir failed for {} (user: {}): {}",
            ctx.peer_addr, requesting_user.username, e
        );
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_dir_create_failed(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Build the response path (relative to user's view)
    // Normalize backslashes to forward slashes, collapse multiple slashes, and remove "." components
    let normalized_path = path
        .replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/");
    let response_path = if normalized_path.is_empty() {
        name.clone()
    } else {
        format!("{}/{}", normalized_path, name)
    };

    let response = ServerMessage::FileCreateDirResponse {
        success: true,
        error: None,
        path: Some(response_path),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
    };

    fn setup_file_area() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create shared and users directories
        // Note: Don't create users/testuser - we want testuser to fall back to shared/
        fs::create_dir_all(temp_dir.path().join("shared")).expect("Failed to create shared");
        fs::create_dir_all(temp_dir.path().join("users")).expect("Failed to create users dir");

        // Create an upload folder
        fs::create_dir_all(temp_dir.path().join("shared/Uploads [NEXUS-UL]"))
            .expect("Failed to create upload folder");

        // Create a dropbox folder
        fs::create_dir_all(temp_dir.path().join("shared/Submissions [NEXUS-DB]"))
            .expect("Failed to create dropbox folder");

        temp_dir
    }

    #[tokio::test]
    async fn test_create_dir_requires_auth() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_create_dir(
            String::new(),
            "NewFolder".to_string(),
            false,
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err()); // Should disconnect
    }

    #[tokio::test]
    async fn test_create_dir_without_permission_in_regular_folder() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // User without file_create_dir permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(), // Root of user's area (shared/)
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_in_upload_folder_without_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // User without file_create_dir permission but can create in upload folders
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_create_dir(
            "Uploads [NEXUS-UL]".to_string(),
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Should succeed in upload folder: {:?}", error);
                assert!(error.is_none());
                assert_eq!(path, Some("Uploads [NEXUS-UL]/NewFolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(
            file_root
                .join("shared/Uploads [NEXUS-UL]/NewFolder")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_dir_in_dropbox_without_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // User without file_create_dir permission but can create in dropbox
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_create_dir(
            "Submissions [NEXUS-DB]".to_string(),
            "MySubmission".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Should succeed in dropbox: {:?}", error);
                assert!(error.is_none());
                assert_eq!(
                    path,
                    Some("Submissions [NEXUS-DB]/MySubmission".to_string())
                );
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(
            file_root
                .join("shared/Submissions [NEXUS-DB]/MySubmission")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_dir_with_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // User with file_create_dir permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(), // Root of user's area
            "MyNewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Should succeed with permission: {:?}", error);
                assert!(error.is_none());
                assert_eq!(path, Some("MyNewFolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(file_root.join("shared/MyNewFolder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_already_exists() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create the directory first
        fs::create_dir(file_root.join("shared/ExistingFolder")).expect("Failed to create dir");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            "ExistingFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_dir_already_exists(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_empty_name() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            String::new(), // Empty name
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_dir_name_empty(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_invalid_name_with_slash() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            "invalid/name".to_string(), // Contains path separator
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_dir_name_invalid(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_parent_traversal() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            "..".to_string(), // Parent directory reference
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_parent_not_found() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            "NonExistentFolder".to_string(), // Parent doesn't exist
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_file_not_found(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_root_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // User without file_root permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            "NewFolder".to_string(),
            true, // Root browsing
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_admin_anywhere() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Admin user (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        handle_file_create_dir(
            String::new(),
            "AdminFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Admin should succeed: {:?}", error);
                assert!(error.is_none());
                assert_eq!(path, Some("AdminFolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_unicode_name() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            "日本語フォルダ".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Unicode name should succeed: {:?}", error);
                assert!(error.is_none());
                assert_eq!(path, Some("日本語フォルダ".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(file_root.join("shared/日本語フォルダ").exists());
    }

    #[tokio::test]
    async fn test_create_dir_backslash_path() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a subdirectory first
        fs::create_dir(file_root.join("shared/Subdir")).expect("Failed to create subdir");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Use backslash in path (Windows-style)
        handle_file_create_dir(
            "\\Subdir".to_string(),
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Backslash path should succeed: {:?}", error);
                assert!(error.is_none());
                // Response path should be normalized (no leading slash)
                assert_eq!(path, Some("Subdir/NewFolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(file_root.join("shared/Subdir/NewFolder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_with_spaces_in_name() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        handle_file_create_dir(
            String::new(),
            "My New Folder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Name with spaces should succeed: {:?}", error);
                assert!(error.is_none());
                assert_eq!(path, Some("My New Folder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(file_root.join("shared/My New Folder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_dot_name() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Try to create a directory named "." - should fail as it already exists
        handle_file_create_dir(
            String::new(),
            ".".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                // "." resolves to the parent directory, which exists
                assert_eq!(error, Some(err_dir_already_exists(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_upload_folder() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Create a folder with upload suffix
        handle_file_create_dir(
            String::new(),
            "User Uploads [NEXUS-UL]".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(
                    success,
                    "Creating upload folder should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
                assert_eq!(path, Some("User Uploads [NEXUS-UL]".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was created
        assert!(file_root.join("shared/User Uploads [NEXUS-UL]").exists());

        // Now verify that a user WITHOUT file_create_dir can create inside it
        let session_id2 = login_user(
            &mut test_ctx,
            "testuser2",
            "password",
            &[Permission::FileList], // No file_create_dir permission
            false,
        )
        .await;

        handle_file_create_dir(
            "User Uploads [NEXUS-UL]".to_string(),
            "Subfolder".to_string(),
            false,
            Some(session_id2),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response2 = read_server_message(&mut test_ctx.client).await;
        match response2 {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(
                    success,
                    "Creating in user-created upload folder should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
                assert_eq!(path, Some("User Uploads [NEXUS-UL]/Subfolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        assert!(
            file_root
                .join("shared/User Uploads [NEXUS-UL]/Subfolder")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_dir_multiple_slashes_in_path() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a subdirectory first
        fs::create_dir(file_root.join("shared/Subdir")).expect("Failed to create subdir");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Use multiple slashes in path
        handle_file_create_dir(
            "///Subdir//".to_string(),
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(success, "Multiple slashes path should succeed: {:?}", error);
                assert!(error.is_none());
                // Response path should be normalized (collapsed slashes)
                assert_eq!(path, Some("Subdir/NewFolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(file_root.join("shared/Subdir/NewFolder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_windows_drive_letter_rejected() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Try to use Windows drive letter in path (potential path traversal)
        handle_file_create_dir(
            "C:\\Windows".to_string(),
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_file_path_invalid(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_windows_drive_letter_with_leading_slash_rejected() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Try to bypass with leading slash before drive letter
        // This was a discovered bypass: "/C:/Windows" passes naive check at position 0
        // but becomes "C:/Windows" after trim_start_matches
        handle_file_create_dir(
            "/C:/Windows".to_string(),
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse { success, error, .. } => {
                assert!(
                    !success,
                    "Path with leading slash before drive letter should be rejected"
                );
                assert_eq!(error, Some(err_file_path_invalid(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }
    }

    #[tokio::test]
    async fn test_create_dir_dot_components_normalized() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        let file_root = Box::leak(file_area.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create a subdirectory first
        fs::create_dir(file_root.join("shared/Subdir")).expect("Failed to create subdir");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileCreateDir],
            false,
        )
        .await;

        // Use path with "." components
        handle_file_create_dir(
            "/./Subdir/.".to_string(),
            "NewFolder".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => {
                assert!(
                    success,
                    "Path with dot components should succeed: {:?}",
                    error
                );
                assert!(error.is_none());
                // Response path should have "." components removed
                assert_eq!(path, Some("Subdir/NewFolder".to_string()));
            }
            _ => panic!("Expected FileCreateDirResponse"),
        }

        // Verify directory was actually created
        assert!(file_root.join("shared/Subdir/NewFolder").exists());
    }
}
