//! UserInfo message handler

use std::io;

use nexus_common::protocol::{ServerMessage, UserInfoDetailed};
use nexus_common::validators::{self, UsernameError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
    err_user_not_found, err_username_empty, err_username_invalid, err_username_too_long,
};
#[cfg(test)]
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

/// Handle a userinfo request from the client
pub async fn handle_userinfo(
    requested_username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(id) = session_id else {
        eprintln!("UserInfo request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserInfo"))
            .await;
    };

    // Validate username format
    if let Err(e) = validators::validate_username(&requested_username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserInfoResponse {
            success: false,
            error: Some(error_msg),
            user: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserInfo"))
                .await;
        }
    };

    // Check UserInfo permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::UserInfo) {
        eprintln!(
            "UserInfo from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("UserInfo"))
            .await;
    }

    // Look up all sessions for target username (case-insensitive)
    let target_sessions = ctx
        .user_manager
        .get_sessions_by_username(&requested_username)
        .await;

    if target_sessions.is_empty() {
        // User not found - send response with error
        let response = ServerMessage::UserInfoResponse {
            success: false,
            error: Some(err_user_not_found(ctx.locale, &requested_username)),
            user: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch target user account for admin status and created_at
    let target_account = match ctx.db.users.get_user_by_username(&requested_username).await {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserInfo"))
                .await;
        }
    };

    // Aggregate session data
    let session_ids: Vec<u32> = target_sessions.iter().map(|s| s.session_id).collect();
    let earliest_login = target_sessions.iter().map(|s| s.login_time).min().unwrap();
    let locale = target_sessions
        .first()
        .map(|s| s.locale.clone())
        .unwrap_or_else(|| "en".to_string());

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

    // Use the actual username from the database (preserves original casing)
    let actual_username = target_account.username.clone();

    // Build response with appropriate visibility level
    let user_info = if requesting_user.is_admin {
        // Admin gets all fields including target user's admin status and addresses
        UserInfoDetailed {
            username: actual_username,
            login_time: earliest_login,
            session_ids: session_ids.clone(),
            features,
            created_at: target_account.created_at,
            locale: locale.clone(),
            is_admin: Some(target_account.is_admin),
            addresses: Some(addresses),
        }
    } else {
        // Non-admin gets filtered fields
        UserInfoDetailed {
            username: actual_username,
            login_time: earliest_login,
            session_ids,
            features,
            created_at: target_account.created_at,
            locale,
            is_admin: None,
            addresses: None,
        }
    };

    let response = ServerMessage::UserInfoResponse {
        success: true,
        error: None,
        user: Some(user_info),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use crate::users::user::NewSessionParams;
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
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: user.id,
                username: "alice".to_string(),
                is_admin: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: user.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
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
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: user.id,
                username: "alice".to_string(),
                is_admin: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: user.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
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
            ServerMessage::UserInfoResponse {
                success,
                user,
                error,
            } => {
                assert!(!success, "Should not be successful");
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
        // Add requester to UserManager
        let requester_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: requester.id,
                username: "requester".to_string(),
                is_admin: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: requester.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
            .await;

        // Add target to UserManager
        let target_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: target.id,
                username: "target".to_string(),
                is_admin: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: target.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
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
            ServerMessage::UserInfoResponse {
                success,
                user,
                error,
            } => {
                assert!(success, "Should be successful");
                assert!(error.is_none(), "Should have no error");
                assert!(user.is_some(), "Should have user info");
                let user_info = user.unwrap();

                // Verify all basic fields are present
                assert_eq!(user_info.username, "target");
                assert_eq!(user_info.session_ids.len(), 1);
                assert_eq!(user_info.session_ids[0], target_id);
                assert_eq!(user_info.features, vec![FEATURE_CHAT.to_string()]);
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
        // Add admin to UserManager
        let admin_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin.id,
                username: "admin".to_string(),
                is_admin: true,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
            .await;

        // Add target to UserManager
        let target_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: target.id,
                username: "target".to_string(),
                is_admin: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: target.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
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
            ServerMessage::UserInfoResponse {
                success,
                user,
                error,
            } => {
                assert!(success, "Should be successful");
                assert!(error.is_none(), "Should have no error");
                assert!(user.is_some(), "Should have user info");
                let user_info = user.unwrap();

                // Verify all basic fields are present
                assert_eq!(user_info.username, "target");
                assert_eq!(user_info.session_ids.len(), 1);
                assert_eq!(user_info.session_ids[0], target_id);
                assert_eq!(user_info.features, vec![FEATURE_CHAT.to_string()]);
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

        // Add admin1 to UserManager
        let admin1_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin1.id,
                username: "admin1".to_string(),
                is_admin: true,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin1.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
            .await;

        // Add admin2 to UserManager
        let admin2_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin2.id,
                username: "admin2".to_string(),
                is_admin: true,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin2.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
            })
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
            ServerMessage::UserInfoResponse {
                success,
                user,
                error,
            } => {
                assert!(success, "Should be successful");
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

    #[tokio::test]
    async fn test_userinfo_case_insensitive() {
        let mut test_ctx = create_test_context().await;

        // Create admin user to make requests
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user with specific casing
        let _target_id = login_user(&mut test_ctx, "Alice", "password", &[], false).await;

        // Request user info with different casing
        let result = handle_userinfo(
            "alice".to_string(), // lowercase
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse {
                success,
                error,
                user,
            } => {
                assert!(success, "Case-insensitive lookup should succeed");
                assert!(error.is_none(), "Should not have error");
                assert!(user.is_some(), "Should return user info");

                let user_info = user.unwrap();
                // Username should be returned with original casing
                assert_eq!(user_info.username, "Alice");
            }
            _ => panic!("Expected UserInfoResponse, got: {:?}", response_msg),
        }
    }
}
