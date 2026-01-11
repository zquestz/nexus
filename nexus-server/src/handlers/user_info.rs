//! UserInfo message handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ServerMessage, UserInfoDetailed};
use nexus_common::validators::{self, NicknameError};

use super::{
    HandlerContext, err_authentication, err_database, err_nickname_empty, err_nickname_invalid,
    err_nickname_not_online, err_nickname_too_long, err_not_logged_in, err_permission_denied,
};
use crate::constants::DEFAULT_LOCALE;
use crate::db::Permission;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
#[cfg(test)]
use crate::constants::FEATURE_CHAT;

/// Handle a userinfo request from the client
pub async fn handle_user_info<W>(
    nickname: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(id) = session_id else {
        eprintln!("UserInfo request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserInfo"))
            .await;
    };

    // Validate nickname format
    if let Err(e) = validators::validate_nickname(&nickname) {
        let error_msg = match e {
            NicknameError::Empty => err_nickname_empty(ctx.locale),
            NicknameError::TooLong => {
                err_nickname_too_long(ctx.locale, validators::MAX_NICKNAME_LENGTH)
            }
            NicknameError::InvalidCharacters => err_nickname_invalid(ctx.locale),
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

    // Get all sessions by nickname
    // - Regular accounts: nickname == username, so all sessions are returned
    // - Shared accounts: unique nickname, so only that session is returned
    let target_sessions = ctx.user_manager.get_sessions_by_nickname(&nickname).await;

    // Check if user is online
    if target_sessions.is_empty() {
        let response = ServerMessage::UserInfoResponse {
            success: false,
            error: Some(err_nickname_not_online(ctx.locale, &nickname)),
            user: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get username for database lookup from the session
    let db_lookup_username = target_sessions
        .first()
        .map(|s| s.username.clone())
        .expect("target_sessions is non-empty");

    // Fetch target user account for admin status and created_at
    let target_account = match ctx.db.users.get_user_by_username(&db_lookup_username).await {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserInfo"))
                .await;
        }
    };

    // Aggregate session data
    let session_ids: Vec<u32> = target_sessions.iter().map(|s| s.session_id).collect();
    let earliest_login = target_sessions
        .iter()
        .map(|s| s.login_time)
        .min()
        .expect("target_sessions is non-empty");
    let locale = target_sessions
        .first()
        .map(|s| s.locale.clone())
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string());

    // Collect unique features from all sessions
    let mut all_features = std::collections::HashSet::new();
    for session in &target_sessions {
        for feature in &session.features {
            all_features.insert(feature.clone());
        }
    }
    let features: Vec<String> = all_features.into_iter().collect();

    // Get avatar from most recent login session ("latest login wins")
    let avatar = target_sessions
        .iter()
        .max_by_key(|s| s.login_time)
        .and_then(|s| s.avatar.clone());

    // Get away status from most recent login session ("latest login wins")
    let (is_away, status) = target_sessions
        .iter()
        .max_by_key(|s| s.login_time)
        .map(|s| (s.is_away, s.status.clone()))
        .unwrap_or((false, None));

    // Get nickname (display name) for the user from the session
    // (nickname is always populated - equals username for regular accounts)
    let display_nickname = target_sessions
        .first()
        .map(|s| s.nickname.clone())
        .unwrap_or_else(|| target_account.username.clone());

    // Collect IP addresses from all sessions (for admins only)
    let addresses: Vec<String> = target_sessions
        .iter()
        .map(|s| s.address.to_string())
        .collect();

    // Use the actual username from the database (preserves original casing)
    let actual_username = target_account.username.clone();

    // Build response with appropriate visibility level
    // is_admin is visible to everyone (same as in user list)
    // addresses are only visible to admins
    let user_info = if requesting_user.is_admin {
        // Admin gets all fields including addresses
        UserInfoDetailed {
            username: actual_username,
            nickname: display_nickname,
            login_time: earliest_login,
            is_shared: target_account.is_shared,
            session_ids,
            features,
            created_at: target_account.created_at,
            locale,
            avatar,
            is_admin: Some(target_account.is_admin),
            addresses: Some(addresses),
            is_away,
            status,
        }
    } else {
        // Non-admin gets all fields except addresses
        UserInfoDetailed {
            username: actual_username,
            nickname: display_nickname,
            login_time: earliest_login,
            is_shared: target_account.is_shared,
            session_ids,
            features,
            created_at: target_account.created_at,
            locale,
            avatar,
            is_admin: Some(target_account.is_admin),
            addresses: None,
            is_away,
            status,
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
    use crate::handlers::testing::{
        create_test_context, get_cached_password_hash, login_user, read_server_message,
    };
    use crate::users::user::NewSessionParams;

    #[tokio::test]
    async fn test_userinfo_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to get user info without being logged in
        let result =
            handle_user_info("alice".to_string(), None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "UserInfo should require login");
    }

    #[tokio::test]
    async fn test_userinfo_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserInfo permission (non-admin)
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let user = test_ctx
            .db
            .users
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
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
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: user.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Try to get user info without permission
        let result = handle_user_info(
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
        let hashed = get_cached_password_hash(password);
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
            .create_user("alice", &hashed, false, false, true, &perms)
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
                is_shared: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: user.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Request info for non-existent username
        let result = handle_user_info(
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

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
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
                // We don't distinguish "not found" from "not online" for security
                assert!(
                    error_msg.contains("not online"),
                    "Error should mention user not online, got: {}",
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
        let hashed = get_cached_password_hash(password);
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
            .create_user("requester", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Create target user
        let target = test_ctx
            .db
            .users
            .create_user(
                "target",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
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
                is_shared: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: requester.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "requester".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Add target to UserManager
        let target_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: target.id,
                username: "target".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: target.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "target".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Request info about target as non-admin
        let result = handle_user_info(
            "target".to_string(),
            Some(requester_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Should successfully get user info");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
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

                // Verify is_admin is visible to everyone (same as user list)
                assert!(
                    user_info.is_admin.is_some(),
                    "is_admin should be visible to all users"
                );
                assert!(
                    !user_info.is_admin.unwrap(),
                    "Target user should not be admin"
                );
                // Verify addresses are NOT visible to non-admins
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
        let hashed = get_cached_password_hash(password);
        let admin = test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create target user (non-admin)
        let target = test_ctx
            .db
            .users
            .create_user(
                "target",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
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
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Add target to UserManager
        let target_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: target.id,
                username: "target".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: target.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![FEATURE_CHAT.to_string()],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "target".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Admin requests info about target
        // Request info about target as admin
        let result = handle_user_info(
            "target".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Should successfully get user info");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
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
        let hashed = get_cached_password_hash(password);
        let admin1 = test_ctx
            .db
            .users
            .create_user(
                "admin1",
                &hashed,
                true,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        let admin2 = test_ctx
            .db
            .users
            .create_user(
                "admin2",
                &hashed,
                true,
                false,
                true,
                &db::Permissions::new(),
            )
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
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin1.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin1".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Add admin2 to UserManager
        let admin2_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin2.id,
                username: "admin2".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin2.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin2".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Admin1 requests info about admin2
        let result = handle_user_info(
            "admin2".to_string(),
            Some(admin1_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Should successfully get user info");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
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
        let result = handle_user_info(
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

    // =========================================================================
    // Avatar tests
    // =========================================================================

    #[tokio::test]
    async fn test_userinfo_includes_avatar() {
        let mut test_ctx = create_test_context().await;

        // Create admin user to make requests
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user with avatar
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let account = test_ctx
            .db
            .users
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &crate::db::Permissions::new(),
            )
            .await
            .unwrap();

        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();

        // Add session with avatar
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 100,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(avatar_data.clone()),
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Request user info
        let result = handle_user_info(
            "alice".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse { user, .. } => {
                let user_info = user.unwrap();
                assert_eq!(
                    user_info.avatar,
                    Some(avatar_data),
                    "Avatar should be included"
                );
            }
            _ => panic!("Expected UserInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_userinfo_avatar_latest_login_wins() {
        let mut test_ctx = create_test_context().await;

        // Create admin user to make requests
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let account = test_ctx
            .db
            .users
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &crate::db::Permissions::new(),
            )
            .await
            .unwrap();

        let old_avatar = "data:image/png;base64,OLD_AVATAR".to_string();
        let new_avatar = "data:image/png;base64,NEW_AVATAR".to_string();

        // Add first session with old avatar
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 100,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(old_avatar),
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Delay of 1.1 seconds to ensure different login timestamps (timestamps are in seconds)
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Add second session with new avatar (later login time)
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 101,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(new_avatar.clone()),
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Request user info
        let result = handle_user_info(
            "alice".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse { user, .. } => {
                let user_info = user.unwrap();
                assert_eq!(user_info.session_ids.len(), 2, "Should have 2 sessions");
                assert_eq!(
                    user_info.avatar,
                    Some(new_avatar),
                    "Avatar should be from latest login"
                );
            }
            _ => panic!("Expected UserInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_userinfo_no_avatar() {
        let mut test_ctx = create_test_context().await;

        // Create admin user to make requests
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user without avatar
        let _target_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Request user info
        let result = handle_user_info(
            "alice".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse { user, .. } => {
                let user_info = user.unwrap();
                assert_eq!(user_info.avatar, None, "Avatar should be None");
            }
            _ => panic!("Expected UserInfoResponse"),
        }
    }

    // ========================================================================
    // Shared Account Tests
    // ========================================================================

    #[tokio::test]
    async fn test_userinfo_shared_account_lookup_by_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create admin user to make requests
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = get_cached_password_hash("password");
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
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
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

        // Look up by nickname
        let result = handle_user_info(
            "Nick1".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse { success, user, .. } => {
                assert!(success);
                let user_info = user.unwrap();
                assert_eq!(
                    user_info.username, "shared_acct",
                    "Should return the account username"
                );
                assert_eq!(
                    user_info.nickname,
                    "Nick1".to_string(),
                    "Should include nickname"
                );
                assert!(user_info.is_shared, "Should be marked as shared");
                assert_eq!(user_info.session_ids.len(), 1, "Should have one session");
            }
            _ => panic!("Expected UserInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_userinfo_shared_account_lookup_by_username_fails() {
        let mut test_ctx = create_test_context().await;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create admin user to make requests
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = get_cached_password_hash("password");
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
            locale: DEFAULT_TEST_LOCALE.to_string(),
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

        // Look up by username (not nickname) - should fail
        // Shared accounts can only be looked up by their display name (nickname)
        let result = handle_user_info(
            "shared_acct".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse { success, error, .. } => {
                assert!(
                    !success,
                    "Looking up shared account by username should fail"
                );
                assert!(error.is_some(), "Should have error message");
            }
            _ => panic!("Expected UserInfoResponse"),
        }
    }

    #[tokio::test]
    async fn test_userinfo_regular_account_is_shared_false() {
        let mut test_ctx = create_test_context().await;

        // Create admin user to make requests
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create regular (non-shared) user
        let _target_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Request user info
        let result = handle_user_info(
            "alice".to_string(),
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserInfoResponse { success, user, .. } => {
                assert!(success);
                let user_info = user.unwrap();
                assert!(!user_info.is_shared, "Regular account should not be shared");
                assert_eq!(
                    user_info.nickname, user_info.username,
                    "Regular account should have nickname == username"
                );
            }
            _ => panic!("Expected UserInfoResponse"),
        }
    }
}
