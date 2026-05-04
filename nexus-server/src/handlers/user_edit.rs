//! UserEdit message handler - Returns user details for editing

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, warn};

use nexus_common::protocol::ServerMessage;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_cannot_edit_admin, err_cannot_edit_self, err_database,
    err_not_logged_in, err_permission_denied, err_user_not_found,
};
use crate::constants::{
    HANDLER_USER_EDIT, LOG_USER_EDIT_ADMIN, LOG_USER_EDIT_DB_ERROR, LOG_USER_EDIT_NOT_LOGGED_IN,
    LOG_USER_EDIT_PERMISSION_DENIED,
};
use crate::db::Permission;

/// Handle a user edit request (returns user details for editing)
pub async fn handle_user_edit<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_EDIT_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_EDIT))
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
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some(HANDLER_USER_EDIT))
                .await;
        }
    };

    // Prevent self-editing (cheap check before DB query)
    if requesting_user.user_id == id {
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(err_cannot_edit_self(ctx.locale)),
            id: None,
            username: None,
            is_admin: None,
            is_shared: None,
            enabled: None,
            permissions: None,
            group_id: None,
            group_name: None,
            group_permissions: None,
            revoked_permissions: None,
            available_groups: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check UserEdit permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::UserEdit) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_EDIT_PERMISSION_DENIED);
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            username: None,
            is_admin: None,
            is_shared: None,
            enabled: None,
            permissions: None,
            group_id: None,
            group_name: None,
            group_permissions: None,
            revoked_permissions: None,
            available_groups: None,
        };
        return ctx.send_message(&response).await;
    }

    // Look up target user by ID
    let target_user = match ctx.db.users.get_user_by_id(id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            let response = ServerMessage::UserEditResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &id.to_string())),
                id: None,
                username: None,
                is_admin: None,
                is_shared: None,
                enabled: None,
                permissions: None,
                group_id: None,
                group_name: None,
                group_permissions: None,
                revoked_permissions: None,
                available_groups: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_EDIT_DB_ERROR);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_EDIT))
                .await;
        }
    };

    // Prevent non-admins from viewing admin user details for editing
    if target_user.is_admin && !requesting_user.is_admin {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_EDIT_ADMIN);
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(err_cannot_edit_admin(ctx.locale)),
            id: None,
            username: None,
            is_admin: None,
            is_shared: None,
            enabled: None,
            permissions: None,
            group_id: None,
            group_name: None,
            group_permissions: None,
            revoked_permissions: None,
            available_groups: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch user permissions for response
    let user_permissions = match ctx.db.users.get_user_permissions(target_user.id).await {
        Ok(perms) => perms,
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_EDIT_DB_ERROR);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_EDIT))
                .await;
        }
    };

    // Convert permissions to protocol format
    let permissions: Vec<String> = user_permissions
        .to_vec()
        .iter()
        .map(|p| p.as_str().to_string())
        .collect();

    // Fetch group info if user belongs to a group
    let (group_name, group_permissions_list) = if let Some(gid) = target_user.group_id {
        match ctx.db.groups.get_group_by_id(gid).await {
            Ok(Some(group)) => {
                let group_perms: Vec<String> = ctx
                    .db
                    .groups
                    .get_group_permissions(gid)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.as_str().to_string())
                    .collect();
                (Some(group.name), Some(group_perms))
            }
            _ => (None, None),
        }
    } else {
        (None, None)
    };

    // Fetch revoke overrides for this user
    let revoked_permissions: Option<Vec<String>> = if target_user.group_id.is_some() {
        match ctx.db.users.get_revoke_permissions(target_user.id).await {
            Ok(revokes) if !revokes.is_empty() => {
                Some(revokes.iter().map(|p| p.as_str().to_string()).collect())
            }
            _ => None,
        }
    } else {
        None
    };

    // Fetch available groups for the dropdown (3 queries total, not 1 + 2N)
    let available_groups = match ctx.db.groups.get_all_groups_with_details().await {
        Ok(groups) => Some(groups),
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_EDIT_DB_ERROR);
            None
        }
    };

    // Send user details for editing
    let response = ServerMessage::UserEditResponse {
        success: true,
        error: None,
        id: Some(id),
        username: Some(target_user.username),
        is_admin: Some(target_user.is_admin),
        is_shared: Some(target_user.is_shared),
        enabled: Some(target_user.enabled),
        permissions: Some(permissions),
        group_id: target_user.group_id,
        group_name,
        group_permissions: group_permissions_list,
        revoked_permissions,
        available_groups,
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_useredit_get_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_edit(
            99999, // Non-existent ID, doesn't matter - auth check fails first
            None,  // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_useredit_get_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without UserEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to edit
        let bob = test_ctx
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
            })
            .await
            .unwrap();

        let result =
            handle_user_edit(bob.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected UserEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_user_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_user_edit(99999, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_user_not_found(DEFAULT_TEST_LOCALE, "99999"))
                );
            }
            _ => panic!("Expected UserEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_returns_user_details() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with specific permissions
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        perms.permissions.insert(db::Permission::ChatSend);

        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        let result =
            handle_user_edit(bob.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                is_admin,
                enabled: _,
                permissions,
                ..
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username.as_deref(), Some("bob"));
                assert_eq!(is_admin, Some(false));
                assert!(
                    permissions
                        .as_ref()
                        .unwrap()
                        .contains(&"user_list".to_string())
                );
                assert!(
                    permissions
                        .as_ref()
                        .unwrap()
                        .contains(&"chat_send".to_string())
                );
                assert_eq!(permissions.as_ref().unwrap().len(), 2);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_admin_user() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another admin
        let admin2 = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin2",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        let result =
            handle_user_edit(admin2.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                is_admin,
                enabled,
                permissions,
                ..
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username.as_deref(), Some("admin2"));
                assert_eq!(is_admin, Some(true));
                assert_eq!(enabled, Some(true));
                // Admins have no stored permissions (they get all automatically)
                assert_eq!(permissions.as_ref().unwrap().len(), 0);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Create another user
        let bob = test_ctx
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
            })
            .await
            .unwrap();

        let result =
            handle_user_edit(bob.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse {
                success, username, ..
            } => {
                assert!(success);
                assert_eq!(username.as_deref(), Some("bob"));
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_cannot_edit_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Look up admin's database ID
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        // Try to edit self
        let result = handle_user_edit(
            admin_user.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_cannot_edit_self(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected UserEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_useredit_non_admin_cannot_edit_admin() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Login as non-admin user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Look up admin's database ID
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        // Non-admin tries to fetch admin details for editing - should fail
        let result = handle_user_edit(
            admin_user.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(
                    !success,
                    "Non-admin should not be able to fetch admin details"
                );
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("admin"),
                    "Error should mention admin restriction"
                );
            }
            _ => panic!("Expected UserEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_useredit_returns_is_shared_for_shared_account() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account
        let shared_acct = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        let result = handle_user_edit(
            shared_acct.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                is_admin,
                is_shared,
                enabled,
                ..
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username.as_deref(), Some("shared_acct"));
                assert_eq!(is_admin, Some(false));
                assert_eq!(
                    is_shared,
                    Some(true),
                    "is_shared should be true for shared account"
                );
                assert_eq!(enabled, Some(true));
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_returns_is_shared_false_for_regular_account() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a regular (non-shared) account
        let bob = test_ctx
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
            })
            .await
            .unwrap();

        let result =
            handle_user_edit(bob.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse {
                success, is_shared, ..
            } => {
                assert!(success);
                assert_eq!(
                    is_shared,
                    Some(false),
                    "is_shared should be false for regular account"
                );
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_includes_group_info() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with permissions
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user assigned to the group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        let result =
            handle_user_edit(bob.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                group_id,
                group_name,
                group_permissions,
                available_groups,
                ..
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username.as_deref(), Some("bob"));

                // Verify group_id is set
                assert_eq!(group_id, Some(group.id));

                // Verify group_name is populated
                assert_eq!(group_name.as_deref(), Some("Mods"));

                // Verify group_permissions contains the group's permissions
                let gp = group_permissions.expect("group_permissions should be populated");
                assert!(gp.contains(&"chat_send".to_string()));
                assert!(gp.contains(&"user_kick".to_string()));
                assert_eq!(gp.len(), 2);

                // Verify available_groups is populated and contains our group
                let ag = available_groups.expect("available_groups should be populated");
                assert!(!ag.is_empty(), "available_groups should not be empty");
                let mods_group = ag.iter().find(|g| g.name == "Mods");
                assert!(
                    mods_group.is_some(),
                    "available_groups should contain Mods group"
                );
                let mods_group = mods_group.unwrap();
                assert_eq!(mods_group.id, group.id);
                assert!(!mods_group.is_shared);
                assert!(mods_group.permissions.contains(&"chat_send".to_string()));
                assert!(mods_group.permissions.contains(&"user_kick".to_string()));
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }
}
