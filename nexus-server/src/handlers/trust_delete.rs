//! Handler for TrustDelete command

use std::io;
use std::net::IpAddr;

use ipnet::IpNet;
use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;

use super::{
    HandlerContext, err_authentication, err_not_logged_in, err_permission_denied,
    err_trust_not_found,
};
use crate::db::Permission;

/// Handle TrustDelete command
///
/// Removes trusted IP(s). The target can be:
/// - A nickname annotation (removes all trusts with that annotation)
/// - An IP address (removes that specific trust)
/// - A CIDR range (removes the range AND any single IPs/smaller ranges within it)
pub async fn handle_trust_delete<W>(
    target: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("TrustDelete request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrustDelete"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrustDelete"))
                .await;
        }
    };

    // Check trust_delete permission
    if !requesting_user.has_permission(Permission::TrustDelete) {
        eprintln!(
            "TrustDelete from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::TrustDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // First, try to untrust by nickname annotation
    if ctx
        .db
        .trusts
        .has_trusts_for_nickname(&target)
        .await
        .unwrap_or(false)
    {
        match ctx.db.trusts.delete_trusts_by_nickname(&target).await {
            Ok(deleted_ips) => {
                // Update cache
                {
                    let mut cache = ctx
                        .ip_rule_cache
                        .write()
                        .expect("ip rule cache lock poisoned");
                    for ip in &deleted_ips {
                        cache.remove_trust(ip);
                    }
                }

                let response = ServerMessage::TrustDeleteResponse {
                    success: true,
                    error: None,
                    ips: Some(deleted_ips),
                    nickname: Some(target),
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                eprintln!("TrustDelete database error for nickname {}: {}", target, e);
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

    // Try to parse as CIDR range
    if let Ok(net) = target.parse::<IpNet>() {
        // Check if it's actually a range (prefix length < max)
        let is_range = match net {
            IpNet::V4(v4) => v4.prefix_len() < 32,
            IpNet::V6(v6) => v6.prefix_len() < 128,
        };

        if is_range {
            // For CIDR ranges, delete the range itself AND any contained entries
            let mut all_deleted = Vec::new();

            // First, try to delete the exact CIDR entry
            let cidr_str = net.to_string();
            if let Ok(true) = ctx.db.trusts.delete_trust_by_ip(&cidr_str).await {
                all_deleted.push(cidr_str.clone());
            }

            // Then, delete any entries contained within this range
            match ctx.db.trusts.delete_trusts_in_range(&net).await {
                Ok(deleted) => {
                    all_deleted.extend(deleted);
                }
                Err(e) => {
                    eprintln!("TrustDelete database error for CIDR {}: {}", target, e);
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

            // Update cache - remove the CIDR and all contained entries
            {
                let mut cache = ctx
                    .ip_rule_cache
                    .write()
                    .expect("ip rule cache lock poisoned");
                cache.remove_trusts_contained_by(&net.to_string());
                // Also remove the exact CIDR entry if it existed
                cache.remove_trust(&net.to_string());
            }

            let response = ServerMessage::TrustDeleteResponse {
                success: true,
                error: None,
                ips: Some(all_deleted),
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    }

    // Try to parse as single IP address
    if target.parse::<IpAddr>().is_ok() {
        match ctx.db.trusts.delete_trust_by_ip(&target).await {
            Ok(true) => {
                // Update cache
                {
                    let mut cache = ctx
                        .ip_rule_cache
                        .write()
                        .expect("ip rule cache lock poisoned");
                    cache.remove_trust(&target);
                }

                let response = ServerMessage::TrustDeleteResponse {
                    success: true,
                    error: None,
                    ips: Some(vec![target]),
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
            Ok(false) => {
                // No trust found for this IP
                let response = ServerMessage::TrustDeleteResponse {
                    success: false,
                    error: Some(err_trust_not_found(ctx.locale, &target)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                eprintln!("TrustDelete database error for IP {}: {}", target, e);
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

    // Target is neither a nickname with trusts, CIDR, nor valid IP
    let response = ServerMessage::TrustDeleteResponse {
        success: false,
        error: Some(err_trust_not_found(ctx.locale, &target)),
        ips: None,
        nickname: None,
    };
    ctx.send_message(&response).await
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

        let response = read_server_message(&mut test_ctx.client).await;
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

        let response = read_server_message(&mut test_ctx.client).await;
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

        let response = read_server_message(&mut test_ctx.client).await;
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

        let response = read_server_message(&mut test_ctx.client).await;
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

        let response = read_server_message(&mut test_ctx.client).await;
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
}
