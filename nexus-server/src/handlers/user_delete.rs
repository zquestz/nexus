//! Handler for UserDelete command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, UsernameError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_account_deleted, err_authentication, err_cannot_delete_last_admin,
    err_cannot_delete_self, err_database, err_not_logged_in, err_permission_denied,
    err_user_not_found, err_username_empty, err_username_invalid, err_username_too_long,
};
use crate::db::Permission;

/// Handle UserDelete command
pub async fn handle_user_delete<W>(
    target_username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(session_id) = session_id else {
        eprintln!("UserDelete request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserDelete"))
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
        let response = ServerMessage::UserDeleteResponse {
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
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserDelete"))
                .await;
        }
    };

    // Prevent self-deletion (cheap check before DB queries)
    if target_username.to_lowercase() == requesting_user_session.username.to_lowercase() {
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(err_cannot_delete_self(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check UserDelete permission (uses cached permissions, admin bypass built-in)
    if !requesting_user_session.has_permission(Permission::UserDelete) {
        eprintln!(
            "UserDelete from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user_session.username
        );
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Look up target user in database
    let target_user = match ctx.db.users.get_user_by_username(&target_username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            let response = ServerMessage::UserDeleteResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &target_username)),
                username: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserDelete"))
                .await;
        }
    };

    // Handle online user disconnection (all sessions)
    let online_users = ctx
        .user_manager
        .get_sessions_by_username(&target_user.username)
        .await;

    for online_user in online_users {
        // Send error message to the user being deleted (in their locale)
        let disconnect_msg = ServerMessage::Error {
            message: err_account_deleted(&online_user.locale),
            command: None,
        };
        let _ = online_user.tx.send((disconnect_msg, None));

        // Remove them from UserManager
        let session_id = online_user.session_id;
        if let Some(removed_user) = ctx.user_manager.remove_user(session_id).await {
            // Broadcast disconnection to users with user_list permission
            ctx.user_manager
                .broadcast_user_event(
                    ServerMessage::UserDisconnected {
                        session_id,
                        username: removed_user.username.clone(),
                    },
                    &ctx.db.users,
                    Some(session_id), // Exclude the deleted user
                )
                .await;
        }
    }

    // Delete user from database (atomic last-admin protection)
    match ctx.db.users.delete_user(target_user.id).await {
        Ok(deleted) => {
            if deleted {
                // Send success response to the admin who deleted the user
                // Use the database-preserved username casing, not the input
                let response = ServerMessage::UserDeleteResponse {
                    success: true,
                    error: None,
                    username: Some(target_user.username),
                };
                ctx.send_message(&response).await
            } else {
                // Deletion was blocked (likely because they're the last admin)
                let response = ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(err_cannot_delete_last_admin(ctx.locale)),
                    username: None,
                };
                ctx.send_message(&response).await
            }
        }
        Err(e) => {
            eprintln!("Database error deleting user: {}", e);
            ctx.send_error_and_disconnect(&err_database(ctx.locale), Some("UserDelete"))
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use crate::users::user::NewSessionParams;

    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_userdelete_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to delete user without being logged in
        let result =
            handle_user_delete("alice".to_string(), None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "UserDelete should require login");
    }

    #[tokio::test]
    async fn test_userdelete_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserDelete permission (non-admin)
        let user_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create target user
        let target = test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Try to delete user without permission
        let result = handle_user_delete(
            "bob".to_string(),
            Some(user_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed (no disconnect), but user should still exist
        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.to_lowercase().contains("permission"),
                    "Error should mention permission"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify target user still exists
        let still_exists = test_ctx.db.users.get_user_by_id(target.id).await.unwrap();
        assert!(
            still_exists.is_some(),
            "User should not be deleted without permission"
        );
    }

    #[tokio::test]
    async fn test_userdelete_nonexistent_user() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to delete non-existent user
        let result = handle_user_delete(
            "nonexistent".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed (sends error response, doesn't disconnect)
        assert!(
            result.is_ok(),
            "Should send error response for non-existent user"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("not found"),
                    "Error should mention user not found"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_self() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to delete self
        let result = handle_user_delete(
            "admin".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed (sends error response, doesn't disconnect)
        assert!(
            result.is_ok(),
            "Should send error response when trying to delete self"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("delete")
                        && (error_msg.contains("yourself") || error_msg.contains("self")),
                    "Error should mention not being able to delete self"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify admin still exists
        let still_exists = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap();
        assert!(
            still_exists.is_some(),
            "Admin should not be able to delete themselves"
        );
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Create one admin user
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let _admin = test_ctx
            .db
            .users
            .create_user("only_admin", &hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create non-admin user with UserDelete permission
        let deleter_id = login_user(
            &mut test_ctx,
            "deleter",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        // Try to delete the only admin
        let result = handle_user_delete(
            "only_admin".to_string(),
            Some(deleter_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should send error response (not disconnect)
        assert!(
            result.is_ok(),
            "Should send error response when trying to delete last admin"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("Cannot delete"),
                    "Error should mention cannot delete"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify only admin still exists in database
        let remaining_admin = test_ctx
            .db
            .users
            .get_user_by_username("only_admin")
            .await
            .unwrap();
        assert!(remaining_admin.is_some(), "Cannot delete the last admin");
    }

    #[tokio::test]
    async fn test_userdelete_handles_online_and_offline_users() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create offline user to delete
        let offline_user = test_ctx
            .db
            .users
            .create_user("offline_user", "hash", false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create online user to delete
        let online_user = test_ctx
            .db
            .users
            .create_user("online_user", "hash", false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Add online_user to UserManager (they're online)
        let (online_tx, _online_rx) = mpsc::unbounded_channel();
        let online_session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: online_user.id,
                username: "online_user".to_string(),
                is_admin: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: online_user.created_at,
                tx: online_tx,
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
            })
            .await;

        // Verify online user is connected
        let online_before = test_ctx
            .user_manager
            .get_user_by_session_id(online_session_id)
            .await;
        assert!(
            online_before.is_some(),
            "Online user should be connected before deletion"
        );

        // Delete offline user
        let result1 = handle_user_delete(
            "offline_user".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result1.is_ok(), "Should successfully delete offline user");
        let deleted1 = test_ctx
            .db
            .users
            .get_user_by_id(offline_user.id)
            .await
            .unwrap();
        assert!(
            deleted1.is_none(),
            "Offline user should be deleted from database"
        );

        // Delete online user
        let result2 = handle_user_delete(
            "online_user".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result2.is_ok(), "Should successfully delete online user");
        let deleted2 = test_ctx
            .db
            .users
            .get_user_by_id(online_user.id)
            .await
            .unwrap();
        assert!(
            deleted2.is_none(),
            "Online user should be deleted from database"
        );

        // Verify online user was disconnected from UserManager
        let online_after = test_ctx
            .user_manager
            .get_user_by_session_id(online_session_id)
            .await;
        assert!(
            online_after.is_none(),
            "Online user should be disconnected from UserManager"
        );
    }

    #[tokio::test]
    async fn test_userdelete_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user with UserDelete permission
        let deleter_id = login_user(
            &mut test_ctx,
            "deleter",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        // Create target user
        let target = test_ctx
            .db
            .users
            .create_user("target", "hash", false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Delete target user
        let result = handle_user_delete(
            "target".to_string(),
            Some(deleter_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "User with UserDelete permission should be able to delete users"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message on success");
                assert_eq!(username, Some("target".to_string()));
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify target is deleted
        let deleted = test_ctx.db.users.get_user_by_id(target.id).await.unwrap();
        assert!(deleted.is_none(), "Target user should be deleted");
    }
}
