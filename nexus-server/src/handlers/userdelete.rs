//! Handler for UserDelete command

use super::{
    ERR_ACCOUNT_DELETED, ERR_CANNOT_DELETE_LAST_ADMIN, ERR_CANNOT_DELETE_SELF, ERR_DATABASE,
    ERR_NOT_LOGGED_IN, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Error message when session not found
const ERR_SESSION_NOT_FOUND: &str = "Session not found";

/// Error message when requesting user account not found
const ERR_ACCOUNT_NOT_FOUND: &str = "Your user account was not found";

/// Error message when user lacks delete permission
const ERR_NO_DELETE_PERMISSION: &str = "You don't have permission to delete users";

/// Error message when target user not found
const ERR_TARGET_NOT_FOUND: &str = "User not found";

/// Handle UserDelete command
pub async fn handle_userdelete(
    target_username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication
    let Some(session_id) = session_id else {
        return ctx
            .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserDelete"))
            .await;
    };

    // Get requesting user from session
    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(ERR_SESSION_NOT_FOUND, Some("UserDelete"))
                .await;
        }
    };

    // Fetch requesting user account for permission check
    let requesting_user = match ctx
        .db
        .users
        .get_user_by_id(requesting_user_session.db_user_id)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return ctx
                .send_error_and_disconnect(ERR_ACCOUNT_NOT_FOUND, Some("UserDelete"))
                .await;
        }
        Err(e) => {
            eprintln!("Database error getting requesting user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                .await;
        }
    };

    // Check UserDelete permission
    let has_permission = requesting_user.is_admin
        || match ctx
            .db
            .users
            .has_permission(requesting_user.id, Permission::UserDelete)
            .await
        {
            Ok(has_perm) => has_perm,
            Err(e) => {
                eprintln!("Database error checking permissions: {}", e);
                return ctx
                    .send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                    .await;
            }
        };

    if !has_permission {
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(ERR_NO_DELETE_PERMISSION.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Look up target user in database
    let target_user = match ctx.db.users.get_user_by_username(&target_username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            let response = ServerMessage::UserDeleteResponse {
                success: false,
                error: Some(ERR_TARGET_NOT_FOUND.to_string()),
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                .await;
        }
    };

    // Prevent self-deletion
    if target_user.id == requesting_user.id {
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some(ERR_CANNOT_DELETE_SELF.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Handle online user disconnection
    let all_users = ctx.user_manager.get_all_users().await;
    let online_user = all_users.iter().find(|u| u.db_user_id == target_user.id);

    if let Some(online_user) = online_user {
        // Send error message to the user being deleted
        let disconnect_msg = ServerMessage::Error {
            message: ERR_ACCOUNT_DELETED.to_string(),
            command: None,
        };
        let _ = online_user.tx.send(disconnect_msg);

        // Remove them from UserManager
        let session_id = online_user.session_id;
        if let Some(removed_user) = ctx.user_manager.remove_user(session_id).await {
            // Broadcast disconnection to all other users
            ctx.user_manager
                .broadcast(ServerMessage::UserDisconnected {
                    session_id,
                    username: removed_user.username.clone(),
                })
                .await;
        }
    }

    // Delete user from database (atomic last-admin protection)
    match ctx.db.users.delete_user(target_user.id).await {
        Ok(deleted) => {
            if deleted {
                // Send success response to the admin who deleted the user
                let response = ServerMessage::UserDeleteResponse {
                    success: true,
                    error: None,
                };
                ctx.send_message(&response).await
            } else {
                // Deletion was blocked (likely because they're the last admin)
                let response = ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(ERR_CANNOT_DELETE_LAST_ADMIN.to_string()),
                };
                ctx.send_message(&response).await
            }
        }
        Err(e) => {
            eprintln!("Database error deleting user: {}", e);
            ctx.send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user};
    use tokio::io::AsyncReadExt;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_userdelete_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to delete user without being logged in
        let result =
            handle_userdelete("alice".to_string(), None, &mut test_ctx.handler_context()).await;

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
            .create_user("bob", "hash", false, &db::Permissions::new())
            .await
            .unwrap();

        // Try to delete user without permission
        let result = handle_userdelete(
            "bob".to_string(),
            Some(user_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed (no disconnect), but user should still exist
        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Close writer and read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("permission"),
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
        let result = handle_userdelete(
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error } => {
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
        let result = handle_userdelete(
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error } => {
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
            .create_user("only_admin", &hashed, true, &db::Permissions::new())
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
        let result = handle_userdelete(
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error } => {
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
            .create_user("offline_user", "hash", false, &db::Permissions::new())
            .await
            .unwrap();

        // Create online user to delete
        let online_user = test_ctx
            .db
            .users
            .create_user("online_user", "hash", false, &db::Permissions::new())
            .await
            .unwrap();

        // Add online_user to UserManager (they're online)
        let (online_tx, _online_rx) = mpsc::unbounded_channel();
        let online_session_id = test_ctx
            .user_manager
            .add_user(
                online_user.id,
                "online_user".to_string(),
                test_ctx.peer_addr,
                online_user.created_at,
                online_tx,
                vec![],
            )
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
        let result1 = handle_userdelete(
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
        let result2 = handle_userdelete(
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
            .create_user("target", "hash", false, &db::Permissions::new())
            .await
            .unwrap();

        // Delete target user
        let result = handle_userdelete(
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message on success");
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify target is deleted
        let deleted = test_ctx.db.users.get_user_by_id(target.id).await.unwrap();
        assert!(deleted.is_none(), "Target user should be deleted");
    }
}
