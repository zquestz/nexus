use std::io;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::names::fold_name;
use nexus_common::protocol::{ChatAction, ServerMessage};
use nexus_common::validators::{self, MessageError, NicknameError};

use super::{
    HandlerContext, err_cannot_message_self, err_chat_feature_not_enabled,
    err_chat_target_feature_not_enabled, err_chat_too_long, err_flood_disconnect,
    err_flood_warning, err_message_contains_newlines, err_message_empty,
    err_message_invalid_characters, err_nickname_empty, err_nickname_invalid,
    err_nickname_not_online, err_nickname_too_long, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    FEATURE_CHAT, HANDLER_USER_MESSAGE, LOG_FLOOD_DISCONNECT, LOG_FLOOD_LIMITED,
    LOG_USER_MESSAGE_NOT_LOGGED_IN, LOG_USER_MESSAGE_PERMISSION_DENIED,
};
use crate::db::Permission;
use crate::flood::FloodCheck;

/// Outcome of the lock-serialized delivery section, dispatched to a
/// `UserMessageResponse` after the `read_user_state` guard drops (so no socket I/O
/// happens under the lock).
enum DeliveryOutcome {
    /// Sender's session vanished mid-send (raced disconnect); send nothing.
    SenderGone,
    /// Target nickname not online; send a not-online error.
    TargetOffline,
    /// Target nickname is online, but no target session can receive chat DMs.
    TargetFeatureUnavailable,
    /// Target resolves to the sender's own (possibly renamed) session; reject as
    /// a self-message.
    SelfMessage,
    /// Delivered to both parties; carry the target's away state for the response.
    Delivered {
        is_away: bool,
        status: Option<String>,
    },
}

