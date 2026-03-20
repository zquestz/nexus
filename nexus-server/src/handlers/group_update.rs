//! GroupUpdate message handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, GroupNameError, PermissionsError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_database, err_group_already_exists,
    err_group_name_empty, err_group_name_invalid, err_group_name_too_long, err_group_no_fields,
    err_group_not_empty_modify, err_group_not_found, err_group_shared_permission,
    err_not_logged_in, err_permission_denied, err_permissions_contains_newlines,
    err_permissions_empty_permission, err_permissions_invalid_characters,
    err_permissions_permission_too_long, err_permissions_too_many, err_unknown_permission,
};
use crate::db::Permission;

/// Handle a group update request
pub async fn handle_group_update<W>(
    id: i64,
    name: Option<String>,
    is_shared: Option<bool>,
    permissions: Option<Vec<String>>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("GroupUpdate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("GroupUpdate"))
            .await;
    };

    // If all optional fields are None, there's nothing to update
    if name.is_none() && is_shared.is_none() && permissions.is_none() {
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(err_group_no_fields(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Validate name format (if provided)
    if let Some(ref n) = name
        && let Err(e) = validators::validate_group_name(n)
    {
        let error_msg = match e {
            GroupNameError::Empty => err_group_name_empty(ctx.locale),
            GroupNameError::TooLong => err_group_name_too_long(ctx.locale),
            GroupNameError::InvalidCharacters => err_group_name_invalid(ctx.locale),
        };
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Validate permissions format (if provided)
    if let Some(ref perms) = permissions
        && let Err(e) = validators::validate_permissions(perms)
    {
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
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("GroupUpdate"))
                .await;
        }
    };

    // Check GroupEdit permission
    if !requesting_user.has_permission(Permission::GroupEdit) {
        eprintln!(
            "GroupUpdate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Fetch existing group
    let group = match ctx.db.groups.get_group_by_id(id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            let response = ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_group_not_found(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting group: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupUpdate"))
                .await;
        }
    };

    // Shared status toggle check: if changing is_shared, group must have no members
    if let Some(new_shared) = is_shared
        && new_shared != group.is_shared
    {
        let member_count = match ctx.db.groups.get_member_count(id).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Database error getting member count: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupUpdate"))
                    .await;
            }
        };

        if member_count > 0 {
            let response = ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_group_not_empty_modify(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    }

    // Build final values
    let final_name = name.unwrap_or_else(|| group.name.clone());
    let final_is_shared = is_shared.unwrap_or(group.is_shared);

    // Resolve final permissions
    let final_permissions = if let Some(ref requested_perms) = permissions {
        // Parse and validate each permission string
        for perm_str in requested_perms {
            if Permission::parse(perm_str).is_none() {
                let response = ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_unknown_permission(ctx.locale, perm_str)),
                };
                return ctx.send_message(&response).await;
            }
        }

        // Non-admin delegation: merge pattern
        if requesting_user.is_admin {
            // Admins can set any permissions directly
            requested_perms.clone()
        } else {
            // Get current group permissions
            let current_perms = match ctx.db.groups.get_group_permissions(id).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Database error getting group permissions: {}", e);
                    return ctx
                        .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupUpdate"))
                        .await;
                }
            };

            // Preserved = current permissions the requester does NOT have
            let mut final_perms: Vec<String> = current_perms
                .iter()
                .filter(|p| {
                    Permission::parse(p)
                        .map(|perm| !requesting_user.has_permission(perm))
                        .unwrap_or(true)
                })
                .cloned()
                .collect();

            // Add from requested permissions only those the requester has
            for perm_str in requested_perms {
                if let Some(perm) = Permission::parse(perm_str)
                    && requesting_user.has_permission(perm)
                    && !final_perms.contains(perm_str)
                {
                    final_perms.push(perm_str.clone());
                }
            }

            final_perms
        }
    } else {
        // No permission changes requested — pass through current permissions unchanged
        match ctx.db.groups.get_group_permissions(id).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Database error getting group permissions: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupUpdate"))
                    .await;
            }
        }
    };

    // Shared group permission validation: if final group is shared, all permissions must be allowed
    if final_is_shared {
        for perm_str in &final_permissions {
            if !is_shared_account_permission(perm_str) {
                let response = ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_group_shared_permission(ctx.locale)),
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    // Update group in database
    match ctx
        .db
        .groups
        .update_group(id, &final_name, final_is_shared, &final_permissions)
        .await
    {
        Ok(Some(_)) => {
            let response = ServerMessage::GroupUpdateResponse {
                success: true,
                id: Some(id),
                name: Some(final_name),
                error: None,
            };
            ctx.send_message(&response).await
        }
        Ok(None) => {
            let response = ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_group_not_found(ctx.locale)),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            if e.to_string().contains("UNIQUE") {
                let response = ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_group_already_exists(ctx.locale)),
                };
                ctx.send_message(&response).await
            } else {
                eprintln!("Database error updating group: {}", e);
                ctx.send_error_and_disconnect(&err_database(ctx.locale), Some("GroupUpdate"))
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_update_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_update(
            1,
            Some("NewName".to_string()),
            None,
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "GroupUpdate should require login");
    }

    #[tokio::test]
    async fn test_group_update_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without GroupEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_group_update(
            1,
            Some("NewName".to_string()),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with permission denied"),
        }
    }

    #[tokio::test]
    async fn test_group_update_not_found() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        let result = handle_group_update(
            9999,
            Some("NewName".to_string()),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_found(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with not found"),
        }
    }

    #[tokio::test]
    async fn test_group_update_no_fields() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        let result = handle_group_update(
            1,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_no_fields(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with no fields error"),
        }
    }

    #[tokio::test]
    async fn test_group_update_rename() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create a group
        let group = test_ctx
            .db
            .groups
            .create_group("OldName", false, &[])
            .await
            .expect("Failed to create group");

        let result = handle_group_update(
            group.id,
            Some("NewName".to_string()),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert_eq!(name, Some("NewName".to_string()));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify in database
        let updated = test_ctx
            .db
            .groups
            .get_group_by_id(group.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "NewName");
    }

    #[tokio::test]
    async fn test_group_update_duplicate_name() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create two groups
        let _group_a = test_ctx
            .db
            .groups
            .create_group("GroupA", false, &[])
            .await
            .expect("Failed to create GroupA");

        let group_b = test_ctx
            .db
            .groups
            .create_group("GroupB", false, &[])
            .await
            .expect("Failed to create GroupB");

        // Try to rename GroupB to GroupA
        let result = handle_group_update(
            group_b.id,
            Some("GroupA".to_string()),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_already_exists(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with already exists"),
        }
    }

    #[tokio::test]
    async fn test_group_update_permissions() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                db::Permission::GroupEdit,
                db::Permission::ChatSend,
                db::Permission::UserKick,
            ],
            false,
        )
        .await;

        // Create a group with initial permissions
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &["chat_send".to_string()])
            .await
            .expect("Failed to create group");

        // Update permissions
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success, id, error, ..
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify permissions in database
        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&"chat_send".to_string()));
        assert!(perms.contains(&"user_kick".to_string()));
    }

    #[tokio::test]
    async fn test_group_update_shared_toggle_with_members_rejected() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create a non-shared group
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &[])
            .await
            .expect("Failed to create group");

        // Assign a user to this group via create_user with group_id
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "member",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .expect("Failed to create member");

        // Try to toggle is_shared — should fail because group has members
        let result = handle_group_update(
            group.id,
            None,
            Some(true),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_empty_modify(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with not empty error"),
        }
    }

    #[tokio::test]
    async fn test_group_update_shared_toggle_no_members() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create a non-shared group with no members
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &[])
            .await
            .expect("Failed to create group");

        // Toggle is_shared — should succeed (no members)
        let result = handle_group_update(
            group.id,
            None,
            Some(true),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success, id, error, ..
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify in database
        let updated = test_ctx
            .db
            .groups
            .get_group_by_id(group.id)
            .await
            .unwrap()
            .unwrap();
        assert!(updated.is_shared);
    }

    #[tokio::test]
    async fn test_group_update_shared_with_forbidden_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin so permission delegation doesn't interfere
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a non-shared group with no members
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &["user_kick".to_string()])
            .await
            .expect("Failed to create group");

        // Try to make it shared while keeping user_kick (forbidden for shared)
        let result = handle_group_update(
            group.id,
            None,
            Some(true),
            Some(vec!["user_kick".to_string()]),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_group_shared_permission(DEFAULT_TEST_LOCALE))
                );
            }
            _ => panic!("Expected GroupUpdateResponse with shared permission error"),
        }
    }

    #[tokio::test]
    async fn test_group_update_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &[])
            .await
            .expect("Failed to create group");

        // Update name and permissions as admin
        let result = handle_group_update(
            group.id,
            Some("Moderators".to_string()),
            None,
            Some(vec!["user_kick".to_string(), "ban_create".to_string()]),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert_eq!(name, Some("Moderators".to_string()));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify in database
        let updated = test_ctx
            .db
            .groups
            .get_group_by_id(group.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Moderators");

        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&"ban_create".to_string()));
        assert!(perms.contains(&"user_kick".to_string()));
    }
}
