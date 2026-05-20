//! GroupEdit message handler — returns group details for editing

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, warn};

use crate::constants::*;

use nexus_common::protocol::ServerMessage;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_database, err_group_not_found, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;

pub async fn handle_group_edit<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_GROUP_EDIT_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_GROUP_EDIT))
            .await;
    };

    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_GROUP_EDIT))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::GroupEdit) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_GROUP_EDIT_PERMISSION_DENIED);
        let response = ServerMessage::GroupEditResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            name: None,
            is_shared: None,
            permissions: None,
            member_count: None,
            bandwidth_weight: None,
        };
        return ctx.send_message(&response).await;
    }

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
                bandwidth_weight: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_EDIT_DB_ERROR);
            let response = ServerMessage::GroupEditResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
                name: None,
                is_shared: None,
                permissions: None,
                member_count: None,
                bandwidth_weight: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let permissions: Vec<String> = match ctx.db.groups.get_group_permissions(id).await {
        Ok(perms) => perms.into_iter().map(|p| p.as_str().to_string()).collect(),
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_EDIT_DB_ERROR);
            let response = ServerMessage::GroupEditResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
                name: None,
                is_shared: None,
                permissions: None,
                member_count: None,
                bandwidth_weight: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let member_count = match ctx.db.groups.get_member_count(id).await {
        Ok(count) => count,
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_EDIT_DB_ERROR);
            let response = ServerMessage::GroupEditResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
                name: None,
                is_shared: None,
                permissions: None,
                member_count: None,
                bandwidth_weight: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let response = ServerMessage::GroupEditResponse {
        success: true,
        error: None,
        id: Some(group.id),
        name: Some(group.name),
        is_shared: Some(group.is_shared),
        permissions: Some(permissions),
        member_count: Some(member_count),
        bandwidth_weight: Some(group.bandwidth_weight),
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permissions;
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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &Permissions::from(&[Permission::NewsEdit, Permission::ChatSend]),
                1,
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
                bandwidth_weight: _,
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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group("SharedGroup", true, &Permissions::new(), 1)
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
                bandwidth_weight: _,
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
