//! Handler for UserMessage command

use super::{
    HandlerContext, err_authentication, err_chat_too_long, err_database, err_message_empty,
    err_not_logged_in, err_permission_denied, err_user_not_found, err_user_not_online,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Maximum message length
const MAX_MESSAGE_LENGTH: usize = 1024;

/// Handle UserMessage command
pub async fn handle_usermessage(
    to_username: String,
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Step 1: Verify authentication
    let Some(session_id) = session_id else {
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserMessage"))
            .await;
    };

    // Step 2: Validate message is not empty (cheap check first)
    if message.trim().is_empty() {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_message_empty(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Step 3: Validate message length (cheap check)
    if message.len() > MAX_MESSAGE_LENGTH {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_chat_too_long(ctx.locale, MAX_MESSAGE_LENGTH)),
        };
        return ctx.send_message(&response).await;
    }

    // Step 4: Get requesting user from session
    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserMessage"))
                .await;
        }
    };

    // Step 5: Prevent self-messaging (cheap check before DB queries)
    if to_username.to_lowercase() == requesting_user_session.username.to_lowercase() {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Step 6: Fetch requesting user account for permission check
    let requesting_user = match ctx
        .db
        .users
        .get_user_by_id(requesting_user_session.db_user_id)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserMessage"))
                .await;
        }
        Err(e) => {
            eprintln!("Database error getting requesting user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserMessage"))
                .await;
        }
    };

    // Step 7: Check UserMessage permission
    let has_permission = requesting_user.is_admin
        || match ctx
            .db
            .users
            .has_permission(requesting_user.id, Permission::UserMessage)
            .await
        {
            Ok(has_perm) => has_perm,
            Err(e) => {
                eprintln!("Database error checking permissions: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("UserMessage"))
                    .await;
            }
        };

    if !has_permission {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Step 8: Look up target user in database
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

    // Step 9: Check if target user is online
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

    // Step 10: Send success response to sender
    let response = ServerMessage::UserMessageResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Step 11: Broadcast message to all sessions of both sender and receiver
    let broadcast = ServerMessage::UserMessage {
        from_username: requesting_user.username.clone(),
        to_username: target_user_db.username.clone(),
        message: message.clone(),
    };

    // Send to all sender sessions
    ctx.user_manager
        .broadcast_to_username(&requesting_user.username, &broadcast, &ctx.db.users)
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
    use nexus_common::protocol::ServerMessage;

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

        let long_message = "x".repeat(1025);

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
                // We return "Permission denied" for self-messaging, so check for that
                assert!(error.unwrap().to_lowercase().contains("permission"));
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
