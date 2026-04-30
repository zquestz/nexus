//! Handler for BanCreate command

use std::io;
use std::net::IpAddr;

use ipnet::IpNet;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, BanReasonError, DurationError, TargetError};

use super::duration::{format_duration_remaining, parse_duration};
use super::{
    HandlerContext, cleanup_voice_for_ip, cleanup_voice_for_range, err_authentication,
    err_ban_admin_by_ip, err_ban_admin_by_nickname, err_ban_invalid_duration,
    err_ban_invalid_target, err_ban_self, err_database, err_not_logged_in, err_permission_denied,
    err_reason_invalid, err_reason_too_long, err_target_too_long,
};
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::parse_ip_or_cidr;
use crate::users::UserManager;
use crate::users::manager::DisconnectedSession;

/// Handle BanCreate command
///
/// Creates or updates an IP ban. The target can be:
/// - A nickname of an online user (bans their specific IP(s))
/// - An IP address (bans directly)
/// - A CIDR range (bans the entire range, e.g., "192.168.1.0/24")
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
    // Verify authentication
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_BAN_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("BanCreate"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("BanCreate"))
                .await;
        }
    };

    // Check ban_create permission
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

    // Validate target length
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

    // Validate duration length if provided
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

    // Validate reason if provided
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

    // Parse duration
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

    // Resolve target to IP address(es) or CIDR range
    let (targets_to_ban, nickname_annotation, is_cidr) = match resolve_target(
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

    // Check if any of the IPs/ranges have an admin connected
    // (this applies to all bans - by nickname, IP, or CIDR)
    if is_cidr {
        // For CIDR ranges, check if any admin's IP falls within the range
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
        // For single IPs, check each one
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

    // Check if we'd be banning our own IP (always check, even when banning by nickname,
    // because the target user might share our IP)
    let our_ip = ctx.peer_addr.ip();
    let would_ban_self = if is_cidr {
        // For CIDR, check if our IP falls within the range
        parse_ip_or_cidr(&targets_to_ban[0])
            .map(|net| net.contains(&our_ip))
            .unwrap_or(false)
    } else {
        // For single IPs, check direct match
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

    // Create the bans in database
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

    // Update the IP rule cache
    {
        let mut cache = ctx
            .ip_rule_cache
            .write()
            .expect("ip rule cache lock poisoned");
        for target_str in &banned_targets {
            cache.add_ban(target_str, expires_at);
        }
    }

    // Clean up voice sessions before disconnecting users
    // This ensures users are properly removed from voice and other participants are notified.
    // Note: Trusted IPs are skipped - they won't be disconnected.
    if is_cidr {
        if let Some(net) = parse_ip_or_cidr(&banned_targets[0]) {
            cleanup_voice_for_range(
                ctx.user_manager,
                ctx.voice_registry,
                ctx.channel_manager,
                &net,
                |ip| {
                    ctx.ip_rule_cache
                        .read()
                        .expect("ip rule cache lock poisoned")
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
                        .expect("ip rule cache lock poisoned")
                        .is_trusted_read_only(*ip)
                },
            )
            .await;
        }
    }

    // Disconnect affected sessions and broadcast UserDisconnected to other clients
    // Note: Trusted IPs are skipped - they should remain connected even if banned
    // because trust bypasses ban checks on reconnection.
    if is_cidr {
        // For CIDR ranges, disconnect all sessions whose IP falls within the range
        if let Some(net) = parse_ip_or_cidr(&banned_targets[0]) {
            let disconnected = ctx
                .user_manager
                .disconnect_sessions_in_range(
                    &net,
                    |user_locale| build_ban_disconnect_message(user_locale, expires_at),
                    |ip| {
                        // Skip trusted IPs - they should stay connected
                        ctx.ip_rule_cache
                            .read()
                            .expect("ip rule cache lock poisoned")
                            .is_trusted_read_only(*ip)
                    },
                )
                .await;

            broadcast_disconnections(ctx.user_manager, disconnected).await;

            // Also disconnect active file transfers from IPs in the CIDR range
            ctx.transfer_registry.disconnect_matching(|ip| {
                // Disconnect if IP is in range AND not trusted
                net.contains(&ip)
                    && !ctx
                        .ip_rule_cache
                        .read()
                        .expect("ip rule cache lock poisoned")
                        .is_trusted_read_only(ip)
            });
        }
    } else {
        // For single IPs, disconnect sessions from those specific IPs
        for ip in &banned_targets {
            let disconnected = ctx
                .user_manager
                .disconnect_sessions_by_ip(
                    ip,
                    |user_locale| build_ban_disconnect_message(user_locale, expires_at),
                    |ip| {
                        // Skip trusted IPs - they should stay connected
                        ctx.ip_rule_cache
                            .read()
                            .expect("ip rule cache lock poisoned")
                            .is_trusted_read_only(*ip)
                    },
                )
                .await;

            broadcast_disconnections(ctx.user_manager, disconnected).await;
        }

        // Also disconnect active file transfers from the banned IPs
        ctx.transfer_registry.disconnect_matching(|ip| {
            // Disconnect if IP matches any banned target AND not trusted
            let ip_str = ip.to_string();
            banned_targets.contains(&ip_str)
                && !ctx
                    .ip_rule_cache
                    .read()
                    .expect("ip rule cache lock poisoned")
                    .is_trusted_read_only(ip)
        });
    }

    // Log and send success response
    info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %target, "{}", LOG_BAN_CREATE_SUCCESS);
    let response = ServerMessage::BanCreateResponse {
        success: true,
        error: None,
        ips: Some(banned_targets),
        nickname: nickname_annotation,
    };
    ctx.send_message(&response).await
}

/// Broadcast UserDisconnected for each removed session
async fn broadcast_disconnections(
    user_manager: &UserManager,
    disconnected: Vec<DisconnectedSession>,
) {
    for session in disconnected {
        user_manager
            .broadcast_user_event(
                ServerMessage::UserDisconnected {
                    session_id: session.session_id,
                    nickname: session.nickname,
                },
                Some(session.session_id),
            )
            .await;
    }
}

/// Error types for target resolution
enum TargetResolutionError {
    InvalidTarget,
    IsAdmin,
    IsSelf,
}

/// Resolve a target string to IP address(es) or CIDR range
///
/// Returns (list of targets, optional nickname annotation, is_cidr)
/// - For nicknames: returns list of IPs, nickname, false
/// - For single IP: returns list with one IP, None, false
/// - For CIDR: returns list with the CIDR string, None, true
async fn resolve_target<W>(
    target: &str,
    requesting_username: &str,
    ctx: &HandlerContext<'_, W>,
) -> Result<(Vec<String>, Option<String>, bool), TargetResolutionError>
where
    W: AsyncWrite + Unpin,
{
    // First, check if target is an online nickname
    if let Some(session) = ctx.user_manager.get_session_by_nickname(target).await {
        // Check if target is admin
        if session.is_admin {
            return Err(TargetResolutionError::IsAdmin);
        }

        // Check if target is self (compare usernames, case-insensitive)
        if session.username.to_lowercase() == requesting_username.to_lowercase() {
            return Err(TargetResolutionError::IsSelf);
        }

        // Get all IPs for this nickname (may have multiple sessions)
        let ips = ctx.user_manager.get_ips_for_nickname(target).await;

        return Ok((ips, Some(session.nickname.clone()), false));
    }

    // Try parsing as CIDR range (e.g., "192.168.1.0/24")
    if let Ok(net) = target.parse::<IpNet>() {
        // Check if it's actually a range (prefix length < max)
        let is_range = match net {
            IpNet::V4(v4) => v4.prefix_len() < 32,
            IpNet::V6(v6) => v6.prefix_len() < 128,
        };

        if is_range {
            // Return the CIDR notation (normalized)
            return Ok((vec![net.to_string()], None, true));
        } else {
            // It's a single IP written as /32 or /128, treat as single IP
            return Ok((vec![net.addr().to_string()], None, false));
        }
    }

    // Try parsing as single IP address
    if let Ok(ip) = target.parse::<IpAddr>() {
        return Ok((vec![ip.to_string()], None, false));
    }

    // Target is neither online nickname, CIDR, nor valid IP
    Err(TargetResolutionError::InvalidTarget)
}

/// Build disconnect message for banned user
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

        // Should fail with disconnect
        assert!(result.is_err(), "BanCreate should require login");
    }

    #[tokio::test]
    async fn test_bancreate_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without ban_create permission
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

        // Create admin user
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

        // Verify ban exists in database
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

        // Create admin user
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

        // Verify ban exists with expiry
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

        // Create admin user
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

        // Verify no ban was created
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to ban a non-existent nickname (not an IP, not online)
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // The test context peer_addr is 127.0.0.1, try to ban that
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a reason that's too long
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

        // Verify no ban was created
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a reason with control characters
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create initial ban
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

        // Update the same IP with new info
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

        // Verify ban was updated
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

        // Create non-admin user WITH ban_create permission
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

        // Create admin user
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

        // Verify ban exists
        assert!(test_ctx.db.bans.is_ip_banned("2001:db8::1").await.unwrap());
    }

    #[tokio::test]
    async fn test_bancreate_ipv6_cidr() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
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

        // Verify ban exists in DB
        assert!(test_ctx.db.bans.ban_exists("2001:db8::/32").await.unwrap());

        // Verify ban is in cache and blocks IPs in range
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

        // Create admin user
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

        // Ban the entire /24 range
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

        // Verify ban exists in cache
        {
            let mut cache = test_ctx.ip_rule_cache.write().unwrap();
            assert!(cache.is_banned("192.168.1.100".parse().unwrap()));
            assert!(cache.is_banned("192.168.1.200".parse().unwrap()));
            // But trusted IP should be allowed despite ban
            assert!(cache.is_trusted("192.168.1.100".parse().unwrap()));
            assert!(cache.should_allow("192.168.1.100".parse().unwrap()));
            assert!(!cache.should_allow("192.168.1.200".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn test_bancreate_single_ip_skips_if_trusted() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        // Create admin user
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

        // Ban alice's specific IP
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

    // =========================================================================
    // Ban by nickname tests (require users on different IPs)
    // =========================================================================

    #[tokio::test]
    async fn test_bancreate_by_nickname() {
        use crate::handlers::testing::login_user_from_ip;

        let mut test_ctx = create_test_context().await;

        // Create admin user (on 127.0.0.1, the test context peer_addr)
        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create target user on a different IP
        let _target_session_id = login_user_from_ip(
            &mut test_ctx,
            "target",
            "password",
            &[],
            false,
            "192.168.1.50",
        )
        .await;

        // Ban by nickname
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

        // Verify ban exists in database with nickname annotation
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

        // Create moderator user with ban_create permission (on 127.0.0.1)
        let mod_session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[crate::db::Permission::BanCreate],
            false,
        )
        .await;

        // Create admin user on a different IP
        let _admin_session_id = login_user_from_ip(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            "192.168.1.100",
        )
        .await;

        // Try to ban admin by nickname - should fail
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

        // Verify no ban was created
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

        // Create moderator user with ban_create permission (on 127.0.0.1)
        let mod_session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[crate::db::Permission::BanCreate],
            false,
        )
        .await;

        // Create admin user on 192.168.1.100
        let _admin_session_id = login_user_from_ip(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            "192.168.1.100",
        )
        .await;

        // Try to ban that IP directly - should fail because an admin is connected
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

        // Verify no ban was created
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

        // Create admin user (they will try to ban themselves)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to ban self by nickname - should fail
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to ban a nickname that's not online and isn't a valid IP
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
            assert!(error.is_some()); // Should get "invalid target" error
        } else {
            panic!("Expected BanCreateResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_bancreate_target_too_long() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a target that's too long
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

        // Create admin user
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a duration that's too long
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

        // Verify no ban was created
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

        // Register a fake active transfer from an IP we'll ban
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

        // Register another transfer from a different IP (should not be affected)
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Ban the IP with the active transfer
        let result = handle_ban_create(
            "192.168.1.100".to_string(),
            Some("1h".to_string()),
            Some("test ban".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Verify the banned transfer received the ban signal
        assert!(
            ban_rx.try_recv().is_ok(),
            "Banned transfer should receive ban signal"
        );

        // Verify the safe transfer did NOT receive a ban signal
        let safe_result = safe_rx.try_recv();
        assert!(
            safe_result.is_err(),
            "Safe transfer should not receive ban signal"
        );

        // Clean up - unregister the transfers
        test_ctx.transfer_registry.unregister(info.id);
    }

    #[tokio::test]
    async fn test_bancreate_cidr_disconnects_transfers_in_range() {
        use crate::transfers::registry::{TransferDirection, TransferRegistration};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let mut test_ctx = create_test_context().await;

        // Register transfers from IPs in the CIDR range we'll ban (10.0.1.0/24)
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

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Ban the CIDR range
        let result = handle_ban_create(
            "10.0.1.0/24".to_string(),
            None,
            Some("CIDR ban test".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Verify transfers in range received ban signals
        assert!(
            rx1.try_recv().is_ok(),
            "Transfer in range should receive ban signal"
        );
        assert!(
            rx2.try_recv().is_ok(),
            "Transfer in range should receive ban signal"
        );

        // Verify transfer outside range did NOT receive ban signal
        assert!(
            rx3.try_recv().is_err(),
            "Transfer outside range should not receive ban signal"
        );
    }
}
