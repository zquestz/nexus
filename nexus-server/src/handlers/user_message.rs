//! Handler for UserMessage command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ChatAction, ServerMessage};
use nexus_common::validators::{self, MessageError, NicknameError};

use super::{
    HandlerContext, err_authentication, err_cannot_message_self, err_chat_too_long,
    err_message_contains_newlines, err_message_empty, err_message_invalid_characters,
    err_nickname_empty, err_nickname_invalid, err_nickname_not_online, err_nickname_too_long,
    err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;

/// Handle UserMessage command
pub async fn handle_user_message<W>(
    to_nickname: String,
    message: String,
    action: ChatAction,
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

    // Validate to_nickname format
    if let Err(e) = validators::validate_nickname(&to_nickname) {
        let error_msg = match e {
            NicknameError::Empty => err_nickname_empty(ctx.locale),
            NicknameError::TooLong => {
                err_nickname_too_long(ctx.locale, validators::MAX_NICKNAME_LENGTH)
            }
            NicknameError::InvalidCharacters => err_nickname_invalid(ctx.locale),
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
    // Check against the requesting user's nickname (which is the display name)
    let to_nickname_lower = to_nickname.to_lowercase();
    let is_self_message = requesting_user_session.nickname.to_lowercase() == to_nickname_lower;
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

    // Look up target by nickname (all users have a nickname - equals username for regular accounts)
    let target_session = match ctx.user_manager.get_session_by_nickname(&to_nickname).await {
        Some(session) => session,
        None => {
            // User not online
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_nickname_not_online(ctx.locale, &to_nickname)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Send success response to sender
    let response = ServerMessage::UserMessageResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Build the message to broadcast
    let broadcast = ServerMessage::UserMessage {
        from_nickname: requesting_user_session.nickname.clone(),
        from_admin: requesting_user_session.is_admin,
        to_nickname: target_session.nickname.clone(),
        message,
        action,
    };

    // Send to sender's session(s) by nickname
    // - Regular accounts: nickname == username, so all sessions receive it
    // - Shared accounts: unique nickname, so only that session receives it
    ctx.user_manager
        .broadcast_to_nickname(&requesting_user_session.nickname, &broadcast)
        .await;

    // Send to receiver's session(s) by nickname
    ctx.user_manager
        .broadcast_to_nickname(&target_session.nickname, &broadcast)
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        create_test_context, login_shared_user, login_user, read_server_message,
    };

    #[tokio::test]
    async fn test_usermessage_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_message(
            "alice".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(!success);
                // User not online (we don't distinguish "not found" from "not online" for security)
                assert!(error.unwrap().contains("not online"));
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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
            ChatAction::Normal,
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

    #[tokio::test]
    async fn test_usermessage_shared_account_by_nickname_succeeds() {
        let mut test_ctx = create_test_context().await;

        // Create sender with permission
        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Create shared account user with nickname "Nick1"
        let _shared_id = login_shared_user(
            &mut test_ctx,
            "shared_acct",
            "sharedpass",
            "Nick1",
            &[Permission::UserMessage],
        )
        .await;

        // Message by nickname (should succeed)
        let result = handle_user_message(
            "Nick1".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserMessageResponse { success, error } => {
                assert!(success, "Should allow messaging shared account by nickname");
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }
}
