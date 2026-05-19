//! Handler for VoiceJoin command - join voice chat for a channel or user message

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::ServerMessage;

use crate::constants::{
    HANDLER_VOICE_JOIN, LOG_VOICE_JOIN_NOT_LOGGED_IN, LOG_VOICE_JOIN_PERMISSION_DENIED,
};

use super::{
    HandlerContext, err_not_logged_in, err_voice_already_joined, err_voice_invalid_target,
    err_voice_listen_required, err_voice_not_channel_member, err_voice_target_not_online,
};
use crate::db::Permission;
use crate::voice::VoiceSession;

/// Handle VoiceJoin command - join voice chat for a channel or user message
///
/// Target format (from client):
/// - Channel: `"#general"` (user must be a member)
/// - User message: `"bob"` (target must be online)
///
/// Internally, the server converts user message targets to a canonical array
/// format `["alice", "bob"]` (sorted) for registry lookups. Clients only see
/// simple string targets.
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

    let user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_VOICE_JOIN))
                .await;
        }
    };

    if !user.has_permission(Permission::VoiceListen) {
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_VOICE_JOIN_PERMISSION_DENIED);
        let response = ServerMessage::VoiceJoinResponse {
            success: false,
            token: None,
            target: None,
            participants: None,
            error: Some(err_voice_listen_required(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    if target.is_empty() {
        let response = ServerMessage::VoiceJoinResponse {
            success: false,
            token: None,
            target: None,
            participants: None,
            error: Some(err_voice_invalid_target(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Fast-path also preserves error precedence: an already-joined
    // session sending a join for a bad target gets
    // `voice_already_joined` instead of leaking target validation.
    if ctx.voice_registry.has_session(session_id).await {
        let response = ServerMessage::VoiceJoinResponse {
            success: false,
            token: None,
            target: None,
            participants: None,
            error: Some(err_voice_already_joined(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    let is_channel = target.starts_with('#');

    let client_target = target.clone();

    let internal_target = if is_channel {
        if !ctx.channel_manager.is_member(&target, session_id).await {
            let response = ServerMessage::VoiceJoinResponse {
                success: false,
                token: None,
                target: None,
                participants: None,
                error: Some(err_voice_not_channel_member(ctx.locale, &target)),
            };
            return ctx.send_message(&response).await;
        }
        vec![target]
    } else {
        let target_online = ctx
            .user_manager
            .get_session_by_nickname(&target)
            .await
            .is_some();

        if !target_online {
            let response = ServerMessage::VoiceJoinResponse {
                success: false,
                token: None,
                target: None,
                participants: None,
                error: Some(err_voice_target_not_online(ctx.locale, &target)),
            };
            return ctx.send_message(&response).await;
        }

        // Build canonical sorted array [nick1, nick2]
        let mut pair = vec![user.nickname.clone(), target];
        pair.sort_by_key(|a| a.to_lowercase());
        pair
    };

    let target_key = internal_target.join(":");

    // Get current participants before adding the new session
    let mut participants = ctx.voice_registry.get_participants(&target_key).await;

    // Atomic guard: registry.add rejects duplicate session_id and
    // reports `broadcast_joined` so concurrent same-nickname joins
    // can't both broadcast.
    let voice_session = VoiceSession::new(
        user.nickname.clone(),
        internal_target,
        session_id,
        ctx.peer_addr.ip(),
    );
    let (token, broadcast_joined) = match ctx.voice_registry.add(voice_session).await {
        Some(outcome) => (outcome.token, outcome.broadcast_joined),
        None => {
            let response = ServerMessage::VoiceJoinResponse {
                success: false,
                token: None,
                target: None,
                participants: None,
                error: Some(err_voice_already_joined(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    };

    // Add self to participants list (sorted by lowercase)
    participants.push(user.nickname.clone());
    participants.sort_by_key(|a| a.to_lowercase());

    if broadcast_joined {
        if is_channel {
            // For channels: broadcast to ALL channel members with voice_listen permission
            // (not just voice participants) so everyone can see who's in voice
            let channel_name = client_target.clone();
            let members = ctx
                .channel_manager
                .get_members(&channel_name)
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
                    && member.has_permission(Permission::VoiceListen)
                {
                    let join_notification = ServerMessage::VoiceUserJoined {
                        nickname: user.nickname.clone(),
                        target: channel_name.clone(),
                    };
                    let _ = member.tx.send((join_notification, None));
                }
            }
        } else {
            // For user messages: only notify the other participant
            for participant_nickname in &participants {
                if participant_nickname.to_lowercase() == user.nickname.to_lowercase() {
                    continue;
                }

                let join_notification = ServerMessage::VoiceUserJoined {
                    nickname: user.nickname.clone(),
                    target: user.nickname.clone(), // Send joiner's nickname to other participant
                };

                if let Some(participant_user) = ctx
                    .user_manager
                    .get_session_by_nickname(participant_nickname)
                    .await
                {
                    let _ = participant_user.tx.send((join_notification, None));
                }
            }
        }
    }

    let response = ServerMessage::VoiceJoinResponse {
        success: true,
        token: Some(token),
        target: Some(client_target),
        participants: Some(participants),
        error: None,
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::FEATURE_CHAT;
    use crate::db::Permission;
    use crate::handlers::chat_join::handle_chat_join;
    use crate::handlers::testing::{
        create_test_context, login_user, login_user_with_features, read_server_message,
    };

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

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

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
    async fn test_voice_join_empty_target() {
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
                assert!(error.unwrap().contains("#general"));
            }
            _ => panic!("Expected VoiceJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_voice_join_channel_success() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::VoiceListen,
                Permission::ChatJoin,
                Permission::ChatCreate,
            ],
            false,
            vec![FEATURE_CHAT.to_string()],
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

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::VoiceListen,
                Permission::ChatJoin,
                Permission::ChatCreate,
            ],
            false,
            vec![FEATURE_CHAT.to_string()],
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

        let session_id = login_user(
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
    async fn test_voice_join_user_message_success() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_user(
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
    async fn test_voice_join_user_message_both_users_same_session() {
        let mut test_ctx = create_test_context().await;

        let alice_session = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        let bob_session = login_user(
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
