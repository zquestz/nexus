//! Creates or updates a trusted IP entry.

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;

use crate::constants::*;
use nexus_common::validators::{self, DurationError, TargetError, TrustReasonError};

use super::duration::parse_duration;
use super::{
    HandlerContext, err_not_logged_in, err_permission_denied, err_reason_invalid,
    err_reason_too_long, err_target_too_long, err_trust_invalid_duration, err_trust_invalid_target,
};
use crate::db::Permission;
use crate::ip_rule_cache::canonicalize_target;
use crate::users::UserManager;

/// Creates or updates a trusted IP entry. The target is an online user's
/// nickname (trusts their IPs), an IP address, or a CIDR range.
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
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRUST_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRUST_CREATE))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRUST_CREATE),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrustCreate) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_TRUST_CREATE_PERMISSION_DENIED);
        let response = ServerMessage::TrustCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_target(&target) {
        let error_msg = match e {
            TargetError::Empty => err_trust_invalid_target(ctx.locale),
            TargetError::TooLong => err_target_too_long(ctx.locale, validators::MAX_TARGET_LENGTH),
        };
        let response = ServerMessage::TrustCreateResponse {
            success: false,
            error: Some(error_msg),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Some(ref d) = duration
        && let Err(DurationError::TooLong) = validators::validate_duration(d)
    {
        let response = ServerMessage::TrustCreateResponse {
            success: false,
            error: Some(err_trust_invalid_duration(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

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

    // Third tuple field (is_range) is discarded — the trust path doesn't branch
    // on it the way ban_create does.
    let (targets_to_trust, nickname_annotation, _) =
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
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target_str, err = %e, "{}", LOG_TRUST_CREATE_DB_ERROR);
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

    {
        let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
        for target_str in &trusted_targets {
            cache.add_trust(target_str, expires_at);
        }
    }

    info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_TRUST_CREATE_SUCCESS);
    let response = ServerMessage::TrustCreateResponse {
        success: true,
        error: None,
        ips: Some(trusted_targets),
        nickname: nickname_annotation,
    };
    ctx.send_message(&response).await
}

enum TargetResolutionError {
    InvalidTarget,
}

/// Returns (IPs to trust, optional nickname annotation, is_range).
/// IP/CIDR targets are returned canonical-lowercase so `2001:DB8::1` and
/// `2001:db8::1` can't create separate rows for the same host. Nickname
/// targets annotate with the session's canonical nickname (not the
/// admin-typed form), matching `ban_create::resolve_target`.
async fn resolve_target(
    target: &str,
    user_manager: &UserManager,
) -> Result<(Vec<String>, Option<String>, bool), TargetResolutionError> {
    if let Some(session) = user_manager.get_session_by_nickname(target).await {
        let ips = user_manager.get_ips_for_nickname(target).await;
        return Ok((ips, Some(session.nickname.clone()), false));
    }

    // `canonicalize_target` returns the precomputed is_range flag.
    if let Some((canonical, _net, is_range)) = canonicalize_target(target) {
        return Ok((vec![canonical], None, is_range));
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

        // Non-admin user without trust_create permission.
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

    // Note: parse_duration is tested in duration.rs

    #[tokio::test]
    async fn test_trustcreate_target_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a target that's too long
        let long_target = "x".repeat(validators::MAX_TARGET_LENGTH + 1);

        let result = handle_trust_create(
            long_target,
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
    async fn test_trustcreate_target_empty() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_create(
            "".to_string(),
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
    async fn test_trustcreate_duration_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a duration that's too long
        let long_duration = "x".repeat(validators::MAX_DURATION_LENGTH + 1);

        let result = handle_trust_create(
            "192.168.1.100".to_string(),
            Some(long_duration),
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

        // Verify no trust was created
        assert!(
            test_ctx
                .db
                .trusts
                .get_trust_by_ip("192.168.1.100")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_trustcreate_ipv6_uppercase_canonicalized() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin types an IPv6 address using uppercase hex
        let result = handle_trust_create(
            "2001:DB8::1".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::TrustCreateResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 1);
            // Response echoes the canonical form, not what the admin typed
            assert_eq!(ips[0], "2001:db8::1");
        } else {
            panic!("Expected TrustCreateResponse, got: {:?}", response);
        }

        // Trust is stored under canonical form
        assert!(
            test_ctx
                .db
                .trusts
                .is_ip_trusted("2001:db8::1")
                .await
                .unwrap()
        );

        // Re-trusting with the lowercase form upserts the same row, not a new one
        let result = handle_trust_create(
            "2001:db8::1".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let _ = read_server_message(&mut test_ctx).await;

        let trusts = test_ctx.db.trusts.list_active_trusts().await.unwrap();
        assert_eq!(trusts.len(), 1);
    }
}
