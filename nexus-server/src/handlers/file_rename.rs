//! FileRename message handler - Renames a file or directory in the file area

use std::io;

use tokio::io::AsyncWrite;
use tracing::{debug, error, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, DirNameError, FilePathError};

use super::{
    HandlerContext, err_destination_busy, err_dir_name_empty, err_dir_name_invalid,
    err_dir_name_too_long, err_file_area_not_accessible, err_file_area_not_configured,
    err_file_not_found, err_file_path_invalid, err_file_path_too_long, err_not_logged_in,
    err_permission_denied, err_rename_failed, err_rename_target_exists, err_source_busy,
};
use crate::constants::{
    HANDLER_FILE_RENAME, LOG_FILE_RENAME_DESTINATION_BUSY, LOG_FILE_RENAME_FAILED,
    LOG_FILE_RENAME_NOT_LOGGED_IN, LOG_FILE_RENAME_PERMISSION_DENIED, LOG_FILE_RENAME_ROOT_DENIED,
    LOG_FILE_RENAME_SOURCE_BUSY, LOG_FILE_RENAME_SUCCESS,
};
use crate::db::Permission;
use crate::files::{
    PathLockMode, build_and_validate_candidate_path, in_owned_dropbox, lock_key, rename_path_async,
    resolve_path, resolve_user_area,
};

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
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_RENAME_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_FILE_RENAME))
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
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    let Some(file_root) = ctx.file_root else {
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_file_area_not_configured(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    };

    if root && !requesting_user.has_permission(Permission::FileRoot) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_RENAME_ROOT_DENIED);
        let response = ServerMessage::FileRenameResponse {
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
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // New name uses dir-name rules (no path separators, no .., etc.).
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

    let area_root_path = if root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
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
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(error_msg),
            };
            return ctx.send_message(&response).await;
        }
    };

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

    // Owner-of-`[NEXUS-DB-username]` bypass: rename is safe because the
    // destination is always `source.parent().join(new_name)`, so the entry
    // can't escape the drop box. Uses virtual `candidate` so symlinked drop
    // boxes still match. Non-root-mode only.
    let bypass_via_ownership =
        !root && in_owned_dropbox(&candidate, &area_root, &requesting_user.username);
    if !requesting_user.has_permission(Permission::FileRename) && !bypass_via_ownership {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_RENAME_PERMISSION_DENIED);
        let response = ServerMessage::FileRenameResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // See `files::path_lock`. Derive lock keys from `candidate` + `new_name`
    // (pure path arithmetic) so source validation can run AFTER acquisition
    // under the lock — closing the resolve-then-mutate race.
    let candidate_parent = match candidate.parent() {
        Some(p) => p,
        None => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };
    let target_path_for_lock = candidate_parent.join(&new_name);
    let source_lock_key = match lock_key(&candidate) {
        Ok(k) => k,
        Err(_) => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };
    let target_lock_key = match lock_key(&target_path_for_lock) {
        Ok(k) => k,
        Err(_) => {
            let response = ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };
    // Clone before moving into `acquire_many` so we can identify which side
    // (source vs target) the busy lock was on, and report a meaningful error.
    let busy_source_key = source_lock_key.clone();

    // Guards live only inside this block; all socket sends happen after it.
    let response = 'locked: {
        let _lock_guards = match ctx
            .file_mutation_locks
            .acquire_many(vec![source_lock_key, target_lock_key], PathLockMode::Wait)
            .await
        {
            Ok(g) => g,
            Err(e) if e.key() == busy_source_key.as_path() => {
                // Source path held by a `Fail`-mode lock.
                debug!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_RENAME_SOURCE_BUSY);
                break 'locked ServerMessage::FileRenameResponse {
                    success: false,
                    error: Some(err_source_busy(ctx.locale)),
                };
            }
            Err(_) => {
                // Destination path held by a `Fail`-mode lock.
                debug!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_RENAME_DESTINATION_BUSY);
                break 'locked ServerMessage::FileRenameResponse {
                    success: false,
                    error: Some(err_destination_busy(ctx.locale)),
                };
            }
        };

        // Under lock: resolve source from the authoritative filesystem state.
        let symlink_meta = tokio::fs::symlink_metadata(&candidate).await;
        let source_path = match &symlink_meta {
            Ok(meta) if meta.file_type().is_symlink() => {
                // Rename the symlink, not its target.
                candidate.clone()
            }
            Ok(_) => match resolve_path(&area_root, &candidate).await {
                Ok(p) => p,
                Err(_) => {
                    break 'locked ServerMessage::FileRenameResponse {
                        success: false,
                        error: Some(err_file_not_found(ctx.locale)),
                    };
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                break 'locked ServerMessage::FileRenameResponse {
                    success: false,
                    error: Some(err_file_not_found(ctx.locale)),
                };
            }
            Err(_) => {
                break 'locked ServerMessage::FileRenameResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
                };
            }
        };

        if source_path == area_root || candidate == area_root {
            break 'locked ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
            };
        }

        let parent_dir = match source_path.parent() {
            Some(p) => p,
            None => {
                break 'locked ServerMessage::FileRenameResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
                };
            }
        };
        let target_path = parent_dir.join(&new_name);

        if target_path.exists() || target_path.symlink_metadata().is_ok() {
            break 'locked ServerMessage::FileRenameResponse {
                success: false,
                error: Some(err_rename_target_exists(ctx.locale)),
            };
        }

        match rename_path_async(&source_path, &target_path).await {
            Ok(()) => {
                ctx.file_index.mark_dirty();
                debug!(user = %requesting_user.username, ip = %ctx.peer_addr, path = %path, "{}", LOG_FILE_RENAME_SUCCESS);
                ServerMessage::FileRenameResponse {
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_RENAME_FAILED);
                ServerMessage::FileRenameResponse {
                    success: false,
                    error: Some(err_rename_failed(ctx.locale)),
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_file_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/original.txt"), "content")
            .expect("Failed to create file");
        assert!(file_area.path().join("shared/original.txt").exists());

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(!file_area.path().join("shared/original.txt").exists());
        assert!(file_area.path().join("shared/renamed.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_directory_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/original_dir")).expect("Failed to create dir");

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(!file_area.path().join("shared/original_dir").exists());
        assert!(file_area.path().join("shared/renamed_dir").exists());
    }

    #[tokio::test]
    async fn test_rename_target_exists() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/file1.txt"), "content1").unwrap();
        fs::write(file_area.path().join("shared/file2.txt"), "content2").unwrap();

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_new_name_with_path_separator_blocked() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/test.txt"), "content").unwrap();

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_new_name_with_parent_ref_blocked() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/test.txt"), "content").unwrap();

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
    }

    #[tokio::test]
    async fn test_rename_empty_new_name() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/test.txt"), "content").unwrap();

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, .. } => {
                assert!(!success);
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_unicode_filename() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/文件.txt"), "content").unwrap();

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(!file_area.path().join("shared/文件.txt").exists());
        assert!(file_area.path().join("shared/新文件.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_root_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/test.txt"), "content")
            .expect("Failed to create file");

        // User with file_rename but not file_root
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "shared/test.txt".to_string(),
            "renamed.txt".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(file_area.path().join("shared/test.txt").exists());
    }

    #[tokio::test]
    async fn test_rename_root_mode_success() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::write(file_area.path().join("shared/root_test.txt"), "content")
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
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success, "Rename with root mode should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(!file_area.path().join("shared/root_test.txt").exists());
        assert!(file_area.path().join("shared/renamed_root.txt").exists());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_rename_symlink() {
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
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        handle_file_rename(
            "link.txt".to_string(),
            "renamed_link.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success, "Symlink rename should succeed: {:?}", error);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileRenameResponse"),
        }

        assert!(!link.exists());
        assert!(file_area.path().join("shared/renamed_link.txt").exists());
        // We renamed the link, not the target.
        assert!(target.exists(), "Target file should not be affected");
    }

    #[tokio::test]
    async fn test_rename_in_user_personal_area() {
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
            &[Permission::FileList, Permission::FileRename],
            false,
        )
        .await;

        // User sees personal area as root.
        handle_file_rename(
            "myfile.txt".to_string(),
            "renamed_personal.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
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

        assert!(!file_area.path().join("users/testuser/myfile.txt").exists());
        assert!(
            file_area
                .path()
                .join("users/testuser/renamed_personal.txt")
                .exists()
        );
    }

    /// Owner of a `[NEXUS-DB-username]` folder can rename files inside it
    /// without the global `file_rename` permission. Cleanup workflow.
    #[tokio::test]
    async fn test_rename_owner_can_rename_inside_user_dropbox_without_permission() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("IMG_2391.jpg"), "data").expect("Failed to create file");

        // Alice has FileList only — no FileRename.
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_rename(
            "For Alice [NEXUS-DB-alice]/IMG_2391.jpg".to_string(),
            "beach-trip.jpg".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(
                    success,
                    "Owner should be able to rename inside own dropbox without file_rename: {:?}",
                    error
                );
            }
            _ => panic!("Expected FileRenameResponse"),
        }
        assert!(!dropbox.join("IMG_2391.jpg").exists());
        assert!(dropbox.join("beach-trip.jpg").exists());
    }

    /// The owner bypass works for nested files too.
    #[tokio::test]
    async fn test_rename_owner_can_rename_nested_inside_user_dropbox() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir_all(dropbox.join("sub")).expect("Failed to create nested dirs");
        fs::write(dropbox.join("sub/old.txt"), "data").expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_rename(
            "For Alice [NEXUS-DB-alice]/sub/old.txt".to_string(),
            "new.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(success, "Owner nested rename should succeed: {:?}", error);
            }
            _ => panic!("Expected FileRenameResponse"),
        }
        assert!(!dropbox.join("sub/old.txt").exists());
        assert!(dropbox.join("sub/new.txt").exists());
    }

    /// The owner bypass does NOT extend to renaming the dropbox folder
    /// itself — only its contents. The folder is admin-managed.
    #[tokio::test]
    async fn test_rename_owner_cannot_rename_user_dropbox_folder_itself() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

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

        handle_file_rename(
            "For Alice [NEXUS-DB-alice]".to_string(),
            "Just Alice".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success, "Owner must not rename dropbox folder itself");
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }
        assert!(dropbox.exists(), "Dropbox folder should remain unchanged");
        assert!(
            !file_area.path().join("shared/Just Alice").exists(),
            "Renamed-to path should not exist"
        );
    }

    /// A non-owner cannot use the bypass to rename files in someone
    /// else's `[NEXUS-DB-username]` folder.
    #[tokio::test]
    async fn test_rename_non_owner_cannot_use_dropbox_bypass() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("private.txt"), "alice's").expect("Failed to create file");

        // Bob — not the owner.
        let session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_rename(
            "For Alice [NEXUS-DB-alice]/private.txt".to_string(),
            "stolen.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success, "Non-owner must not bypass file_rename");
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }
        assert!(dropbox.join("private.txt").exists());
    }

    /// Without `file_rename`, the owner bypass is scoped to inside their
    /// dropbox — it doesn't grant rename of arbitrary files elsewhere.
    #[tokio::test]
    async fn test_rename_owner_bypass_does_not_apply_outside_dropbox() {
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

        handle_file_rename(
            "elsewhere.txt".to_string(),
            "renamed.txt".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileRenameResponse { success, error } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileRenameResponse"),
        }
        assert!(file_area.path().join("shared/elsewhere.txt").exists());
    }
}