pub async fn handle_user_message<W>(
    to_nickname: String,
    message: String,
    action: ChatAction,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_MESSAGE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_MESSAGE))
            .await;
    };

    let requesting_user_session = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_USER_MESSAGE),
                )
                .await;
        }
    };

    if !requesting_user_session.has_feature(FEATURE_CHAT) {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_chat_feature_not_enabled(ctx.locale)),
            is_away: None,
            status: None,
        };
        return ctx.send_message(&response).await;
    }

    if !requesting_user_session.has_permission(Permission::UserMessage) {
        warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_USER_MESSAGE_PERMISSION_DENIED);
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            is_away: None,
            status: None,
        };
        return ctx.send_message(&response).await;
    }

    match ctx
        .user_manager
        .check_message_flood(session_id, &ctx.flood_config, Instant::now())
        .await
    {
        Some(FloodCheck::Allowed) => {}
        Some(FloodCheck::Limited {
            wait_seconds,
            violation,
            max_violations,
        }) => {
            warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_FLOOD_LIMITED);
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_flood_warning(
                    ctx.locale,
                    wait_seconds,
                    violation,
                    max_violations,
                )),
                is_away: None,
                status: None,
            };
            return ctx.send_message(&response).await;
        }
        Some(FloodCheck::Disconnect) => {
            warn!(user = %requesting_user_session.username, ip = %ctx.peer_addr, "{}", LOG_FLOOD_DISCONNECT);
            return ctx
                .send_error_and_disconnect(
                    &err_flood_disconnect(ctx.locale),
                    Some(HANDLER_USER_MESSAGE),
                )
                .await;
        }
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_USER_MESSAGE),
                )
                .await;
        }
    }

    if let Err(e) = validators::validate_nickname(&to_nickname) {
        let error_msg = match e {
            NicknameError::Empty => err_nickname_empty(ctx.locale),
            NicknameError::TooLong => {
                err_nickname_too_long(ctx.locale, validators::MAX_NICKNAME_LENGTH)
            }
            NicknameError::InvalidCharacters => err_nickname_invalid(ctx.locale),
        };
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(error_msg),
            is_away: None,
            status: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => err_message_empty(ctx.locale),
            MessageError::TooLong => err_chat_too_long(ctx.locale, validators::MAX_MESSAGE_LENGTH),
            MessageError::ContainsNewlines => err_message_contains_newlines(ctx.locale),
            MessageError::InvalidCharacters => err_message_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(error_msg),
            is_away: None,
            status: None,
        };
        return ctx.send_message(&response).await;
    }

    // Prevent self-messaging by nickname (display name), before DB queries.
    let to_nickname_lower = fold_name(&to_nickname);
    let is_self_message = fold_name(&requesting_user_session.nickname) == to_nickname_lower;
    if is_self_message {
        let response = ServerMessage::UserMessageResponse {
            success: false,
            error: Some(err_cannot_message_self(ctx.locale)),
            is_away: None,
            status: None,
        };
        return ctx.send_message(&response).await;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Resolve both the sender and the target, then broadcast, all under read_user_state
    // so a concurrent rename can't re-key either party between the lookup and the
    // nickname-keyed broadcast. The sender must be re-fetched here, not reused from the
    // entry snapshot: a rename runs on another thread holding the write lock, so the
    // snapshot can go stale regardless of there being no await between it and this lock.
    // A stale sender nickname would misroute the echo (the sender never sees her own
    // message) and stamp the recipient's DM tab with the old name; a stale target would
    // silently drop the message while the sender still saw success. Both broadcasts are
    // in-memory; the response is sent after the guard drops.
    let outcome = 'deliver: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let Some(sender) = ctx.user_manager.get_user_by_session_id(session_id).await else {
            break 'deliver DeliveryOutcome::SenderGone;
        };

        // Target by nickname (equals username for regular accounts), but only
        // chat-enabled sessions can receive DMs.
        let target_sessions = ctx
            .user_manager
            .get_sessions_by_nickname(&to_nickname)
            .await;
        if target_sessions.is_empty() {
            break 'deliver DeliveryOutcome::TargetOffline;
        }

        let Some(target_session) = target_sessions
            .into_iter()
            .filter(|session| session.has_feature(FEATURE_CHAT))
            .max_by_key(|session| session.last_activity)
        else {
            break 'deliver DeliveryOutcome::TargetFeatureUnavailable;
        };

        // Re-check self-message under the lock: the pre-lock check used the entry
        // snapshot, which a concurrent rename could have changed so the target now
        // resolves to the sender's own (renamed) session — which would deliver a self-DM.
        if sender.nickname_folded == target_session.nickname_folded {
            break 'deliver DeliveryOutcome::SelfMessage;
        }

        let broadcast = ServerMessage::UserMessage {
            from_nickname: sender.nickname.clone(),
            from_admin: sender.is_admin,
            from_shared: sender.is_shared,
            to_nickname: target_session.nickname.clone(),
            message,
            action,
            timestamp,
        };
        // Broadcast to both parties by nickname. Regular accounts (nickname == username)
        // reach every chat-enabled session; shared accounts reach the one nickname.
        ctx.user_manager
            .broadcast_to_nickname_with_feature(&sender.nickname, FEATURE_CHAT, &broadcast)
            .await;
        ctx.user_manager
            .broadcast_to_nickname_with_feature(&target_session.nickname, FEATURE_CHAT, &broadcast)
            .await;

        DeliveryOutcome::Delivered {
            is_away: target_session.is_away,
            status: target_session.status.clone(),
        }
    };

    match outcome {
        // Sender disconnected mid-send; nothing to deliver and no live socket to answer.
        DeliveryOutcome::SenderGone => Ok(()),
        DeliveryOutcome::TargetOffline => {
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_nickname_not_online(ctx.locale, &to_nickname)),
                is_away: None,
                status: None,
            };
            ctx.send_message(&response).await
        }
        DeliveryOutcome::TargetFeatureUnavailable => {
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_chat_target_feature_not_enabled(
                    ctx.locale,
                    &to_nickname,
                )),
                is_away: None,
                status: None,
            };
            ctx.send_message(&response).await
        }
        DeliveryOutcome::SelfMessage => {
            let response = ServerMessage::UserMessageResponse {
                success: false,
                error: Some(err_cannot_message_self(ctx.locale)),
                is_away: None,
                status: None,
            };
            ctx.send_message(&response).await
        }
        DeliveryOutcome::Delivered { is_away, status } => {
            // Respond via the channel so it queues after the broadcasts — the away
            // notice must appear after the message in the client's receive order.
            let response = ServerMessage::UserMessageResponse {
                success: true,
                error: None,
                is_away: if is_away { Some(true) } else { None },
                status: if is_away { status } else { None },
            };
            ctx.send_message_via_channel(&response)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::JoinPolicy;
    use crate::constants::FEATURE_CHAT;
    use crate::db;
    use crate::db::Permission;
    use crate::flood::FloodConfig;
    use crate::handlers::handle_chat_send;
    use crate::handlers::testing::{
        add_observer_session_for_existing_regular_user,
        add_observer_session_for_existing_regular_user_with_features, create_test_context,
        get_cached_password_hash, login_shared_user_with_features, login_user,
        login_user_with_features, read_channel_response, read_login_response, read_server_message,
    };

    fn chat_features() -> Vec<String> {
        vec![FEATURE_CHAT.to_string()]
    }

    async fn login_chat_user(
        test_ctx: &mut crate::handlers::testing::TestContext,
        username: &str,
        password: &str,
        permissions: &[Permission],
        is_admin: bool,
    ) -> u32 {
        login_user_with_features(
            test_ctx,
            username,
            password,
            permissions,
            is_admin,
            chat_features(),
        )
        .await
    }

    #[tokio::test]
    async fn test_usermessage_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_message(
            "alice".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_usermessage_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user without UserMessage permission
        let _sender_id = login_chat_user(&mut test_ctx, "sender", "pass123", &[], false).await;

        // Create target user with UserMessage permission
        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Try to send message without permission
        let result = handle_user_message(
            "target".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Check response
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().to_lowercase().contains("permission"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_requires_chat_feature() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;
        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_user_message(
            "target".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                let expected =
                    err_chat_feature_not_enabled(crate::handlers::testing::DEFAULT_TEST_LOCALE);
                assert_eq!(error.as_deref(), Some(expected.as_str()));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_target_without_chat_feature_reports_capability_error() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;
        let _target_id = login_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_user_message(
            "target".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                let expected = err_chat_target_feature_not_enabled(
                    crate::handlers::testing::DEFAULT_TEST_LOCALE,
                    "target",
                );
                assert_eq!(error.as_deref(), Some(expected.as_str()));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_skips_non_chat_sessions_for_same_nickname() {
        let mut test_ctx = create_test_context().await;

        let sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;
        let (_sender_no_chat_id, mut sender_no_chat_rx) =
            add_observer_session_for_existing_regular_user(
                &mut test_ctx,
                "sender",
                &[Permission::UserMessage],
            )
            .await;

        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;
        let (_target_no_chat_id, mut target_no_chat_rx) =
            add_observer_session_for_existing_regular_user(
                &mut test_ctx,
                "target",
                &[Permission::UserMessage],
            )
            .await;

        let result = handle_user_message(
            "target".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(sender_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse"),
        }

        assert!(sender_no_chat_rx.try_recv().is_err());
        assert!(target_no_chat_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_usermessage_away_notice_uses_most_recent_chat_session() {
        let mut test_ctx = create_test_context().await;

        let sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;
        let target_first_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;
        let (target_second_id, _target_second_rx) =
            add_observer_session_for_existing_regular_user_with_features(
                &mut test_ctx,
                "target",
                &[Permission::UserMessage],
                chat_features(),
            )
            .await;

        test_ctx
            .user_manager
            .set_status(target_first_id, true, Some("most recent away".to_string()))
            .await;
        test_ctx
            .user_manager
            .set_status(target_second_id, false, Some("older status".to_string()))
            .await;

        let base_activity = std::time::Instant::now();
        test_ctx
            .user_manager
            .set_last_activity_for_test(target_second_id, base_activity)
            .await;
        test_ctx
            .user_manager
            .set_last_activity_for_test(
                target_first_id,
                base_activity + std::time::Duration::from_secs(1),
            )
            .await;

        let result = handle_user_message(
            "target".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(sender_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse {
                success,
                error,
                is_away,
                status,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(is_away, Some(true));
                assert_eq!(status, Some("most recent away".to_string()));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_empty_message() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_user_message(
            "target".to_string(),
            "   ".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().contains("empty"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_message_too_long() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let long_message = "x".repeat(validators::MAX_MESSAGE_LENGTH + 1);

        let result = handle_user_message(
            "target".to_string(),
            long_message,
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().contains("too long"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_cannot_message_self() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_user_message(
            "sender".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().to_lowercase().contains("yourself"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_target_not_found() {
        let mut test_ctx = create_test_context().await;

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let result = handle_user_message(
            "nonexistent".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                // User not online (we don't distinguish "not found" from "not online" for security)
                assert!(error.unwrap().contains("not online"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_target_not_online() {
        let mut test_ctx = create_test_context().await;

        // Create sender with permission (online)
        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Create target user account but DON'T login (offline)
        use crate::db::Permissions;
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "target",
                hashed_password: "pass456",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let result = handle_user_message(
            "target".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().contains("not online"));
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_successful() {
        let mut test_ctx = create_test_context().await;

        // Create sender with permission (session_id 1)
        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Create target with permission (session_id 2)
        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Send message
        let result = handle_user_message(
            "target".to_string(),
            "hello world".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Check sender gets success response (via channel since success uses send_message_via_channel)
        // Skip broadcast messages (UserMessage) to find the response
        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse, got: {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_usermessage_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin sender (no explicit permission needed) (session_id 1)
        let _admin_id = login_chat_user(&mut test_ctx, "admin", "pass123", &[], true).await;

        // Create target (session_id 2)
        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Admin sends message without explicit permission
        let result = handle_user_message(
            "target".to_string(),
            "admin message".to_string(),
            ChatAction::Normal,
            Some(1), // admin's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Check admin gets success response (via channel since success uses send_message_via_channel)
        // Skip broadcast messages (UserMessage) to find the response
        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse, got: {:?}", response),
        }
    }

    // Shared account tests.

    #[tokio::test]
    async fn test_usermessage_to_shared_account_by_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::db;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create admin sender with message permission
        let admin_id = login_chat_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account in the database
        let hashed = get_cached_password_hash("password");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Login to the shared account with a nickname
        let mut shared_session_id = None;
        let login_request = LoginRequest {
            username: "shared_acct".to_string(),
            password: "password".to_string(),
            features: chat_features(),
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Nick1".to_string()),
            handshake_complete: true,
        };
        let _ = handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_login_response(&mut test_ctx).await; // consume login response

        // Send message to shared account by nickname
        let result = handle_user_message(
            "Nick1".to_string(),
            "Hello Nick1!".to_string(),
            ChatAction::Normal,
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Success response comes via channel since success uses send_message_via_channel
        // Skip broadcast messages (UserMessage) to find the response
        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(
                    success,
                    "Message to shared account by nickname should succeed"
                );
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_self_message_by_nickname_prevented() {
        let mut test_ctx = create_test_context().await;
        use crate::db;
        use crate::handlers::login::{LoginRequest, handle_login};

        // First create an admin so system works
        let _admin_id = login_chat_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account with message permission
        let hashed = get_cached_password_hash("password");
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserMessage);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Login to the shared account with a nickname
        let mut shared_session_id = None;
        let login_request = LoginRequest {
            username: "shared_acct".to_string(),
            password: "password".to_string(),
            features: chat_features(),
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Nick1".to_string()),
            handshake_complete: true,
        };
        let _ = handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_login_response(&mut test_ctx).await; // consume login response

        let session_id = shared_session_id.unwrap();

        // Try to send message to self by nickname (should fail)
        let result = handle_user_message(
            "Nick1".to_string(),
            "Message to myself".to_string(),
            ChatAction::Normal,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success, "Self-message by nickname should be prevented");
                assert!(error.is_some());
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_from_shared_account_shows_nickname() {
        let mut test_ctx = create_test_context().await;
        use crate::db;
        use crate::handlers::login::{LoginRequest, handle_login};

        // Create a target user
        let _target_id = login_chat_user(&mut test_ctx, "target", "password", &[], false).await;

        // Create a shared account with message permission
        let hashed = get_cached_password_hash("password");
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserMessage);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Login to the shared account with a nickname
        let mut shared_session_id = None;
        let login_request = LoginRequest {
            username: "shared_acct".to_string(),
            password: "password".to_string(),
            features: chat_features(),
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Sender".to_string()),
            handshake_complete: true,
        };
        let _ = handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_login_response(&mut test_ctx).await; // consume login response

        let session_id = shared_session_id.unwrap();

        // Send message from shared account to target
        let result = handle_user_message(
            "target".to_string(),
            "Hello from shared!".to_string(),
            ChatAction::Normal,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Success response comes via channel since success uses send_message_via_channel
        // Skip broadcast messages (UserMessage) to find the response
        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse { success, .. } => {
                assert!(success, "Message from shared account should succeed");
            }
            _ => panic!("Expected UserMessageResponse"),
        }

        // The UserMessage broadcast would contain from_username: "Sender" (the nickname)
        // This is verified by the implementation using requesting_user_session.nickname
    }

    #[tokio::test]
    async fn test_usermessage_shared_account_by_nickname_succeeds() {
        let mut test_ctx = create_test_context().await;

        // Create sender with permission
        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Create shared account user with nickname "Nick1"
        let _shared_id = login_shared_user_with_features(
            &mut test_ctx,
            "shared_acct",
            "sharedpass",
            "Nick1",
            &[Permission::UserMessage],
            chat_features(),
        )
        .await;

        // Message by nickname (should succeed)
        let result = handle_user_message(
            "Nick1".to_string(),
            "hello".to_string(),
            ChatAction::Normal,
            Some(1), // sender's session_id
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Success response comes via channel since success uses send_message_via_channel
        // Skip broadcast messages (UserMessage) to find the response
        let response = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(success, "Should allow messaging shared account by nickname");
                assert!(error.is_none());
            }
            _ => panic!("Expected UserMessageResponse"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_flood_limited_after_burst() {
        let mut test_ctx = create_test_context().await;

        // Set tight flood config: burst=2, rate=20
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(2, 20));

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // First 2 messages should succeed (burst=2)
        for _ in 0..2 {
            let result = handle_user_message(
                "target".to_string(),
                "hello".to_string(),
                ChatAction::Normal,
                Some(1),
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(result.is_ok());
            // Drain the response (success goes via channel, not frame writer)
            let _ = read_channel_response(&mut test_ctx, |msg| {
                matches!(msg, ServerMessage::UserMessageResponse { .. })
            });
        }

        // 3rd message should be rate limited
        let result = handle_user_message(
            "target".to_string(),
            "spam".to_string(),
            ChatAction::Normal,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_ok(),
            "Rate limit sends error but doesn't disconnect"
        );

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserMessageResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_and_usermessage_share_flood_bucket() {
        let mut test_ctx = create_test_context().await;

        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(1, 20));

        let sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::ChatSend, Permission::UserMessage],
            false,
        )
        .await;

        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", sender_id, JoinPolicy::CreateIfMissing)
            .await
            .expect("sender should join test channel");

        let result = handle_chat_send(
            "first".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(sender_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let result = handle_user_message(
            "target".to_string(),
            "second".to_string(),
            ChatAction::Normal,
            Some(sender_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_ok(),
            "second message across chat surfaces should be limited but not disconnected"
        );

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserMessageResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            response => panic!("Expected UserMessageResponse, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn test_usermessage_flood_disconnect_after_repeated_violations() {
        let mut test_ctx = create_test_context().await;

        // Set tight flood config: burst=1, rate=20
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(1, 20));

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Exhaust the single burst token
        let result = handle_user_message(
            "target".to_string(),
            "first".to_string(),
            ChatAction::Normal,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let _ = read_channel_response(&mut test_ctx, |msg| {
            matches!(msg, ServerMessage::UserMessageResponse { .. })
        });

        // Violations 1 and 2 — Limited
        for _ in 0..2 {
            let result = handle_user_message(
                "target".to_string(),
                "spam".to_string(),
                ChatAction::Normal,
                Some(1),
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(result.is_ok(), "Limited should not disconnect");
            let _ = read_server_message(&mut test_ctx).await;
        }

        // Violation 3 — Disconnect
        let result = handle_user_message(
            "target".to_string(),
            "spam".to_string(),
            ChatAction::Normal,
            Some(1),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_err(),
            "3rd consecutive violation should disconnect"
        );
    }

    #[tokio::test]
    async fn test_usermessage_flood_unlimited_permission_bypasses() {
        let mut test_ctx = create_test_context().await;

        // Set very tight flood config: burst=1, rate=1
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(1, 1));

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage, Permission::ChatUnlimited],
            false,
        )
        .await;

        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Should be able to send many messages without being rate limited
        for i in 0..10 {
            let result = handle_user_message(
                "target".to_string(),
                format!("Message {}", i),
                ChatAction::Normal,
                Some(1),
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(
                result.is_ok(),
                "chat_unlimited user should never be rate limited"
            );
            let _ = read_channel_response(&mut test_ctx, |msg| {
                matches!(msg, ServerMessage::UserMessageResponse { .. })
            });
        }
    }

    #[tokio::test]
    async fn test_usermessage_flood_disabled_when_rate_zero() {
        let mut test_ctx = create_test_context().await;

        // Disable flood protection: rate=0
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(5, 0));

        let _sender_id = login_chat_user(
            &mut test_ctx,
            "sender",
            "pass123",
            &[Permission::UserMessage],
            false,
        )
        .await;

        let _target_id = login_chat_user(
            &mut test_ctx,
            "target",
            "pass456",
            &[Permission::UserMessage],
            false,
        )
        .await;

        // Should be able to send many messages without being rate limited
        for i in 0..10 {
            let result = handle_user_message(
                "target".to_string(),
                format!("Message {}", i),
                ChatAction::Normal,
                Some(1),
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(result.is_ok(), "rate=0 should disable flood protection");
            let _ = read_channel_response(&mut test_ctx, |msg| {
                matches!(msg, ServerMessage::UserMessageResponse { .. })
            });
        }
    }
}
