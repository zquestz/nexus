//! Handler for BanList command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{BanInfo, ServerMessage};

use super::{HandlerContext, err_authentication, err_not_logged_in, err_permission_denied};
use crate::db::Permission;

/// Handle BanList command
///
/// Returns a list of all active (non-expired) bans.
pub async fn handle_ban_list<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("BanList request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("BanList"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("BanList"))
                .await;
        }
    };

    // Check ban_list permission
    if !requesting_user.has_permission(Permission::BanList) {
        eprintln!(
            "BanList from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::BanListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            bans: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get all active bans from database
    match ctx.db.bans.list_active_bans().await {
        Ok(ban_records) => {
            let bans: Vec<BanInfo> = ban_records
                .into_iter()
                .map(|record| BanInfo {
                    ip_address: record.ip_address,
                    nickname: record.nickname,
                    reason: record.reason,
                    created_by: record.created_by,
                    created_at: record.created_at,
                    expires_at: record.expires_at,
                })
                .collect();

            let response = ServerMessage::BanListResponse {
                success: true,
                error: None,
                bans: Some(bans),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("BanList database error: {}", e);
            let response = ServerMessage::BanListResponse {
                success: false,
                error: Some(super::err_database(ctx.locale)),
                bans: None,
            };
            ctx.send_message(&response).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_banlist_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_ban_list(None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "BanList should require login");
    }

    #[tokio::test]
    async fn test_banlist_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without ban_list permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_ban_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanListResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_banlist_admin_can_list() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create some bans
        test_ctx
            .db
            .bans
            .create_or_update_ban(
                "192.168.1.100",
                Some("alice"),
                Some("flooding"),
                "admin",
                None,
            )
            .await
            .unwrap();
        test_ctx
            .db
            .bans
            .create_or_update_ban("10.0.0.1", None, None, "admin", None)
            .await
            .unwrap();

        let result = handle_ban_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanListResponse {
            success,
            bans,
            error,
        } = response
        {
            assert!(success);
            assert!(error.is_none());
            let bans = bans.unwrap();
            assert_eq!(bans.len(), 2);
        } else {
            panic!("Expected BanListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_banlist_empty() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanListResponse {
            success,
            bans,
            error,
        } = response
        {
            assert!(success);
            assert!(error.is_none());
            assert_eq!(bans.unwrap().len(), 0);
        } else {
            panic!("Expected BanListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_banlist_excludes_expired() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create an active ban
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();

        // Create an expired ban
        let expired = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 1;
        test_ctx
            .db
            .bans
            .create_or_update_ban("10.0.0.1", None, None, "admin", Some(expired))
            .await
            .unwrap();

        let result = handle_ban_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanListResponse { success, bans, .. } = response {
            assert!(success);
            let bans = bans.unwrap();
            // Only the active ban should be returned
            assert_eq!(bans.len(), 1);
            assert_eq!(bans[0].ip_address, "192.168.1.100");
        } else {
            panic!("Expected BanListResponse, got: {:?}", response);
        }
    }
}
