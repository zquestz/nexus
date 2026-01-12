//! File search handler

use std::io;
use std::sync::Arc;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, SearchQueryError, validate_search_query};

use super::{
    HandlerContext, err_not_logged_in, err_permission_denied, err_search_failed,
    err_search_query_empty, err_search_query_invalid, err_search_query_too_long,
    err_search_query_too_short,
};
use crate::db::Permission;
use crate::files::resolve_user_area;

/// Handle a file search request
pub async fn handle_file_search<W>(
    query: String,
    root: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("FileSearch request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileSearch"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileSearch"))
                .await;
        }
    };

    // Check file_search permission
    if !requesting_user.has_permission(Permission::FileSearch) {
        eprintln!(
            "FileSearch from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileSearchResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            results: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check file_root permission if root flag is set
    if root && !requesting_user.has_permission(Permission::FileRoot) {
        eprintln!(
            "FileSearch with root from {} (user: {}) without file_root permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileSearchResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            results: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate search query
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

    // Determine search area prefix
    let area_prefix = if root {
        // Admin searching entire file area - no prefix filter
        None
    } else {
        // User searching their own area
        let Some(file_root) = ctx.file_root else {
            let response = ServerMessage::FileSearchResponse {
                success: true,
                error: None,
                results: Some(vec![]),
            };
            return ctx.send_message(&response).await;
        };

        // Get user's area relative path (e.g., "/shared" or "/users/alice")
        let area_root = resolve_user_area(file_root, &requesting_user.username);
        let relative_area = area_root
            .strip_prefix(file_root)
            .map(|p| format!("/{}", p.to_string_lossy().replace('\\', "/")))
            .unwrap_or_else(|_| "/".to_string());

        Some(relative_area)
    };

    // Perform the search on blocking thread pool (grep-searcher does synchronous I/O)
    let file_index = Arc::clone(&ctx.file_index);
    let query_clone = query.clone();
    let area_prefix_clone = area_prefix.clone();
    let search_result = tokio::task::spawn_blocking(move || {
        file_index.search(&query_clone, area_prefix_clone.as_deref())
    })
    .await;

    let mut results = match search_result {
        Ok(Ok(results)) => results,
        Ok(Err(e)) => {
            eprintln!("FileSearch error from {}: {}", ctx.peer_addr, e);
            let response = ServerMessage::FileSearchResponse {
                success: false,
                error: Some(err_search_failed(ctx.locale)),
                results: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("FileSearch task panicked from {}: {}", ctx.peer_addr, e);
            let response = ServerMessage::FileSearchResponse {
                success: false,
                error: Some(err_search_failed(ctx.locale)),
                results: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Strip area prefix from result paths so client sees virtual paths
    // e.g., "/shared/Documents/file.txt" -> "/Documents/file.txt"
    if let Some(prefix) = &area_prefix {
        for result in &mut results {
            if let Some(stripped) = result.path.strip_prefix(prefix) {
                // If stripping leaves empty or just removes trailing content, add leading slash
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
    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

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

        // Create user without FileSearch permission
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

        // Create user with FileSearch but not FileRoot permission
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

        // Create user with FileSearch permission
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
}
