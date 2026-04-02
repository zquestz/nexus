//! Handler for VoiceLeave command - leave current voice session

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use crate::constants::LOG_VOICE_LEAVE_NOT_LOGGED_IN;

use nexus_common::protocol::ServerMessage;

use super::{HandlerContext, err_authentication, err_not_logged_in, err_voice_not_joined};
use crate::voice::send_voice_leave_notifications;

/// Handle VoiceLeave command - leave current voice session
///
/// Removes the user from their active voice session and broadcasts
/// VoiceUserLeft to remaining participants.
pub async fn handle_voice_leave<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_VOICE_LEAVE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("VoiceLeave"))
            .await;
    };

    // Verify session still exists (user might have disconnected)
    if ctx
        .user_manager
        .get_user_by_session_id(session_id)
        .await
        .is_none()
    {
        return ctx
            .send_error_and_disconnect(&err_authentication(ctx.locale), Some("VoiceLeave"))
            .await;
    }

    // Remove the voice session and get notification info
    let Some(info) = ctx.voice_registry.remove_by_session_id(session_id).await else {
        // User was not in a voice session
        let response = ServerMessage::VoiceLeaveResponse {
            success: false,
            error: Some(err_voice_not_joined(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    };

    // Broadcast VoiceUserLeft to remaining participants (not to self - this is explicit leave)
    send_voice_leave_notifications(&info, None, ctx.user_manager, ctx.channel_manager).await;

    // Send success response
    let response = ServerMessage::VoiceLeaveResponse {
        success: true,
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
    use crate::handlers::voice_join::handle_voice_join;

    #[tokio::test]
    async fn test_voice_leave_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_voice_leave(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_voice_leave_not_in_voice() {
        let mut test_ctx = create_test_context().await;

        // Login user
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

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
    async fn test_voice_leave_success() {
        let mut test_ctx = create_test_context().await;

        // Login user with voice_listen and chat permissions
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
        let alice_session = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;

        // Login bob (target must be online)
        let _bob_session = login_user(
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
    async fn test_voice_leave_twice_fails() {
        let mut test_ctx = create_test_context().await;

        // Login user with voice_listen and chat permissions
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
