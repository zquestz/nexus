//! FileCopy message handler - Copies a file or directory in the file area

use std::io;

use tokio::io::AsyncWrite;
use tracing::{debug, error, warn};

use nexus_common::ErrorKind;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_cannot_copy_into_itself, err_copy_failed, err_destination_busy,
    err_destination_exists, err_destination_not_directory, err_file_area_not_accessible,
    err_file_area_not_configured, err_file_not_found, err_file_path_invalid,
    err_file_path_too_long, err_not_logged_in, err_permission_denied, err_source_busy,
};
use crate::constants::{
    HANDLER_FILE_COPY, LOG_FILE_COPY_DELETE_DENIED, LOG_FILE_COPY_DESTINATION_BUSY,
    LOG_FILE_COPY_FAILED, LOG_FILE_COPY_NOT_LOGGED_IN, LOG_FILE_COPY_PERMISSION_DENIED,
    LOG_FILE_COPY_REMOVE_FAILED, LOG_FILE_COPY_ROOT_DENIED, LOG_FILE_COPY_SOURCE_BUSY,
    LOG_FILE_COPY_SUCCESS,
};
use crate::db::Permission;
use crate::files::{
    PathLockMode, build_and_validate_candidate_path, copy_path_recursive_async, is_subpath,
    lock_key, remove_path_async, resolve_path, resolve_user_area,
};

