use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, TargetError};

use super::{
    HandlerContext, err_ban_not_found, err_not_logged_in, err_permission_denied,
    err_target_too_long,
};
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::canonicalize_target;

/// Removes IP ban(s) by target: a nickname annotation (all bans with it), a
/// single IP, or a CIDR range (the range plus any IPs/smaller ranges inside it).
pub async fn handle_ban_delete<W>(
    target: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_BAN_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_BAN_DELETE))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_BAN_DELETE))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::BanDelete) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_BAN_DELETE_PERMISSION_DENIED);
        let response = ServerMessage::BanDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_target(&target) {
        let error_msg = match e {
            TargetError::Empty => err_ban_not_found(ctx.locale, &target),
            TargetError::TooLong => err_target_too_long(ctx.locale, validators::MAX_TARGET_LENGTH),
        };
        let response = ServerMessage::BanDeleteResponse {
            success: false,
            error: Some(error_msg),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Try unban by nickname annotation first.
    if ctx
        .db
        .bans
        .has_bans_for_nickname(&target)
        .await
        .unwrap_or(false)
    {
        match ctx.db.bans.delete_bans_by_nickname(&target).await {
            Ok(deleted_ips) => {
                {
                    let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
                    for ip in &deleted_ips {
                        cache.remove_ban(ip);
                    }
                }

                info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_BAN_DELETE_SUCCESS);
                let response = ServerMessage::BanDeleteResponse {
                    success: true,
                    error: None,
                    ips: Some(deleted_ips),
                    // Echo the admin's typed target, not the folded key — the
                    // fold is internal, and delete matches `nickname_lower`
                    // across possibly-many rows with no single display casing.
                    nickname: Some(target.clone()),
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, err = %e, "{}", LOG_BAN_DELETE_DB_ERROR_NICKNAME);
                let response = ServerMessage::BanDeleteResponse {
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
        let response = ServerMessage::BanDeleteResponse {
            success: false,
            error: Some(err_ban_not_found(ctx.locale, &target)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    };

    if is_range {
        // Delete the CIDR row itself plus any entries contained within it.
        let mut all_deleted = Vec::new();

        if let Ok(true) = ctx.db.bans.delete_ban_by_ip(&canonical).await {
            all_deleted.push(canonical.clone());
        }

        match ctx.db.bans.delete_bans_in_range(&net).await {
            Ok(deleted) => {
                all_deleted.extend(deleted);
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, err = %e, "{}", LOG_BAN_DELETE_DB_ERROR_CIDR);
                let response = ServerMessage::BanDeleteResponse {
                    success: false,
                    error: Some(super::err_database(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        }

        if all_deleted.is_empty() {
            let response = ServerMessage::BanDeleteResponse {
                success: false,
                error: Some(err_ban_not_found(ctx.locale, &target)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }

        {
            let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
            cache.remove_bans_contained_by(&canonical);
            cache.remove_ban(&canonical);
        }

        info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_BAN_DELETE_SUCCESS);
        let response = ServerMessage::BanDeleteResponse {
            success: true,
            error: None,
            ips: Some(all_deleted),
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Single-IP branch (canonical is a bare IP).
    match ctx.db.bans.delete_ban_by_ip(&canonical).await {
        Ok(true) => {
            {
                let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
                cache.remove_ban(&canonical);
            }

            info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_BAN_DELETE_SUCCESS);
            let response = ServerMessage::BanDeleteResponse {
                success: true,
                error: None,
                ips: Some(vec![canonical]),
                nickname: None,
            };
            ctx.send_message(&response).await
        }
        Ok(false) => {
            let response = ServerMessage::BanDeleteResponse {
                success: false,
                error: Some(err_ban_not_found(ctx.locale, &target)),
                ips: None,
                nickname: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, err = %e, "{}", LOG_BAN_DELETE_DB_ERROR_IP);
            let response = ServerMessage::BanDeleteResponse {
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
    async fn test_bandelete_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_ban_delete(
            "192.168.1.100".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "BanDelete should require login");
    }

    #[tokio::test]
    async fn test_bandelete_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without ban_delete permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_ban_delete(
            "192.168.1.100".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bandelete_admin_can_unban() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a ban first
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();

        // Also add to cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("192.168.1.100", None);
        }

        let result = handle_ban_delete(
            "192.168.1.100".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, ips, .. } = response {
            assert!(success);
            assert!(ips.is_some());
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }

        // Verify ban is deleted from DB
        assert!(!test_ctx.db.bans.ban_exists("192.168.1.100").await.unwrap());

        // Verify ban is deleted from cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_bandelete_by_nickname() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bans with nickname annotation
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.100", Some("spammer"), None, "admin", None)
            .await
            .unwrap();
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.101", Some("spammer"), None, "admin", None)
            .await
            .unwrap();

        // Also add to cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("192.168.1.100", None);
            cache.add_ban("192.168.1.101", None);
        }

        let result = handle_ban_delete(
            "spammer".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse {
            success,
            ips,
            nickname,
            ..
        } = response
        {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 2);
            assert_eq!(nickname, Some("spammer".to_string()));
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }

        // Verify both bans are deleted from DB
        assert!(!test_ctx.db.bans.ban_exists("192.168.1.100").await.unwrap());
        assert!(!test_ctx.db.bans.ban_exists("192.168.1.101").await.unwrap());

        // Verify both bans are deleted from cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(!cache.is_banned("192.168.1.100".parse().unwrap()));
            assert!(!cache.is_banned("192.168.1.101".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_bandelete_not_found() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_delete(
            "192.168.1.100".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bandelete_cidr_range() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a CIDR ban and some single IP bans within that range
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.0/24", None, None, "admin", None)
            .await
            .unwrap();
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.50", None, None, "admin", None)
            .await
            .unwrap();
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();
        // This one should NOT be deleted (different range)
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.2.1", None, None, "admin", None)
            .await
            .unwrap();

        // Add to cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("192.168.1.0/24", None);
            cache.add_ban("192.168.1.50", None);
            cache.add_ban("192.168.1.100", None);
            cache.add_ban("192.168.2.1", None);
        }

        // Delete the CIDR range - should also delete contained IPs
        let result = handle_ban_delete(
            "192.168.1.0/24".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            // Should have deleted the CIDR and the two single IPs within it
            assert!(!ips.is_empty()); // At least the CIDR itself
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }

        // Verify CIDR ban is deleted
        assert!(!test_ctx.db.bans.ban_exists("192.168.1.0/24").await.unwrap());

        // Verify the other range's ban still exists
        assert!(test_ctx.db.bans.ban_exists("192.168.2.1").await.unwrap());

        // Verify cache state
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(!cache.is_banned("192.168.1.50".parse().unwrap()));
            assert!(cache.is_banned("192.168.2.1".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_bandelete_target_too_long() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a target that's too long
        let long_target = "x".repeat(validators::MAX_TARGET_LENGTH + 1);

        let result = handle_ban_delete(
            long_target,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bandelete_target_empty() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_delete(
            "".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bandelete_cidr_with_host_bits_finds_canonical_row() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Ban stored in canonical CIDR form (network address, zeroed host bits)
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.0/24", None, None, "admin", None)
            .await
            .unwrap();
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("192.168.1.0/24", None);
        }

        // Admin types the CIDR with host bits set
        let result = handle_ban_delete(
            "192.168.1.5/24".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            // Exact-match canonical delete path returns the canonical CIDR
            assert!(ips.contains(&"192.168.1.0/24".to_string()));
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }

        assert!(!test_ctx.db.bans.ban_exists("192.168.1.0/24").await.unwrap());
    }

    #[tokio::test]
    async fn test_bandelete_slash32_finds_bare_ip() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Ban stored as a bare IPv4 (the canonical form a /32 collapses to)
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.100", None, None, "admin", None)
            .await
            .unwrap();
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("192.168.1.100", None);
        }

        // Admin types the /32 form — should still find and delete the bare-IP row
        let result = handle_ban_delete(
            "192.168.1.100/32".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips, vec!["192.168.1.100".to_string()]);
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }

        assert!(!test_ctx.db.bans.ban_exists("192.168.1.100").await.unwrap());
    }

    #[tokio::test]
    async fn test_bandelete_echoes_typed_nickname_not_fold() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Ban annotated with a mixed-case nickname (display in `nickname`,
        // folded in `nickname_lower`).
        test_ctx
            .db
            .bans
            .create_or_update_ban("192.168.1.100", Some("Renée"), None, "admin", None)
            .await
            .unwrap();
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("192.168.1.100", None);
        }

        // Admin deletes by typing a different case; the response echoes what
        // they typed, never the internal folded key.
        let result = handle_ban_delete(
            "RENÉE".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::BanDeleteResponse {
                success,
                ips,
                nickname,
                ..
            } => {
                assert!(success);
                assert_eq!(ips.unwrap(), vec!["192.168.1.100".to_string()]);
                assert_eq!(nickname, Some("RENÉE".to_string()));
            }
            other => panic!("Expected BanDeleteResponse, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_bandelete_finds_ban_with_uppercase_input() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Ban stored in canonical lowercase form
        test_ctx
            .db
            .bans
            .create_or_update_ban("2001:db8::1", None, None, "admin", None)
            .await
            .unwrap();
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_ban("2001:db8::1", None);
        }

        // Admin types the uppercase form to delete it
        let result = handle_ban_delete(
            "2001:DB8::1".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanDeleteResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips, vec!["2001:db8::1".to_string()]);
        } else {
            panic!("Expected BanDeleteResponse, got: {:?}", response);
        }

        assert!(!test_ctx.db.bans.ban_exists("2001:db8::1").await.unwrap());
    }
}
