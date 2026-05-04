//! FileList message handler - Returns directory listing for file area

use std::io;
use std::path::Path;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::{FileEntry, ServerMessage};
use nexus_common::validators::{self, FilePathError};

use super::{
    HandlerContext, err_file_not_directory, err_file_not_found, err_file_path_invalid,
    err_file_path_too_long, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    HANDLER_FILE_LIST, LOG_FILE_LIST_NOT_LOGGED_IN, LOG_FILE_LIST_PERMISSION_DENIED,
    LOG_FILE_LIST_ROOT_DENIED,
};
use crate::db::Permission;
use crate::files::path::PathError;
use crate::files::{
    FolderType, allows_upload, build_and_validate_candidate_path, is_hidden_name,
    parse_folder_type, resolve_path, resolve_user_area,
};

/// Read directory entries synchronously (called from spawn_blocking)
///
/// Returns None if the directory cannot be read, Some(entries) otherwise.
/// Entries are sorted with directories first, then by name.
///
/// `can_upload` on each entry is a pure folder-type flag (`allows_upload`). The
/// client composes it with the user's upload permissions to decide whether to
/// show upload UI; this lets `file_upload_anywhere` grants take effect
/// immediately on permission change without requiring a file-list refresh.
fn read_directory_entries(
    resolved: &Path,
    area_root: &Path,
    show_hidden: bool,
) -> Option<Vec<FileEntry>> {
    let read_dir = std::fs::read_dir(resolved).ok()?;

    let mut entries = Vec::new();

    for entry_result in read_dir {
        let Ok(entry) = entry_result else {
            continue;
        };

        // Use path().metadata() to follow symlinks (entry.metadata() doesn't follow them)
        let Ok(metadata) = entry.path().metadata() else {
            continue;
        };

        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue; // Skip non-UTF8 filenames
        };

        // Skip hidden files unless show_hidden is true
        // Hidden: dotfiles (.), NAS metadata (@eaDir, @tmp), NAS recycle (#recycle)
        if !show_hidden && is_hidden_name(name_str) {
            continue;
        }

        let is_dir = metadata.is_dir();

        // Parse folder type for directories
        let folder_type = if is_dir {
            Some(parse_folder_type(name_str))
        } else {
            None
        };

        // Check if uploads are allowed at this path (pure folder-type flag).
        let entry_path = resolved.join(&file_name);
        let can_upload = is_dir && allows_upload(area_root, &entry_path);

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

    Some(entries)
}

