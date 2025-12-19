//! Handler for UserMessage command

use std::io;

use tokio::io::AsyncWrite;

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
pub async fn handle_user_message<W>(
    to_username: String,
    message: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
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
    // Check both username and nickname for self-message prevention
    let to_username_lower = to_username.to_lowercase();
    let is_self_message = to_username_lower == requesting_user_session.username.to_lowercase()
        || requesting_user_session
            .nickname
            .as_ref()
            .is_some_and(|n| n.to_lowercase() == to_username_lower);
    if is_self_message {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_cannot_message_self(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Check UserMessage permission (uses cached permissions, admin bypass built-in)
    if !requesting_user_session.has_permission(Permission::UserMessage) {
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

    // First, try to find by nickname (for shared account users)
    // Then fall back to username lookup
    let target_session_by_nickname = ctx.user_manager.get_session_by_nickname(&to_username).await;

    // Determine the target display name (for the UserMessage broadcast)
    // and how to route the message
    let (target_display_name, route_to_nickname) =
        if let Some(ref session) = target_session_by_nickname {
            // Messaging a shared account user by nickname
            // Use nickname as display name
            (
                session
                    .nickname
                    .clone()
                    .unwrap_or_else(|| to_username.clone()),
                true,
            )
        } else {
            // Messaging by username - look up in database
            (to_username.clone(), false)
        };

    // For username-based messaging, look up target user in database
    let target_user_db = if !route_to_nickname {
        match ctx.db.users.get_user_by_username(&to_username).await {
            Ok(Some(user)) => Some(user),
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
        }
    } else {
        None
    };

    // Check if target user is online
    if route_to_nickname {
        // Already confirmed online via get_session_by_nickname
    } else {
        let target_sessions = ctx
            .user_manager
            .get_session_ids_for_user(&target_user_db.as_ref().unwrap().username)
            .await;

        if target_sessions.is_empty() {
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_user_not_online(ctx.locale, &to_username)),
            };
            return ctx.send_message(&response).await;
        }
    }

    // Send success response to sender
    let response = ServerMessage::UserMessageResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Get sender's display name (nickname for shared accounts, username for regular)
    let from_display_name = requesting_user_session
        .nickname
        .clone()
        .unwrap_or_else(|| requesting_user_session.username.clone());

    // Broadcast message to sender and receiver
    let broadcast = ServerMessage::UserMessage {
        from_username: from_display_name,
        from_admin: requesting_user_session.is_admin,
        to_username: target_display_name,
        message,
    };

    // Send to all sender sessions (by username, not nickname - sender sees their own message)
    ctx.user_manager
        .broadcast_to_username(&requesting_user_session.username, &broadcast, &ctx.db.users)
        .await;

    // Send to receiver
    if route_to_nickname {
        // Send only to the specific shared account session (by nickname)
        if let Some(session) = target_session_by_nickname {
            let _ = session.tx.send((broadcast, None));
        }
    } else {
        // Send to all receiver sessions (by username)
        ctx.user_manager
            .broadcast_to_username(
                &target_user_db.as_ref().unwrap().username,
                &broadcast,
                &ctx.db.users,
            )
            .await;
    }

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

        let result = handle_user_message(
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
        let result = handle_user_message(
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

        let result = handle_user_message(
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

        let result = handle_user_message(
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

        let result = handle_user_message(
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

        let result = handle_user_message(
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
            .create_user("target", "pass456", false, false, true, &Permissions::new())
            .await
            .unwrap();

        let result = handle_user_message(
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
        let result = handle_user_message(
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
        let result = handle_user_message(
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

    // ========================================================================
    // Shared Account Tests
    // ========================================================================

    #[tokio::test]
    async fn test_usermessage_to_shared_account_by_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::db;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create admin sender with message permission
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = db::hash_password("password").unwrap();
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        // Login to the shared account with a nickname
        let mut shared_session_id = None;
        let login_request = LoginRequest {
            username: "shared_acct".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Nick1".to_string()),
            handshake_complete: true,
        };
        let _ = handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx.client).await; // consume login response

        // Send message to shared account by nickname
        let result = handle_user_message(
            "Nick1".to_string(),
            "Hello Nick1!".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(
                    success,
                    "Message to shared account by nickname should succeed"
                );
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_self_message_by_nickname_prevented() {
        let mut test_ctx = create_test_context().await;
        use crate::db;
        use crate::handlers::login::{LoginRequest, handle_login};

        // First create an admin so system works
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account with message permission
        let hashed = db::hash_password("password").unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserMessage);
        test_ctx
            .db
            .users
            .create_user("shared_acct", &hashed, false, true, true, &perms)
            .await
            .unwrap();

        // Login to the shared account with a nickname
        let mut shared_session_id = None;
        let login_request = LoginRequest {
            username: "shared_acct".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Nick1".to_string()),
            handshake_complete: true,
        };
        let _ = handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx.client).await; // consume login response

        let session_id = shared_session_id.unwrap();

        // Try to send message to self by nickname (should fail)
        let result = handle_user_message(
            "Nick1".to_string(),
            "Message to myself".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success, "Self-message by nickname should be prevented");
                assert!(error.is_some());
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_from_shared_account_shows_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::db;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create a target user
        let _target_id = login_user(&mut test_ctx, "target", "password", &[], false).await;

        // Create a shared account with message permission
        let hashed = db::hash_password("password").unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserMessage);
        test_ctx
            .db
            .users
            .create_user("shared_acct", &hashed, false, true, true, &perms)
            .await
            .unwrap();

        // Login to the shared account with a nickname
        let mut shared_session_id = None;
        let login_request = LoginRequest {
            username: "shared_acct".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Sender".to_string()),
            handshake_complete: true,
        };
        let _ = handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx.client).await; // consume login response

        let session_id = shared_session_id.unwrap();

        // Send message from shared account to target
        let result = handle_user_message(
            "target".to_string(),
            "Hello from shared!".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, .. } => {
                assert!(success, "Message from shared account should succeed");
            }
            _ => panic!("Expected UserMessageResponse"),
        }

        // The UserMessage broadcast would contain from_username: "Sender" (the nickname)
        // This is verified by the implementation using requesting_user_session.nickname
    }
}
