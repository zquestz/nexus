//! GroupEdit message handler - Returns group details for editing

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_database, err_group_not_found, err_not_logged_in,
    err_permission_denied,
};
use crate::db::Permission;

/// Handle a group edit request (returns group details for editing)
pub async fn handle_group_edit<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(requesting_session_id) = session_id else {
        eprintln!("GroupEdit request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("GroupEdit"))
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
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("GroupEdit"))
                .await;
        }
    };

    // Check GroupEdit permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::GroupEdit) {
        eprintln!(
            "GroupEdit from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::GroupEditResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            name: None,
            is_shared: None,
            permissions: None,
            member_count: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch group by ID
    let group = match ctx.db.groups.get_group_by_id(id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            let response = ServerMessage::GroupEditResponse {
                success: false,
                error: Some(err_group_not_found(ctx.locale)),
                id: None,
                name: None,
                is_shared: None,
                permissions: None,
                member_count: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting group: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupEdit"))
                .await;
        }
    };

    // Fetch group permissions
    let permissions = match ctx.db.groups.get_group_permissions(id).await {
        Ok(perms) => perms,
        Err(e) => {
            eprintln!("Database error getting group permissions: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupEdit"))
                .await;
        }
    };

    // Fetch member count
    let member_count = match ctx.db.groups.get_member_count(id).await {
        Ok(count) => count,
        Err(e) => {
            eprintln!("Database error getting group member count: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("GroupEdit"))
                .await;
        }
    };

    // Send group details for editing
    let response = ServerMessage::GroupEditResponse {
        success: true,
        error: None,
        id: Some(group.id),
        name: Some(group.name),
        is_shared: Some(group.is_shared),
        permissions: Some(permissions),
        member_count: Some(member_count),
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_edit_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_edit(
            1,
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_group_edit_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without GroupEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_group_edit(1, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_group_edit_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_group_edit(9999, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_found(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_group_edit_success() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with permissions
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &["news_edit".to_string(), "chat_send".to_string()],
            )
            .await
            .unwrap();

        let result =
            handle_group_edit(group.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupEditResponse {
                success,
                error,
                id,
                name,
                is_shared,
                permissions,
                member_count,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(group.id));
                assert_eq!(name.as_deref(), Some("Editors"));
                assert_eq!(is_shared, Some(false));
                let perms = permissions.unwrap();
                assert_eq!(perms.len(), 2);
                assert!(perms.contains(&"news_edit".to_string()));
                assert!(perms.contains(&"chat_send".to_string()));
                assert_eq!(member_count, Some(0));
            }
            _ => panic!("Expected GroupEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_group_edit_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin (admins bypass permission checks)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared group
        let group = test_ctx
            .db
            .groups
            .create_group("SharedGroup", true, &[])
            .await
            .unwrap();

        let result =
            handle_group_edit(group.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupEditResponse {
                success,
                error,
                id,
                name,
                is_shared,
                permissions,
                member_count,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(group.id));
                assert_eq!(name.as_deref(), Some("SharedGroup"));
                assert_eq!(is_shared, Some(true));
                assert_eq!(permissions, Some(vec![]));
                assert_eq!(member_count, Some(0));
            }
            _ => panic!("Expected GroupEditResponse"),
        }
    }
}
