//! FileMove message handler - Moves a file or directory in the file area

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::FileErrorKind;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_cannot_move_into_itself, err_destination_exists,
    err_destination_not_directory, err_file_not_found, err_file_path_invalid,
    err_file_path_too_long, err_move_failed, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;
use crate::files::{
    build_and_validate_candidate_path, is_subpath, remove_path_async, rename_path_async,
    resolve_path, resolve_user_area,
};

/// Handle a file move request
pub async fn handle_file_move<W>(
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
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("FileMove request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileMove"))
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
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                error_kind: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check file root (cheap check, should always be set in production)
    let Some(file_root) = ctx.file_root else {
        // File area not configured
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
            error_kind: Some(FileErrorKind::NotFound.into()),
        };
        return ctx.send_message(&response).await;
    };

    // Check FileMove permission
    if !requesting_user.has_permission(Permission::FileMove) {
        eprintln!(
            "FileMove from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(FileErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Check FileRoot permission if either root flag is set
    if (source_root || destination_root) && !requesting_user.has_permission(Permission::FileRoot) {
        eprintln!(
            "FileMove (root) from {} (user: {}) without file_root permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(FileErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Check FileDelete permission if overwrite is requested
    if overwrite && !requesting_user.has_permission(Permission::FileDelete) {
        eprintln!(
            "FileMove (overwrite) from {} (user: {}) without file_delete permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(FileErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Validate source path
    if let Err(e) = validators::validate_file_path(&source_path) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_file_path_invalid(ctx.locale),
        };
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(error_msg),
            error_kind: Some(FileErrorKind::InvalidPath.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Validate destination path
    if let Err(e) = validators::validate_file_path(&destination_dir) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_file_path_invalid(ctx.locale),
        };
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(error_msg),
            error_kind: Some(FileErrorKind::InvalidPath.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Resolve source area root
    let source_area_root_path = if source_root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
    };

    let source_area_root = match source_area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                error_kind: Some(FileErrorKind::NotFound.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Resolve destination area root
    let dest_area_root_path = if destination_root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &requesting_user.username)
    };

    let dest_area_root = match dest_area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                error_kind: Some(FileErrorKind::NotFound.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build and validate source candidate path
    let source_candidate = match build_and_validate_candidate_path(&source_area_root, &source_path)
    {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(FileErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build and validate destination candidate path
    let dest_candidate = match build_and_validate_candidate_path(&dest_area_root, &destination_dir)
    {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(FileErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check if source exists (using symlink_metadata to not follow symlinks)
    let source_symlink_meta = std::fs::symlink_metadata(&source_candidate);

    // Determine source path (handle symlinks vs regular files)
    let resolved_source = match &source_symlink_meta {
        Ok(meta) if meta.file_type().is_symlink() => {
            // It's a symlink - move the symlink itself
            source_candidate.clone()
        }
        Ok(_) => {
            // Not a symlink - resolve and validate
            match resolve_path(&source_area_root, &source_candidate) {
                Ok(p) => p,
                Err(_) => {
                    let response = ServerMessage::FileMoveResponse {
                        success: false,
                        error: Some(err_file_not_found(ctx.locale)),
                        error_kind: Some(FileErrorKind::NotFound.into()),
                    };
                    return ctx.send_message(&response).await;
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                error_kind: Some(FileErrorKind::NotFound.into()),
            };
            return ctx.send_message(&response).await;
        }
        Err(_) => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(FileErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Prevent moving area root itself
    if resolved_source == source_area_root || source_candidate == source_area_root {
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            error_kind: Some(FileErrorKind::Permission.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Resolve destination directory (must exist and be a directory)
    let resolved_dest_dir = match resolve_path(&dest_area_root, &dest_candidate) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                error_kind: Some(FileErrorKind::NotFound.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check that destination is a directory
    if !resolved_dest_dir.is_dir() {
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_destination_not_directory(ctx.locale)),
            error_kind: Some(FileErrorKind::InvalidPath.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Get the source filename
    let source_filename = match resolved_source.file_name() {
        Some(name) => name,
        None => {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                error_kind: Some(FileErrorKind::InvalidPath.into()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build target path (destination directory + source filename)
    let target_path = resolved_dest_dir.join(source_filename);

    // Check if source is a directory and prevent moving into itself
    if resolved_source.is_dir() && is_subpath(&resolved_dest_dir, &resolved_source) {
        let response = ServerMessage::FileMoveResponse {
            success: false,
            error: Some(err_cannot_move_into_itself(ctx.locale)),
            error_kind: Some(FileErrorKind::InvalidPath.into()),
        };
        return ctx.send_message(&response).await;
    }

    // Check if moving file to itself (no-op success)
    if resolved_source == target_path {
        let response = ServerMessage::FileMoveResponse {
            success: true,
            error: None,
            error_kind: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check if target already exists
    if target_path.exists() || target_path.symlink_metadata().is_ok() {
        if !overwrite {
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_destination_exists(ctx.locale)),
                error_kind: Some(FileErrorKind::Exists.into()),
            };
            return ctx.send_message(&response).await;
        }

        // Remove existing target for overwrite (async to avoid blocking runtime)
        if let Err(e) = remove_path_async(&target_path).await {
            eprintln!(
                "FileMove failed to remove existing target for {} (user: {}): {}",
                ctx.peer_addr, requesting_user.username, e
            );
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_move_failed(ctx.locale)),
                error_kind: None,
            };
            return ctx.send_message(&response).await;
        }
    }

    // Perform the move (atomic rename - fails if cross-filesystem)
    // Uses async wrapper to avoid blocking the runtime
    match rename_path_async(&resolved_source, &target_path).await {
        Ok(()) => {
            let response = ServerMessage::FileMoveResponse {
                success: true,
                error: None,
                error_kind: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!(
                "FileMove failed for {} (user: {}): {}",
                ctx.peer_addr, requesting_user.username, e
            );
            let response = ServerMessage::FileMoveResponse {
                success: false,
                error: Some(err_move_failed(ctx.locale)),
                error_kind: None,
            };
            ctx.send_message(&response).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        TestContext, create_test_context, login_user, read_server_message,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_file_area(test_ctx: &mut TestContext) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let file_root: &'static Path = Box::leak(temp_dir.path().to_path_buf().into_boxed_path());
        test_ctx.file_root = Some(file_root);

        // Create shared directory
        fs::create_dir_all(temp_dir.path().join("shared")).unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_move_requires_auth() {
        let mut test_ctx = create_test_context().await;
        let _temp_dir = setup_file_area(&mut test_ctx);

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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
    async fn test_move_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let _temp_dir = setup_file_area(&mut test_ctx);

        // Login without FileMove permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_file_success() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source file and destination directory
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify file was moved
        assert!(!shared_dir.join("test.txt").exists());
        assert!(shared_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_move_directory_success() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source directory with contents and destination directory
        let shared_dir = temp_dir.path().join("shared");
        let source_dir = shared_dir.join("source");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("file.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify directory was moved
        assert!(!source_dir.exists());
        assert!(shared_dir.join("dest/source/file.txt").exists());
    }

    #[tokio::test]
    async fn test_move_source_not_found() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create destination directory but not source
        let shared_dir = temp_dir.path().join("shared");
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::NotFound.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_destination_exists_no_overwrite() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source and destination with existing file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "source").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();
        fs::write(shared_dir.join("dest/test.txt"), "existing").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::Exists.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify files unchanged
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
    async fn test_move_destination_exists_with_overwrite() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source and destination with existing file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "source").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();
        fs::write(shared_dir.join("dest/test.txt"), "existing").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove, Permission::FileDelete],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify source moved and overwrote destination
        assert!(!shared_dir.join("test.txt").exists());
        assert_eq!(
            fs::read_to_string(shared_dir.join("dest/test.txt")).unwrap(),
            "source"
        );
    }

    #[tokio::test]
    async fn test_move_overwrite_requires_delete_permission() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source and destination with existing file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "source").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();
        fs::write(shared_dir.join("dest/test.txt"), "existing").unwrap();

        // Login with FileMove but NOT FileDelete
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_path_traversal_blocked() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create test file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::InvalidPath.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_cannot_move_into_itself() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source directory with subdirectory
        let shared_dir = temp_dir.path().join("shared");
        let source_dir = shared_dir.join("source");
        fs::create_dir(&source_dir).unwrap();
        fs::create_dir(source_dir.join("sub")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::InvalidPath.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_root_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create test file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        // Login with FileMove but NOT FileRoot
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_root_mode_success() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create test file in shared area
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove, Permission::FileRoot],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify file was moved
        assert!(!shared_dir.join("test.txt").exists());
        assert!(shared_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_move_cross_area() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

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
            &[Permission::FileMove, Permission::FileRoot],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify file was moved across areas
        assert!(!shared_dir.join("test.txt").exists());
        assert!(users_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_move_unicode_filename() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source file with unicode name
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("文件.txt"), "content").unwrap();
        fs::create_dir(shared_dir.join("目录")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify file was moved
        assert!(!shared_dir.join("文件.txt").exists());
        assert!(shared_dir.join("目录/文件.txt").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_move_symlink() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create a symlink
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("target.txt"), "content").unwrap();
        std::os::unix::fs::symlink(shared_dir.join("target.txt"), shared_dir.join("link.txt"))
            .unwrap();
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify symlink was moved (not the target)
        assert!(!shared_dir.join("link.txt").exists());
        assert!(shared_dir.join("dest/link.txt").symlink_metadata().is_ok());
        assert!(shared_dir.join("target.txt").exists()); // Original target still exists
    }

    #[tokio::test]
    async fn test_move_in_user_personal_area() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create user's personal area
        let users_dir = temp_dir.path().join("users");
        let alice_dir = users_dir.join("alice");
        fs::create_dir_all(&alice_dir).unwrap();
        fs::write(alice_dir.join("test.txt"), "content").unwrap();
        fs::create_dir(alice_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify file was moved within user's area
        assert!(!alice_dir.join("test.txt").exists());
        assert!(alice_dir.join("dest/test.txt").exists());
    }

    #[tokio::test]
    async fn test_move_cannot_move_area_root() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create destination
        let shared_dir = temp_dir.path().join("shared");
        fs::create_dir(shared_dir.join("dest")).unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        // Try to move area root (empty path = root)
        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::Permission.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }

    #[tokio::test]
    async fn test_move_destination_is_file_not_directory() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source file and a file (not directory) as destination
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("source.txt"), "content").unwrap();
        fs::write(shared_dir.join("dest"), "i am a file").unwrap(); // dest is a file, not dir

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::InvalidPath.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // Verify files unchanged
        assert!(shared_dir.join("source.txt").exists());
        assert!(shared_dir.join("dest").exists());
    }

    #[tokio::test]
    async fn test_move_to_same_directory() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source file
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        // Try to move file to its current directory (move test.txt to "")
        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error,
                error_kind,
            } => {
                // Moving file to itself is a no-op success
                assert!(success);
                assert!(error.is_none());
                assert!(error_kind.is_none());
            }
            _ => panic!("Expected FileMoveResponse"),
        }

        // File should still exist
        assert!(shared_dir.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_move_destination_not_found() {
        let mut test_ctx = create_test_context().await;
        let temp_dir = setup_file_area(&mut test_ctx);

        // Create source file but not destination directory
        let shared_dir = temp_dir.path().join("shared");
        fs::write(shared_dir.join("test.txt"), "content").unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileMove],
            false,
        )
        .await;

        let mut ctx = test_ctx.handler_context();
        handle_file_move(
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

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileMoveResponse {
                success,
                error_kind,
                ..
            } => {
                assert!(!success);
                assert_eq!(error_kind, Some(FileErrorKind::NotFound.into()));
            }
            _ => panic!("Expected FileMoveResponse"),
        }
    }
}
