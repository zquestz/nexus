//! Handler for BanCreate command

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, BanReasonError, DurationError, TargetError};

use super::duration::{format_duration_remaining, parse_duration};
use super::{
    HandlerContext, cleanup_voice_for_ip, cleanup_voice_for_range, err_ban_admin_by_ip,
    err_ban_admin_by_nickname, err_ban_invalid_duration, err_ban_invalid_target, err_ban_self,
    err_database, err_not_logged_in, err_permission_denied, err_reason_invalid,
    err_reason_too_long, err_target_too_long,
};
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::{canonicalize_target, parse_ip_or_cidr};

/// Creates or updates an IP ban. Target is an online nickname (bans its
/// IP(s)), a bare IP, or a CIDR range.
pub async fn handle_ban_create<W>(
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
        warn!(ip = %ctx.peer_addr, "{}", LOG_BAN_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_BAN_CREATE))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_BAN_CREATE))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::BanCreate) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_BAN_CREATE_PERMISSION_DENIED);
        let response = ServerMessage::BanCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_target(&target) {
        let error_msg = match e {
            TargetError::Empty => err_ban_invalid_target(ctx.locale),
            TargetError::TooLong => err_target_too_long(ctx.locale, validators::MAX_TARGET_LENGTH),
        };
        let response = ServerMessage::BanCreateResponse {
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
        let response = ServerMessage::BanCreateResponse {
            success: false,
            error: Some(err_ban_invalid_duration(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Some(ref r) = reason
        && let Err(e) = validators::validate_ban_reason(r)
    {
        let error_msg = match e {
            BanReasonError::TooLong => {
                err_reason_too_long(ctx.locale, validators::MAX_BAN_REASON_LENGTH)
            }
            BanReasonError::InvalidCharacters => err_reason_invalid(ctx.locale),
        };
        let response = ServerMessage::BanCreateResponse {
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
            let response = ServerMessage::BanCreateResponse {
                success: false,
                error: Some(err_ban_invalid_duration(ctx.locale)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let (targets_to_ban, nickname_annotation, is_range) = match resolve_target(
        &target,
        &requesting_user.username,
        ctx,
    )
    .await
    {
        Ok(result) => result,
        Err(TargetResolutionError::InvalidTarget) => {
            let response = ServerMessage::BanCreateResponse {
                success: false,
                error: Some(err_ban_invalid_target(ctx.locale)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TargetResolutionError::IsAdmin) => {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_BAN_CREATE_ADMIN_NICKNAME);
            let response = ServerMessage::BanCreateResponse {
                success: false,
                error: Some(err_ban_admin_by_nickname(ctx.locale)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TargetResolutionError::IsSelf) => {
            let response = ServerMessage::BanCreateResponse {
                success: false,
                error: Some(err_ban_self(ctx.locale)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Refuse any ban (by nickname, IP, or CIDR) that would cover a connected admin.
    if is_range {
        if let Some(net) = parse_ip_or_cidr(&targets_to_ban[0])
            && ctx.user_manager.is_admin_connected_in_range(&net).await
        {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %targets_to_ban[0], "{}", LOG_BAN_CREATE_ADMIN_CIDR);
            let response = ServerMessage::BanCreateResponse {
                success: false,
                error: Some(err_ban_admin_by_ip(ctx.locale)),
                ips: None,
                nickname: None,
            };
            return ctx.send_message(&response).await;
        }
    } else {
        for target_ip in &targets_to_ban {
            if ctx.user_manager.is_admin_connected_from_ip(target_ip).await {
                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target_ip, "{}", LOG_BAN_CREATE_ADMIN_IP);
                let response = ServerMessage::BanCreateResponse {
                    success: false,
                    error: Some(err_ban_admin_by_ip(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    // Always check (even banning by nickname): the target may share our IP.
    let our_ip = ctx.peer_addr.ip();
    let would_ban_self = if is_range {
        parse_ip_or_cidr(&targets_to_ban[0])
            .map(|net| net.contains(&our_ip))
            .unwrap_or(false)
    } else {
        targets_to_ban.contains(&our_ip.to_string())
    };

    if would_ban_self {
        let response = ServerMessage::BanCreateResponse {
            success: false,
            error: Some(err_ban_self(ctx.locale)),
            ips: None,
            nickname: None,
        };
        return ctx.send_message(&response).await;
    }

    let mut banned_targets = Vec::new();
    for target_str in &targets_to_ban {
        match ctx
            .db
            .bans
            .create_or_update_ban(
                target_str,
                nickname_annotation.as_deref(),
                reason.as_deref(),
                &requesting_user.username,
                expires_at,
            )
            .await
        {
            Ok(_) => {
                banned_targets.push(target_str.clone());
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target_str, err = %e, "{}", LOG_BAN_CREATE_DB_ERROR);
                let response = ServerMessage::BanCreateResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                    ips: None,
                    nickname: None,
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    {
        let mut cache = ctx.ip_rule_cache.write().expect(ERR_IP_CACHE_POISONED);
        for target_str in &banned_targets {
            cache.add_ban(target_str, expires_at);
        }
    }

    // Tear down voice (notifying other participants) before disconnecting; trusted IPs skipped.
    if is_range {
        if let Some(net) = parse_ip_or_cidr(&banned_targets[0]) {
            cleanup_voice_for_range(
                ctx.user_manager,
                ctx.voice_registry,
                ctx.channel_manager,
                &net,
                |ip| {
                    ctx.ip_rule_cache
                        .read()
                        .expect(ERR_IP_CACHE_POISONED)
                        .is_trusted_read_only(*ip)
                },
            )
            .await;
        }
    } else {
        for ip in &banned_targets {
            cleanup_voice_for_ip(
                ctx.user_manager,
                ctx.voice_registry,
                ctx.channel_manager,
                ip,
                |ip| {
                    ctx.ip_rule_cache
                        .read()
                        .expect(ERR_IP_CACHE_POISONED)
                        .is_trusted_read_only(*ip)
                },
            )
            .await;
        }
    }

    // Disconnect banned sessions + transfers. Each banned session gets its reason
    // while still live, then `remove_users_and_broadcast` removes them and emits
    // UserDisconnected (+ re-aggregated UserUpdated for any surviving account
    // session). Trusted IPs are left connected: trust bypasses ban checks, so
    // disconnecting them would just churn.
    if is_range {
        if let Some(net) = parse_ip_or_cidr(&banned_targets[0]) {
            let targets = ctx
                .user_manager
                .sessions_in_range(&net, |ip| {
                    ctx.ip_rule_cache
                        .read()
                        .expect(ERR_IP_CACHE_POISONED)
                        .is_trusted_read_only(*ip)
                })
                .await;

            for (session_id, locale) in &targets {
                ctx.user_manager
                    .send_to_session(
                        *session_id,
                        build_ban_disconnect_message(locale, expires_at),
                    )
                    .await;
            }
            let session_ids: Vec<u32> = targets.into_iter().map(|(id, _)| id).collect();
            ctx.user_manager
                .remove_users_and_broadcast(&session_ids)
                .await;

            ctx.transfer_registry.disconnect_matching(|ip| {
                net.contains(&ip)
                    && !ctx
                        .ip_rule_cache
                        .read()
                        .expect(ERR_IP_CACHE_POISONED)
                        .is_trusted_read_only(ip)
            });
        }
    } else {
        // Banning a nickname can span several IPs of one account; gather all of
        // them so the account re-aggregates once (final state), not per IP.
        let mut session_ids = Vec::new();
        for ip in &banned_targets {
            let targets = ctx
                .user_manager
                .sessions_by_ip(ip, |ip| {
                    ctx.ip_rule_cache
                        .read()
                        .expect(ERR_IP_CACHE_POISONED)
                        .is_trusted_read_only(*ip)
                })
                .await;

            for (session_id, locale) in &targets {
                ctx.user_manager
                    .send_to_session(
                        *session_id,
                        build_ban_disconnect_message(locale, expires_at),
                    )
                    .await;
            }
            session_ids.extend(targets.into_iter().map(|(id, _)| id));
        }
        ctx.user_manager
            .remove_users_and_broadcast(&session_ids)
            .await;

        ctx.transfer_registry.disconnect_matching(|ip| {
            let ip_str = ip.to_string();
            banned_targets.contains(&ip_str)
                && !ctx
                    .ip_rule_cache
                    .read()
                    .expect(ERR_IP_CACHE_POISONED)
                    .is_trusted_read_only(ip)
        });
    }

    info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_BAN_CREATE_SUCCESS);
    let response = ServerMessage::BanCreateResponse {
        success: true,
        error: None,
        ips: Some(banned_targets),
        nickname: nickname_annotation,
    };
    ctx.send_message(&response).await
}

enum TargetResolutionError {
    InvalidTarget,
    IsAdmin,
    IsSelf,
}

/// Resolve a target to (IPs, nickname annotation, is_range). IP/CIDR targets are
/// canonicalized (lowercase, host-bits zeroed) so stored/cached/response strings
/// match regardless of admin-typed casing.
async fn resolve_target<W>(
    target: &str,
    requesting_username: &str,
    ctx: &HandlerContext<'_, W>,
) -> Result<(Vec<String>, Option<String>, bool), TargetResolutionError>
where
    W: AsyncWrite + Unpin,
{
    if let Some(session) = ctx.user_manager.get_session_by_nickname(target).await {
        if session.is_admin {
            return Err(TargetResolutionError::IsAdmin);
        }

        if fold_name(&session.username) == fold_name(requesting_username) {
            return Err(TargetResolutionError::IsSelf);
        }

        // A nickname may have multiple sessions, hence multiple IPs.
        let ips = ctx.user_manager.get_ips_for_nickname(target).await;

        return Ok((ips, Some(session.nickname.clone()), false));
    }

    // `canonicalize_target` hands back the precomputed `is_range` flag.
    if let Some((canonical, _net, is_range)) = canonicalize_target(target) {
        return Ok((vec![canonical], None, is_range));
    }

    Err(TargetResolutionError::InvalidTarget)
}

fn build_ban_disconnect_message(locale: &str, expires_at: Option<i64>) -> ServerMessage {
    use super::err_banned_permanent;
    use super::err_banned_with_expiry;

    let message = if let Some(expiry) = expires_at {
        let remaining = format_duration_remaining(expiry);
        err_banned_with_expiry(locale, &remaining)
    } else {
        err_banned_permanent(locale)
    };

    ServerMessage::Error {
        message,
        command: Some("BanCreate".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    // Note: parse_duration and format_duration_remaining are tested in duration.rs

    #[tokio::test]
    async fn test_bancreate_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            None,
            None, // No session
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "BanCreate should require login");
    }

    #[tokio::test]
    async fn test_bancreate_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_admin_can_ban_ip() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            Some("test reason".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse {
            success,
            ips,
            nickname,
            ..
        } = response
        {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 1);
            assert_eq!(ips[0], "192.168.1.100");
            assert!(nickname.is_none()); // No nickname when banning by IP
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(
            test_ctx
                .db
                .bans
                .is_ip_banned("192.168.1.100")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_bancreate_with_duration() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            Some("1h".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, .. } = response {
            assert!(success);
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        let ban = test_ctx
            .db
            .bans
            .get_ban_by_ip("192.168.1.100")
            .await
            .unwrap()
            .expect("Ban should exist");
        assert!(ban.expires_at.is_some());
    }

    #[tokio::test]
    async fn test_bancreate_invalid_duration() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            Some("invalid".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .bans
                .is_ip_banned("192.168.1.100")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_bancreate_invalid_target() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "nonexistent_user".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_cannot_ban_self_by_ip() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // 127.0.0.1 is the test context peer_addr — banning it is self-ban.
        let result = handle_ban_create(
            "127.0.0.1".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        // Verify no ban was created
        assert!(!test_ctx.db.bans.is_ip_banned("127.0.0.1").await.unwrap());
    }

    #[tokio::test]
    async fn test_bancreate_reason_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let long_reason = "x".repeat(validators::MAX_BAN_REASON_LENGTH + 1);

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            Some(long_reason),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .bans
                .is_ip_banned("192.168.1.100")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_bancreate_reason_invalid_characters() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let invalid_reason = "reason\x00with null".to_string();

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            Some(invalid_reason),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_upsert_existing_ban() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        test_ctx
            .db
            .bans
            .create_or_update_ban(
                "192.168.1.100",
                None,
                Some("old reason"),
                "other_admin",
                None,
            )
            .await
            .unwrap();

        // Re-ban the same IP: should upsert, not insert a second row.
        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            Some("1h".to_string()),
            Some("new reason".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, .. } = response {
            assert!(success);
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        let ban = test_ctx
            .db
            .bans
            .get_ban_by_ip("192.168.1.100")
            .await
            .unwrap()
            .expect("Ban should exist");
        assert_eq!(ban.reason, Some("new reason".to_string()));
        assert_eq!(ban.created_by, "admin");
        assert!(ban.expires_at.is_some());
    }

    #[tokio::test]
    async fn test_bancreate_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Non-admin with ban_create permission.
        let session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[crate::db::Permission::BanCreate],
            false,
        )
        .await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, .. } = response {
            assert!(success);
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_ipv6_address() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "2001:db8::1".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 1);
            assert_eq!(ips[0], "2001:db8::1");
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(test_ctx.db.bans.is_ip_banned("2001:db8::1").await.unwrap());
    }

    #[tokio::test]
    async fn test_bancreate_ipv6_cidr() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "2001:db8::/32".to_string(),
            Some("1h".to_string()),
            Some("IPv6 range ban".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 1);
            assert_eq!(ips[0], "2001:db8::/32");
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(test_ctx.db.bans.ban_exists("2001:db8::/32").await.unwrap());

        // Cache must block every IP in the banned range, but nothing outside it.
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(cache.is_banned("2001:db8::1".parse().unwrap()));
            assert!(cache.is_banned("2001:db8:1234::5678".parse().unwrap()));
            assert!(!cache.is_banned("2001:db9::1".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_bancreate_cidr_skips_trusted_ips() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user from an IP that will be trusted
        let _alice_session = login_user_from_ip(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            "192.168.1.100",
        )
        .await;

        // Create a user from an IP that will NOT be trusted
        let bob_session = login_user_from_ip(
            &mut test_ctx,
            "bob",
            "password",
            &[],
            false,
            "192.168.1.200",
        )
        .await;

        // Trust alice's IP before banning the range
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("192.168.1.100", None);
        }
        test_ctx
            .db
            .trusts
            .create_or_update_trust("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .unwrap();

        let result = handle_ban_create(
            "192.168.1.0/24".to_string(),
            None,
            Some("Range ban".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Alice (trusted) should still be connected
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(_alice_session)
                .await
                .is_some(),
            "Alice should still be connected (trusted IP)"
        );

        // Bob (not trusted) should have been disconnected
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_none(),
            "Bob should have been disconnected (not trusted)"
        );

        // Both IPs are banned in cache, but the trusted one is still allowed through.
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
            assert!(cache.is_banned("192.168.1.200".parse().unwrap()));
            assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
            assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
            assert!(!cache.should_allow("192.168.1.200".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_bancreate_single_ip_skips_if_trusted() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user from an IP that will be trusted
        let _alice_session =
            login_user_from_ip(&mut test_ctx, "alice", "password", &[], false, "10.0.0.50").await;

        // Trust alice's IP before banning it
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            cache.add_trust("10.0.0.50", None);
        }
        test_ctx
            .db
            .trusts
            .create_or_update_trust("10.0.0.50", Some("alice"), None, "admin", None)
            .await
            .unwrap();

        let result = handle_ban_create(
            "10.0.0.50".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Alice should still be connected because her IP is trusted
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(_alice_session)
                .await
                .is_some(),
            "Alice should still be connected (trusted IP)"
        );
    }

    #[tokio::test]
    async fn test_bancreate_by_nickname() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        // Admin on 127.0.0.1 (test peer_addr); target on a distinct IP so it isn't self.
        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let _target_session_id = login_user_from_ip(
            &mut test_ctx,
            "target",
            "password",
            &[],
            false,
            "192.168.1.50",
        )
        .await;

        let result = handle_ban_create(
            "target".to_string(),
            None,
            Some("spamming".to_string()),
            Some(admin_session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse {
            success,
            ips,
            nickname,
            ..
        } = response
        {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 1);
            assert_eq!(ips[0], "192.168.1.50");
            assert_eq!(nickname, Some("target".to_string()));
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        let ban = test_ctx
            .db
            .bans
            .get_ban_by_ip("192.168.1.50")
            .await
            .unwrap()
            .expect("Ban should exist");
        assert_eq!(ban.nickname, Some("target".to_string()));
        assert_eq!(ban.reason, Some("spamming".to_string()));
    }

    #[tokio::test]
    async fn test_bancreate_cannot_ban_admin_by_nickname() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        // Moderator on 127.0.0.1; admin on a distinct IP so the ban targets the admin, not self.
        let mod_session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[crate::db::Permission::BanCreate],
            false,
        )
        .await;

        let _admin_session_id = login_user_from_ip(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            "192.168.1.100",
        )
        .await;

        let result = handle_ban_create(
            "admin".to_string(),
            None,
            None,
            Some(mod_session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .bans
                .is_ip_banned("192.168.1.100")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_bancreate_cannot_ban_ip_with_admin_connected() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        // Moderator on 127.0.0.1; admin connected from 192.168.1.100.
        let mod_session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[crate::db::Permission::BanCreate],
            false,
        )
        .await;

        let _admin_session_id = login_user_from_ip(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            "192.168.1.100",
        )
        .await;

        // Banning that IP must fail because an admin is connected from it.
        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            None,
            None,
            Some(mod_session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .bans
                .is_ip_banned("192.168.1.100")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_bancreate_cannot_ban_self_by_nickname() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin bans own nickname — must be rejected as self-ban.
        let result = handle_ban_create(
            "admin".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        // Verify no ban was created
        assert!(!test_ctx.db.bans.is_ip_banned("127.0.0.1").await.unwrap());
    }

    #[tokio::test]
    async fn test_bancreate_user_not_online() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Nickname that's neither online nor a valid IP → invalid target.
        let result = handle_ban_create(
            "offline_user".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_target_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let long_target = "x".repeat(validators::MAX_TARGET_LENGTH + 1);

        let result = handle_ban_create(
            long_target,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_target_empty() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_duration_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let long_duration = "x".repeat(validators::MAX_DURATION_LENGTH + 1);

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            Some(long_duration),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        assert!(
            !test_ctx
                .db
                .bans
                .is_ip_banned("192.168.1.100")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_bancreate_disconnects_active_transfers() {
        use crate::transfers::registry::{TransferDirection, TransferRegistration};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let mut test_ctx = create_test_context().await;

        // Active transfer on the IP we'll ban.
        let banned_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let banned_addr = SocketAddr::new(banned_ip, 12345);
        let (info, mut ban_rx) = test_ctx.transfer_registry.register(TransferRegistration {
            peer_addr: banned_addr,
            nickname: "banned_user".to_string(),
            username: "banned_user".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/test.zip".to_string(),
            total_size: 0,
        });

        // Transfer on a different IP — must be untouched.
        let safe_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 200));
        let safe_addr = SocketAddr::new(safe_ip, 12346);
        let (_safe_info, mut safe_rx) = test_ctx.transfer_registry.register(TransferRegistration {
            peer_addr: safe_addr,
            nickname: "safe_user".to_string(),
            username: "safe_user".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/other.zip".to_string(),
            total_size: 0,
        });

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            Some("1h".to_string()),
            Some("test ban".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        assert!(
            ban_rx.try_recv().is_ok(),
            "Banned transfer should receive ban signal"
        );

        let safe_result = safe_rx.try_recv();
        assert!(
            safe_result.is_err(),
            "Safe transfer should not receive ban signal"
        );

        test_ctx.transfer_registry.unregister(info.id);
    }

    #[tokio::test]
    async fn test_bancreate_cidr_disconnects_transfers_in_range() {
        use crate::transfers::registry::{TransferDirection, TransferRegistration};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let mut test_ctx = create_test_context().await;

        // Two transfers inside the /24 we'll ban, one outside it.
        let ip_in_range_1 = IpAddr::V4(Ipv4Addr::new(10, 0, 1, 50));
        let ip_in_range_2 = IpAddr::V4(Ipv4Addr::new(10, 0, 1, 200));
        let ip_outside_range = IpAddr::V4(Ipv4Addr::new(10, 0, 2, 50));

        let (_info1, mut rx1) = test_ctx.transfer_registry.register(TransferRegistration {
            peer_addr: SocketAddr::new(ip_in_range_1, 12345),
            nickname: "user1".to_string(),
            username: "user1".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/test1.zip".to_string(),
            total_size: 0,
        });
        let (_info2, mut rx2) = test_ctx.transfer_registry.register(TransferRegistration {
            peer_addr: SocketAddr::new(ip_in_range_2, 12346),
            nickname: "user2".to_string(),
            username: "user2".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Upload,
            path: "/uploads".to_string(),
            total_size: 0,
        });
        let (_info3, mut rx3) = test_ctx.transfer_registry.register(TransferRegistration {
            peer_addr: SocketAddr::new(ip_outside_range, 12347),
            nickname: "user3".to_string(),
            username: "user3".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/test3.zip".to_string(),
            total_size: 0,
        });

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_ban_create(
            "10.0.1.0/24".to_string(),
            None,
            Some("CIDR ban test".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        assert!(
            rx1.try_recv().is_ok(),
            "Transfer in range should receive ban signal"
        );
        assert!(
            rx2.try_recv().is_ok(),
            "Transfer in range should receive ban signal"
        );

        assert!(
            rx3.try_recv().is_err(),
            "Transfer outside range should not receive ban signal"
        );
    }

    #[tokio::test]
    async fn test_bancreate_cidr_host_bits_canonicalized() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin types a CIDR with host bits set; expect it to be stored as the
        // zeroed network address so two admins can't end up with duplicate rows.
        let result = handle_ban_create(
            "192.168.1.5/24".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips, vec!["192.168.1.0/24".to_string()]);
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        // Re-banning the canonical form upserts the same row, not a new one
        let result = handle_ban_create(
            "192.168.1.0/24".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let _ = read_server_message(&mut test_ctx).await;

        let bans = test_ctx.db.bans.list_active_bans().await.unwrap();
        assert_eq!(bans.len(), 1);
        assert_eq!(bans[0].ip_address, "192.168.1.0/24");
    }

    #[tokio::test]
    async fn test_bancreate_ipv6_uppercase_canonicalized() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin types an IPv6 address using uppercase hex
        let result = handle_ban_create(
            "2001:DB8::1".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::BanCreateResponse { success, ips, .. } = response {
            assert!(success);
            let ips = ips.unwrap();
            assert_eq!(ips.len(), 1);
            // Response echoes the canonical form, not what the admin typed
            assert_eq!(ips[0], "2001:db8::1");
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }

        // Ban is stored under canonical form
        assert!(test_ctx.db.bans.is_ip_banned("2001:db8::1").await.unwrap());

        // Re-banning with the lowercase form upserts the same row, not a new one
        let result = handle_ban_create(
            "2001:db8::1".to_string(),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let _ = read_server_message(&mut test_ctx).await;

        let bans = test_ctx.db.bans.list_active_bans().await.unwrap();
        assert_eq!(bans.len(), 1);
    }
}
