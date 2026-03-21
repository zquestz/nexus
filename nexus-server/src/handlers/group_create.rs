//! GroupCreate message handler - Creates a new account group

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, GroupNameError, PermissionsError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_database, err_group_already_exists,
    err_group_name_empty, err_group_name_invalid, err_group_name_too_long,
    err_group_shared_permission, err_not_logged_in, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_unknown_permission,
};
use crate::db::Permission;

/// Handle a group creation request from the client
pub async fn handle_group_create<W>(
    name: String,
    is_shared: bool,
    permissions: Vec<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(requesting_session_id) = session_id else {
        eprintln!("GroupCreate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("GroupCreate"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("GroupCreate"))
                .await;
        }
    };

    // Check GroupCreate permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::GroupCreate) {
        eprintln!(
            "GroupCreate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("GroupCreate"))
            .await;
    }

    // Validate group name format
    if let Err(e) = validators::validate_group_name(&name) {
        let error_msg = match e {
            GroupNameError::Empty => err_group_name_empty(ctx.locale),
            GroupNameError::TooLong => {
                err_group_name_too_long(ctx.locale, validators::MAX_GROUP_NAME_LENGTH)
            }
            GroupNameError::InvalidCharacters => err_group_name_invalid(ctx.locale),
        };
        let response = ServerMessage::GroupCreateResponse {
            success: false,
            error: Some(error_msg),
            id: None,
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate permissions format
    if let Err(e) = validators::validate_permissions(&permissions) {
        let error_msg = match e {
            PermissionsError::TooMany => {
                err_permissions_too_many(ctx.locale, nexus_common::PERMISSIONS_COUNT)
            }
            PermissionsError::EmptyPermission => err_permissions_empty_permission(ctx.locale),
            PermissionsError::PermissionTooLong => {
                err_permissions_permission_too_long(ctx.locale, validators::MAX_PERMISSION_LENGTH)
            }
            PermissionsError::ContainsNewlines => err_permissions_contains_newlines(ctx.locale),
            PermissionsError::InvalidCharacters => err_permissions_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::GroupCreateResponse {
            success: false,
            error: Some(error_msg),
            id: None,
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // For shared groups, validate that only shared-account permissions are requested
    if is_shared {
        for perm_str in &permissions {
            if !is_shared_account_permission(perm_str) {
                let response = ServerMessage::GroupCreateResponse {
                    success: false,
                    error: Some(err_group_shared_permission(ctx.locale)),
                    id: None,
                    name: None,
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    // Parse and validate requested permissions
    let mut parsed_permissions: Vec<Permission> = Vec::new();
    for perm_str in &permissions {
        let perm = match Permission::parse(perm_str) {
            Some(p) => p,
            None => {
                let response = ServerMessage::GroupCreateResponse {
                    success: false,
                    error: Some(err_unknown_permission(ctx.locale, perm_str)),
                    id: None,
                    name: None,
                };
                return ctx.send_message(&response).await;
            }
        };

        // Non-admins can only grant permissions they have
        if !requesting_user.has_permission(perm) {
            eprintln!(
                "GroupCreate from {} (user: {}) trying to grant permission they don't have: {}",
                ctx.peer_addr, requesting_user.username, perm_str
            );
            return ctx
                .send_error(&err_permission_denied(ctx.locale), Some("GroupCreate"))
                .await;
        }

        parsed_permissions.push(perm);
    }

    // Create group in database
    match ctx
        .db
        .groups
        .create_group(&name, is_shared, &parsed_permissions)
        .await
    {
        Ok(group) => {
            let response = ServerMessage::GroupCreateResponse {
                success: true,
                error: None,
                id: Some(group.id),
                name: Some(group.name),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            // Check if failure was due to duplicate name
            if ctx
                .db
                .groups
                .get_group_by_name(&name)
                .await
                .ok()
                .flatten()
                .is_some()
            {
                let response = ServerMessage::GroupCreateResponse {
                    success: false,
                    error: Some(err_group_already_exists(ctx.locale)),
                    id: None,
                    name: None,
                };
                return ctx.send_message(&response).await;
            }
            eprintln!("Database error creating group: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupCreate"))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_create_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_create(
            "TestGroup".to_string(),
            false,
            vec![],
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "GroupCreate should require login");
    }

    #[tokio::test]
    async fn test_group_create_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without GroupCreate permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_group_create(
            "TestGroup".to_string(),
            false,
            vec![],
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

        // Login as user with GroupCreate and some permissions to grant
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

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create group first time
        let result = handle_group_create(
            "UniqueGroup".to_string(),
            false,
            vec![],
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
    async fn test_group_create_invalid_name() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create group with empty name
        let result = handle_group_create(
            "".to_string(),
            false,
            vec![],
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

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create a shared group with a non-shared permission
        let result = handle_group_create(
            "SharedGroup".to_string(),
            true,
            vec![
                "chat_send".to_string(),   // allowed for shared
                "user_create".to_string(), // forbidden for shared
            ],
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

        // Login as admin (no explicit permissions needed)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_group_create(
            "AdminGroup".to_string(),
            false,
            vec!["chat_send".to_string(), "user_kick".to_string()],
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

        // Login as non-admin with GroupCreate and ChatSend, but NOT UserKick
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
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }
}
