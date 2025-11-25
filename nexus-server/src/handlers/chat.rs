//! Chat message handler
//! Handler for ChatSend command

use super::{
    ERR_AUTHENTICATION, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Maximum length for chat messages (in characters)
const MAX_CHAT_LENGTH: usize = 1024;

/// Handle a chat send request from the client
pub async fn handle_chat_send(
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Validate message content
    if message.trim().is_empty() {
        eprintln!("ChatSend from {} with empty message", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Message cannot be empty", Some("ChatSend"))
            .await;
    }

    if message.len() > MAX_CHAT_LENGTH {
        eprintln!(
            "ChatSend from {} exceeds length limit: {} chars",
            ctx.peer_addr,
            message.len()
        );
        return ctx
            .send_error_and_disconnect(
                &format!("Message too long (max {} characters)", MAX_CHAT_LENGTH),
                Some("ChatSend"),
            )
            .await;
    }

    // Verify user is logged in
    let id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("ChatSend from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("ChatSend"))
                .await;
        }
    };

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("ChatSend"))
                .await;
        }
    };

    // Check chat feature
    if !user.has_feature("chat") {
        eprintln!(
            "ChatSend from {} without chat feature enabled",
            ctx.peer_addr
        );
        return ctx
            .send_error_and_disconnect("Chat feature not enabled", Some("ChatSend"))
            .await;
    }

    // Check permission
    let has_perm = match ctx
        .db.users
        .has_permission(user.db_user_id, Permission::ChatSend)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("ChatSend permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("ChatSend"))
                .await;
        }
    };

    if !has_perm {
        eprintln!(
            "ChatSend from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("ChatSend"))
            .await;
    }

    // Broadcast to all users with chat feature and ChatReceive permission
    ctx.user_manager
        .broadcast_to_feature(
            "chat",
            ServerMessage::ChatMessage {
                session_id: id,
                username: user.username.clone(),
                message: message.clone(),
            },
            &ctx.db.users,
            Permission::ChatReceive,
        )
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user_with_features};

    #[tokio::test]
    async fn test_chat_requires_login() {
        let mut test_ctx = create_test_context().await;
        let session_id = None; // Not logged in

        // Try to send chat without login
        let result = handle_chat_send(
            "Hello".to_string(),
            session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Chat should require login");
    }

    #[tokio::test]
    async fn test_chat_message_too_long() {
        let mut test_ctx = create_test_context().await;
        let session_id = Some(1); // Fake session (length check happens first)

        // Create message over MAX_CHAT_LENGTH characters
        let long_message = "a".repeat(MAX_CHAT_LENGTH + 1);

        // Try to send too-long message
        let result =
            handle_chat_send(long_message, session_id, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(
            result.is_err(),
            "Message over MAX_CHAT_LENGTH should be rejected"
        );
    }

    #[tokio::test]
    async fn test_chat_message_at_limit() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission and feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec!["chat".to_string()],
        )
        .await;

        // Create message at exactly MAX_CHAT_LENGTH characters
        let max_message = "a".repeat(MAX_CHAT_LENGTH);

        // Should succeed
        let result = handle_chat_send(
            max_message,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_ok(),
            "Message at MAX_CHAT_LENGTH should be accepted"
        );
    }

    #[tokio::test]
    async fn test_chat_empty_message() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission and feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec!["chat".to_string()],
        )
        .await;

        // Try to send empty message
        let result = handle_chat_send(
            "".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Empty message should be rejected");

        // Try to send whitespace-only message
        let result = handle_chat_send(
            "   ".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(
            result.is_err(),
            "Whitespace-only message should be rejected"
        );
    }

    #[tokio::test]
    async fn test_chat_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT chat permission but WITH chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec!["chat".to_string()],
        )
        .await;

        // Try to send chat without permission
        let result = handle_chat_send(
            "Hello".to_string(),
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
    async fn test_chat_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH chat permission but WITHOUT chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![], // No chat feature
        )
        .await;

        // Try to send chat without chat feature
        let result = handle_chat_send(
            "Hello".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Chat should require chat feature");
    }

    #[tokio::test]
    async fn test_chat_successful() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission and feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec!["chat".to_string()],
        )
        .await;

        // Send valid chat message
        let result = handle_chat_send(
            "Hello, world!".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Valid chat message should succeed");
    }

    #[tokio::test]
    async fn test_chat_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Use a session ID that doesn't exist in UserManager
        let invalid_session_id = Some(999);

        // Try to send chat with invalid session
        let result = handle_chat_send(
            "Hello".to_string(),
            invalid_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail (ERR_AUTHENTICATION)
        assert!(
            result.is_err(),
            "Chat with invalid session should be rejected"
        );
    }

    #[tokio::test]
    async fn test_chat_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user WITHOUT explicit ChatSend permission
        // Admins should have all permissions automatically
        let session_id = login_user_with_features(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            vec!["chat".to_string()],
        )
        .await;

        // Admin should be able to send chat
        let result = handle_chat_send(
            "Admin message!".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Admin should be able to chat without explicit permission"
        );
    }
}