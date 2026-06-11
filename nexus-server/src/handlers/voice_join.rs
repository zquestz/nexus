//! Handler for VoiceJoin command — join voice for a channel or user message

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use uuid::Uuid;

use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;

use crate::constants::{
    FEATURE_VOICE, HANDLER_VOICE_JOIN, LOG_VOICE_JOIN_NOT_LOGGED_IN,
    LOG_VOICE_JOIN_PERMISSION_DENIED,
};

use super::{
    HandlerContext, err_not_logged_in, err_voice_already_joined, err_voice_feature_not_enabled,
    err_voice_invalid_target, err_voice_listen_required, err_voice_not_channel_member,
    err_voice_target_feature_not_enabled, err_voice_target_not_online,
};
use crate::db::Permission;
use crate::voice::VoiceSession;

/// Outcome of the lock-serialized join section, dispatched to a `VoiceJoinResponse`
/// after the `read_user_state` guard drops (so no socket I/O happens under the lock).
enum VoiceJoinOutcome {
    /// Session vanished mid-join; disconnect like other logged-in handlers.
    Disconnect,
    /// Join rejected; send a failure response carrying this error.
    Error(String),
    /// Join succeeded; send a success response.
    Success {
        token: Uuid,
        participants: Vec<String>,
    },
}

