//! FileList message handler - Returns directory listing for file area

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{FileEntry, ServerMessage};
use nexus_common::validators::{self, FilePathError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_file_not_directory, err_file_not_found, err_file_path_invalid,
    err_file_path_too_long, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;
use crate::files::path::PathError;
use crate::files::{
    FolderType, allows_upload, build_candidate_path, parse_folder_type, resolve_path,
    resolve_user_area,
};

/// Handle a file list request
pub async fn handle_file_list<W>(
    path: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("FileList request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileList"))
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
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                path: None,
                entries: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check file root (cheap check, should always be set in production)
    let Some(file_root) = ctx.file_root else {
        // File area not configured
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(err_file_not_found(ctx.locale)),
            path: None,
            entries: None,
        };
        return ctx.send_message(&response).await;
    };

    // Check FileList permission
    if !requesting_user.has_permission(Permission::FileList) {
        eprintln!(
            "FileList from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            path: None,
            entries: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate path
    if let Err(e) = validators::validate_file_path(&path) {
        let error_msg = match e {
            FilePathError::TooLong => {
                err_file_path_too_long(ctx.locale, validators::MAX_FILE_PATH_LENGTH)
            }
            FilePathError::ContainsNull | FilePathError::InvalidCharacters => {
                err_file_path_invalid(ctx.locale)
            }
        };
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(error_msg),
            path: None,
            entries: None,
        };
        return ctx.send_message(&response).await;
    }

    // Resolve user's area root
    let area_root_path = resolve_user_area(file_root, &requesting_user.username);

    // Canonicalize area_root (it might not exist yet for new users)
    let area_root = match area_root_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // User's area doesn't exist - return empty listing
            let response = ServerMessage::FileListResponse {
                success: true,
                error: None,
                path: Some(path),
                entries: Some(Vec::new()),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build candidate path and resolve it
    let candidate = build_candidate_path(&area_root, &path);
    let resolved = match resolve_path(&area_root, &candidate) {
        Ok(p) => p,
        Err(PathError::NotFound) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                path: None,
                entries: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(_) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                path: None,
                entries: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Verify it's a directory
    if !resolved.is_dir() {
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(err_file_not_directory(ctx.locale)),
            path: None,
            entries: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check if we're inside a dropbox - if unauthorized, return empty listing
    // This check is done once before the loop for efficiency
    let is_admin = requesting_user.is_admin;
    let username = &requesting_user.username;
    if should_hide_entry(&resolved, &area_root, is_admin, username) {
        // User is inside a dropbox they can't see - return empty listing
        let response = ServerMessage::FileListResponse {
            success: true,
            error: None,
            path: Some(path),
            entries: Some(Vec::new()),
        };
        return ctx.send_message(&response).await;
    }

    // Read directory entries
    let read_dir = match std::fs::read_dir(&resolved) {
        Ok(rd) => rd,
        Err(_) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                path: None,
                entries: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let mut entries = Vec::new();

    for entry_result in read_dir {
        let Ok(entry) = entry_result else {
            continue;
        };

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue; // Skip non-UTF8 filenames
        };

        let is_dir = metadata.is_dir();

        // Parse folder type for directories
        let folder_type = if is_dir {
            Some(parse_folder_type(name_str))
        } else {
            None
        };

        // Check if uploads are allowed at this path
        let entry_path = resolved.join(&file_name);
        let can_upload = if is_dir {
            allows_upload(&area_root, &entry_path)
        } else {
            false
        };

        // Get file size (0 for directories)
        let size = if is_dir { 0 } else { metadata.len() };

        // Get modified time
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Convert folder type to string
        let dir_type = folder_type.map(|ft| match ft {
            FolderType::Default => "default".to_string(),
            FolderType::Upload => "upload".to_string(),
            FolderType::DropBox => "dropbox".to_string(),
            FolderType::UserDropBox(owner) => format!("dropbox:{}", owner),
        });

        entries.push(FileEntry {
            name: name_str.to_owned(),
            size,
            modified,
            dir_type,
            can_upload,
        });
    }

    // Sort entries: directories first, then by name
    entries.sort_by(|a, b| {
        let a_is_dir = a.dir_type.is_some();
        let b_is_dir = b.dir_type.is_some();
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    let response = ServerMessage::FileListResponse {
        success: true,
        error: None,
        path: Some(path),
        entries: Some(entries),
    };

    ctx.send_message(&response).await
}

/// Check if we should hide entries because we're inside a dropbox
fn should_hide_entry(
    current_dir: &std::path::Path,
    area_root: &std::path::Path,
    is_admin: bool,
    username: &str,
) -> bool {
    // Walk up from current_dir to area_root, checking for dropbox folders
    let mut path = current_dir;

    while path != area_root {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            match parse_folder_type(name) {
                FolderType::DropBox => {
                    // Inside a generic dropbox - only admins can see contents
                    return !is_admin;
                }
                FolderType::UserDropBox(owner) => {
                    // Inside a user dropbox - only the owner and admins can see contents
                    return !is_admin && owner.to_lowercase() != username.to_lowercase();
                }
                _ => {}
            }
        }

        match path.parent() {
            Some(parent) => path = parent,
            None => break,
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use std::fs;
    use tempfile::TempDir;

    fn setup_file_area() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        // Create shared directory structure
        fs::create_dir_all(root.join("shared")).expect("Failed to create shared");
        fs::create_dir_all(root.join("shared/Documents")).expect("Failed to create Documents");
        fs::create_dir_all(root.join("shared/Uploads [NEXUS-UL]"))
            .expect("Failed to create Uploads");
        fs::write(root.join("shared/readme.txt"), "test content").expect("Failed to create file");
        fs::write(root.join("shared/Documents/file.txt"), "doc content")
            .expect("Failed to create file");

        // Create users directory
        fs::create_dir_all(root.join("users")).expect("Failed to create users");

        temp_dir
    }

    #[tokio::test]
    async fn test_file_list_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_list("/".to_string(), None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_list_requires_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as user without FileList permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_file_list_admin_has_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as admin (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see Documents, Uploads, and readme.txt
                assert!(entries.len() >= 2);
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_with_permission() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as user with FileList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::FileList],
            false,
        )
        .await;

        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success,
                path,
                entries,
                ..
            } => {
                assert!(success);
                assert_eq!(path, Some("/".to_string()));
                let entries = entries.expect("Expected entries");
                // Should have at least Documents and readme.txt
                assert!(!entries.is_empty());
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_path_validation() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Test path with null byte
        let result = handle_file_list(
            "/path\0with/null".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_file_list_not_found() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/nonexistent".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_file_list_not_directory() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to list a file instead of a directory
        let result = handle_file_list(
            "/readme.txt".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_file_list_sorted() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Create additional files with specific names
        fs::write(file_area.path().join("shared/zebra.txt"), "z").unwrap();
        fs::write(file_area.path().join("shared/alpha.txt"), "a").unwrap();
        fs::create_dir(file_area.path().join("shared/Zebra")).unwrap();
        fs::create_dir(file_area.path().join("shared/Alpha")).unwrap();

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");

                // Find where directories end and files begin
                let first_file_idx = entries.iter().position(|e| e.dir_type.is_none());

                if let Some(idx) = first_file_idx {
                    // Verify all directories come before files
                    for (i, entry) in entries.iter().enumerate() {
                        if i < idx {
                            assert!(
                                entry.dir_type.is_some(),
                                "Entry {} should be a directory",
                                entry.name
                            );
                        } else {
                            assert!(
                                entry.dir_type.is_none(),
                                "Entry {} should be a file",
                                entry.name
                            );
                        }
                    }
                }
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_dropbox_hidden_from_non_admin() {
        let file_area = setup_file_area();

        // Create a dropbox folder with content
        let dropbox = file_area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("secret.txt"), "hidden").expect("Failed to create file");

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as regular user with FileList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::FileList],
            false,
        )
        .await;

        // List the dropbox contents - should be empty for non-admin
        let result = handle_file_list(
            "/Inbox [NEXUS-DB]".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Non-admin should see empty listing inside dropbox
                assert!(
                    entries.is_empty(),
                    "Non-admin should not see dropbox contents"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_dropbox_visible_to_admin() {
        let file_area = setup_file_area();

        // Create a dropbox folder with content
        let dropbox = file_area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("secret.txt"), "hidden").expect("Failed to create file");

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // List the dropbox contents - admin should see the file
        let result = handle_file_list(
            "/Inbox [NEXUS-DB]".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Admin should see the secret file
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "secret.txt");
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_user_dropbox_visible_to_owner() {
        let file_area = setup_file_area();

        // Create a user dropbox folder with content
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create user dropbox");
        fs::write(dropbox.join("private.txt"), "for alice").expect("Failed to create file");

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as alice (the owner)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::FileList],
            false,
        )
        .await;

        // List the user dropbox contents - alice should see her files
        let result = handle_file_list(
            "/For Alice [NEXUS-DB-alice]".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Alice should see her private file
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "private.txt");
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_user_dropbox_hidden_from_other_users() {
        let file_area = setup_file_area();

        // Create a user dropbox folder for alice with content
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create user dropbox");
        fs::write(dropbox.join("private.txt"), "for alice").expect("Failed to create file");

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as bob (not the owner)
        let session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[db::Permission::FileList],
            false,
        )
        .await;

        // List alice's dropbox contents - bob should see empty
        let result = handle_file_list(
            "/For Alice [NEXUS-DB-alice]".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Bob should not see alice's files
                assert!(
                    entries.is_empty(),
                    "Other users should not see user dropbox contents"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_dropbox_entry_visible_in_parent() {
        let file_area = setup_file_area();

        // Create a dropbox folder
        let dropbox = file_area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        // Login as regular user with FileList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::FileList],
            false,
        )
        .await;

        // List root - the dropbox folder entry should be visible
        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see the dropbox folder entry (with suffix - client strips for display)
                let inbox = entries.iter().find(|e| e.name == "Inbox [NEXUS-DB]");
                assert!(
                    inbox.is_some(),
                    "Dropbox folder entry should be visible in parent"
                );
                let inbox = inbox.unwrap();
                assert_eq!(inbox.dir_type, Some("dropbox".to_string()));
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_same_name_with_and_without_suffix() {
        let file_area = setup_file_area();

        // Create two folders with same base name - one with suffix, one without
        fs::create_dir(file_area.path().join("shared/Downloads"))
            .expect("Failed to create Downloads");
        fs::create_dir(file_area.path().join("shared/Downloads [NEXUS-UL]"))
            .expect("Failed to create Downloads [NEXUS-UL]");

        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");

                // Should find both folders with their actual names
                let downloads_plain = entries.iter().find(|e| e.name == "Downloads");
                let downloads_upload = entries.iter().find(|e| e.name == "Downloads [NEXUS-UL]");

                assert!(downloads_plain.is_some(), "Should find Downloads folder");
                assert!(
                    downloads_upload.is_some(),
                    "Should find Downloads [NEXUS-UL] folder"
                );

                // Verify their types are different
                let plain = downloads_plain.unwrap();
                let upload = downloads_upload.unwrap();

                assert_eq!(plain.dir_type, Some("default".to_string()));
                assert_eq!(upload.dir_type, Some("upload".to_string()));

                // Only the upload folder should allow uploads
                assert!(!plain.can_upload);
                assert!(upload.can_upload);
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_upload_folder_can_upload() {
        let file_area = setup_file_area();
        let mut test_ctx = create_test_context().await;
        test_ctx.file_root = Some(Box::leak(file_area.path().to_path_buf().into_boxed_path()));

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");

                // Find the Uploads folder (with suffix - client strips for display)
                let uploads = entries.iter().find(|e| e.name == "Uploads [NEXUS-UL]");
                assert!(uploads.is_some(), "Should find Uploads folder");
                let uploads = uploads.unwrap();
                assert!(uploads.can_upload, "Uploads folder should allow uploads");
                assert_eq!(uploads.dir_type, Some("upload".to_string()));

                // Regular Documents folder should not allow uploads
                let docs = entries.iter().find(|e| e.name == "Documents");
                assert!(docs.is_some(), "Should find Documents folder");
                let docs = docs.unwrap();
                assert!(
                    !docs.can_upload,
                    "Documents folder should not allow uploads"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }
}
