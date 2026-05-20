//! UserList message handler

use std::collections::HashMap;
use std::io;

use tracing::{error, warn};

use crate::constants::{
    HANDLER_USER_LIST, LOG_USER_LIST_DB_ERROR, LOG_USER_LIST_NOT_LOGGED_IN,
    LOG_USER_LIST_PERMISSION_DENIED,
};

/// Aggregated user data for deduplication of regular (non-shared) accounts.
///
/// Avatar/locale/group use latest login (stable selection).
/// Away/status use most recently active session (accurate presence).
struct UserAggregateData {
    login_time: i64,
    is_admin: bool,
    is_shared: bool,
    session_ids: Vec<u32>,
    locale: String,
    avatar: Option<String>,
    latest_session_login_time: i64,
    is_away: bool,
    status: Option<String>,
    group_id: Option<i64>,
    group_name: Option<String>,
    bandwidth_weight: u16,
    user_id: i64,
    most_recent_activity: std::time::Instant,
}

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ServerMessage, UserInfo};
use nexus_common::validators::resolve_bandwidth_weight;

use super::{HandlerContext, err_database, err_not_logged_in, err_permission_denied};
use crate::db::Permission;

/// `all=false`: online sessions only (requires `user_list`).
/// `all=true`: all DB accounts for the management panel (requires
/// `user_create` OR `user_edit` OR `user_delete`).
pub async fn handle_user_list<W>(
    all: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_LIST_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_LIST))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_LIST))
                .await;
        }
    };

    let has_permission = if all {
        requesting_user.has_permission(Permission::UserCreate)
            || requesting_user.has_permission(Permission::UserEdit)
            || requesting_user.has_permission(Permission::UserDelete)
    } else {
        requesting_user.has_permission(Permission::UserList)
    };

    if !has_permission {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, all = all, "{}", LOG_USER_LIST_PERMISSION_DENIED);
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some(HANDLER_USER_LIST))
            .await;
    }

    if all {
        let db_users = match ctx.db.users.get_all_users().await {
            Ok(users) => users,
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_LIST_DB_ERROR);
                return ctx
                    .send_error(&err_database(ctx.locale), Some(HANDLER_USER_LIST))
                    .await;
            }
        };

        // Fetch all groups once for O(1) lookup of name + weight (avoids N+1).
        let group_map: HashMap<i64, (String, u16)> = ctx
            .db
            .groups
            .get_all_groups()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|g| (g.id, (g.name, g.bandwidth_weight)))
            .collect();

        let mut user_infos: Vec<UserInfo> = db_users
            .into_iter()
            .map(|db_user| {
                let group_entry = db_user.group_id.and_then(|gid| group_map.get(&gid));
                let group_name = group_entry.map(|(name, _)| name.clone());
                let resolved_weight = resolve_bandwidth_weight(
                    db_user.bandwidth_weight,
                    group_entry.map(|(_, w)| *w),
                    db_user.is_admin,
                );
                UserInfo {
                    id: db_user.id,
                    nickname: db_user.username.clone(), // For accounts, nickname == username
                    username: db_user.username,
                    login_time: db_user.created_at,
                    is_admin: db_user.is_admin,
                    is_shared: db_user.is_shared,
                    session_ids: vec![], // /list all reports accounts, not sessions
                    locale: String::new(),
                    avatar: None,
                    is_away: false,
                    status: None,
                    group_id: db_user.group_id,
                    group_name,
                    bandwidth_weight: Some(resolved_weight),
                }
            })
            .collect();

        // Clients require a sorted list (case-insensitive by username).
        user_infos.sort_by_key(|u| u.username.to_lowercase());

        let response = ServerMessage::UserListResponse {
            success: true,
            error: None,
            users: Some(user_infos),
        };
        return ctx.send_message(&response).await;
    }

    let online_users = ctx.user_manager.get_all_users().await;

    // Regular accounts aggregate by username (one entry for all sessions);
    // shared accounts stay per-session (each has its own nickname).
    let mut user_map: HashMap<String, UserAggregateData> = HashMap::new();
    let mut shared_user_infos: Vec<UserInfo> = Vec::new();

    for user in online_users {
        if user.is_shared {
            shared_user_infos.push(UserInfo {
                id: user.user_id,
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
                group_id: user.group_id,
                group_name: user.group_name.clone(),
                bandwidth_weight: Some(
                    user.bandwidth_weight
                        .load(std::sync::atomic::Ordering::Relaxed),
                ),
            });
        } else {
            user_map
                .entry(user.username.clone())
                .and_modify(|agg| {
                    agg.login_time = agg.login_time.min(user.login_time);
                    agg.session_ids.push(user.session_id);
                    // Avatar/locale/group/weight track the latest login (stable).
                    if user.login_time > agg.latest_session_login_time {
                        agg.avatar = user.avatar.clone();
                        agg.locale = user.locale.clone();
                        agg.latest_session_login_time = user.login_time;
                        agg.group_id = user.group_id;
                        agg.group_name = user.group_name.clone();
                        agg.bandwidth_weight = user
                            .bandwidth_weight
                            .load(std::sync::atomic::Ordering::Relaxed);
                    }
                    // Away/status track the most recently active session (accurate presence).
                    if user.last_activity > agg.most_recent_activity {
                        agg.is_away = user.is_away;
                        agg.status = user.status.clone();
                        agg.most_recent_activity = user.last_activity;
                    }
                })
                .or_insert(UserAggregateData {
                    login_time: user.login_time,
                    is_admin: user.is_admin,
                    is_shared: false,
                    session_ids: vec![user.session_id],
                    locale: user.locale.clone(),
                    avatar: user.avatar.clone(),
                    latest_session_login_time: user.login_time,
                    is_away: user.is_away,
                    status: user.status.clone(),
                    group_id: user.group_id,
                    group_name: user.group_name.clone(),
                    bandwidth_weight: user
                        .bandwidth_weight
                        .load(std::sync::atomic::Ordering::Relaxed),
                    user_id: user.user_id,
                    most_recent_activity: user.last_activity,
                });
        }
    }

    let mut user_infos: Vec<UserInfo> = user_map
        .into_iter()
        .map(|(username, agg)| UserInfo {
            id: agg.user_id,
            nickname: username.clone(), // Regular account: nickname == username
            username,
            login_time: agg.login_time,
            is_admin: agg.is_admin,
            is_shared: agg.is_shared,
            session_ids: agg.session_ids,
            locale: agg.locale,
            avatar: agg.avatar,
            is_away: agg.is_away,
            status: agg.status,
            group_id: agg.group_id,
            group_name: agg.group_name,
            bandwidth_weight: Some(agg.bandwidth_weight),
        })
        .collect();

    user_infos.extend(shared_user_infos);

    // Clients require a sorted list (case-insensitive by display nickname).
    user_infos.sort_by_key(|u| u.nickname.to_lowercase());

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
    use crate::handlers::testing::{create_test_context, get_cached_password_hash, login_user};

    #[tokio::test]
    async fn test_userlist_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_list(false, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "UserList should require login");
    }

    #[tokio::test]
    async fn test_userlist_invalid_session() {
        let mut test_ctx = create_test_context().await;

        let invalid_session_id = Some(999);

        let result =
            handle_user_list(false, invalid_session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "UserList with invalid session should be rejected"
        );
    }

    #[tokio::test]
    async fn test_userlist_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;

        // Sends an error but does not disconnect.
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_userlist_with_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserList],
            false,
        )
        .await;

        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Valid userlist request should succeed");

        use crate::handlers::testing::read_server_message;
        let response = read_server_message(&mut test_ctx).await;
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

        // Admin has all permissions implicitly (no explicit UserList grant).
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(
            result.is_ok(),
            "Admin should be able to list users without explicit permission"
        );

        use crate::handlers::testing::read_server_message;
        let response = read_server_message(&mut test_ctx).await;
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

    #[tokio::test]
    async fn test_userlist_includes_avatar() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();

        let session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                user_id: account.id,
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
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let old_avatar = "data:image/png;base64,OLD_AVATAR".to_string();
        let new_avatar = "data:image/png;base64,NEW_AVATAR".to_string();

        // Session 1: old avatar, earlier login.
        let _session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                user_id: account.id,
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
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        // Login timestamps are second-granularity, so force a >1s gap.
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Session 2: new avatar, later login.
        let session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 2,
                user_id: account.id,
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
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        let result = handle_user_list(false, Some(session2), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                user_id: account.id,
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
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].avatar, None, "Avatar should be None");
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_all_requires_additional_permission() {
        let mut test_ctx = create_test_context().await;

        // UserList alone is not enough for all=true.
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserList],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;

        // Sends an error but does not disconnect.
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_userlist_all_with_user_edit_permission() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "UserList all with user_edit should succeed");

        let response = read_server_message(&mut test_ctx).await;
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

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(
            result.is_ok(),
            "UserList all with user_delete should succeed"
        );

        let response = read_server_message(&mut test_ctx).await;
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

        // bob exists in the DB but never logs in.
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let perms = db::Permissions::new();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

                assert!(users.iter().any(|u| u.username == "guest"));
                assert!(users.iter().any(|u| u.username == "bob"));
                assert!(users.iter().any(|u| u.username == "alice"));

                // DB accounts carry no session info; nickname == username.
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

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserCreate],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(
            result.is_ok(),
            "UserList all with user_create should succeed"
        );

        let response = read_server_message(&mut test_ctx).await;
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

        // Admin bypasses the user_edit/user_delete/user_create gate.
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "Admin should be able to list all users");

        let response = read_server_message(&mut test_ctx).await;
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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let hashed = get_cached_password_hash("sharedpass");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &crate::db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .expect("shared account creation should succeed");

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "UserList all should succeed");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserListResponse { success, users, .. } => {
                assert!(success);
                let users = users.expect("users should be present");

                assert_eq!(
                    users.len(),
                    3,
                    "Should have guest, admin and shared account"
                );

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
        // Real-world scenario: a mix of regular accounts, an admin, and an
        // offline shared account must sort case-insensitively by nickname.
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let perms = db::Permissions::new();

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "kalani",
                hashed_password: &hashed,
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "love",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "Lovelady",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "steve",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Offline shared account (no sessions).
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "quest",
            "password",
            &[db::Permission::UserEdit],
            true,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

                let usernames: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();

                // guest sorts between bob and kalani; shared between quest and steve.
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

        // Mixed-case names inserted out of order to exercise case-insensitive sort.
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let perms = db::Permissions::new();

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "Zebra",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "apple",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "Banana",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "cherry",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let session_id = login_user(
            &mut test_ctx,
            "Admin",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

                let nicknames: Vec<&str> = users.iter().map(|u| u.nickname.as_str()).collect();

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
    async fn test_userlist_away_status_most_recently_active_wins() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Session 1: older login, away=true.
        let session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                user_id: account.id,
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
                status: Some("session1 status".to_string()),
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add first session");

        // Login timestamps are second-granularity, so force a >1s gap.
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Session 2: newer login, away=false.
        let session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 2,
                user_id: account.id,
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
                status: Some("session2 status".to_string()),
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add second session");

        // Make the OLDER session most recently active — away/status should
        // follow activity, not login time.
        test_ctx.user_manager.update_last_activity(session1).await;

        let result = handle_user_list(false, Some(session2), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].session_ids.len(), 2, "Should have 2 sessions");
                assert!(
                    users[0].is_away,
                    "is_away should be from most recently active session (session1, true)"
                );
                assert_eq!(
                    users[0].status,
                    Some("session1 status".to_string()),
                    "status should be from most recently active session"
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

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Session 1: away=true.
        let session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 1,
                user_id: account.id,
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
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add first session");

        // Session 2: away=false.
        let _session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 2,
                user_id: account.id,
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
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add second session");

        let result = handle_user_list(false, Some(session1), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                // Shared sessions are not aggregated: one entry each, own away/status.
                assert_eq!(
                    users.len(),
                    2,
                    "Should have 2 separate entries for shared account sessions"
                );

                let user_one = users
                    .iter()
                    .find(|u| u.nickname == "user_one")
                    .expect("user_one should exist");
                let user_two = users
                    .iter()
                    .find(|u| u.nickname == "user_two")
                    .expect("user_two should exist");

                assert!(user_one.is_away, "user_one should be away");
                assert_eq!(user_one.status, Some("user one away".to_string()));
                assert!(!user_two.is_away, "user_two should NOT be away");
                assert_eq!(user_two.status, None);
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    #[tokio::test]
    async fn test_userlist_includes_group_fields_for_online_user() {
        use crate::handlers::testing::read_server_message;
        use crate::users::user::NewSessionParams;

        let mut test_ctx = create_test_context().await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let hashed = get_cached_password_hash("password");
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let effective = test_ctx
            .db
            .users
            .get_user_permissions(account.id)
            .await
            .unwrap();

        let session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 200,
                user_id: account.id,
                username: "bob".to_string(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                is_admin: false,
                is_shared: false,
                permissions: effective.permissions,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add bob");

        let result =
            handle_user_list(false, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                let bob_info = users
                    .iter()
                    .find(|u| u.username == "bob")
                    .expect("Bob should be in user list");
                assert_eq!(bob_info.group_id, Some(group.id), "Should include group_id");
                assert_eq!(
                    bob_info.group_name,
                    Some("Staff".to_string()),
                    "Should include group_name"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }

    /// Regression: `/list all` resolves bandwidth_weight without consulting
    /// the cache (the target may be offline). Admins with no per-user override
    /// must resolve to `DEFAULT_ADMIN_BANDWIDTH_WEIGHT`, not the inheritance
    /// baseline — admins skip group lookup entirely.
    #[tokio::test]
    async fn test_userlist_all_admin_bandwidth_weight_uses_admin_default() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Admin account created directly in the DB (offline, no cache).
        let hashed = get_cached_password_hash("password");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "ada",
                hashed_password: &hashed,
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Logged-in caller needs UserEdit (any /list all gate) but isn't admin.
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        let result =
            handle_user_list(true, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserListResponse { users, .. } => {
                let users = users.unwrap();
                let ada = users
                    .iter()
                    .find(|u| u.username == "ada")
                    .expect("ada should be in user list");
                assert!(ada.is_admin);
                assert_eq!(
                    ada.bandwidth_weight,
                    Some(nexus_common::validators::DEFAULT_ADMIN_BANDWIDTH_WEIGHT),
                    "offline admin must resolve to DEFAULT_ADMIN_BANDWIDTH_WEIGHT, not the inheritance baseline"
                );
            }
            _ => panic!("Expected UserListResponse"),
        }
    }
}
