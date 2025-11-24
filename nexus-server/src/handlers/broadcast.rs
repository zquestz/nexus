//! Broadcast message handler
//! Handler for UserBroadcast command

use super::{
    ERR_AUTHENTICATION, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle a broadcast request from the client
///
/// Broadcasts a message to all connected users including the sender.
/// Also sends a UserBroadcastReply to the sender indicating success or failure.
pub async fn handle_user_broadcast(
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Check message is not empty
    if message.trim().is_empty() {
        eprintln!("UserBroadcast from {} with empty message", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Message cannot be empty", Some("UserBroadcast"))
            .await;
    }

    // Check message length limit (1024 characters)
    if message.len() > 1024 {
        eprintln!(
            "UserBroadcast from {} exceeds length limit: {} chars",
            ctx.peer_addr,
            message.len()
        );
        return ctx
            .send_error_and_disconnect(
                "Message too long (max 1024 characters)",
                Some("UserBroadcast"),
            )
            .await;
    }

    let id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserBroadcast from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserBroadcast"))
                .await;
        }
    };

    // Get the user
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("UserBroadcast"))
                .await;
        }
    };

    // Check UserBroadcast permission
    let has_perm = match ctx
        .user_db
        .has_permission(user.db_user_id, Permission::UserBroadcast)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserBroadcast permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserBroadcast"))
                .await;
        }
    };

    if !has_perm {
        eprintln!(
            "UserBroadcast from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserBroadcast"))
            .await;
    }

    // Broadcast to all users INCLUDING the sender
    ctx.user_manager
        .broadcast(ServerMessage::ServerBroadcast {
            session_id: id,
            username: user.username.clone(),
            message: message.clone(),
        })
        .await;

    // Send success reply to the sender
    ctx.send_message(&ServerMessage::UserBroadcastReply {
        success: true,
        error: None,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user};

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

        // Create message over 1024 characters
        let long_message = "a".repeat(1025);

        // Try to send too-long message
        let result =
            handle_user_broadcast(long_message, session_id, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(
            result.is_err(),
            "Message over 1024 chars should be rejected"
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

        // Create message at exactly 1024 characters
        let max_message = "a".repeat(1024);

        // Should succeed
        let result = handle_user_broadcast(
            max_message,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "Message at 1024 chars should be accepted");
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

        // Should fail
        assert!(result.is_err(), "Empty message should be rejected");

        // Try to send whitespace-only message
        let result = handle_user_broadcast(
            "   ".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Whitespace-only message should be rejected");
    }

    #[tokio::test]
    async fn test_broadcast_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT broadcast permission (non-admin)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
        )
        .await;

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
}
