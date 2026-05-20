use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, TargetError};

use super::{
    HandlerContext, err_not_logged_in, err_permission_denied, err_target_too_long,
    err_trust_not_found,
};
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::canonicalize_target;

/// Removes trusted IP(s) by target: a nickname annotation (all trusts with it),
/// a single IP, or a CIDR range (the range plus any IPs/smaller ranges inside it).
pub async fn handle_trust_delete<W>(
    target: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRUST_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRUST_DELETE))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRUST_DELETE),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrustDelete) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_TRUST_DELETE_PERMISSION_DENIED);
        let response = ServerMessage::TrustDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_target(&target) {
        let error_msg = match e {
            TargetError::Empty => err_trust_not_found(ctx.locale, &target),
            TargetError::TooLong => err_target_too_long(ctx.locale, validators::MAX_TARGET_LENGTH),
        };
        let response = ServerMessage::TrustDeleteResponse {
            success: false,
            error: Some(error_msg),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Try untrust by nickname annotation first.
    if ctx
        .db
        .trusts
        .has_trusts_for_nickname(&target)
        .await
        .unwrap_or(false)
    {
        match ctx.db.trusts.delete_trusts_by_nickname(&target).await {
            Ok(deleted_ips) => {
                {
                    let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
                    for ip in &deleted_ips {
                        cache.remove_trust(ip);
                    }
                }

                info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_TRUST_DELETE_SUCCESS);
                let response = ServerMessage::TrustDeleteResponse {
                    success: true,
                    error: None,
                    ips: Some(deleted_ips),
                    // Echo the canonical lowercase form the DB stores, not the
                    // admin-typed casing, for consistency with `trust_create`.
                    nickname: Some(target.to_lowercase()),
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, err = %e, "{}", LOG_TRUST_DELETE_DB_ERROR_NICKNAME);
                let response = ServerMessage::TrustDeleteResponse {
                    success: false,
                    error: Some(super::err_database(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    // Canonicalize once: /32 and /128 collapse to bare IP, CIDR host bits
    // zero out, uppercase-hex IPv6 folds to lowercase. `net`/`is_range` come
    // back from `canonicalize_target` so we dispatch without re-parsing.
    let Some((canonical, net, is_range)) = canonicalize_target(&target) else {
        // Neither nickname annotation, CIDR, nor valid IP.
        let response = ServerMessage::TrustDeleteResponse {
            success: false,
            error: Some(err_trust_not_found(ctx.locale, &target)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    };

    if is_range {
        // Delete the CIDR row itself plus any entries contained within it.
        let mut all_deleted = Vec::new();

        if let Ok(true) = ctx.db.trusts.delete_trust_by_ip(&canonical).await {
            all_deleted.push(canonical.clone());
        }

        match ctx.db.trusts.delete_trusts_in_range(&net).await {
            Ok(deleted) => {
                all_deleted.extend(deleted);
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, err = %e, "{}", LOG_TRUST_DELETE_DB_ERROR_CIDR);
                let response = ServerMessage::TrustDeleteResponse {
                    success: false,
                    error: Some(super::err_database(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        }

        if all_deleted.is_empty() {
            let response = ServerMessage::TrustDeleteResponse {
                success: false,
                error: Some(err_trust_not_found(ctx.locale, &target)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }

        {
            let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
            cache.remove_trusts_contained_by(&canonical);
            cache.remove_trust(&canonical);
        }

        info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_TRUST_DELETE_SUCCESS);
        let response = ServerMessage::TrustDeleteResponse {
            success: true,
            error: None,
            ips: Some(all_deleted),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Single-IP branch (canonical is a bare IP).
    match ctx.db.trusts.delete_trust_by_ip(&canonical).await {
        Ok(true) => {
            {
                let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
                cache.remove_trust(&canonical);
            }

            info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_TRUST_DELETE_SUCCESS);
            let response = ServerMessage::TrustDeleteResponse {
                success: true,
                error: None,
                ips: Some(vec![canonical]),
                nickname: None,
            };
            ctx.send_message(&response).await
        }
        Ok(false) => {
            let response = ServerMessage::TrustDeleteResponse {
                success: false,
                error: Some(err_trust_not_found(ctx.locale, &target)),
                ips: None,
                nickname: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, err = %e, "{}", LOG_TRUST_DELETE_DB_ERROR_IP);
            let response = ServerMessage::TrustDeleteResponse {
                success: false,
                error: Some(super::err_database(ctx.locale)),
                ips: None,
                nickname: None,
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
    async fn test_trustdelete_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_trust_delete(
            "192.168.1.100".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "TrustDelete should require login");
    }

    #[tokio::test]
    async fn test_trustdelete_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without trust_delete permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_trust_delete(
            "192.168.1.100".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustdelete_admin_can_untrust() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a trust first
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();

        // Also add to cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("192.168.1.100", None);
        }

        let result = handle_trust_delete(
            "192.168.1.100".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, ips, .. } = response {
            assert!(success);
            assert!(ips.is_some());
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }

        // Verify trust is deleted from DB
        assert!(
            !test_ctx
                .db
                .trusts
                .trust_exists("192.168.1.100")
                .await
                .unwrap()
        );

        // Verify trust is deleted from cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(!cache.is_trusted("192.168.1.100".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_trustdelete_by_nickname() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create trusts with nickname annotation
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .unwrap();
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.101", Some("alice"), None, "admin", None)
            .await
            .unwrap();

        // Also add to cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("192.168.1.100", None);
            cache.add_trust("192.168.1.101", None);
        }

        let result = handle_trust_delete(
            "alice".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse {
            success,
            ips,
            nickname,
            ..
        } = response
        {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 2);
            assert_eq!(nickname, Some("alice".to_string()));
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }

        // Verify both trusts are deleted from DB
        assert!(
            !test_ctx
                .db
                .trusts
                .trust_exists("192.168.1.100")
                .await
                .unwrap()
        );
        assert!(
            !test_ctx
                .db
                .trusts
                .trust_exists("192.168.1.101")
                .await
                .unwrap()
        );

        // Verify both trusts are deleted from cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(!cache.is_trusted("192.168.1.100".parse().unwrap()));
            assert!(!cache.is_trusted("192.168.1.101".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_trustdelete_not_found() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_delete(
            "192.168.1.100".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustdelete_cidr_range() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a CIDR trust and some single IP trusts within that range
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.0/24", None, None, "admin", None)
            .await
            .unwrap();
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.50", None, None, "admin", None)
            .await
            .unwrap();
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();
        // This one should NOT be deleted (different range)
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.2.1", None, None, "admin", None)
            .await
            .unwrap();

        // Add to cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("192.168.1.0/24", None);
            cache.add_trust("192.168.1.50", None);
            cache.add_trust("192.168.1.100", None);
            cache.add_trust("192.168.2.1", None);
        }

        // Delete the CIDR range - should also delete contained IPs
        let result = handle_trust_delete(
            "192.168.1.0/24".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            // Should have deleted the CIDR and the two single IPs within it
            assert!(!ips.is_empty()); // At least the CIDR itself
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }

        // Verify CIDR trust is deleted
        assert!(
            !test_ctx
                .db
                .trusts
                .trust_exists("192.168.1.0/24")
                .await
                .unwrap()
        );

        // Verify the other range's trust still exists
        assert!(
            test_ctx
                .db
                .trusts
                .trust_exists("192.168.2.1")
                .await
                .unwrap()
        );

        // Verify cache state
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(!cache.is_trusted("192.168.1.50".parse().unwrap()));
            assert!(cache.is_trusted("192.168.2.1".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_trustdelete_target_too_long() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a target that's too long
        let long_target = "x".repeat(validators::MAX_TARGET_LENGTH + 1);

        let result = handle_trust_delete(
            long_target,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustdelete_target_empty() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_delete(
            "".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustdelete_cidr_with_host_bits_finds_canonical_row() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Trust stored in canonical CIDR form (network address, zeroed host bits)
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.0/24", None, None, "admin", None)
            .await
            .unwrap();
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("192.168.1.0/24", None);
        }

        // Admin types the CIDR with host bits set
        let result = handle_trust_delete(
            "192.168.1.5/24".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert!(ips.contains(&"192.168.1.0/24".to_string()));
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .trusts
                .trust_exists("192.168.1.0/24")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_trustdelete_finds_trust_with_uppercase_input() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Trust stored in canonical lowercase form
        test_ctx
            .db
            .trusts
            .create_or_update_trust("2001:db8::1", None, None, "admin", None)
            .await
            .unwrap();
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("2001:db8::1", None);
        }

        // Admin types the uppercase form to delete it
        let result = handle_trust_delete(
            "2001:DB8::1".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips, vec!["2001:db8::1".to_string()]);
        } else {
            panic!("Expected TrustDeleteResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .trusts
                .trust_exists("2001:db8::1")
                .await
                .unwrap()
        );
    }
}
