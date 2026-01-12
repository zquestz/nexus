//! Handler for ChatLeave command - leave a channel

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators;

use super::{
    HandlerContext, channel_error_to_message, err_authentication, err_channel_not_found,
    err_chat_feature_not_enabled, err_not_logged_in,
};

use crate::constants::FEATURE_CHAT;

/// Handle ChatLeave command - leave a channel
pub async fn handle_chat_leave<W>(
    channel: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("ChatLeave request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("ChatLeave"))
            .await;
    };

    // Validate channel name
    if let Err(e) = validators::validate_channel(&channel) {
        let response = ServerMessage::ChatLeaveResponse {
            success: false,
            error: Some(channel_error_to_message(e, ctx.locale)),
            channel: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("ChatLeave"))
                .await;
        }
    };

    // Check chat feature
    if !user.has_feature(FEATURE_CHAT) {
        let response = ServerMessage::ChatLeaveResponse {
            success: false,
            error: Some(err_chat_feature_not_enabled(ctx.locale)),
            channel: None,
        };
        return ctx.send_message(&response).await;
    }

    // Leave the channel
    // For security/consistency, return "not found" if not a member to avoid
    // leaking existence of secret channels
    let Some(result) = ctx.channel_manager.leave(&channel, session_id).await else {
        // User wasn't a member or channel doesn't exist
        let response = ServerMessage::ChatLeaveResponse {
            success: false,
            error: Some(err_channel_not_found(ctx.locale, &channel)),
            channel: None,
        };
        return ctx.send_message(&response).await;
    };

    // Broadcast ChatUserLeft to remaining channel members, but ONLY if this nickname
    // no longer has any sessions in the channel.
    //
    // "Member counts are nicknames" (deduped), so join/leave announcements should only fire
    // when the first session for a nickname joins and when the last session leaves.
    let nickname_session_ids = ctx
        .user_manager
        .get_session_ids_for_nickname(&user.nickname)
        .await;

    let nickname_still_present_in_channel = nickname_session_ids
        .iter()
        .any(|&sid| result.remaining_member_session_ids.contains(&sid));

    if !nickname_still_present_in_channel {
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

    // Note: Channel membership is session-based.

    // Send success response
    let response = ServerMessage::ChatLeaveResponse {
        success: true,
        error: None,
        channel: Some(channel),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::chat_join::handle_chat_join;
    use crate::handlers::testing::{
        create_test_context, login_user_with_features, read_server_message,
    };

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

        // Login user with chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Test missing # prefix
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

        // Login user with ChatJoin permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // First join the channel
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse (includes channel data)

        // Now leave the channel
        let result = handle_chat_leave(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
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

        // Verify channel was deleted (empty)
        assert!(!test_ctx.channel_manager.exists("#general").await);
    }

    #[tokio::test]
    async fn test_chat_leave_not_member() {
        let mut test_ctx = create_test_context().await;

        // Login user with chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Try to leave a channel we never joined
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
}
