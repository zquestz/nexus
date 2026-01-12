//! File reindex handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;

use super::{HandlerContext, err_not_logged_in, err_permission_denied};
use crate::db::Permission;

/// Handle a file reindex request
pub async fn handle_file_reindex<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("FileReindex request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileReindex"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("FileReindex"))
                .await;
        }
    };

    // Check file_reindex permission
    if !requesting_user.has_permission(Permission::FileReindex) {
        eprintln!(
            "FileReindex from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::FileReindexResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Trigger reindex (returns false if already reindexing)
    let started = ctx.file_index.trigger_reindex();

    if ctx.debug {
        if started {
            eprintln!(
                "FileReindex triggered by {} (user: {})",
                ctx.peer_addr, requesting_user.username
            );
        } else {
            eprintln!(
                "FileReindex already in progress, requested by {} (user: {})",
                ctx.peer_addr, requesting_user.username
            );
        }
    }

    // Always return success - if already reindexing, that's fine
    let response = ServerMessage::FileReindexResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_file_reindex_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_file_reindex(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_reindex_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user without FileReindex permission
        let session_id = login_user(&mut test_ctx, "user", "password", &[], false).await;

        let result = handle_file_reindex(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileReindexResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected FileReindexResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_reindex_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (has all permissions implicitly)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_file_reindex(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileReindexResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileReindexResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_file_reindex_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user with FileReindex permission
        let session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[Permission::FileReindex],
            false,
        )
        .await;

        let result = handle_file_reindex(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::FileReindexResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected FileReindexResponse, got: {:?}", response),
        }
    }
}
