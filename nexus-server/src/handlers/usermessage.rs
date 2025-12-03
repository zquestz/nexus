//! Handler for UserMessage command

use std::io;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, MessageError, UsernameError};

use super::{
    HandlerContext, err_authentication, err_cannot_message_self, err_chat_too_long, err_database,
    err_message_contains_newlines, err_message_empty, err_message_invalid_characters,
    err_not_logged_in, err_permission_denied, err_user_not_found, err_user_not_online,
    err_username_empty, err_username_invalid, err_username_too_long,
};
use crate::db::Permission;

/// Handle UserMessage command
pub async fn handle_usermessage(
    to_username: String,
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(session_id) = session_id else {
        eprintln!("UserMessage request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserMessage"))
            .await;
    };

    // Validate to_username format
    if let Err(e) = validators::validate_username(&to_username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Validate message content
    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => err_message_empty(ctx.locale),
            MessageError::TooLong => err_chat_too_long(ctx.locale, validators::MAX_MESSAGE_LENGTH),
            MessageError::ContainsNewlines => err_message_contains_newlines(ctx.locale),
            MessageError::InvalidCharacters => err_message_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserMessage"))
                .await;
        }
    };

    // Prevent self-messaging (cheap check before DB queries)
    let to_username_lower = to_username.to_lowercase();
    if to_username_lower == requesting_user_session.username.to_lowercase() {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_cannot_message_self(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Check UserMessage permission (use is_admin from UserManager to avoid DB lookup for admins)
    let has_permission = if requesting_user_session.is_admin {
        true
    } else {
        match ctx
            .db
            .users
            .has_permission(requesting_user_session.db_user_id, Permission::UserMessage)
            .await
        {
            Ok(has_perm) => has_perm,
            Err(e) => {
                eprintln!("Database error checking permissions: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("UserMessage"))
                    .await;
            }
        }
    };

    if !has_permission {
        eprintln!(
            "UserMessage from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user_session.username
        );
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Look up target user in database
    let target_user_db = match ctx.db.users.get_user_by_username(&to_username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &to_username)),
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserMessage"))
                .await;
        }
    };

    // Check if target user is online
    let target_sessions = ctx
        .user_manager
        .get_session_ids_for_user(&target_user_db.username)
        .await;

    if target_sessions.is_empty() {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_user_not_online(ctx.locale, &to_username)),
        };
        return ctx.send_message(&response).await;
    }

    // Send success response to sender
    let response = ServerMessage::UserMessageResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Broadcast message to all sessions of both sender and receiver
    let broadcast = ServerMessage::UserMessage {
        from_username: requesting_user_session.username.clone(),
        to_username: target_user_db.username.clone(),
        message,
    };

    // Send to all sender sessions
    ctx.user_manager
        .broadcast_to_username(&requesting_user_session.username, &broadcast, &ctx.db.users)
        .await;

    // Send to all receiver sessions
    ctx.user_manager
        .broadcast_to_username(&target_user_db.username, &broadcast, &ctx.db.users)
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_usermessage_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_usermessage(
            "alice".to_string(),
            "hello".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_usermessage_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user without UserMessage permission
        let _sender_id = login_user(&mut test_ctx, "sender", "pass123", &[], false).await;

        // Create target user with UserMessage permission
        let _target_id = login_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Try to send message without permission
        let result = handle_usermessage(
            "target".to_string(),
            "hello".to_string(),
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Check response
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().to_lowercase().contains("permission"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_empty_message() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_usermessage(
            "target".to_string(),
            "   ".to_string(),
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                assert!(error.unwrap().contains("empty"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_message_too_long() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let long_message = "x".repeat(validators::MAX_MESSAGE_LENGTH + 1);

        let result = handle_usermessage(
            "target".to_string(),
            long_message,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                assert!(error.unwrap().contains("too long"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_cannot_message_self() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_usermessage(
            "sender".to_string(),
            "hello".to_string(),
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                assert!(error.unwrap().to_lowercase().contains("yourself"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_target_not_found() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_usermessage(
            "nonexistent".to_string(),
            "hello".to_string(),
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                assert!(error.unwrap().contains("not found"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_target_not_online() {
        let mut test_ctx = create_test_context().await;

        // Create sender with permission (online)
        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Create target user account but DON'T login (offline)
        use crate::db::Permissions;
        test_ctx
            .db
            .users
            .create_user("target", "pass456", false, true, &Permissions::new())
            .await
            .unwrap();

        let result = handle_usermessage(
            "target".to_string(),
            "hello".to_string(),
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                assert!(error.unwrap().contains("not online"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_successful() {
        let mut test_ctx = create_test_context().await;

        // Create sender with permission (session_id 1)
        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Create target with permission (session_id 2)
        let _target_id = login_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Send message
        let result = handle_usermessage(
            "target".to_string(),
            "hello world".to_string(),
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Check sender gets success response
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_usermessage_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin sender (no explicit permission needed) (session_id 1)
        let _admin_id = login_user(&mut test_ctx, "admin", "pass123", &[], true).await;

        // Create target (session_id 2)
        let _target_id = login_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Admin sends message without explicit permission
        let result = handle_usermessage(
            "target".to_string(),
            "admin message".to_string(),
            Some(1), // admin's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Check admin gets success response
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse, got: {:?}", response),
        }
    }
}
