//! Updates a channel topic.

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, warn};

use crate::constants::{
    HANDLER_CHAT_TOPIC_UPDATE, LOG_CHAT_TOPIC_DB_ERROR, LOG_CHAT_TOPIC_NOT_LOGGED_IN,
    LOG_CHAT_TOPIC_PERMISSION_DENIED,
};

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, ChatTopicError};

use super::{
    HandlerContext, channel_error_to_message, err_channel_not_found, err_chat_feature_not_enabled,
    err_database, err_not_logged_in, err_permission_denied, err_topic_contains_newlines,
    err_topic_invalid_characters, err_topic_too_long,
};
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

enum TopicUpdateOutcome {
    Disconnect,
    Queued,
    Send(ServerMessage),
}

pub async fn handle_chat_topic_update<W>(
    topic: String,
    channel: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_CHAT_TOPIC_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(
                &err_not_logged_in(ctx.locale),
                Some(HANDLER_CHAT_TOPIC_UPDATE),
            )
            .await;
    };

    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_CHAT_TOPIC_UPDATE),
                )
                .await;
        }
    };

    if !user.has_feature(FEATURE_CHAT) {
        let response = ServerMessage::ChatTopicUpdateResponse {
            success: false,
            error: Some(err_chat_feature_not_enabled(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    if !user.has_permission(Permission::ChatTopicEdit) {
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_CHAT_TOPIC_PERMISSION_DENIED);
        let response = ServerMessage::ChatTopicUpdateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_chat_topic(&topic) {
        let error_msg = match e {
            ChatTopicError::TooLong => {
                err_topic_too_long(ctx.locale, validators::MAX_CHAT_TOPIC_LENGTH)
            }
            ChatTopicError::ContainsNewlines => err_topic_contains_newlines(ctx.locale),
            ChatTopicError::InvalidCharacters => err_topic_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::ChatTopicUpdateResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    if let Err(e) = validators::validate_channel(&channel) {
        let response = ServerMessage::ChatTopicUpdateResponse {
            success: false,
            error: Some(channel_error_to_message(e, ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let Some(current) = ctx.user_manager.get_user_by_session_id(id).await else {
            break 'locked TopicUpdateOutcome::Disconnect;
        };

        // Non-members get "not found" so secret-channel existence isn't leaked.
        if !ctx.channel_manager.is_member(&channel, id).await {
            break 'locked TopicUpdateOutcome::Send(ServerMessage::ChatTopicUpdateResponse {
                success: false,
                error: Some(err_channel_not_found(ctx.locale, &channel)),
            });
        }

        // Channel manager handles persistence for persistent channels.
        let (topic_value, set_by) = if topic.is_empty() {
            (None, None)
        } else {
            (Some(topic.clone()), Some(current.username.clone()))
        };

        match ctx
            .channel_manager
            .set_topic(&channel, topic_value, set_by)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                // Channel deleted between the membership check and here.
                break 'locked TopicUpdateOutcome::Send(ServerMessage::ChatTopicUpdateResponse {
                    success: false,
                    error: Some(err_channel_not_found(ctx.locale, &channel)),
                });
            }
            Err(e) => {
                error!(user = %current.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_CHAT_TOPIC_DB_ERROR);
                break 'locked TopicUpdateOutcome::Send(ServerMessage::ChatTopicUpdateResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                });
            }
        }

        let members = ctx
            .channel_manager
            .get_members(&channel)
            .await
            .unwrap_or_default();

        let topic_message = ServerMessage::ChatUpdated {
            channel,
            topic: Some(topic.clone()),
            topic_set_by: Some(current.nickname.clone()),
            secret: None,
            secret_set_by: None,
        };

        ctx.send_message_via_channel(&ServerMessage::ChatTopicUpdateResponse {
            success: true,
            error: None,
        })?;

        // Broadcast only to members with the chat feature and ChatTopic permission.
        for member_session_id in members {
            if let Some(member) = ctx
                .user_manager
                .get_user_by_session_id(member_session_id)
                .await
                && member.has_feature(FEATURE_CHAT)
                && member.has_permission(Permission::ChatTopic)
            {
                ctx.user_manager
                    .send_to_session(member_session_id, topic_message.clone())
                    .await;
            }
        }

        TopicUpdateOutcome::Queued
    };

    match outcome {
        TopicUpdateOutcome::Disconnect => {
            ctx.send_error_and_disconnect(
                &err_not_logged_in(ctx.locale),
                Some(HANDLER_CHAT_TOPIC_UPDATE),
            )
            .await
        }
        TopicUpdateOutcome::Queued => Ok(()),
        TopicUpdateOutcome::Send(response) => ctx.send_message(&response).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::Channel;
    use crate::channels::JoinPolicy;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, TestContext, create_test_context, login_user,
        login_user_with_features, read_server_message,
    };
    use nexus_common::validators::DEFAULT_CHANNEL;

    async fn read_queued_server_message(test_ctx: &mut TestContext) -> ServerMessage {
        test_ctx
            .rx
            .recv()
            .await
            .expect("should receive queued server message")
            .0
    }

    #[tokio::test]
    async fn test_chattopic_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_chat_topic_update(
            "Test topic".to_string(),
            "#general".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Should disconnect on unauthenticated request"
        );
    }

    #[tokio::test]
    async fn test_chattopic_requires_channel() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_topic_update(
            "Test topic".to_string(),
            "".to_string(), // Empty channel name
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().to_lowercase().contains("channel")); // Error about channel
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Login user WITH ChatTopicEdit permission but WITHOUT chat feature
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_topic_update(
            "Test topic".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(
                    error.unwrap(),
                    err_chat_feature_not_enabled(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login user without ChatTopicEdit permission but with chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_topic_update(
            "Test topic".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(error.unwrap(), err_permission_denied(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_too_long() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        // Create topic that's too long (> 256 chars)
        let long_topic = "a".repeat(257);

        let result = handle_chat_topic_update(
            long_topic,
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("256"),
                    "Error should mention max length: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains("Topic cannot exceed"),
                    "Error should be about topic length: {}",
                    error_msg
                );
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_at_limit() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        // Create topic at exactly 256 chars
        let topic = "a".repeat(256);

        let result = handle_chat_topic_update(
            topic.clone(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_queued_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_empty_allowed() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_topic_update(
            "".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_queued_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_uses_renamed_nickname_after_final_state_gate() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatTopic, Permission::ChatTopicEdit],
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

        {
            let mut ctx = test_ctx.handler_context();
            let handler = handle_chat_topic_update(
                "renamed topic".to_string(),
                "#general".to_string(),
                Some(session_id),
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

        match read_queued_server_message(&mut test_ctx).await {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            other => panic!("Expected ChatTopicUpdateResponse, got {other:?}"),
        }

        match read_queued_server_message(&mut test_ctx).await {
            ServerMessage::ChatUpdated { topic_set_by, .. } => {
                assert_eq!(topic_set_by, Some("alicia".to_string()));
            }
            other => panic!("Expected ChatUpdated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_chattopic_newlines_rejected() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        // Test with \n
        let result = handle_chat_topic_update(
            "Topic with\nnewline".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(
                    error.unwrap(),
                    err_topic_contains_newlines(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login admin user (admins automatically have all permissions) with chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join a channel
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_topic_update(
            "Admin topic".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_queued_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_to_nonexistent_channel() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_topic_update(
            "Test topic".to_string(),
            "#nonexistent".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(
                    error.unwrap(),
                    err_channel_not_found(DEFAULT_TEST_LOCALE, "#nonexistent")
                );
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_not_member() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Create channel but don't join it
        let _ = test_ctx
            .channel_manager
            .join("#general", 999, JoinPolicy::CreateIfMissing)
            .await;

        let result = handle_chat_topic_update(
            "Test topic".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(
                    error.unwrap(),
                    err_channel_not_found(DEFAULT_TEST_LOCALE, "#general")
                );
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_ephemeral_channel_in_memory_only() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join #general channel (ephemeral)
        test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_topic_update(
            "General topic".to_string(),
            "#general".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_queued_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }

        // Verify topic was stored in channel manager
        let channel = test_ctx
            .channel_manager
            .get_channel("#general")
            .await
            .unwrap();
        assert_eq!(channel.topic, Some("General topic".to_string()));
    }

    #[tokio::test]
    async fn test_chattopic_persistent_channel_saves_to_db() {
        let mut test_ctx = create_test_context().await;

        // Initialize default channel as a persistent channel
        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![Channel::new(DEFAULT_CHANNEL.to_string())])
            .await;

        // Login user with ChatTopicEdit permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join the persistent channel
        test_ctx
            .channel_manager
            .join(DEFAULT_CHANNEL, session_id, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        let result = handle_chat_topic_update(
            "Welcome to Nexus!".to_string(),
            DEFAULT_CHANNEL.to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_queued_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }

        // Verify topic was stored in channel manager
        let channel = test_ctx
            .channel_manager
            .get_channel(DEFAULT_CHANNEL)
            .await
            .unwrap();
        assert_eq!(channel.topic, Some("Welcome to Nexus!".to_string()));

        // Verify topic was persisted to database
        let db_settings = test_ctx
            .db
            .channels
            .get_channel_settings(DEFAULT_CHANNEL)
            .await
            .unwrap();
        assert!(db_settings.is_some());
        let settings = db_settings.unwrap();
        assert_eq!(settings.topic, "Welcome to Nexus!");
        assert_eq!(settings.topic_set_by, "testuser");
    }
}
