//! Handler for BanCreate command

use std::io;
use std::net::IpAddr;

use ipnet::IpNet;
use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::time::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};
use nexus_common::validators::{self, BanReasonError};

use super::duration::parse_duration;
use super::{
    HandlerContext, err_authentication, err_ban_admin_by_ip, err_ban_admin_by_nickname,
    err_ban_invalid_duration, err_ban_invalid_target, err_ban_self, err_database,
    err_not_logged_in, err_permission_denied, err_reason_invalid, err_reason_too_long,
};
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
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("BanCreate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("BanCreate"))
            .await;
    };

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
        eprintln!(
            "BanCreate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::BanCreateResponse {
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
    let (targets_to_ban, nickname_annotation, is_cidr) =
        match resolve_target(&target, &requesting_user.username, ctx).await {
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
                eprintln!(
                    "BanCreate from {} (user: {}) attempted to ban admin by nickname",
                    ctx.peer_addr, requesting_user.username
                );
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
            eprintln!(
                "BanCreate from {} (user: {}) attempted to ban CIDR {} with admin connected",
                ctx.peer_addr, requesting_user.username, targets_to_ban[0]
            );
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
        for ip in &targets_to_ban {
            if ctx.user_manager.is_admin_connected_from_ip(ip).await {
                eprintln!(
                    "BanCreate from {} (user: {}) attempted to ban IP {} with admin connected",
                    ctx.peer_addr, requesting_user.username, ip
                );
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
                eprintln!("BanCreate database error for {}: {}", target_str, e);
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
    }

    // Send success response
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

/// Format remaining duration for display (e.g., "2h 30m")
fn format_duration_remaining(expires_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs() as i64;

    let remaining_secs = (expires_at - now).max(0);

    let days = remaining_secs / SECONDS_PER_DAY as i64;
    let hours = (remaining_secs % SECONDS_PER_DAY as i64) / SECONDS_PER_HOUR as i64;
    let minutes = (remaining_secs % SECONDS_PER_HOUR as i64) / SECONDS_PER_MINUTE as i64;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    // =========================================================================
    // Unit tests for parse_duration and format_duration_remaining
    // =========================================================================

    #[test]
    fn test_parse_duration_none() {
        assert_eq!(parse_duration(&None), Ok(None));
    }

    #[test]
    fn test_parse_duration_empty() {
        assert_eq!(parse_duration(&Some("".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_zero() {
        assert_eq!(parse_duration(&Some("0".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_minutes() {
        let result = parse_duration(&Some("10m".to_string()));
        assert!(result.is_ok());
        let expires = result.unwrap().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // Should be approximately 10 minutes from now (allow 2 second tolerance)
        assert!((expires - now - 600).abs() < 2);
    }

    #[test]
    fn test_parse_duration_hours() {
        let result = parse_duration(&Some("4h".to_string()));
        assert!(result.is_ok());
        let expires = result.unwrap().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!((expires - now - 14400).abs() < 2);
    }

    #[test]
    fn test_parse_duration_days() {
        let result = parse_duration(&Some("7d".to_string()));
        assert!(result.is_ok());
        let expires = result.unwrap().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!((expires - now - 604800).abs() < 2);
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration(&Some("10x".to_string())).is_err());
        assert!(parse_duration(&Some("abc".to_string())).is_err());
        assert!(parse_duration(&Some("10".to_string())).is_err());
    }

    #[test]
    fn test_format_duration_remaining() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // 2 hours 30 minutes
        assert_eq!(format_duration_remaining(now + 9000), "2h 30m");

        // 1 day 5 hours
        assert_eq!(format_duration_remaining(now + 104400), "1d 5h");

        // 45 minutes
        assert_eq!(format_duration_remaining(now + 2700), "45m");

        // 1 minute (minimum)
        assert_eq!(format_duration_remaining(now + 30), "1m");
    }

    // =========================================================================
    // Handler integration tests
    // =========================================================================

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

    #[test]
    fn test_parse_duration_zero_variants() {
        // All zero variants should return permanent (None)
        assert_eq!(parse_duration(&Some("0m".to_string())), Ok(None));
        assert_eq!(parse_duration(&Some("0h".to_string())), Ok(None));
        assert_eq!(parse_duration(&Some("0d".to_string())), Ok(None));
    }

    #[test]
    fn test_parse_duration_whitespace() {
        // Whitespace should be trimmed
        assert_eq!(parse_duration(&Some("  0  ".to_string())), Ok(None));
        assert_eq!(parse_duration(&Some(" ".to_string())), Ok(None));
    }

    // =========================================================================
    // Trusted IP protection tests
    // =========================================================================

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
}
