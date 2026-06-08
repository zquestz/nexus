//! GroupDelete message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use crate::constants::*;

use nexus_common::protocol::ServerMessage;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_database, err_group_not_empty_delete, err_group_not_found,
    err_not_logged_in, err_permission_denied,
};
use crate::db::{DeleteGroupResult, Permission};

pub async fn handle_group_delete<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_GROUP_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_GROUP_DELETE))
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
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_GROUP_DELETE),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::GroupDelete) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_GROUP_DELETE_PERMISSION_DENIED);
        let response = ServerMessage::GroupDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch the group to echo its name in the response.
    let group = match ctx.db.groups.get_group_by_id(id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            let response = ServerMessage::GroupDeleteResponse {
                success: false,
                error: Some(err_group_not_found(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_DELETE_DB_ERROR);
            let response = ServerMessage::GroupDeleteResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    match ctx.db.groups.delete_group(id).await {
        Ok(DeleteGroupResult::Deleted) => {}
        Ok(DeleteGroupResult::NotFound) => {
            let response = ServerMessage::GroupDeleteResponse {
                success: false,
                error: Some(err_group_not_found(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Ok(DeleteGroupResult::NotEmpty) => {
            let response = ServerMessage::GroupDeleteResponse {
                success: false,
                error: Some(err_group_not_empty_delete(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_DELETE_DB_ERROR);
            let response = ServerMessage::GroupDeleteResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    info!(user = %requesting_user.username, ip = %ctx.peer_addr, group = %group.name, "{}", LOG_GROUP_DELETE_SUCCESS);
    let response = ServerMessage::GroupDeleteResponse {
        success: true,
        error: None,
        id: Some(id),
        name: Some(group.name),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, Permissions};
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_delete_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_delete(1, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_group_delete_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_group_delete(1, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupDeleteResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupDeleteResponse with permission denied"),
        }
    }

    #[tokio::test]
    async fn test_group_delete_not_found() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupDelete],
            false,
        )
        .await;

        let result =
            handle_group_delete(99999, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupDeleteResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_found(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupDeleteResponse with not found error"),
        }
    }

    #[tokio::test]
    async fn test_group_delete_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupDelete],
            false,
        )
        .await;

        let group = test_ctx
            .db
            .groups
            .create_group("TestGroup", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let result =
            handle_group_delete(group.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupDeleteResponse {
                success,
                error,
                id,
                name,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(group.id));
                assert_eq!(name, Some("TestGroup".to_string()));
            }
            _ => panic!("Expected successful GroupDeleteResponse"),
        }

        // Verify it's actually deleted
        let fetched = test_ctx.db.groups.get_group_by_id(group.id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_group_delete_with_members_rejected() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupDelete],
            false,
        )
        .await;

        // Create a group and assign a user to it
        let group = test_ctx
            .db
            .groups
            .create_group("BusyGroup", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let hashed = crate::handlers::testing::get_cached_password_hash("hash");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "member",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let result =
            handle_group_delete(group.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupDeleteResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_empty_delete(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupDeleteResponse with not empty error"),
        }

        // Verify it's NOT deleted
        let fetched = test_ctx.db.groups.get_group_by_id(group.id).await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_group_delete_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group("AdminDeleteMe", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let result =
            handle_group_delete(group.id, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupDeleteResponse {
                success,
                error,
                id,
                name,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(group.id));
                assert_eq!(name, Some("AdminDeleteMe".to_string()));
            }
            _ => panic!("Expected successful GroupDeleteResponse"),
        }

        // Verify it's actually deleted
        let fetched = test_ctx.db.groups.get_group_by_id(group.id).await.unwrap();
        assert!(fetched.is_none());
    }
}
