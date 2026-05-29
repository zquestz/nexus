//! Handler for ChatLeave command

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use crate::constants::{HANDLER_CHAT_LEAVE, LOG_CHAT_LEAVE_NOT_LOGGED_IN};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators;

use super::{
    HandlerContext, channel_error_to_message, err_channel_not_found, err_chat_feature_not_enabled,
    err_not_logged_in,
};
use crate::voice::send_voice_leave_notifications;

use crate::constants::FEATURE_CHAT;

enum LeaveOutcome {
    Disconnect,
    Queued,
    Send(ServerMessage),
}

pub async fn handle_chat_leave<W>(
    channel: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_CHAT_LEAVE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_LEAVE))
            .await;
    };

    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let Some(user) = ctx.user_manager.get_user_by_session_id(session_id).await else {
            break 'locked LeaveOutcome::Disconnect;
        };

        if let Err(e) = validators::validate_channel(&channel) {
            break 'locked LeaveOutcome::Send(ServerMessage::ChatLeaveResponse {
                success: false,
                error: Some(channel_error_to_message(e, ctx.locale)),
                channel: None,
            });
        }

        if !user.has_feature(FEATURE_CHAT) {
            break 'locked LeaveOutcome::Send(ServerMessage::ChatLeaveResponse {
                success: false,
                error: Some(err_chat_feature_not_enabled(ctx.locale)),
                channel: None,
            });
        }

        // Non-members get "not found" so secret channels' existence isn't leaked.
        match ctx.channel_manager.leave(&channel, session_id).await {
            None => {
                break 'locked LeaveOutcome::Send(ServerMessage::ChatLeaveResponse {
                    success: false,
                    error: Some(err_channel_not_found(ctx.locale, &channel)),
                    channel: None,
                });
            }
            Some(result) => {
                ctx.send_message_via_channel(&ServerMessage::ChatLeaveResponse {
                    success: true,
                    error: None,
                    channel: Some(channel.clone()),
                })?;

                if let Some(voice_session) = ctx.voice_registry.get_by_session_id(session_id).await
                    && voice_session.is_channel()
                    && voice_session.target_matches_channel(&channel)
                    && let Some(info) = ctx.voice_registry.remove_by_session_id(session_id).await
                {
                    send_voice_leave_notifications(
                        &info,
                        Some(&user.tx),
                        ctx.user_manager,
                        ctx.channel_manager,
                    )
                    .await;
                }

                // Broadcast ChatUserLeft only when this nickname becomes absent from
                // the channel (nickname-based membership; multiple sessions may map to
                // the same nickname).
                let nickname_present_elsewhere = ctx
                    .user_manager
                    .sessions_contain_nickname(
                        &result.remaining_member_session_ids,
                        &user.nickname,
                        None,
                    )
                    .await;
                if !nickname_present_elsewhere {
                    let leave_broadcast = ServerMessage::ChatUserLeft {
                        channel: channel.clone(),
                        nickname: user.nickname.clone(),
                    };
                    for member_session_id in &result.remaining_member_session_ids {
                        ctx.user_manager
                            .send_to_session(*member_session_id, leave_broadcast.clone())
                            .await;
                    }
                }
            }
        };

        LeaveOutcome::Queued
    };

    match outcome {
        LeaveOutcome::Disconnect => {
            ctx.send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_LEAVE))
                .await
        }
        LeaveOutcome::Queued => Ok(()),
        LeaveOutcome::Send(response) => ctx.send_message(&response).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::JoinPolicy;
    use crate::db::Permission;
    use crate::handlers::chat_join::handle_chat_join;
    use crate::handlers::testing::{
        TestContext, create_test_context, login_user_with_features, read_server_message,
    };

    async fn read_queued_server_message(test_ctx: &mut TestContext) -> ServerMessage {
        test_ctx
            .rx
            .recv()
            .await
            .expect("should receive queued server message")
            .0
    }

    #[tokio::test]
    async fn test_chat_leave_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_chat_leave(
            "#general".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_chat_leave_validates_channel_name() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Channel name missing the # prefix → validation error.
        let result = handle_chat_leave(
            "general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatLeaveResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatLeaveResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        let result = handle_chat_leave(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_queued_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatLeaveResponse {
                success,
                error,
                channel,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(channel, Some("#general".to_string()));
            }
            _ => panic!("Expected ChatLeaveResponse, got {:?}", response),
        }

        // Empty ephemeral channel is deleted on last leave.
        assert!(
            test_ctx
                .channel_manager
                .get_channel("#general")
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_chat_leave_not_member() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Leaving a never-joined channel → not-found error.
        let result = handle_chat_leave(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatLeaveResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatLeaveResponse, got {:?}", response),
        }
    }

    /// Add a second session for the same user (same nickname) to UserManager.
    async fn add_second_session(
        test_ctx: &mut TestContext,
        username: &str,
        permissions: &[Permission],
        features: Vec<String>,
    ) -> u32 {
        use crate::users::user::NewSessionParams;
        use std::collections::HashSet;
        use std::time::Instant;

        let perms: HashSet<Permission> = permissions.iter().copied().collect();

        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 1,
                username: username.to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features,
                locale: "en".to_string(),
                avatar: None,
                nickname: username.to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .expect("Failed to add second session")
    }

    /// Add a shared-account session with a custom nickname to UserManager.
    async fn add_shared_session(
        test_ctx: &mut TestContext,
        account_username: &str,
        nickname: &str,
        permissions: &[Permission],
        features: Vec<String>,
    ) -> u32 {
        use crate::users::user::NewSessionParams;
        use std::collections::HashSet;
        use std::time::Instant;

        let perms: HashSet<Permission> = permissions.iter().copied().collect();

        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 1,
                username: account_username.to_string(),
                is_admin: false,
                is_shared: true,
                permissions: perms,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features,
                locale: "en".to_string(),
                avatar: None,
                nickname: nickname.to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .expect("Failed to add shared session")
    }

    #[tokio::test]
    async fn test_chat_leave_broadcasts_user_left_to_remaining_members() {
        let mut test_ctx = create_test_context().await;

        // Login alice with chat permissions (including ChatCreate to create the channel)
        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice creates #general; bob joins as the observer.
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        let _ = test_ctx.rx.recv().await; // drain bob's ChatUserJoined

        // Alice (last session with her nickname) leaves → bob gets ChatUserLeft.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserLeft, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_no_broadcast_when_nickname_still_present() {
        let mut test_ctx = create_test_context().await;

        // Alice has two sessions in #general; bob observes.
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        while test_ctx.rx.try_recv().is_ok() {} // drain pending

        // Session 1 leaves but session 2 keeps the nickname present → no ChatUserLeft.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive ChatUserLeft when nickname still present via another session"
        );
    }

    #[tokio::test]
    async fn test_chat_leave_last_session_triggers_broadcast() {
        let mut test_ctx = create_test_context().await;

        // Login bob first (he'll be the observer, needs ChatCreate to create the channel)
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Alice has two sessions sharing one nickname.
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session1, JoinPolicy::CreateIfMissing)
            .await;
        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session2, JoinPolicy::CreateIfMissing)
            .await;

        while test_ctx.rx.try_recv().is_ok() {} // drain pending

        // First session out: nickname still present via session 2 → no broadcast.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Should NOT broadcast when first session leaves"
        );

        // Last session out → ChatUserLeft fires.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserLeft, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_shared_account_different_nicknames() {
        let mut test_ctx = create_test_context().await;

        // Bob is the observer.
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Two shared-account sessions with distinct nicknames.
        let guest1_session = add_shared_session(
            &mut test_ctx,
            "guest",
            "Guest1",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let guest2_session = add_shared_session(
            &mut test_ctx,
            "guest",
            "Guest2",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = test_ctx
            .channel_manager
            .join("#general", guest1_session, JoinPolicy::CreateIfMissing)
            .await;
        let _ = test_ctx
            .channel_manager
            .join("#general", guest2_session, JoinPolicy::CreateIfMissing)
            .await;

        while test_ctx.rx.try_recv().is_ok() {} // drain pending

        // Guest1 leaves: distinct nickname from Guest2, so ChatUserLeft fires.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(guest1_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Same message reaches both remaining members via one tx, so recv once then drain.
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "Guest1");
            }
            _ => panic!("Expected ChatUserLeft for Guest1, got {:?}", msg),
        }

        while test_ctx.rx.try_recv().is_ok() {} // drain duplicates

        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(guest2_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "Guest2");
            }
            _ => panic!("Expected ChatUserLeft for Guest2, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_regular_user_multiple_sessions_no_broadcast_until_last() {
        let mut test_ctx = create_test_context().await;

        // Bob is the observer.
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Regular user, two sessions sharing one nickname: no broadcast until the last leaves.
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session1, JoinPolicy::CreateIfMissing)
            .await;
        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session2, JoinPolicy::CreateIfMissing)
            .await;

        while test_ctx.rx.try_recv().is_ok() {} // drain pending

        // First session out: nickname still held by session 2 → no broadcast.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Should NOT broadcast when another session has same nickname"
        );

        // Last session out → ChatUserLeft fires.
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_queued_server_message(&mut test_ctx).await; // ChatLeaveResponse

        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserLeft, got {:?}", msg),
        }
    }
}
