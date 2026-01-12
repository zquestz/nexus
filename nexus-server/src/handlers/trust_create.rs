//! Handler for TrustCreate command

use std::io;
use std::net::IpAddr;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, TrustReasonError};

use super::duration::parse_duration;
use super::{
    HandlerContext, err_authentication, err_not_logged_in, err_permission_denied,
    err_reason_invalid, err_reason_too_long, err_trust_invalid_duration, err_trust_invalid_target,
};
use crate::db::Permission;
use crate::ip_rule_cache::parse_ip_or_cidr;
use crate::users::UserManager;

/// Handle TrustCreate command
///
/// Creates or updates a trusted IP entry. The target can be:
/// - A nickname of an online user (trusts their specific IP(s))
/// - An IP address (trusts directly)
/// - A CIDR range (trusts the entire range, e.g., "192.168.1.0/24")
pub async fn handle_trust_create<W>(
    target: String,
    duration: Option<String>,
    reason: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("TrustCreate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrustCreate"))
            .await;
    };

    // Validate reason if provided
    if let Some(ref r) = reason
        && let Err(e) = validators::validate_trust_reason(r)
    {
        let error_msg = match e {
            TrustReasonError::TooLong => {
                err_reason_too_long(ctx.locale, validators::MAX_TRUST_REASON_LENGTH)
            }
            TrustReasonError::InvalidCharacters => err_reason_invalid(ctx.locale),
        };
        let response = ServerMessage::TrustCreateResponse {
            success: false,
            error: Some(error_msg),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrustCreate"))
                .await;
        }
    };

    // Check trust_create permission
    if !requesting_user.has_permission(Permission::TrustCreate) {
        eprintln!(
            "TrustCreate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::TrustCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    // Parse duration
    let expires_at = match parse_duration(&duration) {
        Ok(expires) => expires,
        Err(_) => {
            let response = ServerMessage::TrustCreateResponse {
                success: false,
                error: Some(err_trust_invalid_duration(ctx.locale)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Resolve target to IP address(es) or CIDR range
    let (targets_to_trust, nickname_annotation, _is_cidr) =
        match resolve_target(&target, ctx.user_manager).await {
            Ok(result) => result,
            Err(TargetResolutionError::InvalidTarget) => {
                let response = ServerMessage::TrustCreateResponse {
                    success: false,
                    error: Some(err_trust_invalid_target(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        };

    // Create the trusts in database
    let mut trusted_targets = Vec::new();
    for target_str in &targets_to_trust {
        match ctx
            .db
            .trusts
            .create_or_update_trust(
                target_str,
                nickname_annotation.as_deref(),
                reason.as_deref(),
                &requesting_user.username,
                expires_at,
            )
            .await
        {
            Ok(_) => {
                trusted_targets.push(target_str.clone());
            }
            Err(e) => {
                eprintln!("TrustCreate database error for {}: {}", target_str, e);
                let response = ServerMessage::TrustCreateResponse {
                    success: false,
                    error: Some(super::err_database(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    // Update the IP rule cache
    {
        let mut cache = ctx
            .ip_rule_cache
            .write()
            .expect("ip rule cache lock poisoned");
        for target_str in &trusted_targets {
            cache.add_trust(target_str, expires_at);
        }
    }

    // Send success response
    let response = ServerMessage::TrustCreateResponse {
        success: true,
        error: None,
        ips: Some(trusted_targets),
        nickname: nickname_annotation,
    };
    ctx.send_message(&response).await
}

/// Error type for target resolution
enum TargetResolutionError {
    InvalidTarget,
}

/// Resolve a target string to IP addresses
///
/// Returns (IPs to trust, optional nickname annotation, is_cidr)
async fn resolve_target(
    target: &str,
    user_manager: &UserManager,
) -> Result<(Vec<String>, Option<String>, bool), TargetResolutionError> {
    // First, check if target is an online user's nickname
    let ips = user_manager.get_ips_for_nickname(target).await;
    if !ips.is_empty() {
        return Ok((ips, Some(target.to_string()), false));
    }

    // Try to parse as CIDR range
    if let Some(net) = parse_ip_or_cidr(target) {
        let is_cidr = match net {
            ipnet::IpNet::V4(v4) => v4.prefix_len() < 32,
            ipnet::IpNet::V6(v6) => v6.prefix_len() < 128,
        };
        return Ok((vec![target.to_string()], None, is_cidr));
    }

    // Try to parse as single IP address
    if target.parse::<IpAddr>().is_ok() {
        return Ok((vec![target.to_string()], None, false));
    }

    Err(TargetResolutionError::InvalidTarget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_trustcreate_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_trust_create(
            "192.168.1.100".to_string(),
            None,
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "TrustCreate should require login");
    }

    #[tokio::test]
    async fn test_trustcreate_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without trust_create permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_trust_create(
            "192.168.1.100".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustcreate_admin_can_trust() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_create(
            "192.168.1.100".to_string(),
            None,
            Some("office network".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustCreateResponse {
            success,
            ips,
            error,
            ..
        } = response
        {
            assert!(success, "Expected success, got error: {:?}", error);
            assert_eq!(ips, Some(vec!["192.168.1.100".to_string()]));
        } else {
            panic!("Expected TrustCreateResponse, got: {:?}", response);
        }

        // Verify trust is in DB
        assert!(
            test_ctx
                .db
                .trusts
                .trust_exists("192.168.1.100")
                .await
                .unwrap()
        );

        // Verify trust is in cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_trustcreate_invalid_target() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_create(
            "not-a-valid-ip".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected TrustCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_trustcreate_cidr_range() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_create(
            "192.168.1.0/24".to_string(),
            None,
            Some("office subnet".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustCreateResponse {
            success,
            ips,
            error,
            ..
        } = response
        {
            assert!(success, "Expected success, got error: {:?}", error);
            assert_eq!(ips, Some(vec!["192.168.1.0/24".to_string()]));
        } else {
            panic!("Expected TrustCreateResponse, got: {:?}", response);
        }

        // Verify any IP in the range is trusted
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
            assert!(cache.is_trusted("192.168.1.1".parse().unwrap()));
            assert!(!cache.is_trusted("192.168.2.1".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_trustcreate_with_duration() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_create(
            "192.168.1.100".to_string(),
            Some("1h".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustCreateResponse { success, error, .. } = response {
            assert!(success, "Expected success, got error: {:?}", error);
        } else {
            panic!("Expected TrustCreateResponse, got: {:?}", response);
        }

        // Verify trust has expiry set
        let trust = test_ctx
            .db
            .trusts
            .get_trust_by_ip("192.168.1.100")
            .await
            .unwrap()
            .unwrap();
        assert!(trust.expires_at.is_some());
    }

    #[test]
    fn test_parse_duration() {
        // Permanent (no duration)
        assert_eq!(parse_duration(&None), Ok(None));
        assert_eq!(parse_duration(&Some("".to_string())), Ok(None));
        assert_eq!(parse_duration(&Some("0".to_string())), Ok(None));

        // Valid durations
        assert!(parse_duration(&Some("10m".to_string())).is_ok());
        assert!(parse_duration(&Some("1h".to_string())).is_ok());
        assert!(parse_duration(&Some("7d".to_string())).is_ok());

        // Invalid durations
        assert!(parse_duration(&Some("invalid".to_string())).is_err());
        assert!(parse_duration(&Some("10x".to_string())).is_err());
        assert!(parse_duration(&Some("m".to_string())).is_err());
    }
}
