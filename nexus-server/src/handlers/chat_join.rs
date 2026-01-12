//! Handler for ChatJoin command - join or create a channel

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, MAX_CHANNELS_PER_USER};

use super::{
    HandlerContext, channel_error_to_message, err_authentication, err_channel_already_member,
    err_channel_limit_exceeded, err_not_logged_in, err_permission_denied,
};
use crate::channels::JoinError;
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

/// Helper to create an error response with all fields set to None
fn error_response(error_msg: String) -> ServerMessage {
    ServerMessage::ChatJoinResponse {
        success: false,
        error: Some(error_msg),
        channel: None,
        topic: None,
        topic_set_by: None,
        secret: None,
        members: None,
    }
}

/// Handle ChatJoin command - join or create a channel
pub async fn handle_chat_join<W>(
    channel: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("ChatJoin request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("ChatJoin"))
            .await;
    };

    // Validate channel name
    if let Err(e) = validators::validate_channel(&channel) {
        return ctx
            .send_message(&error_response(channel_error_to_message(e, ctx.locale)))
            .await;
    }

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("ChatJoin"))
                .await;
        }
    };

    // Check chat feature
    if !user.has_feature(FEATURE_CHAT) {
        return ctx
            .send_message(&error_response(err_permission_denied(ctx.locale)))
            .await;
    }

    // Check ChatJoin permission
    if !user.has_permission(Permission::ChatJoin) {
        eprintln!(
            "ChatJoin from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_message(&error_response(err_permission_denied(ctx.locale)))
            .await;
    }

    // Join the channel (or create if it doesn't exist)
    // This enforces MAX_CHANNELS_PER_USER internally
    let result = match ctx.channel_manager.join(&channel, session_id).await {
        Ok(result) => {
            // If already a member, return an error
            if result.already_member {
                return ctx
                    .send_message(&error_response(err_channel_already_member(
                        ctx.locale, &channel,
                    )))
                    .await;
            }
            result
        }
        Err(JoinError::TooManyChannels) => {
            return ctx
                .send_message(&error_response(err_channel_limit_exceeded(
                    ctx.locale,
                    MAX_CHANNELS_PER_USER,
                )))
                .await;
        }
    };

    // Get sorted nicknames for all channel members (session-based),
    // then deduplicate (member counts are nicknames, not sessions).
    let mut member_nicknames = ctx
        .user_manager
        .get_nicknames_for_sessions(&result.member_session_ids)
        .await;

    member_nicknames.dedup_by_key(|n| n.to_lowercase());

    // Broadcast ChatUserJoined to other channel members (not to the joining user),
    // but ONLY if this nickname was not already present in the channel via another session.
    //
    // "Member counts are nicknames" (deduped), so join/leave announcements should only fire
    // when the first session for a nickname joins and when the last session leaves.
    let nickname_session_ids = ctx
        .user_manager
        .get_session_ids_for_nickname(&user.nickname)
        .await;

    let nickname_already_present_in_channel = nickname_session_ids
        .iter()
        .any(|&sid| sid != session_id && result.member_session_ids.contains(&sid));

    if !nickname_already_present_in_channel {
        let join_broadcast = ServerMessage::ChatUserJoined {
            channel: channel.clone(),
            nickname: user.nickname.clone(),
            is_admin: user.is_admin,
            is_shared: user.is_shared,
        };

        for member_session_id in &result.member_session_ids {
            if *member_session_id != session_id {
                ctx.user_manager
                    .send_to_session(*member_session_id, join_broadcast.clone())
                    .await;
            }
        }
    }

    // Send success response with full channel data
    let response = ServerMessage::ChatJoinResponse {
        success: true,
        error: None,
        channel: Some(channel),
        topic: result.topic,
        topic_set_by: result.topic_set_by,
        secret: Some(result.secret),
        members: Some(member_nicknames),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{
        create_test_context, login_user, login_user_with_features, read_server_message,
    };

    /// Dummy session ID used when creating channels directly via channel_manager
    /// for test setup (e.g., to pre-populate a channel with a topic)
    const DUMMY_SESSION_ID: u32 = 999;

    #[tokio::test]
    async fn test_chat_join_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_chat_join(
            "#general".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_chat_join_validates_channel_name() {
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

        // Test missing # prefix
        let result = handle_chat_join(
            "general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_success() {
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

        let result = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Single response with full channel data
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success,
                error,
                channel,
                members,
                ..
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(channel, Some("#general".to_string()));
                let members = members.unwrap();
                assert_eq!(members.len(), 1);
                assert!(members.contains(&"alice".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }

        // Verify channel was created
        assert!(test_ctx.channel_manager.exists("#general").await);
    }

    #[tokio::test]
    async fn test_chat_join_already_member_is_error() {
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

        // Join once - should succeed
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Join again - should fail with "already member" error
        let result = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success,
                error,
                channel,
                members,
                ..
            } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().contains("#general"));
                // Error responses should not include channel data
                assert!(channel.is_none());
                assert!(members.is_none());
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login user WITHOUT ChatJoin permission but WITH chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Login user WITH ChatJoin permission but WITHOUT chat feature
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin],
            false,
        )
        .await;

        let result = handle_chat_join(
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_includes_topic_info() {
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

        // Create channel with topic via channel manager directly
        // Use a dummy session to create the channel and set topic
        // Don't remove the dummy - leaving would delete the ephemeral channel
        let channel_name = "#topical";
        test_ctx
            .channel_manager
            .join(channel_name, DUMMY_SESSION_ID)
            .await
            .unwrap();
        test_ctx
            .channel_manager
            .set_topic(
                channel_name,
                Some("Test topic".to_string()),
                Some("admin".to_string()),
            )
            .await
            .unwrap();

        // Now join and verify topic info is included
        let result = handle_chat_join(
            channel_name.to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success,
                topic,
                topic_set_by,
                ..
            } => {
                assert!(success);
                assert_eq!(topic, Some("Test topic".to_string()));
                assert_eq!(topic_set_by, Some("admin".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_limit_exceeded() {
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

        // Join MAX_CHANNELS_PER_USER channels
        // Join MAX_CHANNELS_PER_USER channels (all should succeed)
        for i in 0..MAX_CHANNELS_PER_USER {
            let result = test_ctx
                .channel_manager
                .join(&format!("#channel{}", i), session_id)
                .await;
            assert!(result.is_ok(), "Should be able to join channel {}", i);
        }

        // Try to join one more channel - should fail
        let result = handle_chat_join(
            "#onemore".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success,
                error,
                channel,
                ..
            } => {
                assert!(!success, "Should fail due to channel limit");
                assert!(error.is_some(), "Should have error message");
                assert!(
                    error.unwrap().contains(&MAX_CHANNELS_PER_USER.to_string()),
                    "Error should mention the limit"
                );
                assert!(channel.is_none(), "Should not have channel on error");
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }
}
