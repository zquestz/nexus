//! UserList message handler

use std::collections::HashMap;
use std::io;

/// Aggregated user data for deduplication
/// Fields: (login_time, is_admin, is_shared, session_ids, locale, avatar, avatar_login_time)
type UserAggregateData = (i64, bool, bool, Vec<u32>, String, Option<String>, i64);

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

    // Separate handling for regular vs shared accounts:
    // - Regular accounts: aggregate by username (multiple sessions = one entry)
    // - Shared accounts: each session is a separate entry with its nickname
    let mut user_map: HashMap<String, UserAggregateData> = HashMap::new();
    let mut shared_user_infos: Vec<UserInfo> = Vec::new();

    for user in online_users {
        if user.is_shared {
            // Shared accounts are NOT aggregated - each session is a separate entry
            shared_user_infos.push(UserInfo {
                username: user.username.clone(),
                nickname: user.nickname.clone(),
                login_time: user.login_time,
                is_admin: false, // Shared accounts are never admin
                is_shared: true,
                session_ids: vec![user.session_id],
                locale: user.locale.clone(),
                avatar: user.avatar.clone(),
            });
        } else {
            // Regular accounts: deduplicate by username and aggregate sessions
            // Use is_admin from UserManager instead of querying DB for each user
            // Avatar uses "latest login wins" - track login_time for avatar selection
            user_map
                .entry(user.username.clone())
                .and_modify(
                    |(login_time, _, _, session_ids, _, avatar, avatar_login_time)| {
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
                    false,         // Regular accounts are not shared
                    vec![user.session_id],
                    user.locale.clone(),
                    user.avatar.clone(),
                    user.login_time, // Track login time for avatar selection
                ));
        }
    }

    // If `all` requested, merge in offline users from database
    // This includes both regular accounts and shared accounts that have no active sessions
    if all {
        match ctx.db.users.get_all_users().await {
            Ok(db_users) => {
                for db_user in db_users {
                    if db_user.is_shared {
                        // For shared accounts, check if there are any online sessions
                        // by looking for entries with matching username in shared_user_infos
                        let has_online_sessions = shared_user_infos
                            .iter()
                            .any(|u| u.username.to_lowercase() == db_user.username.to_lowercase());

                        if !has_online_sessions {
                            // Shared account with no online sessions - show the account itself
                            // so admins can manage it (edit/delete)
                            user_map.entry(db_user.username.clone()).or_insert((
                                db_user.created_at, // Use created_at as login_time
                                false,              // Shared accounts are never admin
                                true,               // This is a shared account
                                vec![],             // No session IDs (offline)
                                String::new(),      // No locale (offline)
                                None,               // No avatar (offline)
                                0,                  // No avatar login time
                            ));
                        }
                        // If there are online sessions, they're already in shared_user_infos
                        continue;
                    }
                    // Regular account: only add if not already in the map (i.e., offline)
                    user_map.entry(db_user.username.clone()).or_insert((
                        db_user.created_at, // Use created_at as login_time for offline users
                        db_user.is_admin,
                        false,         // Regular accounts are not shared
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

    // Build user info list from aggregated users (regular accounts + offline shared accounts)
    let mut user_infos: Vec<UserInfo> = user_map
        .into_iter()
        .map(
            |(username, (login_time, is_admin, is_shared, session_ids, locale, avatar, _))| {
                UserInfo {
                    username,
                    nickname: None, // Aggregated users don't have nicknames (shared accounts show as account name)
                    login_time,
                    is_admin,
                    is_shared,
                    session_ids,
                    locale,
                    avatar,
                }
            },
        )
        .collect();

    // Add shared account users (each session is a separate entry)
    user_infos.extend(shared_user_infos);

    // Sort: regular users by username, shared users by nickname (case-insensitive)
    user_infos.sort_by(|a, b| {
        let a_sort_key = a.nickname.as_ref().unwrap_or(&a.username).to_lowercase();
        let b_sort_key = b.nickname.as_ref().unwrap_or(&b.username).to_lowercase();
        a_sort_key.cmp(&b_sort_key)
    });

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
                nickname: None,
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
                nickname: None,
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
                nickname: None,
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
                nickname: None,
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
            .create_user("offline_bob", &hashed, false, false, true, &perms)
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

    #[tokio::test]
    async fn test_userlist_all_includes_offline_shared_accounts() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create an offline shared account directly in the database
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

        // Get all users - should include the offline shared account
        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "UserList all should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, users, .. } => {
                assert!(success);
                let users = users.expect("users should be present");

                // Should include the admin and the shared account
                assert!(
                    users.len() >= 2,
                    "Should have at least admin and shared account"
                );

                // Find the shared account
                let shared_account = users.iter().find(|u| u.username == "shared_acct");
                assert!(
                    shared_account.is_some(),
                    "Offline shared account should appear in all users list"
                );

                let shared = shared_account.unwrap();
                assert!(shared.is_shared, "Account should be marked as shared");
                assert!(!shared.is_admin, "Shared account should not be admin");
                assert!(
                    shared.session_ids.is_empty(),
                    "Offline shared account should have no sessions"
                );
                assert!(
                    shared.nickname.is_none(),
                    "Offline shared account should have no nickname"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_shared_account_with_online_sessions() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = crate::db::hash_password("sharedpass").unwrap();
        let shared_account = test_ctx
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

        // Add an online session for the shared account with a nickname
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: shared_account.id,
                username: "shared_acct".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: shared_account.created_at,
                tx,
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: Some("Alice".to_string()),
            })
            .await
            .expect("Failed to add shared user session");

        // Get all users
        let result = handle_user_list(
            true,
            Some(admin_session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "UserList all should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserListResponse { success, users, .. } => {
                assert!(success);
                let users = users.expect("users should be present");

                // Find the shared account session (should appear with nickname)
                let alice_session = users
                    .iter()
                    .find(|u| u.nickname.as_ref().map(|n| n == "Alice").unwrap_or(false));
                assert!(
                    alice_session.is_some(),
                    "Online shared account session should appear with nickname"
                );

                let alice = alice_session.unwrap();
                assert!(alice.is_shared, "Session should be marked as shared");
                assert_eq!(
                    alice.username, "shared_acct",
                    "Username should be the account name"
                );
                assert!(!alice.session_ids.is_empty(), "Should have session ID");

                // The offline "shared_acct" account entry should NOT appear separately
                // since there's an online session
                let offline_shared = users
                    .iter()
                    .filter(|u| {
                        u.username == "shared_acct"
                            && u.nickname.is_none()
                            && u.session_ids.is_empty()
                    })
                    .count();
                assert_eq!(
                    offline_shared, 0,
                    "Should not have separate offline entry when sessions are online"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }
}
