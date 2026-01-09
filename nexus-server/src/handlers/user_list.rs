//! UserList message handler

use std::collections::HashMap;
use std::io;

/// Aggregated user data for deduplication
/// Fields: (login_time, is_admin, is_shared, session_ids, locale, avatar, latest_session_login_time, is_away, status)
/// Note: avatar, is_away, and status all use "latest login wins" - tracked via latest_session_login_time
type UserAggregateData = (
    i64,
    bool,
    bool,
    Vec<u32>,
    String,
    Option<String>,
    i64,
    bool,
    Option<String>,
);

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ServerMessage, UserInfo};

use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
};
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

    // Handle "all" (database accounts) vs "online only" (connected sessions) separately
    if all {
        // For /list all: return all accounts from database sorted alphabetically
        // This is used by the user management panel and /list all command
        let db_users = match ctx.db.users.get_all_users().await {
            Ok(users) => users,
            Err(e) => {
                eprintln!("Failed to fetch all users from database: {}", e);
                return ctx
                    .send_error(&err_database(ctx.locale), Some("UserList"))
                    .await;
            }
        };

        // Convert to UserInfo and sort by username (nickname == username for accounts)
        let mut user_infos: Vec<UserInfo> = db_users
            .into_iter()
            .map(|db_user| UserInfo {
                nickname: db_user.username.clone(), // For accounts, nickname == username
                username: db_user.username,
                login_time: db_user.created_at,
                is_admin: db_user.is_admin,
                is_shared: db_user.is_shared,
                session_ids: vec![], // Not tracking online status for /list all
                locale: String::new(),
                avatar: None,
                is_away: false,
                status: None,
            })
            .collect();

        // Sort by username case-insensitively
        user_infos.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));

        let response = ServerMessage::UserListResponse {
            success: true,
            error: None,
            users: Some(user_infos),
        };
        return ctx.send_message(&response).await;
    }

    // For online user list: aggregate connected sessions
    let online_users = ctx.user_manager.get_all_users().await;

    // Separate handling for regular vs shared accounts:
    // - Regular accounts: aggregate by username (multiple sessions = one entry)
    // - Shared accounts: each session is a separate entry with its nickname
    let mut user_map: HashMap<String, UserAggregateData> = HashMap::new();
    let mut shared_user_infos: Vec<UserInfo> = Vec::new();

    for user in online_users {
        if user.is_shared {
            // Shared accounts are NOT aggregated - each session is a separate entry
            // For shared accounts, nickname is the session's display name
            shared_user_infos.push(UserInfo {
                username: user.username.clone(),
                nickname: user.nickname.clone(),
                login_time: user.login_time,
                is_admin: false, // Shared accounts are never admin
                is_shared: true,
                session_ids: vec![user.session_id],
                locale: user.locale.clone(),
                avatar: user.avatar.clone(),
                is_away: user.is_away,
                status: user.status.clone(),
            });
        } else {
            // Regular accounts: deduplicate by username and aggregate sessions
            // Use is_admin from UserManager instead of querying DB for each user
            // Avatar uses "latest login wins" - track login_time for avatar selection
            user_map
                .entry(user.username.clone())
                .and_modify(
                    |(
                        login_time,
                        _,
                        _,
                        session_ids,
                        _,
                        avatar,
                        latest_session_login_time,
                        is_away,
                        status,
                    )| {
                        // Keep earliest login time for display
                        *login_time = (*login_time).min(user.login_time);
                        session_ids.push(user.session_id);
                        // Avatar, away status, and status message: latest login wins
                        if user.login_time > *latest_session_login_time {
                            *avatar = user.avatar.clone();
                            *latest_session_login_time = user.login_time;
                            *is_away = user.is_away;
                            *status = user.status.clone();
                        }
                    },
                )
                .or_insert((
                    user.login_time,
                    user.is_admin, // Use is_admin from UserManager
                    false,         // Regular accounts are not shared
                    vec![user.session_id],
                    user.locale.clone(),
                    user.avatar.clone(),
                    user.login_time, // Track login time for avatar selection
                    user.is_away,
                    user.status.clone(),
                ));
        }
    }

    // Build user info list from aggregated online users
    let mut user_infos: Vec<UserInfo> = user_map
        .into_iter()
        .map(
            |(
                username,
                (login_time, is_admin, is_shared, session_ids, locale, avatar, _, is_away, status),
            )| {
                UserInfo {
                    // For regular accounts, nickname == username
                    nickname: username.clone(),
                    username,
                    login_time,
                    is_admin,
                    is_shared,
                    session_ids,
                    locale,
                    avatar,
                    is_away,
                    status,
                }
            },
        )
        .collect();

    // Add shared account sessions (each session is a separate entry)
    user_infos.extend(shared_user_infos);

    // Sort by nickname (display name) case-insensitively
    user_infos.sort_by(|a, b| a.nickname.to_lowercase().cmp(&b.nickname.to_lowercase()));

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
            .create_user("alice", &hashed, false, false, true, &perms)
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
                is_shared: false,
                permissions: perms.permissions.clone(),
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
            .create_user("alice", &hashed, false, false, true, &perms)
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
                is_shared: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(old_avatar.clone()),
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

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
                is_shared: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: Some(new_avatar.clone()),
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

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
            .create_user("alice", &hashed, false, false, true, &perms)
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
                is_shared: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

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
    async fn test_userlist_all_returns_database_accounts() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create a user in the database (not logged in)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let perms = db::Permissions::new();
        test_ctx
            .db
            .users
            .create_user("bob", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Create a logged-in user with necessary permissions
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Get all users - returns database accounts, not sessions
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
                assert_eq!(users.len(), 3, "Should have 3 accounts (guest, alice, bob)");

                // All accounts should be present
                assert!(users.iter().any(|u| u.username == "guest"));
                assert!(users.iter().any(|u| u.username == "bob"));
                assert!(users.iter().any(|u| u.username == "alice"));

                // /list all returns database accounts - no session info
                for user in &users {
                    assert!(user.session_ids.is_empty(), "Accounts have no session IDs");
                    assert_eq!(
                        user.nickname, user.username,
                        "nickname == username for accounts"
                    );
                }
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

    #[tokio::test]
    async fn test_userlist_all_includes_shared_accounts() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = crate::db::hash_password("sharedpass").unwrap();
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false, // not admin
                true,  // is_shared
                true,  // enabled
                &crate::db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Get all users - returns database accounts
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "UserList all should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, users, .. } => {
                assert!(success);
                let users = users.expect("users should be present");

                // Should include guest, the admin, and the shared account
                assert_eq!(
                    users.len(),
                    3,
                    "Should have guest, admin and shared account"
                );

                // Find the shared account
                let shared_account = users.iter().find(|u| u.username == "shared_acct");
                assert!(shared_account.is_some(), "Shared account should be in list");

                let shared = shared_account.unwrap();
                assert!(shared.is_shared, "Account should be marked as shared");
                assert!(!shared.is_admin, "Shared account should not be admin");
                assert_eq!(
                    shared.nickname, shared.username,
                    "nickname == username for accounts"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_sorted_alphabetically_with_shared_account() {
        // Test case based on real-world scenario:
        // alice, bob, shared (offline shared account), @kalani, love, Lovelady, @quest, steve
        // Expected sorted: alice, bob, @kalani, love, Lovelady, @quest, shared, steve
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let perms = db::Permissions::new();

        // Create regular users
        test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("bob", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("kalani", &hashed, true, false, true, &perms) // admin
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("love", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("Lovelady", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("steve", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Create shared account (offline - no sessions)
        test_ctx
            .db
            .users
            .create_user("shared", &hashed, false, true, true, &perms)
            .await
            .unwrap();

        // Create admin user to make the request
        let session_id = login_user(
            &mut test_ctx,
            "quest",
            "password",
            &[db::Permission::UserEdit],
            true, // admin
        )
        .await;

        // Get all users
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
                assert_eq!(users.len(), 9, "Should have 9 users (including guest)");

                // Extract usernames in order
                let usernames: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();

                // Verify alphabetical order (case-insensitive)
                // guest comes between bob and kalani, shared comes between quest and steve
                assert_eq!(
                    usernames,
                    vec![
                        "alice", "bob", "guest", "kalani", "love", "Lovelady", "quest", "shared",
                        "steve"
                    ],
                    "Users should be sorted alphabetically by nickname (case-insensitive)"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_sorted_alphabetically() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create users with names that would be out of order if not sorted
        // Using different cases to verify case-insensitive sorting
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let perms = db::Permissions::new();

        // Create users in non-alphabetical order
        test_ctx
            .db
            .users
            .create_user("Zebra", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("apple", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("Banana", &hashed, false, false, true, &perms)
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("cherry", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Create an admin user to make the request
        let session_id = login_user(
            &mut test_ctx,
            "Admin",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Get all users
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
                assert_eq!(users.len(), 6, "Should have 6 users (including guest)");

                // Extract nicknames in order (nickname == username for regular accounts)
                let nicknames: Vec<&str> = users.iter().map(|u| u.nickname.as_str()).collect();

                // Verify alphabetical order (case-insensitive)
                assert_eq!(
                    nicknames,
                    vec!["Admin", "apple", "Banana", "cherry", "guest", "Zebra"],
                    "Users should be sorted alphabetically by nickname (case-insensitive)"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_away_status_latest_login_wins() {
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
            .create_user("alice", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Add first session (older) with is_away=true and old status
        let _session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: true,
                status: Some("old status".to_string()),
            })
            .await
            .expect("Failed to add first session");

        // Delay to ensure different login timestamps (timestamps are in seconds)
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Add second session (newer) with is_away=false and new status
        let session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 2,
                db_user_id: account.id,
                username: "alice".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: false,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: false,
                status: Some("new status".to_string()),
            })
            .await
            .expect("Failed to add second session");

        // Get user list
        let result = handle_user_list(false, Some(session2), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].session_ids.len(), 2, "Should have 2 sessions");
                assert!(
                    !users[0].is_away,
                    "is_away should be from latest login (false)"
                );
                assert_eq!(
                    users[0].status,
                    Some("new status".to_string()),
                    "status should be from latest login"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_shared_account_no_aggregation() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        // Create shared account
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user("shared_acct", &hashed, false, true, true, &perms)
            .await
            .unwrap();

        // Add first session with is_away=true
        let session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                db_user_id: account.id,
                username: "shared_acct".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: true,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "user_one".to_string(),
                is_away: true,
                status: Some("user one away".to_string()),
            })
            .await
            .expect("Failed to add first session");

        // Add second session with is_away=false
        let _session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 2,
                db_user_id: account.id,
                username: "shared_acct".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: true,
                permissions: perms.permissions.clone(),
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "user_two".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add second session");

        // Get user list
        let result = handle_user_list(false, Some(session1), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                // Shared accounts are NOT aggregated - each session is separate
                assert_eq!(
                    users.len(),
                    2,
                    "Should have 2 separate entries for shared account sessions"
                );

                // Find each user by nickname
                let user_one = users
                    .iter()
                    .find(|u| u.nickname == "user_one")
                    .expect("user_one should exist");
                let user_two = users
                    .iter()
                    .find(|u| u.nickname == "user_two")
                    .expect("user_two should exist");

                // Each should have their own away/status
                assert!(user_one.is_away, "user_one should be away");
                assert_eq!(user_one.status, Some("user one away".to_string()));
                assert!(!user_two.is_away, "user_two should NOT be away");
                assert_eq!(user_two.status, None);
            }
            _ => panic!("Expected UserListResponse"),
        }
    }
}
