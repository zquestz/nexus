//! Handler for UserDelete command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, UsernameError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_account_deleted, err_authentication, err_cannot_delete_admin,
    err_cannot_delete_guest, err_cannot_delete_last_admin, err_cannot_delete_self, err_database,
    err_not_logged_in, err_permission_denied, err_user_not_found, err_username_empty,
    err_username_invalid, err_username_too_long,
};
use crate::db::Permission;
use crate::db::sql::GUEST_USERNAME;

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

    // Prevent deleting the guest account (cheap check before DB queries)
    if target_username.to_lowercase() == GUEST_USERNAME {
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(err_cannot_delete_guest(ctx.locale)),
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

    // Prevent non-admins from deleting admin users
    if target_user.is_admin && !requesting_user_session.is_admin {
        eprintln!(
            "UserDelete from {} (user: {}) trying to delete admin user",
            ctx.peer_addr, requesting_user_session.username
        );
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(err_cannot_delete_admin(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

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

        // Remove them from UserManager and broadcast disconnection
        let session_id = online_user.session_id;
        ctx.user_manager.remove_user_and_broadcast(session_id).await;
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
            .create_user("bob", "hash", false, false, true, &db::Permissions::new())
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

        // Create two admins - admin1 will try to delete admin2
        let admin1_id = login_user(&mut test_ctx, "admin1", "password", &[], true).await;
        let _admin2_id = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        // Admin1 deletes admin2 (should succeed, admin1 still exists)
        let result = handle_user_delete(
            "admin2".to_string(),
            Some(admin1_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Admin should be able to delete other admin");
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Should successfully delete admin2");
                assert!(error.is_none());
                assert_eq!(username, Some("admin2".to_string()));
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Create a new test context for the last admin test
        let mut test_ctx2 = create_test_context().await;

        // Create single admin (the only admin)
        let _only_admin_id = login_user(&mut test_ctx2, "only_admin", "password", &[], true).await;

        // Create a target non-admin user
        test_ctx2
            .db
            .users
            .create_user(
                "target",
                "hash",
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        // First verify admin can delete non-admin (to confirm permissions work)
        // But we want to test the last admin protection, so we need another admin to try to delete the only admin

        // Create another admin to attempt deletion
        let _admin2_id = login_user(&mut test_ctx2, "admin2", "password", &[], true).await;

        // Admin2 tries to delete only_admin (should fail - last admin protection)
        // But wait, now there are two admins... Let's restructure this test

        // Actually, the last admin protection is in the database layer.
        // Let's test it properly: have the only admin try to delete themselves
        // But self-delete is blocked earlier. So we need admin2 to delete admin1,
        // then admin1 (now the only admin) tries to get deleted by someone.

        // Simpler approach: just verify the database protection directly
        // The delete_user function returns false if it would delete the last admin
        let only_admin = test_ctx2
            .db
            .users
            .get_user_by_username("only_admin")
            .await
            .unwrap()
            .unwrap();

        // Delete admin2 first so only_admin becomes the last admin
        let admin2 = test_ctx2
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let deleted = test_ctx2.db.users.delete_user(admin2.id).await.unwrap();
        assert!(deleted, "Should delete admin2");

        // Now try to delete the last admin via the database directly
        let deleted = test_ctx2.db.users.delete_user(only_admin.id).await.unwrap();
        assert!(!deleted, "Should not be able to delete the last admin");

        // Verify only_admin still exists
        let remaining = test_ctx2
            .db
            .users
            .get_user_by_username("only_admin")
            .await
            .unwrap();
        assert!(remaining.is_some(), "Last admin should still exist");
    }

    #[tokio::test]
    async fn test_userdelete_non_admin_cannot_delete_admin() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create non-admin user with UserDelete permission
        let deleter_id = login_user(
            &mut test_ctx,
            "deleter",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        // Non-admin tries to delete admin (should fail)
        let result = handle_user_delete(
            "admin".to_string(),
            Some(deleter_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Non-admin should not be able to delete admin");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("admin"),
                    "Error should mention admin restriction"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify admin still exists
        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap();
        assert!(admin.is_some(), "Admin should still exist");
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
            .create_user(
                "offline_user",
                "hash",
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        // Create online user to delete
        let online_user = test_ctx
            .db
            .users
            .create_user(
                "online_user",
                "hash",
                false,
                false,
                true,
                &db::Permissions::new(),
            )
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
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: online_user.created_at,
                tx: online_tx,
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "online_user".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

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
            .create_user(
                "target",
                "hash",
                false,
                false,
                true,
                &db::Permissions::new(),
            )
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

    #[tokio::test]
    async fn test_userdelete_cannot_delete_guest() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to delete the guest account
        let result = handle_user_delete(
            "guest".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Should receive error response
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(!success, "Should not be able to delete guest account");
                assert!(error.is_some(), "Should have error message");
                assert!(
                    error.unwrap().contains("guest"),
                    "Error should mention guest"
                );
                assert!(username.is_none(), "Should not have username on failure");
            }
            _ => panic!("Expected UserDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_guest_case_insensitive() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to delete the guest account with different casing
        let result = handle_user_delete(
            "GUEST".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Should receive error response
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(!success, "Should not be able to delete GUEST account");
                assert!(error.is_some(), "Should have error message");
                assert!(username.is_none(), "Should not have username on failure");
            }
            _ => panic!("Expected UserDeleteResponse"),
        }
    }
}
