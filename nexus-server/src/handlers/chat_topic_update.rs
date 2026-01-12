//! Handler for ChatTopicUpdate command

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, ChatTopicError};

use super::{
    HandlerContext, channel_error_to_message, err_authentication, err_channel_not_found,
    err_chat_feature_not_enabled, err_database, err_not_logged_in, err_permission_denied,
    err_topic_contains_newlines, err_topic_invalid_characters, err_topic_too_long,
};
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

/// Handle ChatTopicUpdate command
pub async fn handle_chat_topic_update<W>(
    topic: String,
    channel: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(id) = session_id else {
        eprintln!("ChatTopicUpdate from {} without login", ctx.peer_addr);
        return ctx
            .send_error(&err_not_logged_in(ctx.locale), Some("ChatTopicUpdate"))
            .await;
    };

    // Validate topic format
    if let Err(e) = validators::validate_chat_topic(&topic) {
        let error_msg = match e {
            ChatTopicError::TooLong => {
                err_topic_too_long(ctx.locale, validators::MAX_CHAT_TOPIC_LENGTH)
            }
            ChatTopicError::ContainsNewlines => err_topic_contains_newlines(ctx.locale),
            ChatTopicError::InvalidCharacters => err_topic_invalid_characters(ctx.locale),
        };
        return ctx.send_error(&error_msg, Some("ChatTopicUpdate")).await;
    }

    // Validate channel name
    if let Err(e) = validators::validate_channel(&channel) {
        return ctx
            .send_error(
                &channel_error_to_message(e, ctx.locale),
                Some("ChatTopicUpdate"),
            )
            .await;
    }

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error(&err_authentication(ctx.locale), Some("ChatTopicUpdate"))
                .await;
        }
    };

    // Check chat feature
    if !user.has_feature(FEATURE_CHAT) {
        return ctx
            .send_error(
                &err_chat_feature_not_enabled(ctx.locale),
                Some("ChatTopicUpdate"),
            )
            .await;
    }

    // Check ChatTopicEdit permission (uses cached permissions, admin bypass built-in)
    if !user.has_permission(Permission::ChatTopicEdit) {
        eprintln!(
            "ChatTopicUpdate from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("ChatTopicUpdate"))
            .await;
    }

    // Check if user is a member of the channel
    // For security, always return "not found" to non-members to avoid leaking
    // existence of secret channels
    if !ctx.channel_manager.is_member(&channel, id).await {
        return ctx
            .send_error(
                &err_channel_not_found(ctx.locale, &channel),
                Some("ChatTopicUpdate"),
            )
            .await;
    }

    // Update topic in channel manager (handles persistence for persistent channels)
    let (topic_value, set_by) = if topic.is_empty() {
        (None, None)
    } else {
        (Some(topic.clone()), Some(user.username.clone()))
    };

    match ctx
        .channel_manager
        .set_topic(&channel, topic_value, set_by)
        .await
    {
        Ok(true) => {} // Success, channel exists
        Ok(false) => {
            // Channel doesn't exist (race condition - was deleted after membership check)
            return ctx
                .send_error(
                    &err_channel_not_found(ctx.locale, &channel),
                    Some("ChatTopicUpdate"),
                )
                .await;
        }
        Err(e) => {
            eprintln!("Database error setting topic: {}", e);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ChatTopicUpdate"))
                .await;
        }
    }

    // Get channel members for broadcast
    let members = ctx
        .channel_manager
        .get_members(&channel)
        .await
        .unwrap_or_default();

    // Build the topic update message
    let topic_message = ServerMessage::ChatUpdated {
        channel,
        topic: Some(topic.clone()),
        topic_set_by: Some(user.nickname.clone()),
        secret: None,
        secret_set_by: None,
    };

    // Broadcast ChatUpdated to all channel members with chat feature and ChatTopic permission
    for member_session_id in members {
        if let Some(member) = ctx
            .user_manager
            .get_user_by_session_id(member_session_id)
            .await
        {
            // Check if member has chat feature and topic permission
            if member.has_feature(FEATURE_CHAT) && member.has_permission(Permission::ChatTopic) {
                ctx.user_manager
                    .send_to_session(member_session_id, topic_message.clone())
                    .await;
            }
        }
    }

    // Send success response to updater
    ctx.send_message(&ServerMessage::ChatTopicUpdateResponse {
        success: true,
        error: None,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::Channel;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, login_user_with_features,
        read_server_message,
    };
    use nexus_common::validators::DEFAULT_CHANNEL;

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

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_not_logged_in(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_requires_channel() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatTopicEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
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
            ServerMessage::Error { message, command } => {
                assert!(message.to_lowercase().contains("channel")); // Error about channel
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
            .join("#general", session_id)
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
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_chat_feature_not_enabled(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
            .join("#general", session_id)
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
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_permission_denied(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
            .join("#general", session_id)
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
            ServerMessage::Error { message, command } => {
                assert!(
                    message.contains("256"),
                    "Error should mention max length: {}",
                    message
                );
                assert!(
                    message.contains("Topic cannot exceed"),
                    "Error should be about topic length: {}",
                    message
                );
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
            .join("#general", session_id)
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

        let response = read_server_message(&mut test_ctx).await;
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
            .join("#general", session_id)
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
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
            .join("#general", session_id)
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
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_topic_contains_newlines(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
            .join("#general", session_id)
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

        let response = read_server_message(&mut test_ctx).await;
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
            ServerMessage::Error { message, command } => {
                assert_eq!(
                    message,
                    err_channel_not_found(DEFAULT_TEST_LOCALE, "#nonexistent")
                );
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
        let _ = test_ctx.channel_manager.join("#general", 999).await;

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
            ServerMessage::Error { message, command } => {
                assert_eq!(
                    message,
                    err_channel_not_found(DEFAULT_TEST_LOCALE, "#general")
                );
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
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
            .join("#general", session_id)
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

        let response = read_server_message(&mut test_ctx).await;
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
            .join(DEFAULT_CHANNEL, session_id)
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

        let response = read_server_message(&mut test_ctx).await;
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
