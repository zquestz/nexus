//! UserList message handler

use super::{
    ERR_AUTHENTICATION, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::{ServerMessage, UserInfo};
use std::io;

/// Handle a userlist request from the client
pub async fn handle_userlist(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication
    let id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserList request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserList"))
                .await;
        }
    };

    // Get requesting user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            eprintln!("UserList request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("UserList"))
                .await;
        }
    };

    // Check UserList permission
    let has_perm = match ctx
        .user_db
        .has_permission(user.db_user_id, Permission::UserList)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserList permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserList"))
                .await;
        }
    };

    if !has_perm {
        eprintln!(
            "UserList from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserList"))
            .await;
    }

    // Fetch all connected users
    let all_users = ctx.user_manager.get_all_users().await;
    
    // Deduplicate by username and aggregate sessions
    use std::collections::HashMap;
    let mut user_map: HashMap<String, (u64, bool, Vec<u32>)> = HashMap::new(); // (earliest_login, is_admin, session_ids)
    
    for user in all_users {
        // Get user account to check admin status
        let is_admin = match ctx.user_db.get_user_by_id(user.db_user_id).await {
            Ok(Some(account)) => account.is_admin,
            _ => false, // Default to non-admin if lookup fails
        };
        
        user_map.entry(user.username.clone())
            .and_modify(|(login_time, _, session_ids)| {
                // Keep earliest login time
                *login_time = (*login_time).min(user.login_time);
                session_ids.push(user.session_id);
            })
            .or_insert((user.login_time, is_admin, vec![user.session_id]));
    }
    
    // Build deduplicated user info list
    let mut user_infos: Vec<UserInfo> = user_map.into_iter()
        .map(|(username, (login_time, is_admin, session_ids))| UserInfo {
            username,
            login_time,
            is_admin,
            session_ids,
        })
        .collect();
    
    // Sort by username (case-insensitive) for consistent ordering
    user_infos.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));

    // Send user list response
    let response = ServerMessage::UserListResponse { users: user_infos };
    ctx.send_message(&response).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user};

    #[tokio::test]
    async fn test_userlist_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to get user list without being logged in
        let result = handle_userlist(None, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(result.is_err(), "UserList should require login");
    }

    #[tokio::test]
    async fn test_userlist_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Use a session ID that doesn't exist in UserManager
        let invalid_session_id = Some(999);

        // Try to get user list with invalid session
        let result = handle_userlist(invalid_session_id, &mut test_ctx.handler_context()).await;

        // Should fail (ERR_AUTHENTICATION)
        assert!(
            result.is_err(),
            "UserList with invalid session should be rejected"
        );
    }

    #[tokio::test]
    async fn test_userlist_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserList permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Try to get user list without permission
        let result = handle_userlist(Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed (send error but not disconnect)
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_userlist_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH UserList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserList],
            false,
        )
        .await;

        // Get user list with permission
        let result = handle_userlist(Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed
        assert!(result.is_ok(), "Valid userlist request should succeed");

        // Verify response contains the user
        use crate::handlers::testing::read_server_message;
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users } => {
                assert_eq!(users.len(), 1, "Should have 1 user in the list");
                assert_eq!(users[0].username, "alice");
                assert_eq!(users[0].session_ids.len(), 1);
                assert_eq!(users[0].session_ids[0], session_id);
                assert_eq!(users[0].is_admin, false, "alice should not be admin");
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user WITHOUT explicit UserList permission
        // Admins should have all permissions automatically
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin should be able to list users
        let result = handle_userlist(Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Admin should be able to list users without explicit permission"
        );
        
        // Verify admin flag is set
        use crate::handlers::testing::read_server_message;
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users } => {
                assert_eq!(users.len(), 1, "Should have 1 user in the list");
                assert_eq!(users[0].username, "admin");
                assert_eq!(users[0].session_ids.len(), 1);
                assert_eq!(users[0].session_ids[0], session_id);
                assert_eq!(users[0].is_admin, true, "admin should have is_admin=true");
            }
            _ => panic!("Expected UserListResponse"),
        }
    }
}