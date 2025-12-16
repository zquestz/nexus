//! UserList message handler

use std::collections::HashMap;
use std::io;

/// Aggregated user data for deduplication
/// Fields: (login_time, is_admin, session_ids, locale, avatar, avatar_login_time)
type UserAggregateData = (i64, bool, Vec<u32>, String, Option<String>, i64);

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ServerMessage, UserInfo};

use super::{HandlerContext, err_authentication, err_not_logged_in, err_permission_denied};
use crate::db::Permission;

/// Handle a userlist request from the client
///
/// If `all` is false (default), returns only online users.
/// If `all` is true, returns all users from database (online + offline).
/// The `all` option requires additional permissions: user_edit OR user_delete.
pub async fn handle_user_list<W>(
    all: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(id) = session_id else {
        eprintln!("UserList request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserList"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserList"))
                .await;
        }
    };

    // Permission check depends on whether requesting all users or just online users
    // - all: true -> requires user_create OR user_edit OR user_delete (for user management panel)
    // - all: false -> requires user_list (for online user list)
    let has_permission = if all {
        requesting_user.has_permission(Permission::UserCreate)
            || requesting_user.has_permission(Permission::UserEdit)
            || requesting_user.has_permission(Permission::UserDelete)
    } else {
        requesting_user.has_permission(Permission::UserList)
    };

    if !has_permission {
        eprintln!(
            "UserList (all={}) from {} (user: {}) without permission",
            all, ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("UserList"))
            .await;
    }

    // Fetch all connected users
    let online_users = ctx.user_manager.get_all_users().await;

    // Deduplicate by username and aggregate sessions
    // Use is_admin from UserManager instead of querying DB for each user
    // Avatar uses "latest login wins" - track login_time for avatar selection
    let mut user_map: HashMap<String, UserAggregateData> = HashMap::new();

    for user in online_users {
        user_map
            .entry(user.username.clone())
            .and_modify(
                |(login_time, _, session_ids, _, avatar, avatar_login_time)| {
                    // Keep earliest login time for display
                    *login_time = (*login_time).min(user.login_time);
                    session_ids.push(user.session_id);
                    // Avatar: latest login wins
                    if user.login_time > *avatar_login_time {
                        *avatar = user.avatar.clone();
                        *avatar_login_time = user.login_time;
                    }
                },
            )
            .or_insert((
                user.login_time,
                user.is_admin, // Use is_admin from UserManager
                vec![user.session_id],
                user.locale.clone(),
                user.avatar.clone(),
                user.login_time, // Track login time for avatar selection
            ));
    }

    // If `all` requested, merge in offline users from database
    if all {
        match ctx.db.users.get_all_users().await {
            Ok(db_users) => {
                for db_user in db_users {
                    // Only add users not already in the map (i.e., offline users)
                    user_map.entry(db_user.username.clone()).or_insert((
                        db_user.created_at, // Use created_at as login_time for offline users
                        db_user.is_admin,
                        vec![],        // No session IDs (offline)
                        String::new(), // No locale (offline)
                        None,          // No avatar (offline)
                        0,             // No avatar login time
                    ));
                }
            }
            Err(e) => {
                eprintln!("Failed to fetch all users from database: {}", e);
                // Continue with just online users rather than failing entirely
            }
        }
    }

    // Build deduplicated user info list
    let mut user_infos: Vec<UserInfo> = user_map
        .into_iter()
        .map(
            |(username, (login_time, is_admin, session_ids, locale, avatar, _))| UserInfo {
                username,
                login_time,
                is_admin,
                session_ids,
                locale,
                avatar,
            },
        )
        .collect();

    // Sort by username (case-insensitive) for consistent ordering
    user_infos.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));

    // Send user list response
    let response = ServerMessage::UserListResponse {
        success: true,
        error: None,
        users: Some(user_infos),
    };
    ctx.send_message(&response).await
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
        let result = handle_user_list(false, None, &mut test_ctx.handler_context()).await;

        // Should fail
        assert!(result.is_err(), "UserList should require login");
    }

    #[tokio::test]
    async fn test_userlist_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Use a session ID that doesn't exist in UserManager
        let invalid_session_id = Some(999);

        // Try to get user list with invalid session
        let result =
            handle_user_list(false, invalid_session_id, &mut test_ctx.handler_context()).await;

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
        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;

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
        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed
        assert!(result.is_ok(), "Valid userlist request should succeed");

        // Verify response contains the user
        use crate::handlers::testing::read_server_message;
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse {
                success,
                error,
                users,
            } => {
                assert!(success);
                assert!(error.is_none());
                let users = users.unwrap();
                assert_eq!(users.len(), 1, "Should have 1 user in the list");
                assert_eq!(users[0].username, "alice");
                assert_eq!(users[0].session_ids.len(), 1);
                assert_eq!(users[0].session_ids[0], session_id);
                assert!(!users[0].is_admin, "alice should not be admin");
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
        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Admin should be able to list users without explicit permission"
        );

        // Verify admin flag is set
        use crate::handlers::testing::read_server_message;
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse {
                success,
                error,
                users,
            } => {
                assert!(success);
                assert!(error.is_none());
                let users = users.unwrap();
                assert_eq!(users.len(), 1, "Should have 1 user in the list");
                assert_eq!(users[0].username, "admin");
                assert_eq!(users[0].session_ids.len(), 1);
                assert_eq!(users[0].session_ids[0], session_id);
                assert!(users[0].is_admin, "admin should have is_admin=true");
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    // =========================================================================
    // Avatar aggregation tests
    // =========================================================================

    #[tokio::test]
    async fn test_userlist_includes_avatar() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        // Create user with avatar
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &perms)
            .await
            .unwrap();

        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();

        // Add session with avatar
        let session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(avatar_data.clone()),
            })
            .await;

        // Get user list
        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                assert_eq!(users.len(), 1);
                assert_eq!(
                    users[0].avatar,
                    Some(avatar_data),
                    "Avatar should be included"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_avatar_latest_login_wins() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        // Create user
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &perms)
            .await
            .unwrap();

        let old_avatar = "data:image/png;base64,OLD_AVATAR".to_string();
        let new_avatar = "data:image/png;base64,NEW_AVATAR".to_string();

        // Add first session with old avatar (earlier login time)
        let _session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(old_avatar.clone()),
            })
            .await;

        // Delay of 1.1 seconds to ensure different login timestamps (timestamps are in seconds)
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Add second session with new avatar (later login time)
        let session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 2,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(new_avatar.clone()),
            })
            .await;

        // Get user list
        let result = handle_user_list(false, Some(session2), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].session_ids.len(), 2, "Should have 2 sessions");
                assert_eq!(
                    users[0].avatar,
                    Some(new_avatar),
                    "Avatar should be from latest login"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_no_avatar() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        // Create user without avatar
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &perms)
            .await
            .unwrap();

        // Add session without avatar
        let session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
            })
            .await;

        // Get user list
        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].avatar, None, "Avatar should be None");
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    // =========================================================================
    // /list all tests
    // =========================================================================

    #[tokio::test]
    async fn test_userlist_all_requires_additional_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH UserList permission but WITHOUT user_edit or user_delete
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserList],
            false,
        )
        .await;

        // Try to get all users - should fail due to missing permission
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed (send error but not disconnect)
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_userlist_all_with_user_edit_permission() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create user WITH user_edit permission (user_list not needed for all: true)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Get all users - should succeed
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "UserList all with user_edit should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_with_user_delete_permission() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create user WITH user_delete permission (user_list not needed for all: true)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        // Get all users - should succeed
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(
            result.is_ok(),
            "UserList all with user_delete should succeed"
        );

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_includes_offline_users() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create an offline user in the database (not logged in)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let perms = db::Permissions::new();
        test_ctx
            .db
            .users
            .create_user("offline_bob", &hashed, false, true, &perms)
            .await
            .unwrap();

        // Create an online user with necessary permissions (user_list not needed for all: true)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Get all users (including offline)
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse {
                success,
                error,
                users,
            } => {
                assert!(success);
                assert!(error.is_none());
                let users = users.unwrap();
                assert_eq!(users.len(), 2, "Should have 2 users (1 online, 1 offline)");

                // Find the offline user
                let offline_user = users.iter().find(|u| u.username == "offline_bob");
                assert!(offline_user.is_some(), "Offline user should be in list");
                let offline_user = offline_user.unwrap();
                assert!(
                    offline_user.session_ids.is_empty(),
                    "Offline user should have no session IDs"
                );
                assert!(
                    offline_user.avatar.is_none(),
                    "Offline user should have no avatar"
                );

                // Find the online user
                let online_user = users.iter().find(|u| u.username == "alice");
                assert!(online_user.is_some(), "Online user should be in list");
                let online_user = online_user.unwrap();
                assert!(
                    !online_user.session_ids.is_empty(),
                    "Online user should have session IDs"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_with_user_create_permission() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create user WITH user_create permission (user_list not needed for all: true)
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserCreate],
            false,
        )
        .await;

        // Get all users - should succeed
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(
            result.is_ok(),
            "UserList all with user_create should succeed"
        );

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_admin_bypass() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Admin should be able to list all users without explicit user_edit/user_delete/user_create
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Get all users - should succeed (admin bypass)
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "Admin should be able to list all users");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserListResponse"),
        }
    }
}
