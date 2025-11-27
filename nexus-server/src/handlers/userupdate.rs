//! UserUpdate message handler

use super::{
    ERR_CANNOT_DEMOTE_LAST_ADMIN, ERR_CANNOT_DISABLE_LAST_ADMIN, ERR_CANNOT_EDIT_SELF,
    ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, ERR_USER_NOT_FOUND,
    ERR_USERNAME_EXISTS, HandlerContext,
};
use crate::db::{Permission, Permissions, hash_password};
use nexus_common::protocol::{ServerMessage, UserInfo};
use std::io;

/// User update request parameters
pub struct UserUpdateRequest {
    pub username: String,
    pub requested_username: Option<String>,
    pub requested_password: Option<String>,
    pub requested_is_admin: Option<bool>,
    pub requested_enabled: Option<bool>,
    pub requested_permissions: Option<Vec<String>>,
    pub session_id: Option<u32>,
}

/// Handle a user update request from the client
pub async fn handle_userupdate(
    request: UserUpdateRequest,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication
    let requesting_session_id = match request.session_id {
        Some(id) => id,
        None => {
            eprintln!("UserUpdate request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserUpdate"))
                .await;
        }
    };

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            eprintln!("UserUpdate request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                .await;
        }
    };

    // Check UserEdit permission
    let has_permission = match ctx
        .db
        .users
        .has_permission(requesting_user.db_user_id, Permission::UserEdit)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserUpdate permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                .await;
        }
    };

    if !has_permission {
        eprintln!(
            "UserUpdate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserUpdate"))
            .await;
    }

    // Prevent self-editing
    if request.username == requesting_user.username {
        eprintln!("UserUpdate from {} attempting to edit self", ctx.peer_addr);
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(ERR_CANNOT_EDIT_SELF.to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Note: Last admin protection is now handled atomically at the database level
    // in update_user() SQL query to prevent race conditions

    // Fetch requesting user account to check admin status
    let requesting_account = match ctx
        .db
        .users
        .get_user_by_username(&requesting_user.username)
        .await
    {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                .await;
        }
    };

    // Verify admin flag modification privilege
    if request.requested_is_admin.is_some() && !requesting_account.is_admin {
        eprintln!(
            "UserUpdate from {} (non-admin) trying to change admin status",
            ctx.peer_addr
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserUpdate"))
            .await;
    }

    // Parse and validate requested permissions
    let parsed_permissions = if let Some(ref perm_strings) = request.requested_permissions {
        let mut perms = Permissions::new();
        for perm_str in perm_strings {
            if let Some(perm) = Permission::parse(perm_str) {
                // Check permission delegation authority
                if !requesting_account.is_admin {
                    let has_perm = match ctx
                        .db
                        .users
                        .has_permission(requesting_user.db_user_id, perm)
                        .await
                    {
                        Ok(has) => has,
                        Err(e) => {
                            eprintln!("Permission check error: {}", e);
                            return ctx
                                .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                                .await;
                        }
                    };

                    if !has_perm {
                        eprintln!(
                            "UserUpdate from {} (user: {}) trying to set permission they don't have: {}",
                            ctx.peer_addr, requesting_user.username, perm_str
                        );
                        return ctx
                            .send_error(ERR_PERMISSION_DENIED, Some("UserUpdate"))
                            .await;
                    }
                }

                perms.permissions.insert(perm);
            } else {
                eprintln!("Warning: unknown permission '{}'", perm_str);
            }
        }

        // Apply permission merge logic for non-admins
        if !requesting_account.is_admin {
            // Get target user's account
            if let Ok(Some(target_account)) =
                ctx.db.users.get_user_by_username(&request.username).await
            {
                // Get target user's current permissions
                if let Ok(target_perms) = ctx.db.users.get_user_permissions(target_account.id).await
                {
                    // Start with an empty set for the final permissions
                    let mut final_perms = Permissions::new();

                    // Add all permissions from target that requesting user DOESN'T have
                    // (these are preserved and cannot be modified)
                    for target_perm in &target_perms.permissions {
                        let requester_has_perm = match ctx
                            .db
                            .users
                            .has_permission(requesting_user.db_user_id, *target_perm)
                            .await
                        {
                            Ok(has) => has,
                            Err(e) => {
                                eprintln!("Permission check error: {}", e);
                                return ctx
                                    .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                                    .await;
                            }
                        };

                        if !requester_has_perm {
                            // Preserve this permission - requester can't modify it
                            final_perms.permissions.insert(*target_perm);
                        }
                    }

                    // Add all requested permissions that the requester DOES have
                    // (these are the ones the requester can control)
                    for requested_perm in &perms.permissions {
                        final_perms.permissions.insert(*requested_perm);
                    }

                    // Replace the requested permissions with the merged set
                    perms = final_perms;
                }
            }
        }

        Some(perms)
    } else {
        None
    };

    // Process password change request
    let requested_password_hash = if let Some(ref password) = request.requested_password {
        if password.trim().is_empty() {
            // Empty password = no change
            None
        } else {
            match hash_password(password) {
                Ok(hash) => Some(hash),
                Err(e) => {
                    eprintln!("Password hashing error: {}", e);
                    return ctx
                        .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                        .await;
                }
            }
        }
    } else {
        None
    };

    // Validate new username if provided
    if let Some(ref new_name) = request.requested_username
        && new_name.trim().is_empty()
    {
        eprintln!("UserUpdate from {} with empty username", ctx.peer_addr);
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some("Username cannot be empty".to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Get old username and admin status before update (to detect changes)
    let old_account = match ctx.db.users.get_user_by_username(&request.username).await {
        Ok(Some(acc)) => Some((acc.username.clone(), acc.is_admin)),
        _ => None,
    };

    // Attempt to update the user (with atomic last-admin protection in SQL)
    match ctx
        .db
        .users
        .update_user(
            &request.username,
            request.requested_username.as_deref(),
            requested_password_hash.as_deref(),
            request.requested_is_admin,
            request.requested_enabled,
            parsed_permissions.as_ref(),
        )
        .await
    {
        Ok(true) => {
            // Success - send response to requester
            let response = ServerMessage::UserUpdateResponse {
                success: true,
                error: None,
            };
            ctx.send_message(&response).await?;

            // Notify all sessions of the updated user about their new permissions
            // Get the updated user's account to read final permissions
            // Use the final username (in case it changed)
            let final_username = request
                .requested_username
                .as_ref()
                .unwrap_or(&request.username);
            if let Ok(Some(updated_account)) =
                ctx.db.users.get_user_by_username(final_username).await
            {
                // Get the final permissions
                if let Ok(final_permissions) =
                    ctx.db.users.get_user_permissions(updated_account.id).await
                {
                    let permission_strings: Vec<String> = final_permissions
                        .permissions
                        .iter()
                        .map(|p| p.as_str().to_string())
                        .collect();

                    let permissions_update = ServerMessage::PermissionsUpdated {
                        is_admin: updated_account.is_admin,
                        permissions: permission_strings,
                    };

                    // Send to all sessions belonging to the updated user
                    ctx.user_manager
                        .broadcast_to_username(
                            &updated_account.username,
                            &permissions_update,
                            &ctx.db.users,
                        )
                        .await;
                }

                // If user was disabled, disconnect all their active sessions
                //
                // Clean Disconnect Flow:
                // 1. Send Error message to user ("Account disabled by admin")
                // 2. Remove user from UserManager (drops the tx sender)
                // 3. Connection handler's rx.recv() returns None (channel closed)
                // 4. Connection loop breaks cleanly
                // 5. TCP connection closes
                //
                // This approach avoids manual shutdown signals and relies on channel semantics:
                // - User struct contains a tx (clone of the channel sender)
                // - UserManager.remove_user() drops the User, which drops tx
                // - When all senders are dropped, rx.recv() returns None
                // - Connection handler detects None and breaks the loop
                //
                // Note: UserDisconnected is only broadcast once here (connection.rs cleanup
                // doesn't re-broadcast because the user is already removed from manager)
                if let Some(false) = request.requested_enabled {
                    // Get all session IDs for this user
                    let session_ids = ctx
                        .user_manager
                        .get_session_ids_for_user(&updated_account.username)
                        .await;

                    // Disconnect each session
                    for session_id in session_ids {
                        // Send disconnect message to inform the user why they're being disconnected
                        let disconnect_msg = ServerMessage::Error {
                            message: "Account disabled by admin".to_string(),
                            command: None,
                        };

                        if let Some(user) =
                            ctx.user_manager.get_user_by_session_id(session_id).await
                        {
                            let _ = user.tx.send(disconnect_msg);
                        }

                        // Remove user from manager - this drops tx, causing rx.recv() to return None,
                        // which breaks the connection loop and triggers cleanup
                        if let Some(removed_user) = ctx.user_manager.remove_user(session_id).await {
                            // Broadcast disconnect to users with user_list permission
                            ctx.user_manager
                                .broadcast_user_event(
                                    ServerMessage::UserDisconnected {
                                        session_id,
                                        username: removed_user.username.clone(),
                                    },
                                    &ctx.db.users,
                                    Some(session_id), // Exclude the disabled user
                                )
                                .await;
                        }
                    }
                }

                // Check if username or admin status changed
                let username_changed = old_account
                    .as_ref()
                    .map(|(old_name, _)| old_name != &updated_account.username)
                    .unwrap_or(false);
                let admin_status_changed = old_account
                    .as_ref()
                    .map(|(_, old_admin)| *old_admin != updated_account.is_admin)
                    .unwrap_or(false);

                // Only broadcast UserUpdated if username or admin status changed
                if username_changed || admin_status_changed {
                    let session_ids = ctx
                        .user_manager
                        .get_session_ids_for_user(&updated_account.username)
                        .await;

                    // Get earliest login time from all sessions
                    let login_time = if !session_ids.is_empty() {
                        let users = ctx.user_manager.get_all_users().await;
                        users
                            .iter()
                            .filter(|u| u.username == updated_account.username)
                            .map(|u| u.login_time)
                            .min()
                            .unwrap_or(0)
                    } else {
                        0 // User not currently online
                    };

                    let user_info = UserInfo {
                        username: updated_account.username.clone(),
                        login_time,
                        is_admin: updated_account.is_admin,
                        session_ids,
                    };

                    let user_updated = ServerMessage::UserUpdated {
                        previous_username: old_account
                            .as_ref()
                            .map(|(name, _)| name.clone())
                            .unwrap_or(updated_account.username.clone()),
                        user: user_info,
                    };
                    ctx.user_manager
                        .broadcast_to_permission(user_updated, &ctx.db.users, Permission::UserList)
                        .await;
                }
            }

            Ok(())
        }
        Ok(false) => {
            // Update was blocked (user not found, last admin, or duplicate username)
            // We need to determine which error to return
            let error_message = if ctx
                .db
                .users
                .get_user_by_username(&request.username)
                .await
                .ok()
                .flatten()
                .is_none()
            {
                ERR_USER_NOT_FOUND
            } else if request.requested_is_admin == Some(false) {
                ERR_CANNOT_DEMOTE_LAST_ADMIN
            } else if request.requested_enabled == Some(false) {
                ERR_CANNOT_DISABLE_LAST_ADMIN
            } else if request.requested_username.is_some() {
                ERR_USERNAME_EXISTS
            } else {
                "Update failed"
            };

            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error_message.to_string()),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("Database error updating user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserUpdate"))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::*;

    #[tokio::test]
    async fn test_userupdate_requires_login() {
        let mut test_ctx = create_test_context().await;

        let request = UserUpdateRequest {
            username: "alice".to_string(),
            requested_username: Some("alice2".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: None, // Not logged in
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_userupdate_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without UserEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: Some("bob2".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

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
    async fn test_userupdate_cannot_edit_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = UserUpdateRequest {
            username: "admin".to_string(),
            requested_username: Some("admin2".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(error.unwrap(), ERR_CANNOT_EDIT_SELF);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_admin_can_edit() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: Some("bobby".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify username was changed
        let user = test_ctx
            .db
            .users
            .get_user_by_username("bobby")
            .await
            .unwrap();
        assert!(user.is_some());
        let user = test_ctx.db.users.get_user_by_username("bob").await.unwrap();
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn test_userupdate_user_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = UserUpdateRequest {
            username: "nonexistent".to_string(),
            requested_username: Some("newname".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error } => {
                assert!(!success);
                assert_eq!(error.unwrap(), ERR_USER_NOT_FOUND);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_demote_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Create two admins
        let admin1_session = login_user(&mut test_ctx, "admin1", "password", &[], true).await;
        let admin2_session = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        // Admin1 demotes Admin2 (should succeed, admin1 still exists)
        let request = UserUpdateRequest {
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Demote to non-admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Now admin2 tries to demote admin1 (should fail - no permission)
        let request = UserUpdateRequest {
            username: "admin1".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Try to demote last admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin2_session),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

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
    async fn test_userupdate_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: Some("robert".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_change_admin_status() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Try to make bob an admin
        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(true), // Try to make admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

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
    async fn test_userupdate_duplicate_username() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create two users
        test_ctx
            .db
            .users
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Try to rename bob to alice (should fail)
        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: Some("alice".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_change_password() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user
        test_ctx
            .db
            .users
            .create_user("alice", "oldhash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Change alice's password
        let request = UserUpdateRequest {
            username: "alice".to_string(),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify password was changed (hash should be different)
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_ne!(user.hashed_password, "oldhash");
    }

    #[tokio::test]
    async fn test_userupdate_change_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with no permissions
        let bob = test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Give bob some permissions
        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify permissions were set
        assert!(
            test_ctx
                .db
                .users
                .has_permission(bob.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            test_ctx
                .db
                .users
                .has_permission(bob.id, Permission::ChatSend)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_userupdate_empty_password_means_no_change() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with a specific password hash
        let original_hash = "original_hash_12345";
        test_ctx
            .db
            .users
            .create_user("alice", original_hash, false, true, &Permissions::new())
            .await
            .unwrap();

        // Try to edit alice with empty password (should not change password)
        let request = UserUpdateRequest {
            username: "alice".to_string(),
            requested_username: None,
            requested_password: Some("".to_string()), // Empty password
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify password was NOT changed
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            user.hashed_password, original_hash,
            "Password should not have been changed"
        );
    }

    #[tokio::test]
    async fn test_userupdate_cannot_revoke_permissions_user_doesnt_have() {
        let mut test_ctx = create_test_context().await;

        // Create Alice with user_list, user_info, and chat_send permissions
        let _alice_session = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::UserList,
                Permission::UserInfo,
                Permission::ChatSend,
            ],
            false,
        )
        .await;

        // Create Bob with only user_edit and user_list permissions
        let bob_session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::UserEdit, Permission::UserList],
            false,
        )
        .await;

        // Get Alice's user ID for verification later
        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        // Bob tries to update Alice, removing user_info and chat_send (permissions Bob doesn't have)
        // Bob tries to set Alice's permissions to just user_list (which Bob has)
        let request = UserUpdateRequest {
            username: "alice".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec!["user_list".to_string()]), // Bob only grants user_list
            session_id: Some(bob_session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success, "Update should succeed with merged permissions");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify Alice's permissions were merged correctly:
        // - user_list: Bob set this (and has it), Alice should have it
        // - user_info: Bob can't modify this (he doesn't have it), Alice should keep it
        // - chat_send: Bob can't modify this (he doesn't have it), Alice should keep it
        assert!(
            test_ctx
                .db
                .users
                .has_permission(alice.id, Permission::UserList)
                .await
                .unwrap(),
            "Alice should have user_list (Bob set it)"
        );
        assert!(
            test_ctx
                .db
                .users
                .has_permission(alice.id, Permission::UserInfo)
                .await
                .unwrap(),
            "Alice should keep user_info (Bob can't modify it)"
        );
        assert!(
            test_ctx
                .db
                .users
                .has_permission(alice.id, Permission::ChatSend)
                .await
                .unwrap(),
            "Alice should keep chat_send (Bob can't modify it)"
        );
    }

    #[tokio::test]
    async fn test_userupdate_cannot_disable_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to disable self (will be caught by self-edit check)
        let request = UserUpdateRequest {
            username: "admin".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Try to disable
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error } => {
                assert!(!success, "Should not allow self-edit");
                assert_eq!(error, Some(ERR_CANNOT_EDIT_SELF.to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_disable_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Login as the only admin
        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a non-admin user with user_edit permission
        let mut perms = Permissions::new();
        perms.permissions.insert(Permission::UserEdit);
        let editor = test_ctx
            .db
            .users
            .create_user("editor", "hash", false, true, &perms)
            .await
            .unwrap();

        // Add editor to UserManager
        let editor_session = test_ctx
            .user_manager
            .add_user(
                editor.id,
                "editor".to_string(),
                test_ctx.peer_addr,
                editor.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Editor tries to disable admin (the last admin) - should fail
        let request = UserUpdateRequest {
            username: "admin".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Try to disable last admin
            requested_permissions: None,
            session_id: Some(editor_session),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error } => {
                assert!(!success, "Should not allow disabling last admin");
                assert_eq!(error, Some(ERR_CANNOT_DISABLE_LAST_ADMIN.to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_change_enabled_status() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a regular user
        let bob = test_ctx
            .db
            .users
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Verify bob is enabled
        assert!(bob.enabled, "Bob should be enabled initially");

        // Disable bob
        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Disable
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success, "Should successfully disable user");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify bob is now disabled in database
        let bob_after = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert!(!bob_after.enabled, "Bob should be disabled");

        // Re-enable bob
        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(true), // Enable
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success, "Should successfully re-enable user");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify bob is enabled again
        let bob_final = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert!(bob_final.enabled, "Bob should be enabled again");
    }

    #[tokio::test]
    async fn test_userupdate_disconnects_when_disabling() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Login as bob (the user we'll disable)
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Verify bob is in the user manager
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_some(),
            "Bob should be in user manager"
        );

        // Admin disables bob
        let request = UserUpdateRequest {
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Disable
            requested_permissions: None,
            session_id: Some(admin_session),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Bob should be removed from user manager
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_none(),
            "Bob should be removed from user manager after being disabled"
        );
    }

    #[tokio::test]
    async fn test_userupdate_atomic_admin_demotion_protection() {
        let mut test_ctx = create_test_context().await;

        // Create two admin users
        let admin1 = test_ctx
            .db
            .users
            .create_user("admin1", "hash1", true, true, &Permissions::new())
            .await
            .unwrap();
        let admin2 = test_ctx
            .db
            .users
            .create_user("admin2", "hash2", true, true, &Permissions::new())
            .await
            .unwrap();

        // Login both admins
        let admin1_session = test_ctx
            .user_manager
            .add_user(
                admin1.id,
                "admin1".to_string(),
                test_ctx.peer_addr,
                admin1.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        let _admin2_session = test_ctx
            .user_manager
            .add_user(
                admin2.id,
                "admin2".to_string(),
                test_ctx.peer_addr,
                admin2.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Admin1 demotes admin2 to non-admin (should succeed - 2 admins exist)
        let request = UserUpdateRequest {
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Demote to non-admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let result = handle_userupdate(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(
                    success,
                    "Should successfully demote admin2 (2 admins exist)"
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify admin2 is now non-admin in database
        let admin2_account = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        assert!(
            !admin2_account.is_admin,
            "Admin2 should be demoted to non-admin"
        );

        // Now admin2 (now a non-admin with user_edit permission) tries to demote admin1
        // First, give admin2 the user_edit permission
        let mut perms = Permissions::new();
        perms.permissions.insert(Permission::UserEdit);
        test_ctx
            .db
            .users
            .set_permissions(admin2.id, &perms)
            .await
            .unwrap();

        // Admin2 tries to demote admin1 (last admin) - should fail at DB level atomically
        // Note: This bypasses the "non-admin cannot change admin status" check by using
        // the database directly to test the atomic SQL protection
        let result = test_ctx
            .db
            .users
            .update_user(
                "admin1",
                None,
                None,
                Some(false), // Try to demote last admin
                None,
                None,
            )
            .await;

        // Should return Ok(false) - update blocked by atomic SQL protection
        assert!(result.is_ok());
        assert!(
            !result.unwrap(),
            "Database should block demoting last admin atomically"
        );

        // Verify admin1 is still admin
        let admin1_account = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();
        assert!(
            admin1_account.is_admin,
            "Admin1 should still be admin (protected by atomic SQL)"
        );
    }
}
