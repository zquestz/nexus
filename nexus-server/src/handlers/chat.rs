//! Handler for ChatSend command

use std::io;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::{ChatAction, ServerMessage};
use nexus_common::validators::{self, MessageError};

use super::{
    HandlerContext, channel_error_to_message, err_channel_not_found, err_chat_feature_not_enabled,
    err_chat_too_long, err_flood_disconnect, err_flood_warning, err_message_contains_newlines,
    err_message_empty, err_message_invalid_characters, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    FEATURE_CHAT, HANDLER_CHAT_SEND, LOG_CHAT_SEND_NOT_LOGGED_IN, LOG_CHAT_SEND_PERMISSION_DENIED,
    LOG_FLOOD_DISCONNECT, LOG_FLOOD_LIMITED,
};
use crate::db::Permission;
use crate::flood::{FloodCheck, FloodTracker};

enum ChatSendOutcome {
    Disconnect,
    SendError(String),
    Sent,
}

pub async fn handle_chat_send<W>(
    message: String,
    action: ChatAction,
    channel: String,
    session_id: Option<u32>,
    flood_tracker: &mut FloodTracker,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_CHAT_SEND_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_SEND))
            .await;
    };

    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_SEND))
                .await;
        }
    };

    // Disabled-by-admin isn't a protocol violation.
    if !user.has_feature(FEATURE_CHAT) {
        return ctx
            .send_error(
                &err_chat_feature_not_enabled(ctx.locale),
                Some(HANDLER_CHAT_SEND),
            )
            .await;
    }

    if !user.has_permission(Permission::ChatSend) {
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_CHAT_SEND_PERMISSION_DENIED);
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some(HANDLER_CHAT_SEND))
            .await;
    }

    // Check flood protection (skip if disabled or user has chat_unlimited)
    let rate = ctx.flood_config.rate();
    if rate == 0 || user.has_permission(Permission::ChatUnlimited) {
        if flood_tracker.has_violations() {
            flood_tracker.reset_violations();
        }
    } else {
        let burst = ctx.flood_config.burst();
        match flood_tracker.check(burst, rate, Instant::now()) {
            FloodCheck::Allowed => {}
            FloodCheck::Limited {
                wait_seconds,
                violation,
                max_violations,
            } => {
                warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_FLOOD_LIMITED);
                return ctx
                    .send_error(
                        &err_flood_warning(ctx.locale, wait_seconds, violation, max_violations),
                        Some(HANDLER_CHAT_SEND),
                    )
                    .await;
            }
            FloodCheck::Disconnect => {
                warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_FLOOD_DISCONNECT);
                return ctx
                    .send_error_and_disconnect(
                        &err_flood_disconnect(ctx.locale),
                        Some(HANDLER_CHAT_SEND),
                    )
                    .await;
            }
        }
    }

    // Content rules (empty / over the char cap / newlines / invalid chars) —
    // the framing layer already rejected oversized payloads.
    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => err_message_empty(ctx.locale),
            MessageError::TooLong => err_chat_too_long(ctx.locale, validators::MAX_MESSAGE_LENGTH),
            MessageError::ContainsNewlines => err_message_contains_newlines(ctx.locale),
            MessageError::InvalidCharacters => err_message_invalid_characters(ctx.locale),
        };
        return ctx.send_error(&error_msg, Some(HANDLER_CHAT_SEND)).await;
    }

    if let Err(e) = validators::validate_channel(&channel) {
        return ctx
            .send_error(
                &channel_error_to_message(e, ctx.locale),
                Some(HANDLER_CHAT_SEND),
            )
            .await;
    }

    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let Some(current) = ctx.user_manager.get_user_by_session_id(id).await else {
            break 'locked ChatSendOutcome::Disconnect;
        };

        // Non-members get "not found" so secret channels' existence isn't leaked.
        if !ctx.channel_manager.is_member(&channel, id).await {
            break 'locked ChatSendOutcome::SendError(err_channel_not_found(ctx.locale, &channel));
        }

        let members = ctx
            .channel_manager
            .get_members(&channel)
            .await
            .unwrap_or_default();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let chat_message = ServerMessage::ChatMessage {
            session_id: id,
            nickname: current.nickname.clone(),
            is_admin: current.is_admin,
            is_shared: current.is_shared,
            message,
            action,
            channel,
            timestamp,
        };

        ctx.user_manager
            .broadcast_chat_message_to_members(&members, &chat_message)
            .await;

        ChatSendOutcome::Sent
    };

    match outcome {
        ChatSendOutcome::Disconnect => {
            ctx.send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_SEND))
                .await
        }
        ChatSendOutcome::SendError(error) => ctx.send_error(&error, Some(HANDLER_CHAT_SEND)).await,
        ChatSendOutcome::Sent => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::JoinPolicy;
    use crate::db;
    use crate::flood::{FloodConfig, FloodTracker};
    use crate::handlers::testing::{
        create_test_context, login_user_with_features, read_server_message,
    };

    #[tokio::test]
    async fn test_chat_requires_login() {
        let mut test_ctx = create_test_context().await;
        let session_id = None; // Not logged in

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            session_id,
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Chat should require login");
    }

    #[tokio::test]
    async fn test_chat_message_too_long() {
        let mut test_ctx = create_test_context().await;
        let session_id = Some(1); // Fake session (length check happens first)

        let long_message = "a".repeat(validators::MAX_MESSAGE_LENGTH + 1);

        let result = handle_chat_send(
            long_message,
            ChatAction::Normal,
            "#general".to_string(),
            session_id,
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Message over MAX_MESSAGE_LENGTH should be rejected"
        );
    }

    #[tokio::test]
    async fn test_chat_message_at_limit() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let max_message = "a".repeat(validators::MAX_MESSAGE_LENGTH);

        let result = handle_chat_send(
            max_message,
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_ok(),
            "Message at MAX_MESSAGE_LENGTH should be accepted"
        );
    }

    #[tokio::test]
    async fn test_chat_empty_message() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_send(
            "".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some(HANDLER_CHAT_SEND.to_string()));
            }
            other => panic!("Expected Error for empty message, got {other:?}"),
        }

        let result = handle_chat_send(
            "   ".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some(HANDLER_CHAT_SEND.to_string()));
            }
            other => panic!("Expected Error for whitespace message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_chat_message_with_newlines() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_send(
            "Hello\nWorld".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some(HANDLER_CHAT_SEND.to_string()));
            }
            other => panic!("Expected Error for newline message, got {other:?}"),
        }

        let result = handle_chat_send(
            "Hello\rWorld".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some(HANDLER_CHAT_SEND.to_string()));
            }
            other => panic!("Expected Error for carriage-return message, got {other:?}"),
        }

        let result = handle_chat_send(
            "Hello\r\nWorld".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some(HANDLER_CHAT_SEND.to_string()));
            }
            other => panic!("Expected Error for CRLF message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_chat_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT chat permission but WITH chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_chat_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH chat permission but WITHOUT chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![], // No chat feature
        )
        .await;

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Feature is admin-toggleable, not a protocol violation — error, not disconnect.
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some(HANDLER_CHAT_SEND.to_string()));
            }
            other => panic!("Expected Error for chat-without-feature, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_chat_successful() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_send(
            "Hello, world!".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Valid chat message should succeed");
    }

    #[tokio::test]
    async fn test_chat_send_uses_renamed_nickname_after_final_state_gate() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatReceive, db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        let gate_manager = test_ctx.user_manager.clone();
        let update_manager = test_ctx.user_manager.clone();
        let user_state = gate_manager.lock_user_state().await;
        let mut flood_tracker = FloodTracker::new();

        {
            let mut ctx = test_ctx.handler_context();
            let handler = handle_chat_send(
                "hello after rename".to_string(),
                ChatAction::Normal,
                "#general".to_string(),
                Some(session_id),
                &mut flood_tracker,
                &mut ctx,
            );
            tokio::pin!(handler);

            tokio::select! {
                biased;
                result = &mut handler => panic!("handler finished before rename gate: {result:?}"),
                _ = tokio::task::yield_now() => {}
            }

            update_manager
                .update_username(alice.id, "alicia".to_string())
                .await;
            drop(user_state);

            handler.await.unwrap();
        }

        match test_ctx
            .rx
            .recv()
            .await
            .expect("should receive chat message")
            .expect_message()
            .0
        {
            ServerMessage::ChatMessage { nickname, .. } => {
                assert_eq!(nickname, "alicia");
            }
            other => panic!("Expected ChatMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_chat_invalid_session() {
        let mut test_ctx = create_test_context().await;

        let invalid_session_id = Some(999);

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            invalid_session_id,
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Chat with invalid session should be rejected"
        );
    }

    #[tokio::test]
    async fn test_chat_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user WITHOUT explicit ChatSend permission
        // Admins should have all permissions automatically
        let session_id = login_user_with_features(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_send(
            "Admin message!".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Admin should be able to chat without explicit permission"
        );
    }

    #[tokio::test]
    async fn test_chat_to_channel_not_member() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend, db::Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Another session creates the channel; alice never joins.
        test_ctx
            .channel_manager
            .join("#general", 999, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_chat_to_nonexistent_channel() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "#nonexistent".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_chat_to_specific_channel() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend, db::Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_send(
            "Hello channel!".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Chat to joined channel should succeed");
    }

    #[tokio::test]
    async fn test_chat_requires_channel() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_send(
            "Hello".to_string(),
            ChatAction::Normal,
            "".to_string(), // Empty channel name
            Some(session_id),
            &mut FloodTracker::new(),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error {
                message, command, ..
            } => {
                assert!(message.to_lowercase().contains("channel"));
                assert_eq!(command, Some("ChatSend".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_flood_limited_after_burst() {
        let mut test_ctx = create_test_context().await;

        // Set tight flood config: burst=2, rate=20
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(2, 20));

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let mut tracker = FloodTracker::new();

        // First 2 messages should succeed (burst=2)
        for _ in 0..2 {
            let result = handle_chat_send(
                "Hello".to_string(),
                ChatAction::Normal,
                "#general".to_string(),
                Some(session_id),
                &mut tracker,
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(result.is_ok());
        }

        // 3rd message should be rate limited
        let result = handle_chat_send(
            "Spam".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut tracker,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_ok(),
            "Rate limit sends error but doesn't disconnect"
        );

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { command, .. } => {
                assert_eq!(command, Some("ChatSend".to_string()));
            }
            _ => panic!("Expected flood warning Error, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_flood_disconnect_after_repeated_violations() {
        let mut test_ctx = create_test_context().await;

        // Set tight flood config: burst=1, rate=20
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(1, 20));

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let mut tracker = FloodTracker::new();

        // Exhaust the single burst token
        let result = handle_chat_send(
            "First".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut tracker,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        // Violations 1 and 2 — Limited
        for _ in 0..2 {
            let result = handle_chat_send(
                "Spam".to_string(),
                ChatAction::Normal,
                "#general".to_string(),
                Some(session_id),
                &mut tracker,
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(result.is_ok(), "Limited should not disconnect");
            let _ = read_server_message(&mut test_ctx).await;
        }

        // Violation 3 — Disconnect
        let result = handle_chat_send(
            "Spam".to_string(),
            ChatAction::Normal,
            "#general".to_string(),
            Some(session_id),
            &mut tracker,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(
            result.is_err(),
            "3rd consecutive violation should disconnect"
        );
    }

    #[tokio::test]
    async fn test_chat_flood_unlimited_permission_bypasses() {
        let mut test_ctx = create_test_context().await;

        // Set very tight flood config: burst=1, rate=1
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(1, 1));

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend, db::Permission::ChatUnlimited],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let mut tracker = FloodTracker::new();

        for i in 0..10 {
            let result = handle_chat_send(
                format!("Message {}", i),
                ChatAction::Normal,
                "#general".to_string(),
                Some(session_id),
                &mut tracker,
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(
                result.is_ok(),
                "chat_unlimited user should never be rate limited"
            );
        }
    }

    #[tokio::test]
    async fn test_chat_flood_disabled_when_rate_zero() {
        let mut test_ctx = create_test_context().await;

        // Disable flood protection: rate=0
        test_ctx.flood_config = std::sync::Arc::new(FloodConfig::new(5, 0));

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::ChatSend],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let mut tracker = FloodTracker::new();

        for i in 0..10 {
            let result = handle_chat_send(
                format!("Message {}", i),
                ChatAction::Normal,
                "#general".to_string(),
                Some(session_id),
                &mut tracker,
                &mut test_ctx.handler_context(),
            )
            .await;
            assert!(result.is_ok(), "rate=0 should disable flood protection");
        }
    }
}
