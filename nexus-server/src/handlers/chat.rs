//! Chat message handler
//! Handler for ChatSend command

use std::io;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, MessageError};

use super::{
    HandlerContext, err_authentication, err_chat_feature_not_enabled, err_chat_too_long,
    err_message_contains_newlines, err_message_empty, err_message_invalid_characters,
    err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

/// Handle a chat send request from the client
pub async fn handle_chat_send(
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(id) = session_id else {
        eprintln!("ChatSend from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("ChatSend"))
            .await;
    };

    // Validate message content
    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => err_message_empty(ctx.locale),
            MessageError::TooLong => err_chat_too_long(ctx.locale, validators::MAX_MESSAGE_LENGTH),
            MessageError::ContainsNewlines => err_message_contains_newlines(ctx.locale),
            MessageError::InvalidCharacters => err_message_invalid_characters(ctx.locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some("ChatSend"))
            .await;
    }

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("ChatSend"))
                .await;
        }
    };

    // Check chat feature
    if !user.has_feature(FEATURE_CHAT) {
        return ctx
            .send_error_and_disconnect(&err_chat_feature_not_enabled(ctx.locale), Some("ChatSend"))
            .await;
    }

    // Check permission (uses cached permissions, admin bypass built-in)
    if !user.has_permission(Permission::ChatSend) {
        eprintln!(
            "ChatSend from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("ChatSend"))
            .await;
    }

    // Broadcast to all users with chat feature and ChatReceive permission
    ctx.user_manager
        .broadcast_to_feature(
            FEATURE_CHAT,
            ServerMessage::ChatMessage {
                session_id: id,
                username: user.username.clone(),
                message,
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

        // Create message over MAX_MESSAGE_LENGTH characters
        let long_message = "a".repeat(validators::MAX_MESSAGE_LENGTH + 1);

        // Try to send too-long message
        let result =
            handle_chat_send(long_message, session_id, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(
            result.is_err(),
            "Message over MAX_MESSAGE_LENGTH should be rejected"
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
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Create message at exactly MAX_MESSAGE_LENGTH characters
        let max_message = "a".repeat(validators::MAX_MESSAGE_LENGTH);

        // Should succeed
        let result = handle_chat_send(
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
    async fn test_chat_empty_message() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission and feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
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
    async fn test_chat_message_with_newlines() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission and feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Try to send message with \n
        let result = handle_chat_send(
            "Hello\nWorld".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Message with newline should be rejected");

        // Try to send message with \r
        let result = handle_chat_send(
            "Hello\rWorld".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(
            result.is_err(),
            "Message with carriage return should be rejected"
        );

        // Try to send message with \r\n
        let result = handle_chat_send(
            "Hello\r\nWorld".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Message with CRLF should be rejected");
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
            vec![FEATURE_CHAT.to_string()],
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
            vec![FEATURE_CHAT.to_string()],
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
            vec![FEATURE_CHAT.to_string()],
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