/// Handle a file list request
pub async fn handle_file_list<W>(
    path: String,
    root: bool,
    show_hidden: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_LIST_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_FILE_LIST))
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
                can_upload: false,
                dropbox_owner: None,
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
            can_upload: false,
            dropbox_owner: None,
        };
        return ctx.send_message(&response).await;
    };

    // Check FileList permission
    if !requesting_user.has_permission(Permission::FileList) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_LIST_PERMISSION_DENIED);
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            path: None,
            entries: None,
            can_upload: false,
            dropbox_owner: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check FileRoot permission if root browsing requested
    if root && !requesting_user.has_permission(Permission::FileRoot) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_LIST_ROOT_DENIED);
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            path: None,
            entries: None,
            can_upload: false,
            dropbox_owner: None,
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
        let response = ServerMessage::FileListResponse {
            success: false,
            error: Some(error_msg),
            path: None,
            entries: None,
            can_upload: false,
            dropbox_owner: None,
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
            // User's area doesn't exist - return empty listing
            let response = ServerMessage::FileListResponse {
                success: true,
                error: None,
                path: Some(path),
                entries: Some(Vec::new()),
                can_upload: false,
                dropbox_owner: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Build candidate path (validates for traversal attacks) and resolve it
    let candidate = match build_and_validate_candidate_path(&area_root, &path) {
        Ok(p) => p,
        Err(_) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                path: None,
                entries: None,
                can_upload: false,
                dropbox_owner: None,
            };
            return ctx.send_message(&response).await;
        }
    };
    let resolved = match resolve_path(&area_root, &candidate) {
        Ok(p) => p,
        Err(PathError::NotFound) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                path: None,
                entries: None,
                can_upload: false,
                dropbox_owner: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(_) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_path_invalid(ctx.locale)),
                path: None,
                entries: None,
                can_upload: false,
                dropbox_owner: None,
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
            can_upload: false,
            dropbox_owner: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check if the current directory allows uploads (pure folder-type flag).
    // The client composes this with the user's upload permissions (including the
    // `file_upload_anywhere` bypass) when deciding whether to show upload UI.
    let current_dir_can_upload = allows_upload(&area_root, &resolved);

    // Classify the listing's drop-box context once before reading entries.
    // Both fields come from a single ancestor walk.
    //
    // We walk the *virtual* `candidate` path (not the canonical `resolved`)
    // so a drop box implemented as a symlink is still recognized by its
    // suffix. This keeps visibility classification consistent with the
    // delete/rename owner bypass, which uses the same virtual path.
    let is_admin = requesting_user.is_admin;
    let username = &requesting_user.username;
    let DropboxContext {
        hide_contents,
        dropbox_owner,
    } = dropbox_context(&candidate, &area_root, is_admin, username);

    if hide_contents {
        // User is inside a dropbox they can't see - return empty listing.
        // `dropbox_owner` is reported as a structural fact regardless of
        // viewer; the owner is already encoded in the folder name (visible
        // from the parent listing) and in the path the client navigated
        // through to get here, so it's not private.
        let response = ServerMessage::FileListResponse {
            success: true,
            error: None,
            path: Some(path),
            entries: Some(Vec::new()),
            can_upload: current_dir_can_upload,
            dropbox_owner,
        };
        return ctx.send_message(&response).await;
    }

    // Read directory entries on a blocking thread to avoid blocking async runtime
    // for directories with many entries (each entry requires metadata syscalls)
    let resolved_clone = resolved.clone();
    let area_root_clone = area_root.clone();
    let entries = tokio::task::spawn_blocking(move || {
        read_directory_entries(&resolved_clone, &area_root_clone, show_hidden)
    })
    .await;

    let entries = match entries {
        Ok(Some(e)) => e,
        Ok(None) | Err(_) => {
            let response = ServerMessage::FileListResponse {
                success: false,
                error: Some(err_file_not_found(ctx.locale)),
                path: None,
                entries: None,
                can_upload: false,
                dropbox_owner: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let response = ServerMessage::FileListResponse {
        success: true,
        error: None,
        path: Some(path),
        entries: Some(entries),
        can_upload: current_dir_can_upload,
        dropbox_owner,
    };

    ctx.send_message(&response).await
}

/// Result of walking the listed directory's ancestor chain to detect drop
/// boxes. Computed once per listing.
struct DropboxContext {
    /// True when the caller should return an empty listing — used for
    /// generic drop boxes seen by non-admins, and for user drop boxes seen
    /// by users who are neither the named owner nor an admin.
    hide_contents: bool,

    /// Owner of the innermost enclosing `[NEXUS-DB-username]` folder
    /// (including the listed directory itself, if it is the drop box).
    /// `None` when the listing is not inside any user drop box.
    ///
    /// Reported regardless of whether the requester is the owner — the
    /// client composes this with its own nickname to decide whether the
    /// file-delete bypass applies. Generic `[NEXUS-DB]` folders have no
    /// owner so this stays `None` for those.
    dropbox_owner: Option<String>,
}

/// Walk from `current_dir` up to `area_root`, classifying the listed
/// directory's drop-box context.
fn dropbox_context(
    current_dir: &std::path::Path,
    area_root: &std::path::Path,
    is_admin: bool,
    username: &str,
) -> DropboxContext {
    let mut path = current_dir;

    while path != area_root {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            match parse_folder_type(name) {
                FolderType::DropBox => {
                    return DropboxContext {
                        hide_contents: !is_admin,
                        dropbox_owner: None,
                    };
                }
                FolderType::UserDropBox(owner) => {
                    let is_owner = owner.to_lowercase() == username.to_lowercase();
                    return DropboxContext {
                        hide_contents: !is_admin && !is_owner,
                        dropbox_owner: Some(owner),
                    };
                }
                _ => {}
            }
        }

        match path.parent() {
            Some(parent) => path = parent,
            None => break,
        }
    }

    DropboxContext {
        hide_contents: false,
        dropbox_owner: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
        setup_file_area_basic, setup_file_area_full,
    };
    use std::fs;

    #[tokio::test]
    async fn test_file_list_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_list(
            "/".to_string(),
            false,
            false,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_list_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        // Login as user without FileList permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_file_list(
            "/".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        // Login as admin (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

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
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/nonexistent".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to list a file instead of a directory
        let result = handle_file_list(
            "/readme.txt".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create additional files with specific names
        fs::write(file_area.path().join("shared/zebra.txt"), "z").unwrap();
        fs::write(file_area.path().join("shared/alpha.txt"), "a").unwrap();
        fs::create_dir(file_area.path().join("shared/Zebra")).unwrap();
        fs::create_dir(file_area.path().join("shared/Alpha")).unwrap();

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a dropbox folder with content
        let dropbox = file_area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("secret.txt"), "hidden").expect("Failed to create file");

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
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a dropbox folder with content
        let dropbox = file_area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("secret.txt"), "hidden").expect("Failed to create file");

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // List the dropbox contents - admin should see the file
        let result = handle_file_list(
            "/Inbox [NEXUS-DB]".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a user dropbox folder with content
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create user dropbox");
        fs::write(dropbox.join("private.txt"), "for alice").expect("Failed to create file");

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
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a user dropbox folder for alice with content
        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create user dropbox");
        fs::write(dropbox.join("private.txt"), "for alice").expect("Failed to create file");

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
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a dropbox folder
        let dropbox = file_area.path().join("shared/Inbox [NEXUS-DB]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");

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
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create two folders with same base name - one with suffix, one without
        fs::create_dir(file_area.path().join("shared/Downloads"))
            .expect("Failed to create Downloads");
        fs::create_dir(file_area.path().join("shared/Downloads [NEXUS-UL]"))
            .expect("Failed to create Downloads [NEXUS-UL]");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
    async fn test_file_list_suffix_matching_stripped_path() {
        // Test that clients can access "Uploads [NEXUS-UL]" using just "Uploads"
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Add a file inside the upload folder
        fs::write(
            file_area.path().join("shared/Uploads [NEXUS-UL]/test.txt"),
            "upload content",
        )
        .expect("Failed to create file");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Request using stripped path "Uploads" (without the [NEXUS-UL] suffix)
        let result = handle_file_list(
            "Uploads".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success,
                path,
                entries,
                can_upload,
                ..
            } => {
                assert!(success, "FileList should succeed with stripped path");
                // Path in response should echo what client sent
                assert_eq!(path, Some("Uploads".to_string()));
                // Should be able to upload in this folder
                assert!(can_upload);
                // Should see the file we created
                let entries = entries.expect("Expected entries");
                assert!(
                    entries.iter().any(|e| e.name == "test.txt"),
                    "Should find test.txt in upload folder"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_suffix_matching_nested_path() {
        // Test that nested paths work with suffix matching
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create nested structure: "Uploads [NEXUS-UL]/subdir/file.txt"
        fs::create_dir(file_area.path().join("shared/Uploads [NEXUS-UL]/subdir"))
            .expect("Failed to create subdir");
        fs::write(
            file_area
                .path()
                .join("shared/Uploads [NEXUS-UL]/subdir/nested.txt"),
            "nested content",
        )
        .expect("Failed to create file");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Request using stripped path "Uploads/subdir"
        let result = handle_file_list(
            "Uploads/subdir".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success,
                path,
                entries,
                ..
            } => {
                assert!(success, "FileList should succeed with nested stripped path");
                assert_eq!(path, Some("Uploads/subdir".to_string()));
                let entries = entries.expect("Expected entries");
                assert!(
                    entries.iter().any(|e| e.name == "nested.txt"),
                    "Should find nested.txt"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_file_list_symlink_never_leaks_real_path() {
        use std::os::unix::fs::symlink;

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create an external directory with a distinctive path we can check for
        let external = tempfile::TempDir::new().expect("Failed to create external dir");
        let external_path = external.path().to_str().unwrap().to_string();
        fs::create_dir(external.path().join("subdir")).expect("Failed to create subdir");
        fs::write(external.path().join("file.txt"), "data").expect("Failed to create file");

        // Create symlink from shared area to external directory
        let shared = file_area.path().join("shared");
        let link_path = shared.join("Linked");
        symlink(external.path(), &link_path).expect("Failed to create symlink");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Navigate into the symlinked directory
        let result = handle_file_list(
            "/Linked".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success,
                error,
                path,
                entries,
                ..
            } => {
                assert!(success);

                // The path must be the client's virtual path, not the real filesystem path
                let path_str = path.unwrap();
                assert_eq!(path_str, "/Linked");
                assert!(
                    !path_str.contains(&external_path),
                    "Path must not contain real filesystem path: {}",
                    external_path
                );

                // Error should be None
                assert!(error.is_none());

                // Entry names must be simple filenames, not full paths
                let entries = entries.expect("Expected entries");
                for entry in &entries {
                    assert!(
                        !entry.name.contains('/'),
                        "Entry name must not contain path separators: {}",
                        entry.name
                    );
                    assert!(
                        !entry.name.contains(&external_path),
                        "Entry name must not contain real filesystem path: {}",
                        entry.name
                    );
                }
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_upload_folder_can_upload() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_list(
            "/".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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

    #[tokio::test]
    async fn test_file_list_root_requires_permission() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        // Login as user with only FileList permission (not FileRoot)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::FileList],
            false,
        )
        .await;

        // Try to browse from root - should fail without FileRoot permission
        let result = handle_file_list(
            "/".to_string(),
            true, // root = true
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected FileListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_file_list_root_with_permission() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        // Login as user with both FileList and FileRoot permissions
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::FileList, db::Permission::FileRoot],
            false,
        )
        .await;

        // Browse from root - should see shared/ and users/ directories
        let result = handle_file_list(
            "/".to_string(),
            true, // root = true
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see shared and users directories
                let shared = entries.iter().find(|e| e.name == "shared");
                let users = entries.iter().find(|e| e.name == "users");
                assert!(shared.is_some(), "Should see shared directory");
                assert!(users.is_some(), "Should see users directory");
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_root_admin_has_permission() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        // Login as admin (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin should be able to browse from root
        let result = handle_file_list(
            "/".to_string(),
            true, // root = true
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see shared and users directories
                let shared = entries.iter().find(|e| e.name == "shared");
                let users = entries.iter().find(|e| e.name == "users");
                assert!(shared.is_some(), "Should see shared directory");
                assert!(users.is_some(), "Should see users directory");
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_root_can_browse_user_areas() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a user area with content
        let alice_area = file_area.path().join("users/alice");
        fs::create_dir_all(&alice_area).expect("Failed to create user area");
        fs::write(alice_area.join("private.txt"), "alice's file").expect("Failed to create file");

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Browse into alice's user area from root
        let result = handle_file_list(
            "/users/alice".to_string(),
            true, // root = true
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see alice's private file
                let private = entries.iter().find(|e| e.name == "private.txt");
                assert!(private.is_some(), "Should see alice's private file");
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_hides_dotfiles_by_default() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create hidden entries in shared/
        fs::write(file_area.path().join("shared/.hidden"), "secret")
            .expect("Failed to create dotfile");
        fs::create_dir(file_area.path().join("shared/@eaDir")).expect("Failed to create @eaDir");
        fs::create_dir(file_area.path().join("shared/#recycle"))
            .expect("Failed to create #recycle");
        fs::write(file_area.path().join("shared/visible.txt"), "hello")
            .expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        // List without show_hidden (default)
        let result = handle_file_list(
            "/".to_string(),
            false,
            false, // show_hidden = false
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see visible.txt but not hidden entries
                assert!(
                    entries.iter().any(|e| e.name == "visible.txt"),
                    "Should see visible.txt"
                );
                assert!(
                    !entries.iter().any(|e| e.name == ".hidden"),
                    "Should not see .hidden"
                );
                assert!(
                    !entries.iter().any(|e| e.name == "@eaDir"),
                    "Should not see @eaDir"
                );
                assert!(
                    !entries.iter().any(|e| e.name == "#recycle"),
                    "Should not see #recycle"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_shows_dotfiles_when_requested() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create hidden entries in shared/
        fs::write(file_area.path().join("shared/.hidden"), "secret")
            .expect("Failed to create dotfile");
        fs::create_dir(file_area.path().join("shared/@eaDir")).expect("Failed to create @eaDir");
        fs::create_dir(file_area.path().join("shared/#recycle"))
            .expect("Failed to create #recycle");
        fs::write(file_area.path().join("shared/visible.txt"), "hello")
            .expect("Failed to create file");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        // List with show_hidden = true
        let result = handle_file_list(
            "/".to_string(),
            false,
            true, // show_hidden = true
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see both visible.txt and .hidden
                assert!(
                    entries.iter().any(|e| e.name == "visible.txt"),
                    "Should see visible.txt"
                );
                assert!(
                    entries.iter().any(|e| e.name == ".hidden"),
                    "Should see .hidden when show_hidden is true"
                );
                assert!(
                    entries.iter().any(|e| e.name == "@eaDir"),
                    "Should see @eaDir when show_hidden is true"
                );
                assert!(
                    entries.iter().any(|e| e.name == "#recycle"),
                    "Should see #recycle when show_hidden is true"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_file_list_hides_dotdirectories_by_default() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_full(&mut test_ctx);

        // Create a hidden directory in shared/
        fs::create_dir(file_area.path().join("shared/.hidden_dir"))
            .expect("Failed to create dotdir");
        fs::create_dir(file_area.path().join("shared/visible_dir")).expect("Failed to create dir");

        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        // List without show_hidden (default)
        let result = handle_file_list(
            "/".to_string(),
            false,
            false, // show_hidden = false
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileListResponse {
                success, entries, ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                // Should see visible_dir but not .hidden_dir
                assert!(
                    entries.iter().any(|e| e.name == "visible_dir"),
                    "Should see visible_dir"
                );
                assert!(
                    !entries.iter().any(|e| e.name == ".hidden_dir"),
                    "Should not see .hidden_dir"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// Listings outside any drop box report `dropbox_owner: None`.
    #[tokio::test]
    async fn test_file_list_dropbox_owner_none_outside_dropbox() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_full(&mut test_ctx);

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_list(
            "/".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                assert_eq!(dropbox_owner, None);
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// When the owner lists their own user drop box, the response carries
    /// the owner string so the client can enable the delete bypass.
    #[tokio::test]
    async fn test_file_list_dropbox_owner_set_when_owner_lists_own_dropbox() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/For Alice [NEXUS-DB-alice]"))
            .expect("Failed to create dropbox");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_list(
            "/For Alice [NEXUS-DB-alice]".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                assert_eq!(dropbox_owner.as_deref(), Some("alice"));
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// When an admin browses someone else's user drop box, `dropbox_owner`
    /// reports the actual owner — it's a structural fact, not a request
    /// for the bypass. The client's identity check decides bypass.
    #[tokio::test]
    async fn test_file_list_dropbox_owner_reports_actual_owner_for_admin() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/For Bob [NEXUS-DB-bob]"))
            .expect("Failed to create dropbox");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        handle_file_list(
            "/For Bob [NEXUS-DB-bob]".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                assert_eq!(dropbox_owner.as_deref(), Some("bob"));
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// Generic `[NEXUS-DB]` drop boxes have no owner — the field is None
    /// even when listing one (admin only, since non-admins get hidden).
    #[tokio::test]
    async fn test_file_list_dropbox_owner_none_for_generic_dropbox() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        fs::create_dir(file_area.path().join("shared/Submissions [NEXUS-DB]"))
            .expect("Failed to create dropbox");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        handle_file_list(
            "/Submissions [NEXUS-DB]".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                assert_eq!(dropbox_owner, None);
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// Non-owner non-admin attempting to list someone else's user drop
    /// box gets an empty (hidden) listing — but `dropbox_owner` is still
    /// reported as a structural fact. The owner is already encoded in the
    /// folder name (visible from the parent listing) and in the path the
    /// client just navigated through, so reporting it here adds nothing
    /// new. The hide protects *contents*, not metadata that's already
    /// public via the path itself.
    #[tokio::test]
    async fn test_file_list_dropbox_owner_reported_even_when_contents_hidden() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir(&dropbox).expect("Failed to create dropbox");
        fs::write(dropbox.join("private.txt"), "alice's").expect("Failed to create file");

        // Bob — not the owner, not an admin.
        let session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_list(
            "/For Alice [NEXUS-DB-alice]".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                entries,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                let entries = entries.expect("Expected entries");
                assert!(entries.is_empty(), "Bob must not see contents");
                assert_eq!(
                    dropbox_owner.as_deref(),
                    Some("alice"),
                    "Owner is structural — reported even when contents hidden"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// A user drop box implemented as a symlink (admin-created) is still
    /// recognized by its suffix in the virtual path. The listing reports
    /// `dropbox_owner` correctly so the client can enable the cleanup
    /// bypass.
    #[tokio::test]
    #[cfg(unix)]
    async fn test_file_list_dropbox_owner_recognized_through_symlink() {
        use std::os::unix::fs::symlink;

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        // Real directory with the dropbox content lives outside the area.
        let external = tempfile::TempDir::new().expect("Failed to create external dir");
        fs::write(external.path().join("for-alice.txt"), "data").expect("Failed to create file");

        // Admin links it into shared/ as a user-dropbox-named symlink.
        symlink(
            external.path(),
            file_area.path().join("shared/For Alice [NEXUS-DB-alice]"),
        )
        .expect("Failed to create dropbox symlink");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_list(
            "/For Alice [NEXUS-DB-alice]".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                assert_eq!(
                    dropbox_owner.as_deref(),
                    Some("alice"),
                    "Symlinked drop box must be recognized via its virtual-path suffix"
                );
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    /// `dropbox_owner` is also reported on subdirectories inside a user
    /// drop box, not just the drop-box folder itself.
    #[tokio::test]
    async fn test_file_list_dropbox_owner_set_on_nested_subdir() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);

        let dropbox = file_area.path().join("shared/For Alice [NEXUS-DB-alice]");
        fs::create_dir_all(dropbox.join("sub")).expect("Failed to create nested dirs");

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileList],
            false,
        )
        .await;

        handle_file_list(
            "/For Alice [NEXUS-DB-alice]/sub".to_string(),
            false,
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::FileListResponse {
                success,
                dropbox_owner,
                ..
            } => {
                assert!(success);
                assert_eq!(dropbox_owner.as_deref(), Some("alice"));
            }
            _ => panic!("Expected FileListResponse"),
        }
    }
}
