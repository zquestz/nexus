//! GroupCreate message handler

use std::io;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use crate::constants::*;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    BandwidthWeightError, MIN_BANDWIDTH_WEIGHT, validate_bandwidth_weight,
};

use super::group_validation::{
    parse_group_permissions_for_handler, validate_group_name_for_handler,
    validate_group_permissions_for_handler, validate_shared_group_permission_names,
};
#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, Outcome, dispatch_outcome, err_bandwidth_weight_delegation,
    err_bandwidth_weight_zero, err_database, err_group_already_exists, err_not_logged_in,
    err_permission_denied,
};
use crate::db::{Permission, Permissions};

pub async fn handle_group_create<W>(
    name: String,
    is_shared: bool,
    permissions: Vec<String>,
    bandwidth_weight: u16,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_GROUP_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_GROUP_CREATE))
            .await;
    };

    // Guard lives only inside this block; all socket sends happen after it.
    let outcome = 'locked: {
        let _state_guard = ctx.user_manager.lock_user_state().await;
        let requesting_user = match ctx
            .user_manager
            .get_user_by_session_id(requesting_session_id)
            .await
        {
            Some(u) => u,
            None => break 'locked Outcome::Disconnect,
        };

        if !requesting_user.has_permission(Permission::GroupCreate) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_GROUP_CREATE_PERMISSION_DENIED);
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                id: None,
                name: None,
            }));
        }

        // Bandwidth weight delegation: a non-admin creating a group can set
        // its weight only at or below their own resolved bandwidth weight.
        // Admins bypass.
        if !requesting_user.is_admin
            && bandwidth_weight > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                success: false,
                error: Some(err_bandwidth_weight_delegation(ctx.locale)),
                id: None,
                name: None,
            }));
        }
        if let Err(BandwidthWeightError::Zero) = validate_bandwidth_weight(bandwidth_weight) {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                success: false,
                error: Some(err_bandwidth_weight_zero(ctx.locale, MIN_BANDWIDTH_WEIGHT)),
                id: None,
                name: None,
            }));
        }

        if let Err(error_msg) = validate_group_name_for_handler(&name, ctx.locale) {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                success: false,
                error: Some(error_msg),
                id: None,
                name: None,
            }));
        }

        if let Err(error_msg) = validate_group_permissions_for_handler(&permissions, ctx.locale) {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                success: false,
                error: Some(error_msg),
                id: None,
                name: None,
            }));
        }

        // For shared groups, only shared-account permissions are accepted.
        if is_shared
            && let Err(error_msg) = validate_shared_group_permission_names(&permissions, ctx.locale)
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                success: false,
                error: Some(error_msg),
                id: None,
                name: None,
            }));
        }

        let parsed_requested = match parse_group_permissions_for_handler(&permissions, ctx.locale) {
            Ok(perms) => perms,
            Err(error_msg) => {
                break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                    success: false,
                    error: Some(error_msg),
                    id: None,
                    name: None,
                }));
            }
        };

        let mut parsed_permissions = Permissions::new();
        for perm in parsed_requested {
            // Non-admins can only grant permissions they have.
            if !requesting_user.has_permission(perm) {
                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm.as_str(), "{}", LOG_GROUP_CREATE_UNOWNED_PERMISSION);
                break 'locked Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                    success: false,
                    error: Some(err_permission_denied(ctx.locale)),
                    id: None,
                    name: None,
                }));
            }

            parsed_permissions.add(perm);
        }

        match ctx
            .db
            .groups
            .create_group(&name, is_shared, &parsed_permissions, bandwidth_weight)
            .await
        {
            Ok(group) => {
                info!(user = %requesting_user.username, ip = %ctx.peer_addr, group = %group.name, "{}", LOG_GROUP_CREATE_SUCCESS);
                Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                    success: true,
                    error: None,
                    id: Some(group.id),
                    name: Some(group.name),
                }))
            }
            Err(e) => {
                // Disambiguate duplicate-name failures (recoverable) from
                // other DB errors (fatal).
                if ctx
                    .db
                    .groups
                    .get_group_by_name(&name)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
                {
                    Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                        success: false,
                        error: Some(err_group_already_exists(ctx.locale)),
                        id: None,
                        name: None,
                    }))
                } else {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_CREATE_DB_ERROR);
                    Outcome::Send(Box::new(ServerMessage::GroupCreateResponse {
                        success: false,
                        error: Some(err_database(ctx.locale)),
                        id: None,
                        name: None,
                    }))
                }
            }
        }
    };

    dispatch_outcome(outcome, ctx, HANDLER_GROUP_CREATE).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::err_group_name_empty;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_create_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_create(
            "TestGroup".to_string(),
            false,
            vec![],
            1,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "GroupCreate should require login");
    }

    #[tokio::test]
    async fn test_group_create_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_group_create(
            "TestGroup".to_string(),
            false,
            vec![],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_group_create_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                db::Permission::GroupCreate,
                db::Permission::ChatSend,
                db::Permission::ChatReceive,
            ],
            false,
        )
        .await;

        let result = handle_group_create(
            "Moderators".to_string(),
            false,
            vec!["chat_send".to_string(), "chat_receive".to_string()],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse {
                success,
                error,
                id,
                name,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should have group id");
                assert_eq!(name, Some("Moderators".to_string()));
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_create_duplicate_name() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create group first time
        let result = handle_group_create(
            "UniqueGroup".to_string(),
            false,
            vec![],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse { success, .. } => {
                assert!(success, "First creation should succeed");
            }
            _ => panic!("Expected GroupCreateResponse"),
        }

        // Try to create group with same name
        let result = handle_group_create(
            "UniqueGroup".to_string(),
            false,
            vec![],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse {
                success,
                error,
                id,
                name,
            } => {
                assert!(!success, "Duplicate should fail");
                assert!(error.is_some(), "Should have error message");
                assert!(id.is_none());
                assert!(name.is_none());
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_create_duplicate_name_unicode() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        handle_group_create(
            "Équipe".to_string(),
            false,
            vec![],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Differs only by Unicode case (É↔é): collides via the folded name_lower,
        // which the old ASCII-only COLLATE NOCASE would have allowed.
        let result = handle_group_create(
            "équipe".to_string(),
            false,
            vec![],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::GroupCreateResponse { success, error, .. } => {
                assert!(!success, "Unicode-case duplicate should be rejected");
                assert_eq!(error, Some(err_group_already_exists(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_create_invalid_name() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create group with empty name
        let result = handle_group_create(
            "".to_string(),
            false,
            vec![],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse { success, error, .. } => {
                assert!(!success, "Empty name should fail");
                assert_eq!(error, Some(err_group_name_empty(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_create_shared_with_forbidden_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create a shared group with a non-shared permission
        let result = handle_group_create(
            "SharedGroup".to_string(),
            true,
            vec![
                "chat_send".to_string(),   // allowed for shared
                "user_create".to_string(), // forbidden for shared
            ],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse {
                success,
                error,
                id,
                name,
            } => {
                assert!(
                    !success,
                    "Shared group with forbidden permission should fail"
                );
                assert!(error.is_some(), "Should have error message");
                assert!(id.is_none());
                assert!(name.is_none());
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_create_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_group_create(
            "AdminGroup".to_string(),
            false,
            vec!["chat_send".to_string(), "user_kick".to_string()],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse {
                success,
                error,
                id,
                name,
            } => {
                assert!(success, "Admin should be able to create groups");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should have group id");
                assert_eq!(name, Some("AdminGroup".to_string()));
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_create_non_admin_cannot_grant_unowned_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "creator",
            "password",
            &[db::Permission::GroupCreate, db::Permission::ChatSend],
            false,
        )
        .await;

        // Try to create a group with user_kick (which creator doesn't have)
        let result = handle_group_create(
            "OverreachGroup".to_string(),
            false,
            vec![
                "chat_send".to_string(), // creator has this - OK
                "user_kick".to_string(), // creator doesn't have this - FAIL
            ],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_group_create_reports_unknown_permission_before_authz_checks() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "creator",
            "password",
            &[db::Permission::GroupCreate, db::Permission::ChatSend],
            false,
        )
        .await;

        let result = handle_group_create(
            "MixedBadGroup".to_string(),
            false,
            vec![
                "user_kick".to_string(),        // valid, but creator does not own it
                "not_a_permission".to_string(), // unknown input is reported before authz
            ],
            1,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(crate::handlers::err_unknown_permission(
                        DEFAULT_TEST_LOCALE,
                        "not_a_permission",
                    ))
                );
            }
            other => panic!("Expected GroupCreateResponse, got {:?}", other),
        }

        let group = test_ctx
            .db
            .groups
            .get_group_by_name("MixedBadGroup")
            .await
            .unwrap();
        assert!(group.is_none(), "rejected group should not be created");
    }

    /// Set up a non-admin "editor" with resolved bandwidth weight = `weight`
    /// by creating an Editors group at that weight and assigning editor to it.
    /// Returns the editor session_id.
    async fn setup_editor_with_weight(
        test_ctx: &mut crate::handlers::testing::TestContext,
        weight: u16,
    ) -> u32 {
        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &db::Permissions::from(&[db::Permission::GroupCreate]),
                weight,
            )
            .await
            .unwrap();
        let editor_session = login_user(
            test_ctx,
            "editor",
            "password",
            &[db::Permission::GroupCreate],
            false,
        )
        .await;
        let admin_session = login_user(test_ctx, "admin", "password", &[], true).await;
        let editor = test_ctx
            .db
            .users
            .get_user_by_username("editor")
            .await
            .unwrap()
            .unwrap();
        crate::handlers::user_update::handle_user_update(
            crate::handlers::user_update::UserUpdateRequest {
                id: editor.id,
                current_password: None,
                username: None,
                password: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                group_id: Some(editor_group.id),
                remove_group: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
                session_id: Some(admin_session),
            },
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(test_ctx).await;
        editor_session
    }

    #[tokio::test]
    async fn test_groupcreate_non_admin_can_create_lower_weight_group() {
        let mut test_ctx = create_test_context().await;
        let editor_session = setup_editor_with_weight(&mut test_ctx, 25).await;

        // Create a group at weight 10 (≤ editor's 25): delegation allows it.
        let result = handle_group_create(
            "Helpers".to_string(),
            false,
            vec![],
            10,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse { success, error, .. } => {
                assert!(success, "delegation-OK weight should succeed: {:?}", error);
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_groupcreate_non_admin_cannot_create_higher_weight_group() {
        let mut test_ctx = create_test_context().await;
        let editor_session = setup_editor_with_weight(&mut test_ctx, 25).await;

        // Create a group at weight 100 (> editor's 25): delegation rejects.
        let result = handle_group_create(
            "PowerMods".to_string(),
            false,
            vec![],
            100,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    crate::handlers::err_bandwidth_weight_delegation(
                        crate::handlers::testing::DEFAULT_TEST_LOCALE
                    )
                );
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_groupcreate_admin_bypasses_delegation() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_group_create(
            "VIP".to_string(),
            false,
            vec![],
            10_000,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupCreateResponse { success, error, .. } => {
                assert!(success, "admin bypass should succeed: {:?}", error);
            }
            _ => panic!("Expected GroupCreateResponse"),
        }
    }
}
