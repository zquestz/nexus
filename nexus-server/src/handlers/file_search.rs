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
use crate::files::resolve_user_area;

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

        if root {
            Ok((None, requesting_user.username))
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

            Ok((Some(relative_area), requesting_user.username))
        }
    };
    let (area_prefix, username) = match user_area_result {
        Ok(user_area) => user_area,
        Err(ServerMessage::Error { message, command }) => {
            return ctx
                .send_error_and_disconnect(&message, command.as_deref())
                .await;
        }
        Err(response) => return ctx.send_message(&response).await,
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
