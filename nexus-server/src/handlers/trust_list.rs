//! Handler for TrustList command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ServerMessage, TrustInfo};

use super::{HandlerContext, err_authentication, err_not_logged_in, err_permission_denied};
use crate::db::Permission;

/// Handle TrustList command
///
/// Returns a list of all active (non-expired) trusted IPs.
pub async fn handle_trust_list<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("TrustList request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrustList"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrustList"))
                .await;
        }
    };

    // Check trust_list permission
    if !requesting_user.has_permission(Permission::TrustList) {
        eprintln!(
            "TrustList from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::TrustListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            entries: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get all active trusts from database
    match ctx.db.trusts.list_active_trusts().await {
        Ok(trusts) => {
            let trust_infos: Vec<TrustInfo> = trusts
                .into_iter()
                .map(|t| TrustInfo {
                    ip_address: t.ip_address,
                    nickname: t.nickname,
                    reason: t.reason,
                    created_by: t.created_by,
                    created_at: t.created_at,
                    expires_at: t.expires_at,
                })
                .collect();

            let response = ServerMessage::TrustListResponse {
                success: true,
                error: None,
                entries: Some(trust_infos),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("TrustList database error: {}", e);
            let response = ServerMessage::TrustListResponse {
                success: false,
                error: Some(super::err_database(ctx.locale)),
                entries: None,
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
    async fn test_trustlist_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_trust_list(None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "TrustList should require login");
    }

    #[tokio::test]
    async fn test_trustlist_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without trust_list permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_trust_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustListResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustlist_admin_can_list() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create some trusts
        test_ctx
            .db
            .trusts
            .create_or_update_trust(
                "192.168.1.100",
                Some("alice"),
                Some("office network"),
                "admin",
                None,
            )
            .await
            .unwrap();
        test_ctx
            .db
            .trusts
            .create_or_update_trust("10.0.0.0/8", None, Some("internal network"), "admin", None)
            .await
            .unwrap();

        let result = handle_trust_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustListResponse {
            success,
            entries,
            error,
            ..
        } = response
        {
            assert!(success, "Expected success, got error: {:?}", error);
            let entries = entries.unwrap();
            assert_eq!(entries.len(), 2);

            // Check first trust
            let alice_trust = entries
                .iter()
                .find(|t| t.nickname == Some("alice".to_string()));
            assert!(alice_trust.is_some());
            let alice_trust = alice_trust.unwrap();
            assert_eq!(alice_trust.ip_address, "192.168.1.100");
            assert_eq!(alice_trust.reason, Some("office network".to_string()));
            assert_eq!(alice_trust.created_by, "admin");
            assert!(alice_trust.expires_at.is_none()); // permanent

            // Check CIDR trust
            let cidr_trust = entries.iter().find(|t| t.ip_address == "10.0.0.0/8");
            assert!(cidr_trust.is_some());
        } else {
            panic!("Expected TrustListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustlist_empty() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustListResponse {
            success, entries, ..
        } = response
        {
            assert!(success);
            let entries = entries.unwrap();
            assert!(entries.is_empty());
        } else {
            panic!("Expected TrustListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustlist_excludes_expired() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a permanent trust
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();

        // Create an expired trust
        let expired = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 1;
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.101", None, None, "admin", Some(expired))
            .await
            .unwrap();

        let result = handle_trust_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustListResponse {
            success, entries, ..
        } = response
        {
            assert!(success);
            let entries = entries.unwrap();
            // Should only have the permanent trust
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].ip_address, "192.168.1.100");
        } else {
            panic!("Expected TrustListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustlist_with_trust_list_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user with only trust_list permission
        let session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[Permission::TrustList],
            false,
        )
        .await;

        let result = handle_trust_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustListResponse { success, .. } = response {
            assert!(success);
        } else {
            panic!("Expected TrustListResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustlist_handles_many_entries() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create 100 trust entries (mix of IPs and CIDR ranges)
        for i in 0..100 {
            let ip = format!("192.168.{}.{}", i / 256, i % 256);
            test_ctx
                .db
                .trusts
                .create_or_update_trust(
                    &ip,
                    Some(&format!("user{}", i)),
                    Some(&format!("Trust reason {}", i)),
                    "admin",
                    None,
                )
                .await
                .unwrap();
        }

        let result = handle_trust_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustListResponse {
            success,
            entries,
            error,
            ..
        } = response
        {
            assert!(success, "Expected success, got error: {:?}", error);
            let entries = entries.unwrap();
            assert_eq!(entries.len(), 100);

            // Verify entries have expected structure
            for entry in &entries {
                assert!(entry.ip_address.starts_with("192.168."));
                assert!(entry.nickname.is_some());
                assert!(entry.reason.is_some());
                assert_eq!(entry.created_by, "admin");
                assert!(entry.expires_at.is_none()); // permanent
            }
        } else {
            panic!("Expected TrustListResponse, got: {:?}", response);
        }
    }
}
