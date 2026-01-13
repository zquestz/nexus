//! Handler for ChatSecret command - toggle secret mode on a channel

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators;

use super::{
    HandlerContext, channel_error_to_message, err_authentication, err_channel_not_found,
    err_chat_feature_not_enabled, err_database, err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

/// Handle ChatSecret command - toggle secret mode on a channel
pub async fn handle_chat_secret<W>(
    channel: String,
    secret: bool,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("ChatSecret request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("ChatSecret"))
            .await;
    };

    // Validate channel name
    if let Err(e) = validators::validate_channel(&channel) {
        let response = ServerMessage::ChatSecretResponse {
            success: false,
            error: Some(channel_error_to_message(e, ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("ChatSecret"))
                .await;
        }
    };

    // Check chat feature
    if !user.has_feature(FEATURE_CHAT) {
        let response = ServerMessage::ChatSecretResponse {
            success: false,
            error: Some(err_chat_feature_not_enabled(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Check ChatSecret permission
    if !user.has_permission(Permission::ChatSecret) {
        eprintln!(
            "ChatSecret from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        let response = ServerMessage::ChatSecretResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Verify user is a member of the channel and set secret mode
    // For security, always return "not found" to non-members to avoid leaking
    // existence of secret channels
    if !ctx.channel_manager.is_member(&channel, session_id).await {
        let response = ServerMessage::ChatSecretResponse {
            success: false,
            error: Some(err_channel_not_found(ctx.locale, &channel)),
        };
        return ctx.send_message(&response).await;
    }

    // Set the secret mode (ChannelManager handles persistence for persistent channels)
    match ctx.channel_manager.set_secret(&channel, secret).await {
        Ok(true) => {} // Success, channel exists
        Ok(false) => {
            // Channel doesn't exist (race condition - was deleted after membership check)
            let response = ServerMessage::ChatSecretResponse {
                success: false,
                error: Some(err_channel_not_found(ctx.locale, &channel)),
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error setting channel secret mode: {}", e);
            let response = ServerMessage::ChatSecretResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    }

    // Send success response to the requester
    let response = ServerMessage::ChatSecretResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Broadcast ChatUpdated to all channel members
    let members = ctx
        .channel_manager
        .get_members(&channel)
        .await
        .unwrap_or_default();

    let update_message = ServerMessage::ChatUpdated {
        channel: channel.clone(),
        topic: None,
        topic_set_by: None,
        secret: Some(secret),
        secret_set_by: Some(user.nickname.clone()),
    };

    for member_session_id in members {
        if let Some(member) = ctx
            .user_manager
            .get_user_by_session_id(member_session_id)
            .await
        {
            // Only send to members with chat feature
            if member.has_feature(FEATURE_CHAT) {
                ctx.user_manager
                    .send_to_session(member_session_id, update_message.clone())
                    .await;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::chat_join::handle_chat_join;
    use crate::handlers::testing::{
        create_test_context, login_user, login_user_with_features, read_server_message,
    };

    #[tokio::test]
    async fn test_chat_secret_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_chat_secret(
            "#general".to_string(),
            true,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_chat_secret_validates_channel_name() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatSecret permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatSecret],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Test missing # prefix
        let result = handle_chat_secret(
            "general".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_secret_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login user WITHOUT ChatSecret permission but WITH chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_secret(
            "#general".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_secret_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Login user WITH ChatSecret permission but WITHOUT chat feature
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatSecret],
            false,
        )
        .await;

        let result = handle_chat_secret(
            "#general".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_secret_success() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatJoin and ChatSecret permissions and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::ChatJoin,
                Permission::ChatCreate,
                Permission::ChatSecret,
            ],
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

        // Set channel to secret
        let result = handle_chat_secret(
            "#general".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }

        // Verify channel is now secret
        let channel = test_ctx
            .channel_manager
            .get_channel("#general")
            .await
            .unwrap();
        assert!(channel.secret);
    }

    #[tokio::test]
    async fn test_chat_secret_not_member() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatSecret permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::ChatJoin,
                Permission::ChatCreate,
                Permission::ChatSecret,
            ],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Create the channel with another user first
        let _ = test_ctx.channel_manager.join("#general", 999).await;

        // Try to set secret on channel we're not a member of
        let result = handle_chat_secret(
            "#general".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_secret_channel_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatSecret permission and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatSecret],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Try to set secret on non-existent channel
        let result = handle_chat_secret(
            "#nonexistent".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_secret_persistent_channel_saves_to_db() {
        use crate::channels::Channel;
        use nexus_common::validators::DEFAULT_CHANNEL;

        let mut test_ctx = create_test_context().await;

        // Initialize default channel as a persistent channel
        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![Channel::new(DEFAULT_CHANNEL.to_string())])
            .await;

        // Login user with ChatSecret permission and chat feature
        // Login user with ChatJoin, ChatCreate, and ChatSecret permissions
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::ChatJoin,
                Permission::ChatCreate,
                Permission::ChatSecret,
            ],
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

        // Set channel to secret
        let result = handle_chat_secret(
            DEFAULT_CHANNEL.to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }

        // Verify secret flag is persisted in database
        let settings = test_ctx
            .db
            .channels
            .get_channel_settings(DEFAULT_CHANNEL)
            .await
            .unwrap()
            .unwrap();
        assert!(settings.secret, "Secret flag should be persisted to DB");

        // Verify channel manager also has secret set
        let channel = test_ctx
            .channel_manager
            .get_channel(DEFAULT_CHANNEL)
            .await
            .unwrap();
        assert!(channel.secret, "Secret flag should be set in memory");
    }

    #[tokio::test]
    async fn test_chat_secret_ephemeral_channel_not_persisted() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatJoin and ChatSecret permissions and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::ChatJoin,
                Permission::ChatCreate,
                Permission::ChatSecret,
            ],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Join an ephemeral channel (not persistent)
        let _ = handle_chat_join(
            "#ephemeral".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Set channel to secret
        let result = handle_chat_secret(
            "#ephemeral".to_string(),
            true,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatSecretResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatSecretResponse, got {:?}", response),
        }

        // Verify channel manager has secret set in memory
        let channel = test_ctx
            .channel_manager
            .get_channel("#ephemeral")
            .await
            .unwrap();
        assert!(channel.secret, "Secret flag should be set in memory");

        // Verify ephemeral channel is NOT persisted to database
        let settings = test_ctx
            .db
            .channels
            .get_channel_settings("#ephemeral")
            .await
            .unwrap();
        assert!(
            settings.is_none(),
            "Ephemeral channel should not have DB settings"
        );
    }
}
