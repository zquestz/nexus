//! File search handler

use std::io;
use std::sync::Arc;

use tokio::io::AsyncWrite;
use tracing::{error, warn};

use crate::constants::{
    HANDLER_FILE_SEARCH, LOG_FILE_SEARCH_ERROR, LOG_FILE_SEARCH_NOT_LOGGED_IN,
    LOG_FILE_SEARCH_PANIC, LOG_FILE_SEARCH_PERMISSION_DENIED, LOG_FILE_SEARCH_ROOT_DENIED,
};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, SearchQueryError, validate_search_query};

use super::{
    HandlerContext, err_not_logged_in, err_permission_denied, err_search_failed,
    err_search_query_empty, err_search_query_invalid, err_search_query_too_long,
    err_search_query_too_short,
};
use crate::db::Permission;
use crate::files::{dropbox_entry_visible, resolve_user_area};

pub async fn handle_file_search<W>(
    query: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_FILE_SEARCH_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_FILE_SEARCH))
            .await;
    };

    if let Err(e) = validate_search_query(&query) {
        let error_msg = match e {
            SearchQueryError::Empty => err_search_query_empty(ctx.locale),
            SearchQueryError::TooShort => {
                err_search_query_too_short(ctx.locale, validators::MIN_QUERY_LENGTH)
            }
            SearchQueryError::TooLong => {
                err_search_query_too_long(ctx.locale, validators::MAX_SEARCH_QUERY_LENGTH)
            }
            SearchQueryError::InvalidCharacters => err_search_query_invalid(ctx.locale),
        };
        let response = ServerMessage::FileSearchResponse {
            success: false,
            error: Some(error_msg),
            results: None,
        };
        return ctx.send_message(&response).await;
    }

    // Root search has no prefix filter; otherwise scope to the user's area.
    let user_area_result = 'user_area: {
        let _state_guard = ctx.user_manager.read_user_state().await;
        let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
            Some(user) => user,
            None => {
                let response = ServerMessage::Error {
                    message: err_not_logged_in(ctx.locale),
                    command: Some(HANDLER_FILE_SEARCH.to_string()),
                    disconnect: false,
                };
                break 'user_area Err(response);
            }
        };

        if !requesting_user.has_permission(Permission::FileSearch) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_SEARCH_PERMISSION_DENIED);
            let response = ServerMessage::FileSearchResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                results: None,
            };
            break 'user_area Err(response);
        }

        // Whole-area search requires FileRoot.
        if root && !requesting_user.has_permission(Permission::FileRoot) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_FILE_SEARCH_ROOT_DENIED);
            let response = ServerMessage::FileSearchResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                results: None,
            };
            break 'user_area Err(response);
        }

        let is_admin = requesting_user.is_admin;
        if root {
            Ok((None, requesting_user.username, is_admin))
        } else {
            let Some(file_root) = ctx.file_root else {
                let response = ServerMessage::FileSearchResponse {
                    success: true,
                    error: None,
                    results: Some(vec![]),
                };
                break 'user_area Err(response);
            };

            // Relative area path, e.g. "/shared" or "/users/alice".
            let area_root = resolve_user_area(file_root, &requesting_user.username).await;
            let relative_area = area_root
                .strip_prefix(file_root)
                .map(|p| format!("/{}", p.to_string_lossy().replace('\\', "/")))
                .unwrap_or_else(|_| "/".to_string());

            Ok((Some(relative_area), requesting_user.username, is_admin))
        }
    };
    let (area_prefix, username, is_admin) = match user_area_result {
        Ok(user_area) => user_area,
        Err(ServerMessage::Error {
            message, command, ..
        }) => {
            return ctx
                .send_error_and_disconnect(&message, command.as_deref())
                .await;
        }
        Err(response) => return ctx.send_message(&response).await,
    };

    // Perform the search on blocking thread pool (grep-searcher does synchronous I/O).
    let file_index = Arc::clone(&ctx.file_index);
    let query_clone = query.clone();
    let area_prefix_clone = area_prefix.clone();
    let username_clone = username.clone();
    let search_result = tokio::task::spawn_blocking(move || {
        // Read-gate each candidate through the same innermost-wins gate FileList
        // uses, bounded by the area root ("/" for root search). Gating inside the
        // search means the result cap counts only entries the requester may read,
        // so search never surfaces — or hints at — drop-box contents it hides.
        let area_root = std::path::Path::new(area_prefix_clone.as_deref().unwrap_or("/"));
        file_index.search_readable(
            &query_clone,
            area_prefix_clone.as_deref(),
            |path, is_dir| {
                dropbox_entry_visible(
                    std::path::Path::new(path),
                    area_root,
                    &username_clone,
                    is_admin,
                    is_dir,
                )
            },
        )
    })
    .await;

    let mut results = match search_result {
        Ok(Ok(results)) => results,
        Ok(Err(e)) => {
            error!(user = %username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_SEARCH_ERROR);
            let response = ServerMessage::FileSearchResponse {
                success: false,
                error: Some(err_search_failed(ctx.locale)),
                results: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %username, ip = %ctx.peer_addr, err = %e, "{}", LOG_FILE_SEARCH_PANIC);
            let response = ServerMessage::FileSearchResponse {
                success: false,
                error: Some(err_search_failed(ctx.locale)),
                results: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Strip the area prefix so the client sees virtual paths, e.g.
    // "/shared/Documents/file.txt" -> "/Documents/file.txt".
    if let Some(prefix) = &area_prefix {
        for result in &mut results {
            if let Some(stripped) = result.path.strip_prefix(prefix) {
                if stripped.is_empty() {
                    result.path = "/".to_string();
                } else if stripped.starts_with('/') {
                    result.path = stripped.to_string();
                } else {
                    result.path = format!("/{}", stripped);
                }
            }
        }
    }

    let response = ServerMessage::FileSearchResponse {
        success: true,
        error: None,
        results: Some(results),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::db::Permission;
    use crate::files::FileIndex;
    use crate::handlers::testing::{
        create_test_context, login_user, read_server_message, setup_file_area_basic,
    };

    #[tokio::test]
    async fn test_file_search_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_search(
            "test".to_string(),
            false,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_search_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "user", "password", &[], false).await;

        let result = handle_file_search(
            "test".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_root_requires_file_root_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "test".to_string(),
            true, // root flag set
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_search(
            "test".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_admin_with_root() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (has all permissions implicitly, including file_root)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_search(
            "test".to_string(),
            true, // root flag set - admin should be able to use it
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_with_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "test".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse {
                success,
                error,
                results,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(results.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_does_not_leak_sibling_user_area() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");
        let index_data = tempfile::TempDir::new().expect("index data dir");
        test_ctx.file_index = Arc::new(FileIndex::new(index_data.path(), file_root));

        fs::create_dir_all(file_area.path().join("users/alice/docs")).unwrap();
        fs::create_dir_all(file_area.path().join("users/alice2/docs")).unwrap();
        fs::write(
            file_area.path().join("users/alice/docs/secret-own.txt"),
            "content",
        )
        .unwrap();
        fs::write(
            file_area
                .path()
                .join("users/alice2/docs/secret-neighbor.txt"),
            "content",
        )
        .unwrap();

        assert!(test_ctx.file_index.trigger_reindex());
        for _ in 0..100 {
            if !test_ctx.file_index.is_reindexing() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!test_ctx.file_index.is_reindexing());

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "secret".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse {
                success,
                error,
                results,
            } => {
                assert!(success);
                assert!(error.is_none());
                let paths: Vec<String> = results
                    .expect("search results")
                    .into_iter()
                    .map(|result| result.path)
                    .collect();
                assert_eq!(paths, vec!["/docs/secret-own.txt"]);
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    fn search_result_paths(response: ServerMessage) -> Vec<String> {
        match response {
            ServerMessage::FileSearchResponse {
                success,
                error,
                results,
            } => {
                assert!(success);
                assert!(error.is_none());
                let mut paths: Vec<String> = results
                    .expect("search results")
                    .into_iter()
                    .map(|result| result.path)
                    .collect();
                paths.sort();
                paths
            }
            other => panic!("Expected FileSearchResponse, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_file_search_hides_dropbox_contents_from_non_owners() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");
        let index_data = tempfile::TempDir::new().expect("index data dir");
        test_ctx.file_index = Arc::new(FileIndex::new(index_data.path(), file_root));

        // Users without a personal dir share /shared. Plant a matching file in
        // the open area, a generic (admin-only) drop box, and alice's drop box.
        let shared = file_area.path().join("shared");
        fs::create_dir_all(shared.join("Public")).unwrap();
        fs::write(shared.join("Public/secret-public.txt"), "x").unwrap();
        fs::create_dir_all(shared.join("Blind [NEXUS-DB]")).unwrap();
        fs::write(shared.join("Blind [NEXUS-DB]/secret-blind.txt"), "x").unwrap();
        fs::create_dir_all(shared.join("For Alice [NEXUS-DB-alice]")).unwrap();
        fs::write(
            shared.join("For Alice [NEXUS-DB-alice]/secret-alice.txt"),
            "x",
        )
        .unwrap();

        assert!(test_ctx.file_index.trigger_reindex());
        for _ in 0..100 {
            if !test_ctx.file_index.is_reindexing() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!test_ctx.file_index.is_reindexing());

        // bob (non-owner) sees only the open-area hit.
        let session = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;
        let result = handle_file_search(
            "secret".to_string(),
            false,
            Some(session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(
            search_result_paths(read_server_message(&mut test_ctx).await),
            vec!["/Public/secret-public.txt"]
        );

        // alice additionally sees her own drop box, but not the blind one.
        let session = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;
        let result = handle_file_search(
            "secret".to_string(),
            false,
            Some(session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(
            search_result_paths(read_server_message(&mut test_ctx).await),
            vec![
                "/For Alice [NEXUS-DB-alice]/secret-alice.txt",
                "/Public/secret-public.txt",
            ]
        );

        // admin sees everything, including the blind drop box.
        let session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let result = handle_file_search(
            "secret".to_string(),
            false,
            Some(session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(
            search_result_paths(read_server_message(&mut test_ctx).await),
            vec![
                "/Blind [NEXUS-DB]/secret-blind.txt",
                "/For Alice [NEXUS-DB-alice]/secret-alice.txt",
                "/Public/secret-public.txt",
            ]
        );
    }

    #[tokio::test]
    async fn test_file_search_empty_query() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "".to_string(),
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_query_too_short() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "ab".to_string(), // Only 2 chars, min is 3
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_query_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "a".repeat(257), // Over 256 char limit
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_query_with_control_chars() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "test\0file".to_string(), // Contains null byte
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_valid_query_at_min_length() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileSearch],
            false,
        )
        .await;

        let result = handle_file_search(
            "abc".to_string(), // Exactly 3 chars (minimum)
            false,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileSearchResponse { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileSearchResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_search_shows_dropbox_folder_but_not_contents() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");
        let index_data = tempfile::TempDir::new().expect("index data dir");
        test_ctx.file_index = Arc::new(FileIndex::new(index_data.path(), file_root));

        let shared = file_area.path().join("shared");
        fs::create_dir_all(shared.join("Reports [NEXUS-DB-alice]")).unwrap();
        fs::write(shared.join("Reports [NEXUS-DB-alice]/report.txt"), "x").unwrap();

        assert!(test_ctx.file_index.trigger_reindex());
        for _ in 0..100 {
            if !test_ctx.file_index.is_reindexing() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!test_ctx.file_index.is_reindexing());

        // "report" matches both the drop box folder and the file inside it. A
        // non-owner sees the folder (it is viewable) but not its contents.
        let bob = login_user(&mut test_ctx, "bob", "pw", &[Permission::FileSearch], false).await;
        let result = handle_file_search(
            "report".to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(
            search_result_paths(read_server_message(&mut test_ctx).await),
            vec!["/Reports [NEXUS-DB-alice]"]
        );
    }

    #[tokio::test]
    async fn test_file_search_shows_owned_nested_dropbox_folder() {
        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");
        let index_data = tempfile::TempDir::new().expect("index data dir");
        test_ctx.file_index = Arc::new(FileIndex::new(index_data.path(), file_root));

        let shared = file_area.path().join("shared");
        fs::create_dir_all(shared.join("Outer [NEXUS-DB-alice]/Inner [NEXUS-DB-bob]")).unwrap();
        fs::write(
            shared.join("Outer [NEXUS-DB-alice]/Inner [NEXUS-DB-bob]/data.txt"),
            "x",
        )
        .unwrap();

        assert!(test_ctx.file_index.trigger_reindex());
        for _ in 0..100 {
            if !test_ctx.file_index.is_reindexing() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!test_ctx.file_index.is_reindexing());

        // M1: bob owns the inner box. His search surfaces the inner folder itself
        // (innermost-wins), even though it sits inside alice's box.
        let bob = login_user(&mut test_ctx, "bob", "pw", &[Permission::FileSearch], false).await;
        let result = handle_file_search(
            "Inner".to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let paths = search_result_paths(read_server_message(&mut test_ctx).await);
        assert!(paths.contains(&"/Outer [NEXUS-DB-alice]/Inner [NEXUS-DB-bob]".to_string()));

        // An unrelated user sees nothing for the same query.
        let carol = login_user(
            &mut test_ctx,
            "carol",
            "pw",
            &[Permission::FileSearch],
            false,
        )
        .await;
        let result = handle_file_search(
            "Inner".to_string(),
            false,
            Some(carol),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        assert!(search_result_paths(read_server_message(&mut test_ctx).await).is_empty());
    }

    #[tokio::test]
    async fn test_file_search_cap_counts_only_readable_entries() {
        use crate::files::index::MAX_SEARCH_RESULTS;

        let mut test_ctx = create_test_context().await;
        let file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");
        let index_data = tempfile::TempDir::new().expect("index data dir");
        test_ctx.file_index = Arc::new(FileIndex::new(index_data.path(), file_root));

        let shared = file_area.path().join("shared");
        // More than a capful of hidden matches inside a blind (admin-only) box…
        let blind = shared.join("Inbox [NEXUS-DB]");
        fs::create_dir_all(&blind).unwrap();
        for i in 0..(MAX_SEARCH_RESULTS + 50) {
            fs::write(blind.join(format!("report-hidden-{i}.txt")), "x").unwrap();
        }
        // …and exactly a capful of readable matches out in the open.
        for i in 0..MAX_SEARCH_RESULTS {
            fs::write(shared.join(format!("report-open-{i}.txt")), "x").unwrap();
        }

        assert!(test_ctx.file_index.trigger_reindex());
        for _ in 0..200 {
            if !test_ctx.file_index.is_reindexing() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!test_ctx.file_index.is_reindexing());

        // A non-admin's search skips the hidden matches *as they are scanned*, so
        // they never consume a result slot. The cap is therefore filled entirely
        // by readable matches — regardless of the order WalkDir produced them in.
        let bob = login_user(&mut test_ctx, "bob", "pw", &[Permission::FileSearch], false).await;
        let result = handle_file_search(
            "report".to_string(),
            false,
            Some(bob),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let paths = search_result_paths(read_server_message(&mut test_ctx).await);
        assert_eq!(paths.len(), MAX_SEARCH_RESULTS);
        assert!(paths.iter().all(|p| !p.contains("[NEXUS-DB]")));
    }
}