pub async fn handle_file_copy<W>(
    source_path: String,
    destination_dir: String,
    overwrite: bool,
    source_root: bool,
    destination_root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_COPY_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_FILE_COPY))
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
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                error_kind: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let Some(file_root) = ctx.file_root else {
        let response = ServerMessage::FileCopyResponse {
            success: false,
            error: Some(err_file_area_not_configured(ctx.locale)),
            error_kind: Some(ErrorKind::NotFound.into()),
        };
        return ctx.send_message(&response).await;
    };

    if !requesting_user.has_permission(Permission::FileCopy) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_COPY_PERMISSION_DENIED);
        let response = ServerMessage::FileCopyResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(ErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    if (source_root || destination_root) && !requesting_user.has_permission(Permission::FileRoot) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_COPY_ROOT_DENIED);
        let response = ServerMessage::FileCopyResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(ErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Overwrite removes the existing target, so it requires file_delete.
    if overwrite && !requesting_user.has_permission(Permission::FileDelete) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_COPY_DELETE_DENIED);
        let response = ServerMessage::FileCopyResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(ErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_file_path(&source_path) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_file_path_invalid(ctx.locale),
        };
        let response = ServerMessage::FileCopyResponse {
            success: false,
            error: Some(error_msg),
            error_kind: Some(ErrorKind::InvalidPath.into()),
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_file_path(&destination_dir) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_file_path_invalid(ctx.locale),
        };
        let response = ServerMessage::FileCopyResponse {
            success: false,
            error: Some(error_msg),
            error_kind: Some(ErrorKind::InvalidPath.into()),
        };
        return ctx.send_message(&response).await;
    }

    let source_area_root_path = if source_root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
    };

    let source_area_root = match source_area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Root mode: admin's file_root is broken. Non-root: the user's
            // personal area dir doesn't exist yet, so the file can't either.
            let error_msg = if source_root {
                err_file_area_not_accessible(ctx.locale)
            } else {
                err_file_not_found(ctx.locale)
            };
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(error_msg),
                error_kind: Some(ErrorKind::NotFound.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    let dest_area_root_path = if destination_root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
    };

    let dest_area_root = match dest_area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let error_msg = if destination_root {
                err_file_area_not_accessible(ctx.locale)
            } else {
                err_file_not_found(ctx.locale)
            };
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(error_msg),
                error_kind: Some(ErrorKind::NotFound.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    let source_candidate = match build_and_validate_candidate_path(&source_area_root, &source_path)
    {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    let dest_candidate = match build_and_validate_candidate_path(&dest_area_root, &destination_dir)
    {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Derive lock keys from candidates (pure path math) so source validation
    // runs AFTER acquisition under the lock — closes the resolve-then-mutate
    // race. Source is locked alongside target so a concurrent rename / delete
    // / overwrite can't race the recursive walk; per-child subtree races
    // remain out of scope.
    let source_basename = match source_candidate.file_name() {
        Some(name) => name.to_owned(),
        None => {
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };
    let target_path_for_lock = dest_candidate.join(&source_basename);
    let source_lock_key = match lock_key(&source_candidate) {
        Ok(k) => k,
        Err(_) => {
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };
    let target_lock_key = match lock_key(&target_path_for_lock) {
        Ok(k) => k,
        Err(_) => {
            let response = ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
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
                debug!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_COPY_SOURCE_BUSY);
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_source_busy(ctx.locale)),
                    error_kind: Some(ErrorKind::Conflict.into()),
                };
            }
            Err(_) => {
                // Destination path held by a `Fail`-mode lock.
                debug!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_COPY_DESTINATION_BUSY);
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_destination_busy(ctx.locale)),
                    error_kind: Some(ErrorKind::Conflict.into()),
                };
            }
        };

        // Under lock: resolve source from the authoritative filesystem state.
        let source_symlink_meta = std::fs::symlink_metadata(&source_candidate);
        let resolved_source = match &source_symlink_meta {
            Ok(meta) if meta.file_type().is_symlink() => {
                // Copy the symlink, not its target.
                source_candidate.clone()
            }
            Ok(_) => match resolve_path(&source_area_root, &source_candidate) {
                Ok(p) => p,
                Err(_) => {
                    break 'locked ServerMessage::FileCopyResponse {
                        success: false,
                        error: Some(err_file_not_found(ctx.locale)),
                        error_kind: Some(ErrorKind::NotFound.into()),
                    };
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_file_not_found(ctx.locale)),
                    error_kind: Some(ErrorKind::NotFound.into()),
                };
            }
            Err(_) => {
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
                    error_kind: Some(ErrorKind::InvalidPath.into()),
                };
            }
        };

        if resolved_source == source_area_root || source_candidate == source_area_root {
            break 'locked ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                error_kind: Some(ErrorKind::Permission.into()),
            };
        }

        let resolved_dest_dir = match resolve_path(&dest_area_root, &dest_candidate) {
            Ok(p) => p,
            Err(_) => {
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_file_not_found(ctx.locale)),
                    error_kind: Some(ErrorKind::NotFound.into()),
                };
            }
        };

        if !resolved_dest_dir.is_dir() {
            break 'locked ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_destination_not_directory(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
            };
        }

        let source_filename = match resolved_source.file_name() {
            Some(name) => name,
            None => {
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_file_path_invalid(ctx.locale)),
                    error_kind: Some(ErrorKind::InvalidPath.into()),
                };
            }
        };

        let target_path = resolved_dest_dir.join(source_filename);

        // No-op self-copy.
        if resolved_source == target_path {
            break 'locked ServerMessage::FileCopyResponse {
                success: true,
                error: None,
                error_kind: None,
            };
        }

        if resolved_source.is_dir() && is_subpath(&resolved_dest_dir, &resolved_source) {
            break 'locked ServerMessage::FileCopyResponse {
                success: false,
                error: Some(err_cannot_copy_into_itself(ctx.locale)),
                error_kind: Some(ErrorKind::InvalidPath.into()),
            };
        }

        if target_path.exists() || target_path.symlink_metadata().is_ok() {
            if !overwrite {
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_destination_exists(ctx.locale)),
                    error_kind: Some(ErrorKind::Exists.into()),
                };
            }

            if let Err(e) = remove_path_async(&target_path).await {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_COPY_REMOVE_FAILED);
                break 'locked ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_copy_failed(ctx.locale)),
                    error_kind: None,
                };
            }
        }

        match copy_path_recursive_async(&resolved_source, &target_path).await {
            Ok(()) => {
                ctx.file_index.mark_dirty();
                debug!(user = %requesting_user.username, ip = %ctx.peer_addr, path = %source_path, "{}", LOG_FILE_COPY_SUCCESS);
                ServerMessage::FileCopyResponse {
                    success: true,
                    error: None,
                    error_kind: None,
                }
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_COPY_FAILED);
                ServerMessage::FileCopyResponse {
                    success: false,
                    error: Some(err_copy_failed(ctx.locale)),
                    error_kind: None,
                }
            }
        }
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        create_test_context, login_user, read_server_message, setup_file_area_basic,
    };
    use std::fs;

    #[tokio::test]
    async fn test_copy_requires_auth() {
        let mut test_ctx = create_test_context().await;
        let _temp_dir = setup_file_area_basic(&mut test_ctx);

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            None,
            &mut ctx,
        )
        .await
        .unwrap_err();
    }

    #[tokio::test]
    async fn test_copy_admin_has_permission() {
        let mut test_ctx = create_test_context().await;
        let _temp_dir = setup_file_area_basic(&mut test_ctx);

        // Login without FileCopy permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_file_success() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert!(shared_dir.join("test.txt").exists());
        assert!(shared_dir.join("dest/test.txt").exists());
        assert_eq!(
            fs::read_to_string(shared_dir.join("dest/test.txt")).unwrap(),
            "content"
        );
    }

    #[tokio::test]
    async fn test_copy_directory_success() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        let source_dir = shared_dir.join("source");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("file.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "source".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert!(source_dir.exists());
        assert!(source_dir.join("file.txt").exists());
        assert!(shared_dir.join("dest/source/file.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_source_not_found() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create destination directory but not source
        let shared_dir = temp_dir.path().join("shared");
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "nonexistent.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::NotFound.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_destination_exists_no_overwrite() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source and destination with existing file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "source").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();
        fs::write(shared_dir.join("dest/test.txt"), "existing").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            false, // overwrite = false
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::Exists.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert_eq!(
            fs::read_to_string(shared_dir.join("test.txt")).unwrap(),
            "source"
        );
        assert_eq!(
            fs::read_to_string(shared_dir.join("dest/test.txt")).unwrap(),
            "existing"
        );
    }

    #[tokio::test]
    async fn test_copy_dest_exists_with_overwrite() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source and destination with existing file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "source").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();
        fs::write(shared_dir.join("dest/test.txt"), "existing").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy, Permission::FileDelete],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            true, // overwrite = true
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        // Verify source still exists and destination was overwritten
        assert!(shared_dir.join("test.txt").exists());
        assert_eq!(
            fs::read_to_string(shared_dir.join("dest/test.txt")).unwrap(),
            "source"
        );
    }

    #[tokio::test]
    async fn test_copy_overwrite_requires_delete_permission() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source and destination with existing file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "source").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();
        fs::write(shared_dir.join("dest/test.txt"), "existing").unwrap();

        // Login with FileCopy but NOT FileDelete
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            true, // overwrite = true
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_path_traversal_dest() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "../test.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::InvalidPath.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_cannot_copy_into_itself() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source directory with subdirectory
        let shared_dir = temp_dir.path().join("shared");
        let source_dir = shared_dir.join("source");
        fs::create_dir(&source_dir).unwrap();
        fs::create_dir(source_dir.join("sub")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "source".to_string(),
            "source/sub".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::InvalidPath.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_root_mode_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        // Login with FileCopy but NOT FileRoot
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "shared/test.txt".to_string(),
            "shared/dest".to_string(),
            false,
            true, // source_root = true (requires FileRoot)
            true, // destination_root = true
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_root_mode_success() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy, Permission::FileRoot],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "shared/test.txt".to_string(),
            "shared/dest".to_string(),
            false,
            true, // source_root = true
            true, // destination_root = true
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert!(shared_dir.join("test.txt").exists());
        assert!(shared_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_user_area() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source in shared, destination in users
        let shared_dir = temp_dir.path().join("shared");
        let users_dir = temp_dir.path().join("users");
        fs::create_dir_all(&users_dir).unwrap();
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(users_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy, Permission::FileRoot],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "shared/test.txt".to_string(),
            "users/dest".to_string(),
            false,
            true, // source_root = true
            true, // destination_root = true
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        // Verify file was copied across areas (original still exists)
        assert!(shared_dir.join("test.txt").exists());
        assert!(users_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_unicode_filename() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("文件.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("目录")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "文件.txt".to_string(),
            "目录".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert!(shared_dir.join("文件.txt").exists());
        assert!(shared_dir.join("目录/文件.txt").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_copy_symlink_file() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("target.txt"), "content").unwrap();
        std::os::unix::fs::symlink(shared_dir.join("target.txt"), shared_dir.join("link.txt"))
            .unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "link.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        // Verify symlink was copied (not the target)
        assert!(shared_dir.join("link.txt").symlink_metadata().is_ok()); // Original symlink still exists
        assert!(shared_dir.join("dest/link.txt").symlink_metadata().is_ok()); // Copy is also a symlink
        assert!(shared_dir.join("target.txt").exists()); // Original target still exists
    }

    #[tokio::test]
    async fn test_copy_in_user_personal_area() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let users_dir = temp_dir.path().join("users");
        let alice_dir = users_dir.join("alice");
        fs::create_dir_all(&alice_dir).unwrap();
        fs::write(alice_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(alice_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        // Verify file was copied within user's area (original still exists)
        assert!(alice_dir.join("test.txt").exists());
        assert!(alice_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_cannot_copy_area_root() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        // Try to copy area root (empty path = root)
        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "".to_string(), // Area root
            "dest".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }

    #[tokio::test]
    async fn test_copy_destination_is_file_not_directory() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source file and a file (not directory) as destination
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("source.txt"), "content").unwrap();
        fs::write(shared_dir.join("dest"), "i am a file").unwrap(); // dest is a file, not dir

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "source.txt".to_string(),
            "dest".to_string(), // This is a file, not a directory
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::InvalidPath.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert!(shared_dir.join("source.txt").exists());
        assert!(shared_dir.join("dest").exists());
    }

    #[tokio::test]
    async fn test_copy_nested_directory() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        // Try to copy file to its current directory (copy test.txt to "")
        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "".to_string(), // Same directory (root of user's area)
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error,
                error_kind,
            } => {
                // Copying file to itself is a no-op success
                assert!(success);
                assert!(error.is_none());
                assert!(error_kind.is_none());
            }
            _ => panic!("Expected FileCopyResponse"),
        }

        assert!(shared_dir.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_dest_not_found() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area_basic(&mut test_ctx);

        // Create source file but not destination directory
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileCopy],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_copy(
            "test.txt".to_string(),
            "nonexistent_dir".to_string(),
            false,
            false,
            false,
            Some(session_id),
            &mut ctx,
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileCopyResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(ErrorKind::NotFound.into()));
            }
            _ => panic!("Expected FileCopyResponse"),
        }
    }
}
