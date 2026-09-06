//! Creates or updates a trusted IP entry.

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, DurationError, TargetError, TrustReasonError};

use super::duration::parse_duration;
use super::{
    HandlerContext, Outcome, dispatch_outcome, err_not_logged_in, err_permission_denied,
    err_reason_invalid, err_reason_too_long, err_target_too_long, err_trust_invalid_duration,
    err_trust_invalid_target,
};
use crate::constants::*;
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

    // Guard lives only inside this block; all socket sends to the requester happen
    // after it. Trusts only need a read guard: they do not tear down live sessions,
    // but `created_by` and nickname target resolution should observe one stable
    // user-map snapshot.
    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
            Some(user) => user,
            None => break 'locked Outcome::Disconnect,
        };

        if !requesting_user.has_permission(Permission::TrustCreate) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_TRUST_CREATE_PERMISSION_DENIED);
            break 'locked Outcome::Send(Box::new(reject_trust_create(err_permission_denied(
                ctx.locale,
            ))));
        }

        let reason = reason.filter(|r| !r.trim().is_empty());

        let validated = match validate_trust_create_inputs(&target, &duration, &reason, ctx.locale)
        {
            Ok(validated) => validated,
            Err(error) => break 'locked Outcome::Send(Box::new(reject_trust_create(error))),
        };
        let expires_at = validated.expires_at;

        let resolved_target = match resolve_target(&target, ctx.user_manager).await {
            Ok(result) => result,
            Err(TargetResolutionError::InvalidTarget) => {
                break 'locked Outcome::Send(Box::new(reject_trust_create(
                    err_trust_invalid_target(ctx.locale),
                )));
            }
        };
        let targets_to_trust = resolved_target.targets;
        let nickname_annotation = resolved_target.nickname;

        let trusted_targets = match create_or_update_trust_rules(
            ctx,
            &targets_to_trust,
            nickname_annotation.as_deref(),
            reason.as_deref(),
            &requesting_user.username,
            expires_at,
        )
        .await
        {
            Ok(targets) => targets,
            Err(response) => break 'locked Outcome::Send(response),
        };

        info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_TRUST_CREATE_SUCCESS);
        Outcome::Send(Box::new(trust_create_success(
            trusted_targets,
            nickname_annotation,
        )))
    };

    dispatch_outcome(outcome, ctx, HANDLER_TRUST_CREATE).await
}

fn reject_trust_create(error: String) -> ServerMessage {
    ServerMessage::TrustCreateResponse {
        success: false,
        error: Some(error),
        ips: None,
        nickname: None,
    }
}

fn trust_create_success(ips: Vec<String>, nickname: Option<String>) -> ServerMessage {
    ServerMessage::TrustCreateResponse {
        success: true,
        error: None,
        ips: Some(ips),
        nickname,
    }
}

struct ValidatedTrustCreate {
    expires_at: Option<i64>,
}

struct ResolvedTrustTarget {
    targets: Vec<String>,
    nickname: Option<String>,
}

fn validate_trust_create_inputs(
    target: &str,
    duration: &Option<String>,
    reason: &Option<String>,
    locale: &str,
) -> Result<ValidatedTrustCreate, String> {
    if let Err(e) = validators::validate_target(target) {
        return Err(match e {
            TargetError::Empty => err_trust_invalid_target(locale),
            TargetError::TooLong => err_target_too_long(locale, validators::MAX_TARGET_LENGTH),
        });
    }

    if let Some(d) = duration
        && let Err(DurationError::TooLong) = validators::validate_duration(d)
    {
        return Err(err_trust_invalid_duration(locale));
    }

    if let Some(r) = reason
        && let Err(e) = validators::validate_trust_reason(r)
    {
        return Err(match e {
            TrustReasonError::TooLong => {
                err_reason_too_long(locale, validators::MAX_TRUST_REASON_LENGTH)
            }
            TrustReasonError::InvalidCharacters => err_reason_invalid(locale),
        });
    }

    let expires_at = parse_duration(duration).map_err(|_| err_trust_invalid_duration(locale))?;

    Ok(ValidatedTrustCreate { expires_at })
}

async fn create_or_update_trust_rules<W>(
    ctx: &HandlerContext<'_, W>,
    targets_to_trust: &[String],
    nickname_annotation: Option<&str>,
    reason: Option<&str>,
    requested_by: &str,
    expires_at: Option<i64>,
) -> Result<Vec<String>, Box<ServerMessage>>
where
    W: AsyncWrite + Unpin,
{
    let _ip_rule_mutation = ctx.ip_rule_cache.lock_mutation().await;

    let trusted_targets: Vec<String> = ctx
        .db
        .trusts
        .create_or_update_trust_targets(
            targets_to_trust.iter().map(String::as_str),
            nickname_annotation,
            reason,
            requested_by,
            expires_at,
        )
        .await
        .map_err(|e| {
            error!(user = %requested_by, ip = %ctx.peer_addr, targets = ?targets_to_trust, err = %e, "{}", LOG_TRUST_CREATE_DB_ERROR);
            Box::new(reject_trust_create(super::err_database(ctx.locale)))
        })?
        .into_iter()
        .map(|trust| trust.ip_address)
        .collect();

    {
        let mut cache = ctx.ip_rule_cache.write();
        for target in &trusted_targets {
            cache.add_trust(target, expires_at);
        }
    }

    Ok(trusted_targets)
}

enum TargetResolutionError {
    InvalidTarget,
}

/// Resolve a target to IP/CIDR strings plus an optional nickname annotation.
/// IP/CIDR targets are canonicalized so stored/cached/response strings match
/// regardless of admin-typed casing.
async fn resolve_target(
    target: &str,
    user_manager: &UserManager,
) -> Result<ResolvedTrustTarget, TargetResolutionError> {
    // Caller holds the user-state read guard so the nickname cannot change while
    // this snapshot is captured. The trust itself is IP-keyed and rename-immune
    // once the IPs are captured.
    if let Some(snapshot) = user_manager.get_nickname_ip_snapshot(target).await {
        return Ok(ResolvedTrustTarget {
            targets: snapshot.ips,
            nickname: Some(snapshot.representative.nickname),
        });
    }

    if let Some((canonical, _net, _is_range)) = canonicalize_target(target) {
        return Ok(ResolvedTrustTarget {
            targets: vec![canonical],
            nickname: None,
        });
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
            let mut cache = test_ctx.ip_rule_cache.write();
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
            let mut cache = test_ctx.ip_rule_cache.write();
            assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
            assert!(cache.is_trusted("192.168.1.1".parse().unwrap()));
            assert!(!cache.is_trusted("192.168.2.1".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_trustcreate_whitespace_reason_stores_none() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_trust_create(
            "192.168.1.100".to_string(),
            None,
            Some("   ".to_string()),
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

        let trust = test_ctx
            .db
            .trusts
            .get_trust_by_ip("192.168.1.100")
            .await
            .unwrap()
            .expect("Trust should exist");
        assert!(trust.reason.is_none());
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
