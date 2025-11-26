//! UserInfo message handler

use super::{
    ERR_AUTHENTICATION, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::{ServerMessage, UserInfoDetailed};
use std::io;

/// Error message when target user not found
const ERR_TARGET_NOT_FOUND: &str = "User not found";

/// Handle a userinfo request from the client
pub async fn handle_userinfo(
    requested_username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication
    let id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserInfo request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserInfo"))
                .await;
        }
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            eprintln!("UserInfo request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("UserInfo"))
                .await;
        }
    };

    // Check UserInfo permission
    let has_perm = match ctx
        .db
        .users
        .has_permission(requesting_user.db_user_id, Permission::UserInfo)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserInfo permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserInfo"))
                .await;
        }
    };

    if !has_perm {
        eprintln!(
            "UserInfo from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserInfo"))
            .await;
    }

    // Look up all sessions for target username
    let all_users = ctx.user_manager.get_all_users().await;
    let target_sessions: Vec<_> = all_users
        .into_iter()
        .filter(|u| u.username == requested_username)
        .collect();

    if target_sessions.is_empty() {
        // User not found - send response with None
        let response = ServerMessage::UserInfoResponse {
            user: None,
            error: Some(ERR_TARGET_NOT_FOUND.to_string()),
        };
        ctx.send_message(&response).await?;
        return Ok(());
    }

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
                .send_error_and_disconnect(ERR_DATABASE, Some("UserInfo"))
                .await;
        }
    };

    // Fetch target user account for admin status and created_at
    let target_account = match ctx.db.users.get_user_by_username(&requested_username).await {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserInfo"))
                .await;
        }
    };

    // Aggregate session data
    let session_ids: Vec<u32> = target_sessions.iter().map(|s| s.session_id).collect();
    let earliest_login = target_sessions.iter().map(|s| s.login_time).min().unwrap();

    // Collect unique features from all sessions
    let mut all_features = std::collections::HashSet::new();
    for session in &target_sessions {
        for feature in &session.features {
            all_features.insert(feature.clone());
        }
    }
    let features: Vec<String> = all_features.into_iter().collect();

    // Collect IP addresses from all sessions (for admins only)
    let addresses: Vec<String> = target_sessions
        .iter()
        .map(|s| s.address.to_string())
        .collect();

    // Build response with appropriate visibility level
    let user_info = if requesting_account.is_admin {
        // Admin gets all fields including target user's admin status and addresses
        UserInfoDetailed {
            username: requested_username.clone(),
            login_time: earliest_login,
            session_ids: session_ids.clone(),
            features,
            created_at: target_account.created_at,
            is_admin: Some(target_account.is_admin),
            addresses: Some(addresses),
        }
    } else {
        // Non-admin gets filtered fields
        UserInfoDetailed {
            username: requested_username.clone(),
            login_time: earliest_login,
            session_ids,
            features,
            created_at: target_account.created_at,
            is_admin: None,
            addresses: None,
        }
    };

    let response = ServerMessage::UserInfoResponse {
        user: Some(user_info),
        error: None,
    };
    ctx.send_message(&response).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::create_test_context;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_userinfo_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to get user info without being logged in
        let result =
            handle_userinfo("alice".to_string(), None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "UserInfo should require login");
    }

    #[tokio::test]
    async fn test_userinfo_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserInfo permission (non-admin)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let user = test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Add user to UserManager
        let user_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Try to get user info without permission
        let result = handle_userinfo(
            "alice".to_string(),
            Some(user_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_userinfo_user_not_found() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH UserInfo permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserInfo);
            set
        };
        let user = test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &perms)
            .await
            .unwrap();

        // Add user to UserManager
        let user_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Request info for non-existent username
        let result = handle_userinfo(
            "nonexistent".to_string(),
            Some(user_id),
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
            ServerMessage::UserInfoResponse { user, error } => {
                assert!(user.is_none(), "User should be None");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("not found"),
                    "Error should mention user not found, got: {}",
                    error_msg
                );
            }
            _ => panic!("Expected UserInfoResponse, got: {:?}", response_msg),
        }
    }

    #[tokio::test]
    async fn test_userinfo_non_admin_sees_filtered_fields() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user WITH UserInfo permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserInfo);
            set
        };
        let requester = test_ctx
            .db
            .users
            .create_user("requester", &hashed, false, true, &perms)
            .await
            .unwrap();

        // Create target user
        let target = test_ctx
            .db
            .users
            .create_user("target", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Add both users to UserManager
        let requester_id = test_ctx
            .user_manager
            .add_user(
                requester.id,
                "requester".to_string(),
                test_ctx.peer_addr,
                requester.created_at,
                test_ctx.tx.clone(),
                vec!["chat".to_string()],
            )
            .await;

        let target_id = test_ctx
            .user_manager
            .add_user(
                target.id,
                "target".to_string(),
                test_ctx.peer_addr,
                target.created_at,
                test_ctx.tx.clone(),
                vec!["chat".to_string()],
            )
            .await;

        // Request info about target as non-admin
        let result = handle_userinfo(
            "target".to_string(),
            Some(requester_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Should successfully get user info");

        // Close writer and read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserInfoResponse { user, error } => {
                assert!(error.is_none(), "Should have no error");
                assert!(user.is_some(), "Should have user info");
                let user_info = user.unwrap();

                // Verify all basic fields are present
                assert_eq!(user_info.username, "target");
                assert_eq!(user_info.session_ids.len(), 1);
                assert_eq!(user_info.session_ids[0], target_id);
                assert_eq!(user_info.features, vec!["chat".to_string()]);
                assert_eq!(user_info.created_at, target.created_at);

                // Verify admin-only fields are NOT present (None)
                assert!(
                    user_info.is_admin.is_none(),
                    "Non-admin should not see is_admin field"
                );
                assert!(
                    user_info.addresses.is_none(),
                    "Non-admin should not see addresses field"
                );
            }
            _ => panic!("Expected UserInfoResponse, got: {:?}", response_msg),
        }
    }

    #[tokio::test]
    async fn test_userinfo_admin_sees_all_fields() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let admin = test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create target user (non-admin)
        let target = test_ctx
            .db
            .users
            .create_user("target", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Add both users to UserManager
        let admin_id = test_ctx
            .user_manager
            .add_user(
                admin.id,
                "admin".to_string(),
                test_ctx.peer_addr,
                admin.created_at,
                test_ctx.tx.clone(),
                vec!["chat".to_string()],
            )
            .await;

        let target_id = test_ctx
            .user_manager
            .add_user(
                target.id,
                "target".to_string(),
                test_ctx.peer_addr,
                target.created_at,
                test_ctx.tx.clone(),
                vec!["chat".to_string()],
            )
            .await;

        // Request info about target as admin
        let result = handle_userinfo(
            "target".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Should successfully get user info");

        // Close writer and read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserInfoResponse { user, error } => {
                assert!(error.is_none(), "Should have no error");
                assert!(user.is_some(), "Should have user info");
                let user_info = user.unwrap();

                // Verify all basic fields are present
                assert_eq!(user_info.username, "target");
                assert_eq!(user_info.session_ids.len(), 1);
                assert_eq!(user_info.session_ids[0], target_id);
                assert_eq!(user_info.features, vec!["chat".to_string()]);
                assert_eq!(user_info.created_at, target.created_at);

                // Verify admin-only fields ARE present
                assert!(
                    user_info.is_admin.is_some(),
                    "Admin should see is_admin field"
                );
                assert!(!user_info.is_admin.unwrap(), "Target user is not admin");

                assert!(
                    user_info.addresses.is_some(),
                    "Admin should see addresses field"
                );
                let addresses = user_info.addresses.unwrap();
                assert!(!addresses.is_empty(), "Addresses should not be empty");
                assert_eq!(addresses.len(), 1, "Should have 1 address");
                assert!(
                    !addresses[0].is_empty(),
                    "Address should not be empty, got: {}",
                    addresses[0]
                );
            }
            _ => panic!("Expected UserInfoResponse, got: {:?}", response_msg),
        }
    }

    #[tokio::test]
    async fn test_userinfo_admin_viewing_admin() {
        let mut test_ctx = create_test_context().await;

        // Create two admin users
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let admin1 = test_ctx
            .db
            .users
            .create_user("admin1", &hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        let admin2 = test_ctx
            .db
            .users
            .create_user("admin2", &hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        // Add both admins to UserManager
        let admin1_id = test_ctx
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

        let admin2_id = test_ctx
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

        // Admin1 requests info about admin2
        let result = handle_userinfo(
            "admin2".to_string(),
            Some(admin1_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Should successfully get user info");

        // Close writer and read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserInfoResponse { user, error } => {
                assert!(error.is_none(), "Should have no error");
                assert!(user.is_some(), "Should have user info");
                let user_info = user.unwrap();

                // Verify basic fields
                assert_eq!(user_info.session_ids.len(), 1);
                assert_eq!(user_info.session_ids[0], admin2_id);
                assert_eq!(user_info.username, "admin2");

                // Verify is_admin shows true for target admin
                assert!(
                    user_info.is_admin.is_some(),
                    "Admin should see is_admin field"
                );
                assert!(user_info.is_admin.unwrap(), "Target user is admin");

                assert!(
                    user_info.addresses.is_some(),
                    "Admin should see address field"
                );
            }
            _ => panic!("Expected UserInfoResponse, got: {:?}", response_msg),
        }
    }
}
