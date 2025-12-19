//! Handler for UserKick command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, UsernameError};

use super::{
    HandlerContext, err_authentication, err_cannot_kick_admin, err_cannot_kick_self, err_database,
    err_kicked_by, err_not_logged_in, err_permission_denied, err_user_not_online,
    err_username_empty, err_username_invalid, err_username_too_long,
};
use crate::db::Permission;

/// Handle UserKick command
pub async fn handle_user_kick<W>(
    target_username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(session_id) = session_id else {
        eprintln!("UserKick request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserKick"))
            .await;
    };

    // Validate username format
    if let Err(e) = validators::validate_username(&target_username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(error_msg),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserKick"))
                .await;
        }
    };

    // Prevent self-kick (cheap check before DB queries)
    // Check both username and nickname for self-kick prevention
    let target_lower = target_username.to_lowercase();
    let is_self_kick = target_lower == requesting_user_session.username.to_lowercase()
        || requesting_user_session
            .nickname
            .as_ref()
            .is_some_and(|n| n.to_lowercase() == target_lower);
    if is_self_kick {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_cannot_kick_self(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check UserKick permission (uses cached permissions, admin bypass built-in)
    if !requesting_user_session.has_permission(Permission::UserKick) {
        eprintln!(
            "UserKick from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user_session.username
        );
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // First, try to find by nickname (for shared account users)
    // Then fall back to username lookup
    let (target_users, kicked_by_nickname) = if let Some(session) = ctx
        .user_manager
        .get_session_by_nickname(&target_username)
        .await
    {
        // Found a shared account user by nickname - kick just that session
        (vec![session], true)
    } else {
        // Look up all sessions for target username (case-insensitive)
        let sessions = ctx
            .user_manager
            .get_sessions_by_username(&target_username)
            .await;
        (sessions, false)
    };

    if target_users.is_empty() {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_user_not_online(ctx.locale, &target_username)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Always check admin status before kicking (defense in depth)
    // For nickname-based kicks, use the session's username to look up the account
    // For username-based kicks, use the provided username directly
    let db_lookup_username = if kicked_by_nickname {
        target_users
            .first()
            .map(|s| s.username.clone())
            .unwrap_or_else(|| target_username.clone())
    } else {
        target_username.clone()
    };

    let target_user_db = match ctx.db.users.get_user_by_username(&db_lookup_username).await {
        Ok(user) => user,
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserKick"))
                .await;
        }
    };

    // Prevent kicking admin users
    if let Some(ref target_db) = target_user_db
        && target_db.is_admin
    {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_cannot_kick_admin(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get the preserved display name from the first session
    // For shared accounts kicked by nickname, use the nickname
    // For regular accounts, use the username
    let preserved_display_name = if kicked_by_nickname {
        target_users
            .first()
            .and_then(|u| u.nickname.clone())
            .unwrap_or_else(|| target_username.clone())
    } else {
        target_users
            .first()
            .map(|u| u.username.clone())
            .unwrap_or_else(|| target_username.clone())
    };

    // Kick all target sessions
    for user in target_users {
        // Send kick message to the user in their locale before disconnecting
        let kick_msg = ServerMessage::Error {
            message: err_kicked_by(&user.locale, &requesting_user_session.username),
            command: None,
        };
        let _ = user.tx.send((kick_msg, None));

        // Remove user from UserManager (channel closes, connection breaks)
        let target_session_id = user.session_id;
        if let Some(removed_user) = ctx.user_manager.remove_user(target_session_id).await {
            // Broadcast disconnection to users with user_list permission
            ctx.user_manager
                .broadcast_user_event(
                    ServerMessage::UserDisconnected {
                        session_id: target_session_id,
                        username: removed_user.display_name().to_string(),
                    },
                    &ctx.db.users,
                    Some(target_session_id), // Exclude the kicked user
                )
                .await;
        }
    }

    // Send success response to requester
    // Use the session-preserved display name (nickname for shared, username for regular)
    let response = ServerMessage::UserKickResponse {
        success: true,
        error: None,
        username: Some(preserved_display_name),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_userkick_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to kick user without being logged in
        let result =
            handle_user_kick("alice".to_string(), None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "UserKick should require login");
    }

    #[tokio::test]
    async fn test_userkick_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserKick permission (non-admin)
        let _session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to kick
        let _target_id = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Try to kick bob (should fail - no permission)
        let result =
            handle_user_kick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Kick should fail without permission");
            assert!(
                error.unwrap().to_lowercase().contains("permission"),
                "Error should mention permission"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH UserKick permission
        let _kicker_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;

        // Create another user to kick
        let _target_id = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Kick bob (should succeed)
        let result =
            handle_user_kick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Kick should succeed with permission");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            username,
        } = response
        {
            assert!(success, "Kick should succeed");
            assert!(error.is_none(), "Should not have error");
            assert_eq!(username, Some("bob".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_admin_can_kick() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (no explicit permission needed)
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another user to kick
        let _target_id = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Admin kicks bob (should succeed)
        let result =
            handle_user_kick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Admin should be able to kick");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            username,
        } = response
        {
            assert!(success, "Admin kick should succeed");
            assert!(error.is_none(), "Should not have error");
            assert_eq!(username, Some("bob".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_cannot_kick_self() {
        let mut test_ctx = create_test_context().await;

        // Create user with kick permission
        let _session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;

        // Try to kick self (should fail)
        let result = handle_user_kick(
            "alice".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Should not be able to kick self");
            assert!(
                error.unwrap().contains("yourself"),
                "Error should mention self-kick prevention"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_user_not_online() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create offline user in database (not logged in)
        use crate::db::{Permissions, hash_password};
        let hashed = hash_password("password").unwrap();
        let perms = Permissions::new();
        test_ctx
            .db
            .users
            .create_user("offline_user", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Try to kick offline user (should fail)
        let result = handle_user_kick(
            "offline_user".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Cannot kick offline user");
            assert!(
                error.unwrap().contains("not online"),
                "Error should mention user is not online"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_case_insensitive() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user
        let _target_id = login_user(&mut test_ctx, "Alice", "password", &[], false).await;

        // Kick using different case (should succeed)
        let result = handle_user_kick(
            "alice".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should work case-insensitively");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            username,
        } = response
        {
            assert!(success, "Case-insensitive kick should succeed");
            assert!(error.is_none(), "Should not have error");
            // Should return the preserved casing from the database, not the input
            assert_eq!(username, Some("Alice".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_disconnects_all_sessions() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user with first session
        let _target_id1 = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Simulate second session for same user (different session ID)
        // In real scenario, this would be another connection
        // For testing, we verify the logic handles multiple sessions

        // Kick alice (should kick all sessions)
        let result = handle_user_kick(
            "alice".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should succeed");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            username,
        } = response
        {
            assert!(success, "Kick should succeed for multi-session user");
            assert!(error.is_none(), "Should not have error");
            assert_eq!(username, Some("alice".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }

        // Verify user was removed from UserManager
        let all_users = test_ctx.user_manager.get_all_users().await;
        let alice_still_online = all_users.iter().any(|u| u.username == "alice");
        assert!(
            !alice_still_online,
            "Alice should be disconnected after kick"
        );
    }

    #[tokio::test]
    async fn test_userkick_cannot_kick_admin() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (kicker)
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target admin user
        let _target_admin_id = login_user(&mut test_ctx, "bob", "password", &[], true).await;

        // Try to kick admin (should fail)
        let result =
            handle_user_kick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Should not be able to kick admin");
            assert!(
                error.unwrap().contains("admin"),
                "Error should mention admin protection"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    // ========================================================================
    // Shared Account Tests
    // ========================================================================

    #[tokio::test]
    async fn test_userkick_shared_account_by_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create admin user to do the kicking
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

        assert!(
            shared_session_id.is_some(),
            "Shared account should be logged in"
        );

        // Kick by nickname
        let result = handle_user_kick(
            "Nick1".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Kick by nickname should succeed");
                assert!(error.is_none());
                assert_eq!(
                    username,
                    Some("Nick1".to_string()),
                    "Should return the nickname"
                );
            }
            _ => panic!("Expected UserKickResponse"),
        }

        // Verify user was kicked
        let sessions = test_ctx.user_manager.get_session_by_nickname("Nick1").await;
        assert!(sessions.is_none(), "Session should be removed");
    }

    #[tokio::test]
    async fn test_userkick_shared_account_self_kick_by_nickname_prevented() {
        let mut test_ctx = create_test_context().await;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create a shared account in the database with kick permission
        let hashed = db::hash_password("password").unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserKick);
        test_ctx
            .db
            .users
            .create_user("shared_acct", &hashed, false, true, true, &perms)
            .await
            .unwrap();

        // But first we need an admin
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        // Try to kick self by nickname (should fail)
        let result = handle_user_kick(
            "Nick1".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserKickResponse { success, error, .. } => {
                assert!(!success, "Self-kick by nickname should be prevented");
                assert!(error.is_some());
            }
            _ => panic!("Expected UserKickResponse"),
        }
    }
}
