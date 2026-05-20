use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, KickReasonError, NicknameError};

use super::{
    HandlerContext, err_cannot_kick_admin, err_cannot_kick_self, err_database,
    err_kick_reason_invalid_characters, err_kick_reason_too_long, err_kicked_by,
    err_kicked_by_with_reason, err_nickname_empty, err_nickname_invalid, err_nickname_not_online,
    err_nickname_too_long, err_not_logged_in, err_permission_denied,
    remove_user_with_voice_cleanup,
};
use crate::constants::{
    HANDLER_USER_KICK, LOG_USER_KICK_DB_ERROR, LOG_USER_KICK_NOT_LOGGED_IN,
    LOG_USER_KICK_PERMISSION_DENIED, LOG_USER_KICK_SUCCESS,
};
use crate::db::Permission;

pub async fn handle_user_kick<W>(
    nickname: String,
    reason: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_KICK_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_KICK))
            .await;
    };

    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_KICK))
                .await;
        }
    };

    if !requesting_user_session.has_permission(Permission::UserKick) {
        warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_USER_KICK_PERMISSION_DENIED);
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_nickname(&nickname) {
        let error_msg = match e {
            NicknameError::Empty => err_nickname_empty(ctx.locale),
            NicknameError::TooLong => {
                err_nickname_too_long(ctx.locale, validators::MAX_NICKNAME_LENGTH)
            }
            NicknameError::InvalidCharacters => err_nickname_invalid(ctx.locale),
        };
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(error_msg),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Reason is formatted into a single-line kick message, so reject
    // control characters (newlines/tabs) and cap at MAX_KICK_REASON_LENGTH.
    if let Some(ref r) = reason
        && let Err(e) = validators::validate_kick_reason(r)
    {
        let error_msg = match e {
            KickReasonError::TooLong => {
                err_kick_reason_too_long(ctx.locale, validators::MAX_KICK_REASON_LENGTH)
            }
            KickReasonError::InvalidCharacters => err_kick_reason_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(error_msg),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Prevent self-kick by nickname (display name), before DB queries.
    let target_lower = nickname.to_lowercase();
    let is_self_kick = requesting_user_session.nickname.to_lowercase() == target_lower;
    if is_self_kick {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_cannot_kick_self(ctx.locale)),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Target by nickname (equals username for regular accounts).
    let target_session = match ctx.user_manager.get_session_by_nickname(&nickname).await {
        Some(session) => session,
        None => {
            let response = ServerMessage::UserKickResponse {
                success: false,
                error: Some(err_nickname_not_online(ctx.locale, &nickname)),
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Look up account in DB to check admin status.
    let db_lookup_username = target_session.username.clone();

    let target_user_db = match ctx.db.users.get_user_by_username(&db_lookup_username).await {
        Ok(user) => user,
        Err(e) => {
            error!(user = %requesting_user_session.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_KICK_DB_ERROR);
            let response = ServerMessage::UserKickResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    if let Some(ref target_db) = target_user_db
        && target_db.is_admin
    {
        let response = ServerMessage::UserKickResponse {
            success: false,
            error: Some(err_cannot_kick_admin(ctx.locale)),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    let preserved_nickname = target_session.nickname.clone();

    // Kick all sessions sharing this nickname. Regular accounts: nickname ==
    // username so every session matches; shared accounts: unique per nickname.
    let sessions_to_kick = ctx
        .user_manager
        .get_sessions_by_nickname(&preserved_nickname)
        .await;

    for user in sessions_to_kick {
        // Send the kick Error (command "UserKick") in the user's locale before
        // disconnecting — the client maps it to UserKicked, not ConnectionLost.
        let kick_message = if let Some(ref r) = reason {
            err_kicked_by_with_reason(&user.locale, &requesting_user_session.username, r)
        } else {
            err_kicked_by(&user.locale, &requesting_user_session.username)
        };
        let kick_msg = ServerMessage::Error {
            message: kick_message,
            command: Some("UserKick".to_string()),
        };
        let _ = user.tx.send((kick_msg, None));

        let target_session_id = user.session_id;
        remove_user_with_voice_cleanup(
            ctx.user_manager,
            ctx.voice_registry,
            ctx.channel_manager,
            target_session_id,
            &user,
        )
        .await;
    }

    info!(user = %requesting_user_session.username, ip = %ctx.peer_addr, target = %preserved_nickname, "{}", LOG_USER_KICK_SUCCESS);
    let response = ServerMessage::UserKickResponse {
        success: true,
        error: None,
        nickname: Some(preserved_nickname),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, get_cached_password_hash, login_shared_user,
        login_user, read_login_response, read_server_message,
    };

    #[tokio::test]
    async fn test_userkick_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to kick user without being logged in
        let result = handle_user_kick(
            "alice".to_string(),
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "UserKick should require login");
    }

    #[tokio::test]
    async fn test_userkick_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserKick permission (non-admin)
        let _session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to kick
        let _target_id = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Try to kick bob (should fail - no permission)
        let result = handle_user_kick(
            "bob".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Kick should fail without permission");
            assert!(
                error.unwrap().to_lowercase().contains("permission"),
                "Error should mention permission"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH UserKick permission
        let _kicker_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;

        // Create another user to kick
        let _target_id = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Kick bob (should succeed)
        let result = handle_user_kick(
            "bob".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should succeed with permission");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            nickname,
        } = response
        {
            assert!(success, "Kick should succeed");
            assert!(error.is_none(), "Should not have error");
            assert_eq!(nickname, Some("bob".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_admin_can_kick() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (no explicit permission needed)
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another user to kick
        let _target_id = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Admin kicks bob (should succeed)
        let result = handle_user_kick(
            "bob".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Admin should be able to kick");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            nickname,
        } = response
        {
            assert!(success, "Admin kick should succeed");
            assert!(error.is_none(), "Should not have error");
            assert_eq!(nickname, Some("bob".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_cannot_kick_self() {
        let mut test_ctx = create_test_context().await;

        // Create user with kick permission
        let _session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;

        // Try to kick self (should fail)
        let result = handle_user_kick(
            "alice".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Should not be able to kick self");
            assert!(
                error.unwrap().contains("yourself"),
                "Error should mention self-kick prevention"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_user_not_online() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create offline user in database (not logged in)
        use crate::db::{Permissions, hash_password};
        let hashed = hash_password(
            "password",
            nexus_common::validators::PasswordStrength::Weak,
            true,
        )
        .unwrap();
        let perms = Permissions::new();
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "offline_user",
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

        // Try to kick offline user (should fail)
        let result = handle_user_kick(
            "offline_user".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Cannot kick offline user");
            assert!(
                error.unwrap().contains("not online"),
                "Error should mention user is not online"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_case_insensitive() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user
        let _target_id = login_user(&mut test_ctx, "Alice", "password", &[], false).await;

        // Kick using different case (should succeed)
        let result = handle_user_kick(
            "alice".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should work case-insensitively");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            nickname,
        } = response
        {
            assert!(success, "Case-insensitive kick should succeed");
            assert!(error.is_none(), "Should not have error");
            // Should return the preserved casing from the database, not the input
            assert_eq!(nickname, Some("Alice".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_userkick_disconnects_all_sessions() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user with first session
        let _target_id1 = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Simulate second session for same user (different session ID)
        // In real scenario, this would be another connection
        // For testing, we verify the logic handles multiple sessions

        // Kick alice (should kick all sessions)
        let result = handle_user_kick(
            "alice".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Kick should succeed");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse {
            success,
            error,
            nickname,
        } = response
        {
            assert!(success, "Kick should succeed for multi-session user");
            assert!(error.is_none(), "Should not have error");
            assert_eq!(nickname, Some("alice".to_string()));
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }

        // Verify user was removed from UserManager
        let all_users = test_ctx.user_manager.get_all_users().await;
        let alice_still_online = all_users.iter().any(|u| u.username == "alice");
        assert!(
            !alice_still_online,
            "Alice should be disconnected after kick"
        );
    }

    #[tokio::test]
    async fn test_userkick_cannot_kick_admin() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (kicker)
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target admin user
        let _target_admin_id = login_user(&mut test_ctx, "bob", "password", &[], true).await;

        // Try to kick admin (should fail)
        let result = handle_user_kick(
            "bob".to_string(),
            None,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        // Read response
        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::UserKickResponse { success, error, .. } = response {
            assert!(!success, "Should not be able to kick admin");
            assert!(
                error.unwrap().contains("admin"),
                "Error should mention admin protection"
            );
        } else {
            panic!("Expected UserKickResponse, got: {:?}", response);
        }
    }

    // Shared account tests.

    #[tokio::test]
    async fn test_userkick_shared_account_by_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create admin user to do the kicking
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = get_cached_password_hash("password");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
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
        let _ = read_login_response(&mut test_ctx).await; // consume login response

        assert!(
            shared_session_id.is_some(),
            "Shared account should be logged in"
        );

        // Kick by nickname
        let result = handle_user_kick(
            "Nick1".to_string(),
            None,
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => {
                assert!(success, "Kick by nickname should succeed");
                assert!(error.is_none());
                assert_eq!(
                    nickname,
                    Some("Nick1".to_string()),
                    "Should return the nickname"
                );
            }
            _ => panic!("Expected UserKickResponse"),
        }

        // Verify user was kicked
        let sessions = test_ctx.user_manager.get_session_by_nickname("Nick1").await;
        assert!(sessions.is_none(), "Session should be removed");
    }

    #[tokio::test]
    async fn test_userkick_shared_account_self_kick_by_nickname_prevented() {
        let mut test_ctx = create_test_context().await;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create a shared account in the database with kick permission
        let hashed = get_cached_password_hash("password");
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserKick);
        test_ctx
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

        // But first we need an admin
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
        let _ = read_login_response(&mut test_ctx).await; // consume login response

        let session_id = shared_session_id.unwrap();

        // Try to kick self by nickname (should fail)
        let result = handle_user_kick(
            "Nick1".to_string(),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse { success, error, .. } => {
                assert!(!success, "Self-kick by nickname should be prevented");
                assert!(error.is_some());
            }
            _ => panic!("Expected UserKickResponse"),
        }
    }

    #[tokio::test]
    async fn test_userkick_shared_account_by_nickname_succeeds() {
        let mut test_ctx = create_test_context().await;

        // Create admin user to perform kick
        let _admin_id = login_user(&mut test_ctx, "admin", "pass123", &[], true).await;

        // Create shared account user with nickname "Nick1"
        let _shared_id =
            login_shared_user(&mut test_ctx, "shared_acct", "sharedpass", "Nick1", &[]).await;

        // Kick by nickname (should succeed)
        let result = handle_user_kick(
            "Nick1".to_string(),
            None,
            Some(1), // admin's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => {
                assert!(success, "Should allow kicking shared account by nickname");
                assert!(error.is_none());
                assert_eq!(nickname, Some("Nick1".to_string()));
            }
            _ => panic!("Expected UserKickResponse"),
        }
    }

    #[tokio::test]
    async fn test_userkick_reason_too_long_rejected() {
        let mut test_ctx = create_test_context().await;
        let _kicker = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;
        let _target = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        let long_reason = "a".repeat(validators::MAX_KICK_REASON_LENGTH + 1);
        let result = handle_user_kick(
            "bob".to_string(),
            Some(long_reason),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => {
                assert!(!success, "Kick with over-cap reason should fail");
                assert!(
                    error.unwrap_or_default().contains("too long"),
                    "Error should mention too long"
                );
                assert!(nickname.is_none());
            }
            other => panic!("Expected UserKickResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_userkick_reason_with_newline_rejected() {
        let mut test_ctx = create_test_context().await;
        let _kicker = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;
        let _target = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        let result = handle_user_kick(
            "bob".to_string(),
            Some("first line\nsecond line".to_string()),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => {
                assert!(!success, "Kick with newline in reason should fail");
                assert!(
                    error.unwrap_or_default().contains("invalid characters"),
                    "Error should mention invalid characters"
                );
                assert!(nickname.is_none());
            }
            other => panic!("Expected UserKickResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_userkick_reason_with_tab_rejected() {
        let mut test_ctx = create_test_context().await;
        let _kicker = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;
        let _target = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        let result = handle_user_kick(
            "bob".to_string(),
            Some("violation:\tspamming".to_string()),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => {
                assert!(!success, "Kick with tab in reason should fail");
                assert!(
                    error.unwrap_or_default().contains("invalid characters"),
                    "Error should mention invalid characters"
                );
                assert!(nickname.is_none());
            }
            other => panic!("Expected UserKickResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_userkick_reason_with_null_rejected() {
        let mut test_ctx = create_test_context().await;
        let _kicker = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::UserKick],
            false,
        )
        .await;
        let _target = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        let result = handle_user_kick(
            "bob".to_string(),
            Some("reason\0with-null".to_string()),
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => {
                assert!(!success, "Kick with null byte in reason should fail");
                assert!(
                    error.unwrap_or_default().contains("invalid characters"),
                    "Error should mention invalid characters"
                );
                assert!(nickname.is_none());
            }
            other => panic!("Expected UserKickResponse, got {other:?}"),
        }
    }
}
