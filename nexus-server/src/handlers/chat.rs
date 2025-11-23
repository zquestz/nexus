//! Chat message handler
//! Handler for ChatSend command

use super::{
    ERR_AUTHENTICATION, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle a chat send request from the client
pub async fn handle_chat_send(
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Check message is not empty
    if message.trim().is_empty() {
        eprintln!("ChatSend from {} with empty message", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Message cannot be empty", Some("ChatSend"))
            .await;
    }

    // Check message length limit (1024 characters)
    if message.len() > 1024 {
        eprintln!(
            "ChatSend from {} exceeds length limit: {} chars",
            ctx.peer_addr,
            message.len()
        );
        return ctx
            .send_error_and_disconnect("Message too long (max 1024 characters)", Some("ChatSend"))
            .await;
    }

    let id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("ChatSend from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("ChatSend"))
                .await;
        }
    };

    // Get the user and check permissions
    let user = match ctx.user_manager.get_user(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("ChatSend"))
                .await;
        }
    };

    if !user.has_feature("chat") {
        eprintln!(
            "ChatSend from {} without chat feature enabled",
            ctx.peer_addr
        );
        return ctx
            .send_error_and_disconnect("Chat feature not enabled", Some("ChatSend"))
            .await;
    }

    let has_perm = match ctx
        .user_db
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
        eprintln!("ChatSend from {} without permission", ctx.peer_addr);
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("ChatSend"))
            .await;
    }

    // Broadcast to all users with chat feature AND ChatReceive permission
    ctx.user_manager
        .broadcast_to_feature(
            "chat",
            ServerMessage::ChatMessage {
                session_id: id,
                username: user.username.clone(),
                message: message.clone(),
            },
            ctx.user_db,
            Permission::ChatReceive,
        )
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::create_test_context;

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
        let session_id = Some(1); // Logged in

        // Create message over 1024 characters
        let long_message = "a".repeat(1025);

        // Try to send too-long message
        let result =
            handle_chat_send(long_message, session_id, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(
            result.is_err(),
            "Message over 1024 chars should be rejected"
        );
    }

    #[tokio::test]
    async fn test_chat_message_at_limit() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission and feature
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::ChatSend);
            set
        };
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &perms)
            .await
            .unwrap();

        // Add user to UserManager with chat feature
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
                vec!["chat".to_string()],
            )
            .await;

        // Create message at exactly 1024 characters
        let max_message = "a".repeat(1024);

        // Should succeed
        let result = handle_chat_send(
            max_message,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "Message at 1024 chars should be accepted");
    }

    #[tokio::test]
    async fn test_chat_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT chat permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &db::Permissions::new())
            .await
            .unwrap();

        // Add user to UserManager with chat feature
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
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
        assert!(result.is_ok(), "Should send error message but not disconnect");
    }

    #[tokio::test]
    async fn test_chat_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH chat permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::ChatSend);
            set
        };
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &perms)
            .await
            .unwrap();

        // Add user to UserManager WITHOUT chat feature
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
                vec![], // No features
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
    async fn test_chat_empty_message() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::ChatSend);
            set
        };
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &perms)
            .await
            .unwrap();

        // Add user to UserManager with chat feature
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
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
        assert!(result.is_err(), "Whitespace-only message should be rejected");
    }

    #[tokio::test]
    async fn test_chat_successful() {
        let mut test_ctx = create_test_context().await;

        // Create user with chat permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::ChatSend);
            set
        };
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &perms)
            .await
            .unwrap();

        // Add user to UserManager with chat feature
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
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
}
