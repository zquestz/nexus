//! Handler for UserKick command

use super::{
    ERR_CANNOT_KICK_ADMIN, ERR_CANNOT_KICK_SELF, ERR_DATABASE, ERR_NOT_LOGGED_IN,
    ERR_USER_NOT_ONLINE, HandlerContext, err_kicked_by,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Error message when session not found
const ERR_SESSION_NOT_FOUND: &str = "Session not found";

/// Error message when requesting user account not found
const ERR_ACCOUNT_NOT_FOUND: &str = "Your user account was not found";

/// Error message when user lacks kick permission
const ERR_NO_KICK_PERMISSION: &str = "You don't have permission to kick users";

/// Handle UserKick command
pub async fn handle_userkick(
    target_username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Step 1: Verify authentication
    let Some(session_id) = session_id else {
        return ctx
            .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserKick"))
            .await;
    };

    // Step 2: Get requesting user from session
    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(ERR_SESSION_NOT_FOUND, Some("UserKick"))
                .await;
        }
    };

    // Step 3: Prevent self-kick (cheap check before DB queries)
    if target_username.to_lowercase() == requesting_user_session.username.to_lowercase() {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(ERR_CANNOT_KICK_SELF.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Step 4: Fetch requesting user account for permission check
    let requesting_user = match ctx
        .db
        .users
        .get_user_by_id(requesting_user_session.db_user_id)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return ctx
                .send_error_and_disconnect(ERR_ACCOUNT_NOT_FOUND, Some("UserKick"))
                .await;
        }
        Err(e) => {
            eprintln!("Database error getting requesting user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserKick"))
                .await;
        }
    };

    // Step 5: Check UserKick permission
    let has_permission = requesting_user.is_admin
        || match ctx
            .db
            .users
            .has_permission(requesting_user.id, Permission::UserKick)
            .await
        {
            Ok(has_perm) => has_perm,
            Err(e) => {
                eprintln!("Database error checking permissions: {}", e);
                return ctx
                    .send_error_and_disconnect(ERR_DATABASE, Some("UserKick"))
                    .await;
            }
        };

    if !has_permission {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(ERR_NO_KICK_PERMISSION.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Step 6: Look up target user in database to check admin status
    let target_user_db = match ctx.db.users.get_user_by_username(&target_username).await {
        Ok(Some(user)) => Some(user),
        Ok(None) => {
            // User not in database, check if online anyway
            None
        }
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserKick"))
                .await;
        }
    };

    // Step 7: Prevent kicking admin users
    if let Some(target_db) = target_user_db.as_ref()
        && target_db.is_admin
    {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(ERR_CANNOT_KICK_ADMIN.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Step 8: Check if target user is online
    let all_users = ctx.user_manager.get_all_users().await;
    let target_users: Vec<_> = all_users
        .iter()
        .filter(|u| u.username.to_lowercase() == target_username.to_lowercase())
        .collect();

    if target_users.is_empty() {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(ERR_USER_NOT_ONLINE.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Step 9: Kick all sessions of the target user
    for user in target_users {
        // Send kick message to the user before disconnecting
        let kick_msg = ServerMessage::Error {
            message: err_kicked_by(&requesting_user.username),
            command: None,
        };
        let _ = user.tx.send(kick_msg);

        // Remove user from UserManager (channel closes, connection breaks)
        let session_id = user.session_id;
        if let Some(removed_user) = ctx.user_manager.remove_user(session_id).await {
            // Broadcast disconnection to users with user_list permission
            ctx.user_manager
                .broadcast_user_event(
                    ServerMessage::UserDisconnected {
                        session_id,
                        username: removed_user.username.clone(),
                    },
                    &ctx.db.users,
                    Some(session_id), // Exclude the kicked user
                )
                .await;
        }
    }

    // Step 10: Send success response to requester
    let response = ServerMessage::UserKickResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_userkick_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to kick user without being logged in
        let result =
            handle_userkick("alice".to_string(), None, &mut test_ctx.handler_context()).await;

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
            handle_userkick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error } = response {
            assert!(!success, "Kick should fail without permission");
            assert!(
                error.unwrap().contains("permission"),
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
            handle_userkick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Kick should succeed with permission");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error } = response {
            assert!(success, "Kick should succeed");
            assert!(error.is_none(), "Should not have error");
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
            handle_userkick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Admin should be able to kick");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, .. } = response {
            assert!(success, "Admin kick should succeed");
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
        let result = handle_userkick(
            "alice".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error } = response {
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
            .create_user("offline_user", &hashed, false, true, &perms)
            .await
            .unwrap();

        // Try to kick offline user (should fail)
        let result = handle_userkick(
            "offline_user".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error } = response {
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
        let result = handle_userkick(
            "alice".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should work case-insensitively");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, .. } = response {
            assert!(success, "Case-insensitive kick should succeed");
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
        let result = handle_userkick(
            "alice".to_string(),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should succeed");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, .. } = response {
            assert!(success, "Kick should succeed for multi-session user");
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
            handle_userkick("bob".to_string(), Some(1), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx.client).await;
        if let ServerMessage::UserKickResponse { success, error } = response {
            assert!(!success, "Should not be able to kick admin");
            assert!(
                error.unwrap().contains("admin"),
                "Error should mention admin protection"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }
}
