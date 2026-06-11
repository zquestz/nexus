//! Handler for VoiceLeave command

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use crate::constants::{FEATURE_VOICE, HANDLER_VOICE_LEAVE, LOG_VOICE_LEAVE_NOT_LOGGED_IN};

use nexus_common::protocol::ServerMessage;

use super::{
    HandlerContext, err_not_logged_in, err_voice_feature_not_enabled, err_voice_not_joined,
};
use crate::voice::send_voice_leave_notifications;

/// Remove the user from their voice session and broadcast VoiceUserLeft.
pub async fn handle_voice_leave<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_VOICE_LEAVE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_VOICE_LEAVE))
            .await;
    };

    // Verify session still exists (user might have disconnected).
    let Some(user) = ctx.user_manager.get_user_by_session_id(session_id).await else {
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_VOICE_LEAVE))
            .await;
    };

    if !user.has_feature(FEATURE_VOICE) {
        let response = ServerMessage::VoiceLeaveResponse {
            success: false,
            error: Some(err_voice_feature_not_enabled(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Serialize removal + leave notifications under read_user_state so a concurrent
    // rename orders consistently with our VoiceUserLeft: the rename either fully precedes
    // (it re-keys the registry entry before we remove it, so we broadcast the new
    // nickname) or fully follows (our VoiceUserLeft is enqueued before its
    // ChatUserRenamed). Otherwise a stale-nickname VoiceUserLeft could arrive after the
    // client re-keyed and fail to clear the voiced participant.
    // send_voice_leave_notifications is in-memory; the response is sent after the guard
    // drops.
    let removed = {
        let _user_state = ctx.user_manager.read_user_state().await;
        match ctx.voice_registry.remove_by_session_id(session_id).await {
            Some(info) => {
                // Notify remaining participants (not self — this is an explicit leave).
                send_voice_leave_notifications(&info, None, ctx.user_manager, ctx.channel_manager)
                    .await;
                true
            }
            None => false,
        }
    };

    let response = if removed {
        ServerMessage::VoiceLeaveResponse {
            success: true,
            error: None,
        }
    } else {
        ServerMessage::VoiceLeaveResponse {
            success: false,
            error: Some(err_voice_not_joined(ctx.locale)),
        }
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{FEATURE_CHAT, FEATURE_VOICE};
    use crate::db::Permission;
    use crate::handlers::chat_join::handle_chat_join;
    use crate::handlers::testing::{
        add_observer_session_for_existing_regular_user_with_features, create_test_context,
        login_observer_user, login_user, login_user_with_features, read_server_message,
    };
    use crate::handlers::voice_join::handle_voice_join;
    use crate::users::user::SessionRx;
    use std::time::Duration;
    use tokio::time::timeout;

    fn voice_features() -> Vec<String> {
        vec![FEATURE_VOICE.to_string()]
    }

    fn chat_voice_features() -> Vec<String> {
        vec![FEATURE_CHAT.to_string(), FEATURE_VOICE.to_string()]
    }

    async fn login_voice_user(
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
            voice_features(),
        )
        .await
    }

    async fn login_chat_voice_user(
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
            chat_voice_features(),
        )
        .await
    }

    async fn expect_voice_user_joined(rx: &mut SessionRx, nickname: &str, target: &str) {
        let event = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for voice join notification")
            .expect("voice join notification")
            .expect_message();
        let (message, message_id) = event;

        assert_eq!(message_id, None);
        match message {
            ServerMessage::VoiceUserJoined {
                nickname: actual_nickname,
                target: actual_target,
            } => {
                assert_eq!(actual_nickname, nickname);
                assert_eq!(actual_target, target);
            }
            other => panic!("Expected VoiceUserJoined, got {:?}", other),
        }
    }

    async fn expect_voice_user_left(rx: &mut SessionRx, nickname: &str, target: &str) {
        let event = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for voice leave notification")
            .expect("voice leave notification")
            .expect_message();
        let (message, message_id) = event;

        assert_eq!(message_id, None);
        match message {
            ServerMessage::VoiceUserLeft {
                nickname: actual_nickname,
                target: actual_target,
            } => {
                assert_eq!(actual_nickname, nickname);
                assert_eq!(actual_target, target);
            }
            other => panic!("Expected VoiceUserLeft, got {:?}", other),
        }
    }

    fn expect_no_session_message(rx: &mut SessionRx) {
        assert!(
            rx.try_recv().is_err(),
            "session should not receive another voice notification"
        );
    }

    fn drain_session_rx(rx: &mut SessionRx) {
        while rx.try_recv().is_ok() {}
    }

    #[tokio::test]
    async fn test_voice_leave_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_voice_leave(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_voice_leave_not_in_voice() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_voice_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_voice_leave(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceLeaveResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected VoiceLeaveResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_leave_requires_voice_feature() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_voice_leave(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceLeaveResponse { success, error } => {
                assert!(!success);
                let expected =
                    err_voice_feature_not_enabled(crate::handlers::testing::DEFAULT_TEST_LOCALE);
                assert_eq!(error.as_deref(), Some(expected.as_str()));
            }
            _ => panic!("Expected VoiceLeaveResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_leave_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_chat_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::VoiceListen,
                Permission::ChatJoin,
                Permission::ChatCreate,
            ],
            false,
        )
        .await;

        // Join the channel
        handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await; // consume ChatJoinResponse

        // Join voice
        handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await; // consume VoiceJoinResponse

        // Verify we're in voice
        assert!(test_ctx.voice_registry.has_session(session_id).await);

        // Leave voice
        let result = handle_voice_leave(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceLeaveResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected VoiceLeaveResponse, got {:?}", response),
        }

        // Verify we're no longer in voice
        assert!(!test_ctx.voice_registry.has_session(session_id).await);
    }

    #[tokio::test]
    async fn test_voice_leave_user_message_success() {
        let mut test_ctx = create_test_context().await;

        // Login alice with voice_listen permission
        let alice_session = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        // Login bob (target must be online)
        let _bob_session = login_voice_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        // Alice joins voice with bob
        handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await; // consume VoiceJoinResponse

        // Alice leaves voice
        let result = handle_voice_leave(Some(alice_session), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceLeaveResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected VoiceLeaveResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_leave_user_message_notifies_all_regular_user_sessions() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let bob_session = login_voice_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;
        let (_bob_second_session, mut bob_second_rx) =
            add_observer_session_for_existing_regular_user_with_features(
                &mut test_ctx,
                "bob",
                &[Permission::VoiceListen],
                voice_features(),
            )
            .await;

        handle_voice_join(
            "alice".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;
        expect_voice_user_joined(&mut test_ctx.rx, "bob", "bob").await;

        handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;
        expect_voice_user_joined(&mut test_ctx.rx, "alice", "alice").await;
        expect_voice_user_joined(&mut bob_second_rx, "alice", "alice").await;

        handle_voice_leave(Some(alice_session), &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceLeaveResponse {
                success: true,
                error: None,
            }
        ));

        expect_voice_user_left(&mut test_ctx.rx, "alice", "alice").await;
        expect_voice_user_left(&mut bob_second_rx, "alice", "alice").await;
        expect_no_session_message(&mut test_ctx.rx);
        expect_no_session_message(&mut bob_second_rx);
    }

    #[tokio::test]
    async fn test_voice_leave_user_message_notifies_target_not_in_voice() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let _bob_session = login_voice_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;
        expect_voice_user_joined(&mut test_ctx.rx, "alice", "alice").await;

        handle_voice_leave(Some(alice_session), &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceLeaveResponse {
                success: true,
                error: None,
            }
        ));

        expect_voice_user_left(&mut test_ctx.rx, "alice", "alice").await;
        expect_no_session_message(&mut test_ctx.rx);
    }

    #[tokio::test]
    async fn test_voice_leave_user_message_skips_target_without_voice_listen() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let (_bob_session, mut bob_rx) =
            login_observer_user(&mut test_ctx, "bob", "password", &[], voice_features()).await;

        handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));
        expect_no_session_message(&mut bob_rx);

        handle_voice_leave(Some(alice_session), &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceLeaveResponse {
                success: true,
                error: None,
            }
        ));
        expect_no_session_message(&mut bob_rx);
    }

    #[tokio::test]
    async fn test_voice_leave_user_message_keeps_quiet_while_same_nickname_session_remains() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;
        let (alice_second_session, _alice_second_rx) =
            add_observer_session_for_existing_regular_user_with_features(
                &mut test_ctx,
                "alice",
                &[Permission::VoiceListen],
                voice_features(),
            )
            .await;

        let (_bob_session, mut bob_rx) = login_observer_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            voice_features(),
        )
        .await;

        handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));
        expect_voice_user_joined(&mut bob_rx, "alice", "alice").await;

        handle_voice_join(
            "bob".to_string(),
            Some(alice_second_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));
        expect_no_session_message(&mut bob_rx);

        handle_voice_leave(Some(alice_session), &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceLeaveResponse {
                success: true,
                error: None,
            }
        ));
        expect_no_session_message(&mut bob_rx);
    }

    #[tokio::test]
    async fn test_channel_voice_notifications_skip_members_without_voice_feature() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_chat_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::VoiceListen,
                Permission::ChatJoin,
                Permission::ChatCreate,
            ],
            false,
        )
        .await;

        let (bob_session, mut bob_rx) = login_observer_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen, Permission::ChatJoin],
            chat_voice_features(),
        )
        .await;

        let (carol_session, mut carol_rx) = login_observer_user(
            &mut test_ctx,
            "carol",
            "password",
            &[Permission::VoiceListen, Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        for session_id in [alice_session, bob_session, carol_session] {
            handle_chat_join(
                "#general".to_string(),
                Some(session_id),
                &mut test_ctx.handler_context(),
            )
            .await
            .unwrap();
            let response = read_server_message(&mut test_ctx).await;
            assert!(matches!(
                response,
                ServerMessage::ChatJoinResponse { success: true, .. }
            ));
        }

        drain_session_rx(&mut bob_rx);
        drain_session_rx(&mut carol_rx);

        handle_voice_join(
            "#general".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));

        expect_voice_user_joined(&mut bob_rx, "alice", "#general").await;
        assert!(
            carol_rx.try_recv().is_err(),
            "voice_listen without negotiated voice must not receive VoiceUserJoined"
        );

        handle_voice_leave(Some(alice_session), &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceLeaveResponse {
                success: true,
                error: None,
            }
        ));

        expect_voice_user_left(&mut bob_rx, "alice", "#general").await;
        assert!(
            carol_rx.try_recv().is_err(),
            "voice_listen without negotiated voice must not receive VoiceUserLeft"
        );
    }

    #[tokio::test]
    async fn test_voice_leave_twice_fails() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_chat_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::VoiceListen,
                Permission::ChatJoin,
                Permission::ChatCreate,
            ],
            false,
        )
        .await;

        // Join channel and voice
        handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Leave voice first time - should succeed
        handle_voice_leave(Some(session_id), &mut test_ctx.handler_context())
            .await
            .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceLeaveResponse { success, .. } => assert!(success),
            _ => panic!("Expected VoiceLeaveResponse"),
        }

        // Leave voice second time - should fail
        handle_voice_leave(Some(session_id), &mut test_ctx.handler_context())
            .await
            .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceLeaveResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected VoiceLeaveResponse"),
        }
    }
}
