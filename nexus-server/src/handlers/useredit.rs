//! UserEdit message handler

use super::{
    ERR_CANNOT_DEMOTE_LAST_ADMIN, ERR_CANNOT_EDIT_SELF, ERR_DATABASE, ERR_NOT_LOGGED_IN,
    ERR_PERMISSION_DENIED, ERR_USERNAME_EXISTS, ERR_USER_NOT_FOUND, HandlerContext,
};
use crate::db::{hash_password, Permission, Permissions};
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle a user edit request from the client
pub async fn handle_useredit(
    username: String,
    requested_username: Option<String>,
    requested_password: Option<String>,
    requested_is_admin: Option<bool>,
    requested_permissions: Option<Vec<String>>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Must be logged in
    let requesting_session_id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserEdit request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserEdit"))
                .await;
        }
    };

    // Get the requesting user
    let requesting_user = match ctx.user_manager.get_user(requesting_session_id).await {
        Some(u) => u,
        None => {
            eprintln!("UserEdit request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    // Check if requesting user has permission (UserEdit permission OR admin)
    let has_permission = match ctx
        .user_db
        .has_permission(requesting_user.db_user_id, Permission::UserEdit)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserEdit permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    if !has_permission {
        eprintln!(
            "UserEdit from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserEdit"))
            .await;
    }

    // Prevent users from editing themselves
    if username == requesting_user.username {
        eprintln!(
            "UserEdit from {} attempting to edit self",
            ctx.peer_addr
        );
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(ERR_CANNOT_EDIT_SELF.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Get the requesting user's account to check if they're admin
    let requesting_account = match ctx
        .user_db
        .get_user_by_username(&requesting_user.username)
        .await
    {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    // Only admins can change the is_admin flag
    if requested_is_admin.is_some() && !requesting_account.is_admin {
        eprintln!(
            "UserEdit from {} (non-admin) trying to change admin status",
            ctx.peer_addr
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserEdit"))
            .await;
    }

    // Parse and validate permissions if provided
    let parsed_permissions = if let Some(ref perm_strings) = requested_permissions {
        let mut perms = Permissions::new();
        for perm_str in perm_strings {
            if let Some(perm) = Permission::from_str(perm_str) {
                // Non-admins can only grant permissions they have
                if !requesting_account.is_admin {
                    let has_perm = match ctx
                        .user_db
                        .has_permission(requesting_user.db_user_id, perm)
                        .await
                    {
                        Ok(has) => has,
                        Err(e) => {
                            eprintln!("Permission check error: {}", e);
                            return ctx
                                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                                .await;
                        }
                    };

                    if !has_perm {
                        eprintln!(
                            "UserEdit from {} (user: {}) trying to grant permission they don't have: {}",
                            ctx.peer_addr, requesting_user.username, perm_str
                        );
                        return ctx
                            .send_error(ERR_PERMISSION_DENIED, Some("UserEdit"))
                            .await;
                    }
                }

                perms.permissions.insert(perm);
            } else {
                eprintln!("Warning: unknown permission '{}'", perm_str);
            }
        }
        Some(perms)
    } else {
        None
    };

    // Hash password if provided (skip if empty string)
    let requested_password_hash = if let Some(ref password) = requested_password {
        if password.trim().is_empty() {
            // Empty password means don't change the password
            None
        } else {
            match hash_password(password) {
                Ok(hash) => Some(hash),
                Err(e) => {
                    eprintln!("Password hashing error: {}", e);
                    return ctx
                        .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                        .await;
                }
            }
        }
    } else {
        None
    };

    // Validate new username if provided
    if let Some(ref new_name) = requested_username {
        if new_name.trim().is_empty() {
            eprintln!("UserEdit from {} with empty username", ctx.peer_addr);
            let response = ServerMessage::UserEditResponse {
                success: false,
                error: Some("Username cannot be empty".to_string()),
            };
            return ctx.send_message(&response).await;
        }
    }

    // Attempt to update the user
    match ctx
        .user_db
        .update_user(
            &username,
            requested_username.as_deref(),
            requested_password_hash.as_deref(),
            requested_is_admin,
            parsed_permissions.as_ref(),
        )
        .await
    {
        Ok(true) => {
            // Success
            let response = ServerMessage::UserEditResponse {
                success: true,
                error: None,
            };
            ctx.send_message(&response).await
        }
        Ok(false) => {
            // Update was blocked (user not found, last admin, or duplicate username)
            // We need to determine which error to return
            let error_message = if ctx.user_db.get_user_by_username(&username).await.ok().flatten().is_none() {
                ERR_USER_NOT_FOUND
            } else if requested_is_admin == Some(false) {
                ERR_CANNOT_DEMOTE_LAST_ADMIN
            } else if requested_username.is_some() {
                ERR_USERNAME_EXISTS
            } else {
                "Update failed"
            };

            let response = ServerMessage::UserEditResponse {
                success: false,
                error: Some(error_message.to_string()),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("Database error updating user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::*;

    #[tokio::test]
    async fn test_useredit_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_useredit(
            "alice".to_string(),
            Some("alice2".to_string()),
            None,
            None,
            None,
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_useredit_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without UserEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to edit
        test_ctx
            .user_db
            .create_user("bob", "hash", false, &Permissions::new())
            .await
            .unwrap();

        let result = handle_useredit(
            "bob".to_string(),
            Some("bob2".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, ERR_PERMISSION_DENIED);
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_useredit_cannot_edit_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_useredit(
            "admin".to_string(),
            Some("admin2".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, error } => {
                assert!(!success);
                assert_eq!(error.unwrap(), ERR_CANNOT_EDIT_SELF);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_admin_can_edit() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another user to edit
        test_ctx
            .user_db
            .create_user("bob", "hash", false, &Permissions::new())
            .await
            .unwrap();

        let result = handle_useredit(
            "bob".to_string(),
            Some("bobby".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserEditResponse"),
        }

        // Verify username was changed
        let user = test_ctx.user_db.get_user_by_username("bobby").await.unwrap();
        assert!(user.is_some());
        let user = test_ctx.user_db.get_user_by_username("bob").await.unwrap();
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn test_useredit_user_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_useredit(
            "nonexistent".to_string(),
            Some("newname".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, error } => {
                assert!(!success);
                assert_eq!(error.unwrap(), ERR_USER_NOT_FOUND);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_cannot_demote_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Create two admins
        let admin1_session = login_user(&mut test_ctx, "admin1", "password", &[], true).await;
        let admin2_session = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        // Admin1 demotes Admin2 (should succeed, admin1 still exists)
        let result = handle_useredit(
            "admin2".to_string(),
            None,
            None,
            Some(false),
            None,
            Some(admin1_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserEditResponse"),
        }

        // Now admin2 tries to demote admin1 (should fail - no permission)
        let result = handle_useredit(
            "admin1".to_string(),
            None,
            None,
            Some(false),
            None,
            Some(admin2_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, ERR_PERMISSION_DENIED);
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_useredit_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserEdit],
            false,
        )
        .await;

        // Create another user to edit
        test_ctx
            .user_db
            .create_user("bob", "hash", false, &Permissions::new())
            .await
            .unwrap();

        let result = handle_useredit(
            "bob".to_string(),
            Some("robert".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_non_admin_cannot_change_admin_status() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserEdit],
            false,
        )
        .await;

        // Create another user to edit
        test_ctx
            .user_db
            .create_user("bob", "hash", false, &Permissions::new())
            .await
            .unwrap();

        // Try to make bob an admin
        let result = handle_useredit(
            "bob".to_string(),
            None,
            None,
            Some(true),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, ERR_PERMISSION_DENIED);
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_useredit_duplicate_username() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create two users
        test_ctx
            .user_db
            .create_user("alice", "hash", false, &Permissions::new())
            .await
            .unwrap();
        test_ctx
            .user_db
            .create_user("bob", "hash", false, &Permissions::new())
            .await
            .unwrap();

        // Try to rename bob to alice (should fail)
        let result = handle_useredit(
            "bob".to_string(),
            Some("alice".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_change_password() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user
        test_ctx
            .user_db
            .create_user("alice", "oldhash", false, &Permissions::new())
            .await
            .unwrap();

        // Change alice's password
        let result = handle_useredit(
            "alice".to_string(),
            None,
            Some("newpassword".to_string()),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserEditResponse"),
        }

        // Verify password was changed (hash should be different)
        let user = test_ctx
            .user_db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_ne!(user.hashed_password, "oldhash");
    }

    #[tokio::test]
    async fn test_useredit_change_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with no permissions
        let bob = test_ctx
            .user_db
            .create_user("bob", "hash", false, &Permissions::new())
            .await
            .unwrap();

        // Give bob some permissions
        let result = handle_useredit(
            "bob".to_string(),
            None,
            None,
            None,
            Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserEditResponse"),
        }

        // Verify permissions were set
        assert!(
            test_ctx
                .user_db
                .has_permission(bob.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            test_ctx
                .user_db
                .has_permission(bob.id, Permission::ChatSend)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_useredit_empty_password_means_no_change() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with a specific password hash
        let original_hash = "original_hash_12345";
        test_ctx
            .user_db
            .create_user("alice", original_hash, false, &Permissions::new())
            .await
            .unwrap();

        // Try to edit alice with empty password (should not change password)
        let result = handle_useredit(
            "alice".to_string(),
            None,
            Some("".to_string()), // Empty password
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserEditResponse"),
        }

        // Verify password was NOT changed
        let user = test_ctx
            .user_db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.hashed_password, original_hash, "Password should not have been changed");
    }
}