//! Creates a new directory in the file area.

use std::io;

use tokio::io::AsyncWrite;
use tracing::{debug, error, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, DirNameError, FilePathError};

use super::{
    HandlerContext, err_destination_busy, err_dir_already_exists, err_dir_create_failed,
    err_dir_name_empty, err_dir_name_invalid, err_dir_name_too_long, err_file_area_not_accessible,
    err_file_area_not_configured, err_file_not_directory, err_file_not_found,
    err_file_path_invalid, err_file_path_too_long, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    HANDLER_FILE_CREATE_DIR, LOG_FILE_CREATE_DIR_FAILED, LOG_FILE_CREATE_DIR_NOT_LOGGED_IN,
    LOG_FILE_CREATE_DIR_PERMISSION_DENIED, LOG_FILE_CREATE_DIR_ROOT_DENIED,
    LOG_FILE_CREATE_DIR_SUCCESS,
};
use crate::db::Permission;
use crate::files::path::PathError;
use crate::files::{
    PathLockMode, allows_upload, build_and_validate_candidate_path, lock_key,
    normalize_client_path, personal_area_names_for_root_child, resolve_new_path, resolve_path,
    resolve_user_area_with_read_lock,
};

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
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_CREATE_DIR_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(
                &err_not_logged_in(ctx.locale),
                Some(HANDLER_FILE_CREATE_DIR),
            )
            .await;
    };

    let Some(file_root) = ctx.file_root else {
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_file_area_not_configured(ctx.locale)),
            path: None,
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
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(error_msg),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

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

    let user_area_result = 'user_area: {
        let _state_guard = ctx.user_manager.read_user_state().await;
        let requesting_user = match ctx
            .user_manager
            .get_user_by_session_id(requesting_session_id)
            .await
        {
            Some(u) => u,
            None => {
                // Race, not a security event — don't log.
                let response = ServerMessage::FileCreateDirResponse {
                    success: false,
                    error: Some(err_not_logged_in(ctx.locale)),
                    path: None,
                };
                break 'user_area Err(response);
            }
        };

        if root && !requesting_user.has_permission(Permission::FileRoot) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_CREATE_DIR_ROOT_DENIED);
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                path: None,
            };
            break 'user_area Err(response);
        }

        let (area_root_path, personal_area_guards) = if root {
            (
                file_root.to_path_buf(),
                ctx.personal_area_locks
                    .read_many(personal_area_names_for_root_child(&path, &name))
                    .await,
            )
        } else {
            resolve_user_area_with_read_lock(
                file_root,
                ctx.personal_area_locks.as_ref(),
                &requesting_user.username,
            )
            .await
        };
        Ok((requesting_user, area_root_path, personal_area_guards))
    };
    let (requesting_user, area_root_path, _personal_area_guards) = match user_area_result {
        Ok(user_area) => user_area,
        Err(response) => return ctx.send_message(&response).await,
    };

    let area_root = match tokio::fs::canonicalize(&area_root_path).await {
        Ok(p) => p,
        Err(_) => {
            // Root mode: admin's file_root is broken. Non-root: the user's
            // personal area dir doesn't exist yet, so the parent can't either.
            let error_msg = if root {
                err_file_area_not_accessible(ctx.locale)
            } else {
                err_file_not_found(ctx.locale)
            };
            let response = ServerMessage::FileCreateDirResponse {
                success: false,
                error: Some(error_msg),
                path: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let parent_candidate = match build_and_validate_candidate_path(&area_root, &path).await {
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

    let parent_resolved = match resolve_path(&area_root, &parent_candidate).await {
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

    let parent_is_dir = tokio::fs::metadata(&parent_resolved)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if !parent_is_dir {
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_file_not_directory(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    let new_dir_candidate = parent_resolved.join(&name);

    // Fast path, and also catches name=`.` (which `validate_dir_name` accepts
    // but `resolve_new_path` rejects). Post-lock `create_dir` still wins the race.
    if tokio::fs::try_exists(&new_dir_candidate)
        .await
        .unwrap_or(false)
    {
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_dir_already_exists(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    // Either `file_create_dir` OR an upload-allowed parent (e.g. `[NEXUS-UL]`).
    let has_create_permission = requesting_user.has_permission(Permission::FileCreateDir);
    let parent_allows_upload = allows_upload(&area_root, &parent_resolved);

    if !has_create_permission && !parent_allows_upload {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_CREATE_DIR_PERMISSION_DENIED);
        let response = ServerMessage::FileCreateDirResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            path: None,
        };
        return ctx.send_message(&response).await;
    }

    let new_dir_path = match resolve_new_path(&area_root, &new_dir_candidate).await {
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

    // Lock for serialization parity; `create_dir` is itself atomic-fails-if-exists.
    // Guards live only inside this block; all socket sends happen after it.
    let response = 'locked: {
        let target_key = match lock_key(&new_dir_path).await {
            Ok(k) => k,
            Err(_) => {
                break 'locked ServerMessage::FileCreateDirResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
                    path: None,
                };
            }
        };
        let _lock_guard = match ctx
            .file_mutation_locks
            .acquire(target_key, PathLockMode::Wait)
            .await
        {
            Ok(g) => g,
            Err(_) => {
                // Destination path held by a `Fail`-mode lock.
                break 'locked ServerMessage::FileCreateDirResponse {
                    success: false,
                    error: Some(err_destination_busy(ctx.locale)),
                    path: None,
                };
            }
        };

        if let Err(e) = tokio::fs::create_dir(&new_dir_path).await {
            // Race-tight: `create_dir` is atomic-fails-if-exists under the lock.
            break 'locked if e.kind() == std::io::ErrorKind::AlreadyExists {
                ServerMessage::FileCreateDirResponse {
                    success: false,
                    error: Some(err_dir_already_exists(ctx.locale)),
                    path: None,
                }
            } else {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_CREATE_DIR_FAILED);
                ServerMessage::FileCreateDirResponse {
                    success: false,
                    error: Some(err_dir_create_failed(ctx.locale)),
                    path: None,
                }
            };
        }

        let normalized_path = normalize_client_path(&path);
        let response_path = if normalized_path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", normalized_path, name)
        };
        debug!(user = %requesting_user.username, ip = %ctx.peer_addr, path = %response_path, "{}", LOG_FILE_CREATE_DIR_SUCCESS);
        ServerMessage::FileCreateDirResponse {
            success: true,
            error: None,
            path: Some(response_path),
        }
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
        setup_file_area_full,
    };

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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(
            file_area
                .path()
                .join("shared/Uploads [NEXUS-UL]/NewFolder")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_dir_in_dropbox_without_permission() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(
            file_area
                .path()
                .join("shared/Submissions [NEXUS-DB]/MySubmission")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_dir_with_permission() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(file_area.path().join("shared/MyNewFolder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_already_exists() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/ExistingFolder"))
            .expect("Failed to create dir");

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(file_area.path().join("shared/日本語フォルダ").exists());
    }

    #[tokio::test]
    async fn test_create_dir_backslash_path() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/Subdir")).expect("Failed to create subdir");

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(file_area.path().join("shared/Subdir/NewFolder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_with_spaces_in_name() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(file_area.path().join("shared/My New Folder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_dot_name() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(
            file_area
                .path()
                .join("shared/User Uploads [NEXUS-UL]")
                .exists()
        );

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

        let response2 = read_server_message(&mut test_ctx).await;
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
            file_area
                .path()
                .join("shared/User Uploads [NEXUS-UL]/Subfolder")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_create_dir_multiple_slashes_in_path() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/Subdir")).expect("Failed to create subdir");

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(file_area.path().join("shared/Subdir/NewFolder").exists());
    }

    #[tokio::test]
    async fn test_create_dir_windows_drive_letter_rejected() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/Subdir")).expect("Failed to create subdir");

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(file_area.path().join("shared/Subdir/NewFolder").exists());
    }
}