/// Join voice. Client target is `"#general"` (must be a member) or `"bob"`
/// (must be online). User-message targets are stored internally as a sorted
/// canonical array `["alice", "bob"]`; clients only see the simple string.
pub async fn handle_voice_join<W>(
    target: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_VOICE_JOIN_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_VOICE_JOIN))
            .await;
    };

    // Serialize the resolve → registry insert → VoiceUserJoined broadcast under
    // read_user_state so a concurrent rename can't land between snapshotting the
    // joiner's nickname and the nickname-keyed registry entry / broadcast. The joiner's
    // nickname keys the DM target pair, the stored VoiceSession, and the VoiceUserJoined
    // nickname — all of which a rename would otherwise leave stale (VoiceUserJoined has
    // no client-side id reconciliation, so a stale nickname is a permanent ghost
    // participant). The read lock forces the rename to fully precede (we re-fetch the
    // new nickname) or fully follow (its update_nickname re-keys our registry entry and
    // its ChatUserRenamed re-keys clients' voiced sets). All work here is in-memory; the
    // VoiceJoinResponse is sent after the guard drops.
    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let Some(current) = ctx.user_manager.get_user_by_session_id(session_id).await else {
            break 'locked VoiceJoinOutcome::Disconnect;
        };

        if !current.has_feature(FEATURE_VOICE) {
            break 'locked VoiceJoinOutcome::Error(err_voice_feature_not_enabled(ctx.locale));
        }

        if !current.has_permission(Permission::VoiceListen) {
            warn!(user = %current.username, ip = %ctx.peer_addr, "{}", LOG_VOICE_JOIN_PERMISSION_DENIED);
            break 'locked VoiceJoinOutcome::Error(err_voice_listen_required(ctx.locale));
        }

        if target.is_empty() {
            break 'locked VoiceJoinOutcome::Error(err_voice_invalid_target(ctx.locale));
        }

        // Fast-path also preserves error precedence: an already-joined
        // session sending a join for a bad target gets
        // `voice_already_joined` instead of leaking target validation.
        if ctx.voice_registry.has_session(session_id).await {
            break 'locked VoiceJoinOutcome::Error(err_voice_already_joined(ctx.locale));
        }

        let is_channel = target.starts_with('#');
        let internal_target = if is_channel {
            if !ctx.channel_manager.is_member(&target, session_id).await {
                break 'locked VoiceJoinOutcome::Error(err_voice_not_channel_member(
                    ctx.locale, &target,
                ));
            }
            vec![target.clone()]
        } else {
            let target_sessions = ctx.user_manager.get_sessions_by_nickname(&target).await;
            if target_sessions.is_empty() {
                break 'locked VoiceJoinOutcome::Error(err_voice_target_not_online(
                    ctx.locale, &target,
                ));
            }

            if !target_sessions
                .iter()
                .any(|session| session.has_feature(FEATURE_VOICE))
            {
                break 'locked VoiceJoinOutcome::Error(err_voice_target_feature_not_enabled(
                    ctx.locale, &target,
                ));
            }

            // Canonical sorted pair [nick1, nick2] from the joiner's current nickname.
            let mut pair = vec![current.nickname.clone(), target.clone()];
            pair.sort_by_key(|a| fold_name(a));
            pair
        };

        let target_key = internal_target.join(":");

        // Current participants before adding the new session.
        let mut participants = ctx.voice_registry.get_participants(&target_key).await;
        let dm_participant_nickname = (!is_channel)
            .then(|| {
                internal_target
                    .iter()
                    .find(|nickname| fold_name(nickname) != fold_name(&current.nickname))
                    .cloned()
            })
            .flatten();

        // Atomic guard: registry.add rejects duplicate session_id and reports
        // `broadcast_joined` so concurrent same-nickname joins can't both broadcast.
        let voice_session =
            VoiceSession::new(current.nickname.clone(), internal_target, session_id);
        let (token, broadcast_joined) = match ctx.voice_registry.add(voice_session).await {
            Some(add_outcome) => (add_outcome.token, add_outcome.broadcast_joined),
            None => break 'locked VoiceJoinOutcome::Error(err_voice_already_joined(ctx.locale)),
        };

        participants.push(current.nickname.clone());
        participants.sort_by_key(|a| fold_name(a));

        if broadcast_joined {
            if is_channel {
                // Notify ALL voice-capable channel members with voice_listen
                // (not just voice participants) so everyone sees who's in voice.
                let members = ctx
                    .channel_manager
                    .get_members(&target)
                    .await
                    .unwrap_or_default();

                for member_session_id in members {
                    if member_session_id == session_id {
                        continue;
                    }

                    if let Some(member) = ctx
                        .user_manager
                        .get_user_by_session_id(member_session_id)
                        .await
                        && member.has_feature(FEATURE_VOICE)
                        && member.has_permission(Permission::VoiceListen)
                    {
                        let join_notification = ServerMessage::VoiceUserJoined {
                            nickname: current.nickname.clone(),
                            target: target.clone(),
                        };
                        let _ = member.tx.send_message(join_notification, None);
                    }
                }
            } else {
                // User messages: notify the other DM participant even if they
                // are not already in voice, so they can see the call invite.
                if let Some(participant_nickname) = &dm_participant_nickname {
                    let join_notification = ServerMessage::VoiceUserJoined {
                        nickname: current.nickname.clone(),
                        target: current.nickname.clone(), // joiner's nickname keys the other's tab
                    };

                    ctx.user_manager
                        .broadcast_to_nickname_with_feature_and_permission(
                            participant_nickname,
                            FEATURE_VOICE,
                            Permission::VoiceListen,
                            &join_notification,
                        )
                        .await;
                }
            }
        }

        VoiceJoinOutcome::Success {
            token,
            participants,
        }
    };

    match outcome {
        VoiceJoinOutcome::Disconnect => {
            ctx.send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_VOICE_JOIN))
                .await
        }
        VoiceJoinOutcome::Error(error) => {
            let response = ServerMessage::VoiceJoinResponse {
                success: false,
                token: None,
                target: None,
                participants: None,
                error: Some(error),
            };
            ctx.send_message(&response).await
        }
        VoiceJoinOutcome::Success {
            token,
            participants,
        } => {
            let response = ServerMessage::VoiceJoinResponse {
                success: true,
                token: Some(token),
                target: Some(target),
                participants: Some(participants),
                error: None,
            };
            ctx.send_message(&response).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{FEATURE_CHAT, FEATURE_VOICE};
    use crate::db::Permission;
    use crate::handlers::chat_join::handle_chat_join;
    use crate::handlers::testing::{
        add_observer_session_for_existing_regular_user,
        add_observer_session_for_existing_regular_user_with_features, create_test_context,
        login_observer_user, login_user, login_user_with_features, read_server_message,
    };
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

    fn expect_no_session_message(rx: &mut SessionRx) {
        assert!(
            rx.try_recv().is_err(),
            "session should not receive another voice notification"
        );
    }

    #[tokio::test]
    async fn test_voice_join_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_voice_join(
            "#general".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_voice_join_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_voice_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_requires_voice_feature() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let result = handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                let expected =
                    err_voice_feature_not_enabled(crate::handlers::testing::DEFAULT_TEST_LOCALE);
                assert_eq!(error.as_deref(), Some(expected.as_str()));
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_empty_target() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let result = handle_voice_join(
            "".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_channel_not_member() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let result = handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().contains("#general"));
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_channel_success() {
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

        handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await; // consume ChatJoinResponse

        let result = handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse {
                success,
                token,
                target,
                participants,
                error,
            } => {
                assert!(success, "Expected success, got error: {:?}", error);
                assert!(token.is_some());
                assert_eq!(target, Some("#general".to_string()));
                assert!(participants.is_some());
                let p = participants.unwrap();
                assert_eq!(p.len(), 1);
                assert!(p.contains(&"alice".to_string()));
                assert!(error.is_none());
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_already_in_voice() {
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

        let result = handle_voice_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_user_message_target_offline() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let result = handle_voice_join(
            "bob".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.unwrap().contains("bob"));
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_user_message_target_without_voice_feature_reports_capability_error() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_voice_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let _bob_session = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let result = handle_voice_join(
            "bob".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse { success, error, .. } => {
                assert!(!success);
                let expected = err_voice_target_feature_not_enabled(
                    crate::handlers::testing::DEFAULT_TEST_LOCALE,
                    "bob",
                );
                assert_eq!(error.as_deref(), Some(expected.as_str()));
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_user_message_success() {
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

        let result = handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse {
                success,
                token,
                target,
                participants,
                error,
            } => {
                assert!(success);
                assert!(token.is_some());
                // Client sees "bob" as the target
                assert_eq!(target, Some("bob".to_string()));
                assert!(participants.is_some());
                assert!(error.is_none());
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }

        let participants = test_ctx.voice_registry.get_participants("alice:bob").await;
        assert_eq!(participants.len(), 1);
        assert!(participants.contains(&"alice".to_string()));
    }

    #[tokio::test]
    async fn test_voice_join_user_message_notifies_all_regular_user_sessions() {
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
        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));
        expect_voice_user_joined(&mut test_ctx.rx, "bob", "bob").await;

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

        expect_voice_user_joined(&mut test_ctx.rx, "alice", "alice").await;
        expect_voice_user_joined(&mut bob_second_rx, "alice", "alice").await;
        expect_no_session_message(&mut test_ctx.rx);
        expect_no_session_message(&mut bob_second_rx);
    }

    #[tokio::test]
    async fn test_voice_join_user_message_notifies_target_not_in_voice() {
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

        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));

        expect_voice_user_joined(&mut test_ctx.rx, "alice", "alice").await;
        expect_no_session_message(&mut test_ctx.rx);
    }

    #[tokio::test]
    async fn test_voice_join_user_message_skips_target_without_voice_listen() {
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
    }

    #[tokio::test]
    async fn test_voice_join_user_message_second_same_nickname_session_does_not_rebroadcast() {
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
    }

    #[tokio::test]
    async fn test_voice_join_user_message_skips_non_voice_sessions_for_same_nickname() {
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
        let (_bob_no_voice_session, mut bob_no_voice_rx) =
            add_observer_session_for_existing_regular_user(
                &mut test_ctx,
                "bob",
                &[Permission::VoiceListen],
            )
            .await;

        handle_voice_join(
            "alice".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        assert!(matches!(
            response,
            ServerMessage::VoiceJoinResponse { success: true, .. }
        ));
        expect_voice_user_joined(&mut test_ctx.rx, "bob", "bob").await;

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

        expect_voice_user_joined(&mut test_ctx.rx, "alice", "alice").await;
        assert!(bob_no_voice_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_voice_join_user_message_both_users_same_session() {
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

        handle_voice_join(
            "bob".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        handle_voice_join(
            "alice".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::VoiceJoinResponse {
                success,
                target,
                participants,
                ..
            } => {
                assert!(success);
                // Bob sees "alice" as the target
                assert_eq!(target, Some("alice".to_string()));
                let p = participants.unwrap();
                assert!(p.contains(&"alice".to_string()));
                assert!(p.contains(&"bob".to_string()));
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }

        let participants = test_ctx.voice_registry.get_participants("alice:bob").await;
        assert_eq!(participants.len(), 2);
        assert!(participants.contains(&"alice".to_string()));
        assert!(participants.contains(&"bob".to_string()));
    }
}
