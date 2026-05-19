//! Broadcast message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, MessageError};

use super::{
    HandlerContext, err_broadcast_too_long, err_message_contains_newlines, err_message_empty,
    err_message_invalid_characters, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    HANDLER_USER_BROADCAST, LOG_USER_BROADCAST_NOT_LOGGED_IN, LOG_USER_BROADCAST_PERMISSION_DENIED,
};
use crate::db::Permission;

/// Handle a broadcast request from the client
///
/// Broadcasts a message to all connected users including the sender.
/// Also sends a UserBroadcastResponse to the sender indicating success or failure.
pub async fn handle_user_broadcast<W>(
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_BROADCAST_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_BROADCAST))
            .await;
    };

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_USER_BROADCAST),
                )
                .await;
        }
    };

    // Check permission (uses cached permissions, admin bypass built-in)
    if !user.has_permission(Permission::UserBroadcast) {
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_USER_BROADCAST_PERMISSION_DENIED);
        let response = ServerMessage::UserBroadcastResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Content rules (empty / over the char cap / newlines / invalid chars) —
    // the framing layer already rejected oversized payloads.
    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => err_message_empty(ctx.locale),
            MessageError::TooLong => {
                err_broadcast_too_long(ctx.locale, validators::MAX_MESSAGE_LENGTH)
            }
            MessageError::ContainsNewlines => err_message_contains_newlines(ctx.locale),
            MessageError::InvalidCharacters => err_message_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::UserBroadcastResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Send broadcast to all users
    ctx.user_manager
        .broadcast(ServerMessage::ServerBroadcast {
            session_id: id,
            username: user.username.clone(),
            message,
        })
        .await;

    // Send success response to the sender
    ctx.send_message(&ServerMessage::UserBroadcastResponse {
        success: true,
        error: None,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_broadcast_requires_login() {
        let mut test_ctx = create_test_context().await;
        let session_id = None; // Not logged in

        // Try to broadcast without login
        let result = handle_user_broadcast(
            "Hello everyone".to_string(),
            session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Broadcast should require login");
    }

    #[tokio::test]
    async fn test_broadcast_message_too_long() {
        let mut test_ctx = create_test_context().await;
        let session_id = Some(1); // Logged in

        // Create message over MAX_MESSAGE_LENGTH characters
        let long_message = "a".repeat(validators::MAX_MESSAGE_LENGTH + 1);

        // Try to send too-long message
        let result =
            handle_user_broadcast(long_message, session_id, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(
            result.is_err(),
            "Message over MAX_MESSAGE_LENGTH should be rejected"
        );
    }

    #[tokio::test]
    async fn test_broadcast_message_at_limit() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH broadcast permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserBroadcast],
            false,
        )
        .await;

        // Create message at exactly MAX_MESSAGE_LENGTH characters
        let max_message = "a".repeat(validators::MAX_MESSAGE_LENGTH);

        // Should succeed
        let result = handle_user_broadcast(
            max_message,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_ok(),
            "Message at MAX_MESSAGE_LENGTH should be accepted"
        );
    }

    #[tokio::test]
    async fn test_broadcast_empty_message() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH broadcast permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserBroadcast],
            false,
        )
        .await;

        // Try to send empty message
        let result = handle_user_broadcast(
            "".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should send typed error response (no disconnect)
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserBroadcastResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected UserBroadcastResponse for empty message, got {other:?}"),
        }

        // Try to send whitespace-only message
        let result = handle_user_broadcast(
            "   ".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should send typed error response (no disconnect)
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserBroadcastResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => {
                panic!("Expected UserBroadcastResponse for whitespace message, got {other:?}")
            }
        }
    }

    #[tokio::test]
    async fn test_broadcast_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT broadcast permission (non-admin)
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Try to broadcast without permission
        let result = handle_user_broadcast(
            "Important announcement!".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed (send error but not disconnect)
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_broadcast_successful() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH broadcast permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserBroadcast],
            false,
        )
        .await;

        // Send valid broadcast message
        let result = handle_user_broadcast(
            "Important announcement!".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Valid broadcast message should succeed");
    }

    #[tokio::test]
    async fn test_broadcast_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Use a session ID that doesn't exist in UserManager
        let invalid_session_id = Some(999);

        // Try to broadcast with invalid session
        let result = handle_user_broadcast(
            "Hello everyone".to_string(),
            invalid_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail (ERR_AUTHENTICATION)
        assert!(
            result.is_err(),
            "Broadcast with invalid session should be rejected"
        );
    }

    #[tokio::test]
    async fn test_broadcast_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user WITHOUT explicit UserBroadcast permission
        // Admins should have all permissions automatically
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin should be able to broadcast
        let result = handle_user_broadcast(
            "Admin announcement!".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Admin should be able to broadcast without explicit permission"
        );
    }
}
