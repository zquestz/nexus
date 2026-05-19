//! FileDelete message handler - Deletes a file or empty directory in the file area

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_delete_failed, err_dir_not_empty, err_file_area_not_accessible,
    err_file_area_not_configured, err_file_not_found, err_file_path_invalid,
    err_file_path_too_long, err_not_logged_in, err_permission_denied, err_source_busy,
};
use crate::constants::{
    HANDLER_FILE_DELETE, LOG_FILE_DELETE_FAILED, LOG_FILE_DELETE_NOT_LOGGED_IN,
    LOG_FILE_DELETE_PERMISSION_DENIED, LOG_FILE_DELETE_ROOT_DENIED, LOG_FILE_DELETE_SUCCESS,
};
use crate::db::Permission;
use crate::files::path::PathError;
use crate::files::{
    PathLockMode, build_and_validate_candidate_path, in_owned_dropbox, lock_key, resolve_path,
    resolve_user_area,
};

pub async fn handle_file_delete<W>(
    path: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_FILE_DELETE))
            .await;
    };

    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            // Race, not a security event — don't log.
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    let Some(file_root) = ctx.file_root else {
        let response = ServerMessage::FileDeleteResponse {
            success: false,
            error: Some(err_file_area_not_configured(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    };

    if root && !requesting_user.has_permission(Permission::FileRoot) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_DELETE_ROOT_DENIED);
        let response = ServerMessage::FileDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

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

    let area_root_path = if root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
    };

    let area_root = match area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Root mode: admin's file_root is broken. Non-root: the user's
            // personal area dir doesn't exist yet, so the file can't either.
            let error_msg = if root {
                err_file_area_not_accessible(ctx.locale)
            } else {
                err_file_not_found(ctx.locale)
            };
            let response = ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(error_msg),
            };
            return ctx.send_message(&response).await;
        }
    };

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

    // Owner-of-`[NEXUS-DB-username]` bypass. Uses virtual `candidate` so
    // symlinked drop boxes still match. Non-root-mode only.
    let bypass_via_ownership =
        !root && in_owned_dropbox(&candidate, &area_root, &requesting_user.username);
    if !requesting_user.has_permission(Permission::FileDelete) && !bypass_via_ownership {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_DELETE_PERMISSION_DENIED);
        let response = ServerMessage::FileDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // See `files::path_lock`. Acquire before any metadata/resolve/type check
    // so a concurrent locked rename/move/copy can't replace the target between
    // inspection and removal. An upload still claiming this path reports as
    // upload-in-progress — the file isn't fully on disk yet.
    //
    // Guards live only inside this block; all socket sends happen after it.
    let response = 'locked: {
        let target_key = match lock_key(&candidate) {
            Ok(k) => k,
            Err(_) => {
                break 'locked ServerMessage::FileDeleteResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
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
                // Path held by a `Fail`-mode lock.
                break 'locked ServerMessage::FileDeleteResponse {
                    success: false,
                    error: Some(err_source_busy(ctx.locale)),
                };
            }
        };

        // `symlink_metadata` so we don't follow the final symlink component.
        let symlink_meta = std::fs::symlink_metadata(&candidate);
        let (path_to_delete, is_dir) = match &symlink_meta {
            Ok(meta) if meta.file_type().is_symlink() => {
                // Delete the symlink, not its target.
                (candidate.clone(), false)
            }
            Ok(_) => {
                let resolved = match resolve_path(&area_root, &candidate) {
                    Ok(p) => p,
                    Err(PathError::NotFound) => {
                        break 'locked ServerMessage::FileDeleteResponse {
                            success: false,
                            error: Some(err_file_not_found(ctx.locale)),
                        };
                    }
                    Err(_) => {
                        break 'locked ServerMessage::FileDeleteResponse {
                            success: false,
                            error: Some(err_file_path_invalid(ctx.locale)),
                        };
                    }
                };

                if resolved == area_root {
                    break 'locked ServerMessage::FileDeleteResponse {
                        success: false,
                        error: Some(err_permission_denied(ctx.locale)),
                    };
                }

                let is_dir = resolved.is_dir();
                (resolved, is_dir)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                break 'locked ServerMessage::FileDeleteResponse {
                    success: false,
                    error: Some(err_file_not_found(ctx.locale)),
                };
            }
            Err(_) => {
                break 'locked ServerMessage::FileDeleteResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
                };
            }
        };

        // Symlink branch also checks the virtual candidate against area_root.
        if candidate == area_root {
            break 'locked ServerMessage::FileDeleteResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
            };
        }

        let result = if is_dir {
            std::fs::remove_dir(&path_to_delete)
        } else {
            std::fs::remove_file(&path_to_delete)
        };

        match result {
            Ok(()) => {
                ctx.file_index.mark_dirty();
                info!(user = %requesting_user.username, ip = %ctx.peer_addr, path = %path, "{}", LOG_FILE_DELETE_SUCCESS);
                ServerMessage::FileDeleteResponse {
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                let error_msg = if is_dir && e.kind() == std::io::ErrorKind::DirectoryNotEmpty {
                    err_dir_not_empty(ctx.locale)
                } else {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_DELETE_FAILED);
                    err_delete_failed(ctx.locale)
                };
                ServerMessage::FileDeleteResponse {
                    success: false,
                    error: Some(error_msg),
                }
            }
        }
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
        setup_file_area_basic,
    };

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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_file_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/test.txt"), "content")
            .expect("Failed to create file");
        assert!(file_area.path().join("shared/test.txt").exists());

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(!file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_empty_directory_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/empty_dir")).expect("Failed to create dir");
        assert!(file_area.path().join("shared/empty_dir").exists());

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(!file_area.path().join("shared/empty_dir").exists());
    }

    #[tokio::test]
    async fn test_delete_non_empty_directory_fails() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/non_empty")).expect("Failed to create dir");
        fs::write(
            file_area.path().join("shared/non_empty/file.txt"),
            "content",
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

        handle_file_delete(
            "non_empty".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_dir_not_empty(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(file_area.path().join("shared/non_empty").exists());
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "/".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/test.txt"), "content")
            .expect("Failed to create file");

        // User with file_delete but not file_root
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileDelete],
            false,
        )
        .await;

        handle_file_delete(
            "shared/test.txt".to_string(),
            true, // root = true
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_admin_can_delete() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/admin_test.txt"), "content")
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Admin delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(!file_area.path().join("shared/admin_test.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_nested_file() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir_all(file_area.path().join("shared/docs/archive"))
            .expect("Failed to create dirs");
        fs::write(
            file_area.path().join("shared/docs/archive/old.txt"),
            "content",
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

        handle_file_delete(
            "docs/archive/old.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        // File should be gone but directories remain
        assert!(
            !file_area
                .path()
                .join("shared/docs/archive/old.txt")
                .exists()
        );
        assert!(file_area.path().join("shared/docs/archive").exists());
    }

    #[tokio::test]
    async fn test_delete_root_mode_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create a file in shared (accessible via root mode)
        fs::write(file_area.path().join("shared/root_test.txt"), "content")
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Delete with root mode should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileDeleteResponse"),
        }

        assert!(!file_area.path().join("shared/root_test.txt").exists());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_delete_symlink() {
        use std::os::unix::fs::symlink;

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let target = file_area.path().join("shared/target.txt");
        let link = file_area.path().join("shared/link.txt");
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

        handle_file_delete(
            "link.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir_all(file_area.path().join("users/testuser"))
            .expect("Failed to create user dir");
        fs::write(
            file_area.path().join("users/testuser/myfile.txt"),
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

        handle_file_delete(
            "myfile.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(!file_area.path().join("users/testuser/myfile.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_empty_path() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let target_dir = file_area.path().join("shared/target_dir");
        let link_dir = file_area.path().join("shared/link_dir");
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

        handle_file_delete(
            "link_dir".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
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

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let target_dir = file_area.path().join("shared/real_docs");
        let link_dir = file_area.path().join("shared/docs");
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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let unicode_filename = "文档_ドキュメント_документ.txt";
        fs::write(
            file_area.path().join("shared").join(unicode_filename),
            "unicode content",
        )
        .expect("Failed to create unicode file");
        assert!(
            file_area
                .path()
                .join("shared")
                .join(unicode_filename)
                .exists()
        );

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

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(
            !file_area
                .path()
                .join("shared")
                .join(unicode_filename)
                .exists()
        );
    }

    /// Owner of a `[NEXUS-DB-username]` folder can delete files inside it
    /// without the global `file_delete` permission. Cleaning up your own
    /// drop box shouldn't require admin-level rights.
    #[tokio::test]
    async fn test_delete_owner_can_delete_inside_user_dropbox_without_permission() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Create alice's user dropbox in shared (alice has no personal area,
        // so her area_root is shared/) with a file inside.
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("incoming.txt"), "data").expect("Failed to create file");

        // Alice has FileList only — no FileDelete.
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "For Alice [NEXUS-DB-alice]/incoming.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(
                    success,
                    "Owner should be able to delete inside own dropbox without file_delete: {:?}",
                    error
                );
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
        assert!(!dropbox.join("incoming.txt").exists());
    }

    /// The owner bypass is recursive — files deep inside their dropbox
    /// also delete-able without the global permission.
    #[tokio::test]
    async fn test_delete_owner_can_delete_nested_inside_user_dropbox() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir_all(dropbox.join("sub/deep")).expect("Failed to create nested dirs");
        fs::write(dropbox.join("sub/deep/buried.txt"), "data").expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "For Alice [NEXUS-DB-alice]/sub/deep/buried.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(success, "Owner nested delete should succeed: {:?}", error);
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
        assert!(!dropbox.join("sub/deep/buried.txt").exists());
    }

    /// The owner bypass does NOT extend to deleting the dropbox folder
    /// itself — only its contents. The folder is admin-managed.
    #[tokio::test]
    async fn test_delete_owner_cannot_delete_user_dropbox_folder_itself() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Empty dropbox (so directory removal would otherwise succeed).
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "For Alice [NEXUS-DB-alice]".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success, "Owner must not be able to delete dropbox folder");
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
        assert!(dropbox.exists(), "Dropbox folder should remain");
    }

    /// A non-owner user cannot use the bypass to delete files in someone
    /// else's `[NEXUS-DB-username]` folder.
    #[tokio::test]
    async fn test_delete_non_owner_cannot_use_dropbox_bypass() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("private.txt"), "alice's").expect("Failed to create file");

        // Bob — not the owner — has FileList only.
        let session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "For Alice [NEXUS-DB-alice]/private.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success, "Non-owner must not bypass file_delete");
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
        assert!(dropbox.join("private.txt").exists());
    }

    /// Without `file_delete`, the owner bypass is scoped to inside their
    /// dropbox — it doesn't grant deletion of arbitrary files elsewhere.
    #[tokio::test]
    async fn test_delete_owner_bypass_does_not_apply_outside_dropbox() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Alice owns a dropbox, but the file we target is NOT inside it.
        fs::create_dir(file_area.path().join("shared/For Alice [NEXUS-DB-alice]"))
            .expect("Failed to create dropbox");
        fs::write(file_area.path().join("shared/elsewhere.txt"), "not yours")
            .expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "elsewhere.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
        assert!(file_area.path().join("shared/elsewhere.txt").exists());
    }

    /// The owner bypass works for empty subdirectories too — cleanup
    /// includes removing folders the user created inside their dropbox.
    #[tokio::test]
    async fn test_delete_owner_can_delete_empty_subdir_inside_dropbox() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir_all(dropbox.join("empty_sub")).expect("Failed to create dirs");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_delete(
            "For Alice [NEXUS-DB-alice]/empty_sub".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileDeleteResponse { success, error } => {
                assert!(
                    success,
                    "Owner empty-subdir delete should succeed: {:?}",
                    error
                );
            }
            _ => panic!("Expected FileDeleteResponse"),
        }
        assert!(!dropbox.join("empty_sub").exists());
    }
}
