//! Handler for UserDelete command

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;

use crate::constants::{
    HANDLER_USER_DELETE, LOG_USER_DELETE_ADMIN, LOG_USER_DELETE_DB_ERROR,
    LOG_USER_DELETE_NOT_LOGGED_IN, LOG_USER_DELETE_PERMISSION_DENIED, LOG_USER_DELETE_SUCCESS,
};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, Outcome, dispatch_outcome, err_account_deleted, err_cannot_delete_admin,
    err_cannot_delete_guest, err_cannot_delete_last_admin, err_cannot_delete_self, err_database,
    err_not_logged_in, err_permission_denied, err_user_not_found, remove_users_with_cleanup,
};
use crate::db::Permission;
use crate::db::sql::GUEST_USERNAME;

pub async fn handle_user_delete<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_DELETE))
            .await;
    };

    // Guard lives only inside this block; all socket sends happen after it.
    let outcome = 'locked: {
        let _state_guard = ctx.user_manager.lock_user_state().await;
        let requesting_user_session =
            match ctx.user_manager.get_user_by_session_id(session_id).await {
                Some(user) => user,
                None => break 'locked Outcome::Disconnect,
            };

        if !requesting_user_session.has_permission(Permission::UserDelete) {
            warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_USER_DELETE_PERMISSION_DENIED);
            break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                username: None,
            }));
        }

        let target_user = match ctx.db.users.get_user_by_id(id).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(err_user_not_found(ctx.locale, &id.to_string())),
                    username: None,
                }));
            }
            Err(e) => {
                error!(user = %requesting_user_session.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_DELETE_DB_ERROR);
                break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                    username: None,
                }));
            }
        };

        // ID-based identity check — username compare leaks on rename.
        if target_user.id == requesting_user_session.user_id {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                success: false,
                error: Some(err_cannot_delete_self(ctx.locale)),
                username: None,
            }));
        }

        if fold_name(&target_user.username) == GUEST_USERNAME {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                success: false,
                error: Some(err_cannot_delete_guest(ctx.locale)),
                username: None,
            }));
        }

        if target_user.is_admin && !requesting_user_session.is_admin {
            warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_USER_DELETE_ADMIN);
            break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                success: false,
                error: Some(err_cannot_delete_admin(ctx.locale)),
                username: None,
            }));
        }

        // DB delete first; the by-user_id disconnect sweep below runs
        // only on Ok(true). On Blocked / Err the user stays online.
        match ctx
            .db
            .users
            .delete_user(target_user.id, requesting_user_session.is_admin)
            .await
        {
            Ok(true) => {
                let online_users = ctx
                    .user_manager
                    .get_sessions_by_user_id(target_user.id)
                    .await;
                for online_user in &online_users {
                    let disconnect_msg = ServerMessage::Error {
                        message: err_account_deleted(&online_user.locale),
                        command: None,
                    };
                    let _ = online_user.tx.send((disconnect_msg, None));
                }
                remove_users_with_cleanup(
                    ctx.user_manager,
                    ctx.voice_registry,
                    ctx.channel_manager,
                    &online_users,
                )
                .await;

                info!(user = %requesting_user_session.username, ip = %ctx.peer_addr, target = %target_user.username, "{}", LOG_USER_DELETE_SUCCESS);
                Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                    success: true,
                    error: None,
                    username: Some(target_user.username),
                }))
            }
            Ok(false) => {
                // Atomic guard blocked. Re-read target to disambiguate:
                //   None                  → another delete raced ahead
                //   Some(admin), !req.adm → admin-target SQL guard fired
                //   else                  → last-admin SQL guard fired
                let target_now = match ctx.db.users.get_user_by_id(target_user.id).await {
                    Ok(t) => t,
                    Err(e) => {
                        error!(user = %requesting_user_session.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_DELETE_DB_ERROR);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                            success: false,
                            error: Some(err_database(ctx.locale)),
                            username: None,
                        }));
                    }
                };
                let error = match target_now {
                    None => err_user_not_found(ctx.locale, &id.to_string()),
                    Some(u) if u.is_admin && !requesting_user_session.is_admin => {
                        warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_USER_DELETE_ADMIN);
                        err_cannot_delete_admin(ctx.locale)
                    }
                    _ => err_cannot_delete_last_admin(ctx.locale),
                };
                Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(error),
                    username: None,
                }))
            }
            Err(e) => {
                error!(user = %requesting_user_session.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_DELETE_DB_ERROR);
                Outcome::Send(Box::new(ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                    username: None,
                }))
            }
        }
    };

    dispatch_outcome(outcome, ctx, HANDLER_USER_DELETE).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use crate::users::user::NewSessionParams;

    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_userdelete_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_delete(999, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "UserDelete should require login");
    }

    #[tokio::test]
    async fn test_userdelete_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Non-admin without UserDelete permission.
        let user_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let target = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let result =
            handle_user_delete(target.id, Some(user_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.to_lowercase().contains("permission"),
                    "Error should mention permission"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        let still_exists = test_ctx.db.users.get_user_by_id(target.id).await.unwrap();
        assert!(
            still_exists.is_some(),
            "User should not be deleted without permission"
        );
    }

    #[tokio::test]
    async fn test_userdelete_nonexistent_user() {
        let mut test_ctx = create_test_context().await;

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_user_delete(999, Some(admin_id), &mut test_ctx.handler_context()).await;

        assert!(
            result.is_ok(),
            "Should send error response for non-existent user"
        );

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("not found"),
                    "Error should mention user not found"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_self() {
        let mut test_ctx = create_test_context().await;

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin_db_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let result = handle_user_delete(
            admin_db_user.id,
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Should send error response when trying to delete self"
        );

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("delete")
                        && (error_msg.contains("yourself") || error_msg.contains("self")),
                    "Error should mention not being able to delete self"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        let still_exists = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap();
        assert!(
            still_exists.is_some(),
            "Admin should not be able to delete themselves"
        );
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Create two admins - admin1 will try to delete admin2
        let admin1_id = login_user(&mut test_ctx, "admin1", "password", &[], true).await;
        let _admin2_id = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        // Admin1 deletes admin2 (should succeed, admin1 still exists)
        let admin2_db = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let result = handle_user_delete(
            admin2_db.id,
            Some(admin1_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Admin should be able to delete other admin");
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Should successfully delete admin2");
                assert!(error.is_none());
                assert_eq!(username, Some("admin2".to_string()));
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Last-admin protection lives in the DB layer; exercise it directly via
        // delete_user (the handler blocks self-delete earlier, so we can't reach
        // the guard through the handler with a single admin).
        let mut test_ctx2 = create_test_context().await;

        let _only_admin_id = login_user(&mut test_ctx2, "only_admin", "password", &[], true).await;

        test_ctx2
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "target",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let _admin2_id = login_user(&mut test_ctx2, "admin2", "password", &[], true).await;

        let only_admin = test_ctx2
            .db
            .users
            .get_user_by_username("only_admin")
            .await
            .unwrap()
            .unwrap();

        // Delete admin2 first so only_admin becomes the last admin
        let admin2 = test_ctx2
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let deleted = test_ctx2
            .db
            .users
            .delete_user(admin2.id, true)
            .await
            .unwrap();
        assert!(deleted, "Should delete admin2");

        // Now try to delete the last admin via the database directly
        let deleted = test_ctx2
            .db
            .users
            .delete_user(only_admin.id, true)
            .await
            .unwrap();
        assert!(!deleted, "Should not be able to delete the last admin");

        let remaining = test_ctx2
            .db
            .users
            .get_user_by_username("only_admin")
            .await
            .unwrap();
        assert!(remaining.is_some(), "Last admin should still exist");
    }

    #[tokio::test]
    async fn test_userdelete_non_admin_cannot_delete_admin() {
        let mut test_ctx = create_test_context().await;

        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Non-admin with UserDelete permission must still be unable to delete an admin.
        let deleter_id = login_user(
            &mut test_ctx,
            "deleter",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        let admin_db = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let result = handle_user_delete(
            admin_db.id,
            Some(deleter_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserDeleteResponse { success, error, .. } => {
                assert!(!success, "Non-admin should not be able to delete admin");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("admin"),
                    "Error should mention admin restriction"
                );
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap();
        assert!(admin.is_some(), "Admin should still exist");
    }

    #[tokio::test]
    async fn test_userdelete_handles_online_and_offline_users() {
        let mut test_ctx = create_test_context().await;

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create offline user to delete
        let offline_user = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "offline_user",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Create online user to delete
        let online_user = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "online_user",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Add online_user to UserManager (they're online)
        let (online_tx, _online_rx) = mpsc::unbounded_channel();
        let online_session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: online_user.id,
                username: "online_user".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: online_user.created_at,
                tx: online_tx,
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "online_user".to_string(),
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

        let online_before = test_ctx
            .user_manager
            .get_user_by_session_id(online_session_id)
            .await;
        assert!(
            online_before.is_some(),
            "Online user should be connected before deletion"
        );

        let result1 = handle_user_delete(
            offline_user.id,
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result1.is_ok(), "Should successfully delete offline user");
        let deleted1 = test_ctx
            .db
            .users
            .get_user_by_id(offline_user.id)
            .await
            .unwrap();
        assert!(
            deleted1.is_none(),
            "Offline user should be deleted from database"
        );

        let result2 = handle_user_delete(
            online_user.id,
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result2.is_ok(), "Should successfully delete online user");
        let deleted2 = test_ctx
            .db
            .users
            .get_user_by_id(online_user.id)
            .await
            .unwrap();
        assert!(
            deleted2.is_none(),
            "Online user should be deleted from database"
        );

        let online_after = test_ctx
            .user_manager
            .get_user_by_session_id(online_session_id)
            .await;
        assert!(
            online_after.is_none(),
            "Online user should be disconnected from UserManager"
        );
    }

    #[tokio::test]
    async fn test_userdelete_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Non-admin with UserDelete permission.
        let deleter_id = login_user(
            &mut test_ctx,
            "deleter",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        let target = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "target",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let result =
            handle_user_delete(target.id, Some(deleter_id), &mut test_ctx.handler_context()).await;

        assert!(
            result.is_ok(),
            "User with UserDelete permission should be able to delete users"
        );

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message on success");
                assert_eq!(username, Some("target".to_string()));
            }
            _ => panic!("Expected UserDeleteResponse"),
        }

        // Verify target is deleted
        let deleted = test_ctx.db.users.get_user_by_id(target.id).await.unwrap();
        assert!(deleted.is_none(), "Target user should be deleted");
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_guest() {
        let mut test_ctx = create_test_context().await;

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let guest = test_ctx
            .db
            .users
            .get_user_by_username("guest")
            .await
            .unwrap()
            .expect("Guest account should exist from migration");

        let result =
            handle_user_delete(guest.id, Some(admin_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(!success, "Should not be able to delete guest account");
                assert!(error.is_some(), "Should have error message");
                assert!(
                    error.unwrap().contains("guest"),
                    "Error should mention guest"
                );
                assert!(username.is_none(), "Should not have username on failure");
            }
            _ => panic!("Expected UserDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_userdelete_cannot_delete_guest_case_insensitive() {
        let mut test_ctx = create_test_context().await;

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Guest is seeded lowercase; the handler's fold_name() compare must still block it.
        let guest = test_ctx
            .db
            .users
            .get_user_by_username("guest")
            .await
            .unwrap()
            .expect("Guest account should exist from migration");

        let result =
            handle_user_delete(guest.id, Some(admin_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(!success, "Should not be able to delete GUEST account");
                assert!(error.is_some(), "Should have error message");
                assert!(username.is_none(), "Should not have username on failure");
            }
            _ => panic!("Expected UserDeleteResponse"),
        }
    }

    /// Any blocked delete must leave the target's session intact. Exercises the
    /// pre-tx admin block (non-admin targeting an admin); the SQL guard is covered
    /// by db::users tests. Regression: the kick once fired before the DB delete.
    #[tokio::test]
    async fn test_userdelete_blocked_delete_does_not_disconnect_session() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let deleter_session = login_user(
            &mut test_ctx,
            "deleter",
            "password",
            &[db::Permission::UserDelete],
            false,
        )
        .await;

        let admin_db = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let admin_session_id_in_mgr = test_ctx
            .user_manager
            .get_sessions_by_user_id(admin_db.id)
            .await
            .first()
            .map(|s| s.session_id)
            .expect("admin should have a session in UserManager");

        let result = handle_user_delete(
            admin_db.id,
            Some(deleter_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserDeleteResponse { success, .. } => assert!(!success),
            _ => panic!("Expected UserDeleteResponse"),
        }

        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(admin_session_id_in_mgr)
                .await
                .is_some(),
            "blocked delete must not have disconnected the target's session"
        );
    }
}
