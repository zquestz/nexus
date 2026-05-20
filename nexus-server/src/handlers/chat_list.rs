//! Handler for ChatList command

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use crate::constants::{
    HANDLER_CHAT_LIST, LOG_CHAT_LIST_NOT_LOGGED_IN, LOG_CHAT_LIST_PERMISSION_DENIED,
};

use nexus_common::protocol::{ChannelInfo, ServerMessage};

use super::{
    HandlerContext, err_chat_feature_not_enabled, err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_CHAT;
use crate::db::Permission;

pub async fn handle_chat_list<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_CHAT_LIST_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_LIST))
            .await;
    };

    let user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_CHAT_LIST))
                .await;
        }
    };

    if !user.has_feature(FEATURE_CHAT) {
        let response = ServerMessage::ChatListResponse {
            success: false,
            error: Some(err_chat_feature_not_enabled(ctx.locale)),
            channels: None,
        };
        return ctx.send_message(&response).await;
    }

    if !user.has_permission(Permission::ChatList) {
        warn!(user = %user.username, ip = %ctx.peer_addr, "{}", LOG_CHAT_LIST_PERMISSION_DENIED);
        let response = ServerMessage::ChatListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            channels: None,
        };
        return ctx.send_message(&response).await;
    }

    let channel_list = ctx.channel_manager.list(session_id, user.is_admin).await;

    let channels: Vec<ChannelInfo> = channel_list
        .into_iter()
        .map(|info| ChannelInfo {
            name: info.name,
            topic: info.topic,
            member_count: info.member_count,
            secret: info.secret,
        })
        .collect();

    let response = ServerMessage::ChatListResponse {
        success: true,
        error: None,
        channels: Some(channels),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        create_test_context, login_user_with_features, read_server_message,
    };

    #[tokio::test]
    async fn test_chat_list_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_chat_list(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_chat_list_requires_permission() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[], // No permissions
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatListResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatListResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_list_requires_feature() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatList],
            false,
            vec![], // No chat feature
        )
        .await;

        let result = handle_chat_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatListResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected ChatListResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_list_with_feature() {
        use crate::channels::Channel;
        use nexus_common::validators::DEFAULT_CHANNEL;

        let mut test_ctx = create_test_context().await;

        // Initialize persistent channel (simulating server startup)
        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![Channel::new(DEFAULT_CHANNEL.to_string())])
            .await;

        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatList],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let result = handle_chat_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatListResponse {
                success,
                error,
                channels,
            } => {
                assert!(success);
                assert!(error.is_none());
                // Should have at least the default persistent channel
                let channels = channels.unwrap();
                assert!(!channels.is_empty());
                assert!(channels.iter().any(|c| c.name == DEFAULT_CHANNEL));
            }
            _ => panic!("Expected ChatListResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_list_hides_secret_channels() {
        use crate::db::Permission;
        use crate::handlers::chat_join::handle_chat_join;

        let mut test_ctx = create_test_context().await;

        let session_id1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::ChatJoin,
                Permission::ChatCreate,
                Permission::ChatSecret,
                Permission::ChatList,
            ],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Create and join a channel, then make it secret
        let _ = handle_chat_join(
            "#secret".to_string(),
            Some(session_id1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse (includes channel data)

        test_ctx
            .channel_manager
            .set_secret("#secret", true)
            .await
            .unwrap();

        let session_id2 = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::ChatJoin, Permission::ChatList],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // User 2 should not see #secret in list
        let result = handle_chat_list(Some(session_id2), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatListResponse { channels, .. } => {
                let channels = channels.unwrap();
                assert!(!channels.iter().any(|c| c.name == "#secret"));
            }
            _ => panic!("Expected ChatListResponse, got {:?}", response),
        }

        // User 1 should see #secret (is member)
        let result = handle_chat_list(Some(session_id1), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatListResponse { channels, .. } => {
                let channels = channels.unwrap();
                assert!(channels.iter().any(|c| c.name == "#secret"));
            }
            _ => panic!("Expected ChatListResponse, got {:?}", response),
        }
    }
}
