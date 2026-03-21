//! GroupList message handler - Returns all groups

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;

/// Handle a group list request
///
/// Returns all groups with their permissions and member counts.
/// Requires any one of: `user_create`, `user_edit`, `group_create`,
/// `group_edit`, or `group_delete` permission.
pub async fn handle_group_list<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(session_id) = session_id else {
        eprintln!("GroupList request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("GroupList"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("GroupList"))
                .await;
        }
    };

    // Check permission — any one of 5 permissions grants access
    let has_access = requesting_user.has_permission(Permission::UserCreate)
        || requesting_user.has_permission(Permission::UserEdit)
        || requesting_user.has_permission(Permission::GroupCreate)
        || requesting_user.has_permission(Permission::GroupEdit)
        || requesting_user.has_permission(Permission::GroupDelete);

    if !has_access {
        eprintln!(
            "GroupList from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::GroupListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            groups: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch all groups with member counts and permissions (3 queries total)
    let groups = match ctx.db.groups.get_all_groups_with_details().await {
        Ok(groups) => groups,
        Err(e) => {
            eprintln!("Database error getting groups: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupList"))
                .await;
        }
    };

    let response = ServerMessage::GroupListResponse {
        success: true,
        error: None,
        groups: Some(groups),
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, Permissions};
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_list_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_list(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_group_list_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without any relevant permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_group_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupListResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_group_list_empty() {
        let mut test_ctx = create_test_context().await;

        // Login as user with GroupEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        let result = handle_group_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupListResponse {
                success,
                error,
                groups,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(groups.unwrap().len(), 0);
            }
            _ => panic!("Expected GroupListResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_list_returns_groups() {
        let mut test_ctx = create_test_context().await;

        // Login as user with GroupEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create some groups directly in the database
        test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::UserKick, Permission::BanCreate]),
            )
            .await
            .expect("Failed to create Staff group");

        test_ctx
            .db
            .groups
            .create_group("Guests", true, &Permissions::new())
            .await
            .expect("Failed to create Guests group");

        let result = handle_group_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupListResponse {
                success,
                error,
                groups,
            } => {
                assert!(success);
                assert!(error.is_none());
                let groups = groups.unwrap();
                assert_eq!(groups.len(), 2);

                // Groups are ordered by name (case-insensitive)
                assert_eq!(groups[0].name, "Guests");
                assert!(groups[0].is_shared);
                assert_eq!(groups[0].member_count, 0);
                assert!(groups[0].permissions.is_empty());

                assert_eq!(groups[1].name, "Staff");
                assert!(!groups[1].is_shared);
                assert_eq!(groups[1].member_count, 0);
                assert_eq!(groups[1].permissions.len(), 2);
                // Permissions are sorted alphabetically
                assert!(groups[1].permissions.contains(&"ban_create".to_string()));
                assert!(groups[1].permissions.contains(&"user_kick".to_string()));
            }
            _ => panic!("Expected GroupListResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_list_admin_has_access() {
        let mut test_ctx = create_test_context().await;

        // Login as admin (no explicit permissions needed)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_group_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected GroupListResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_list_user_create_has_access() {
        let mut test_ctx = create_test_context().await;

        // Login as user with only UserCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserCreate],
            false,
        )
        .await;

        let result = handle_group_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected GroupListResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_list_user_edit_has_access() {
        let mut test_ctx = create_test_context().await;

        // Login as user with only UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        let result = handle_group_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected GroupListResponse"),
        }
    }
}
