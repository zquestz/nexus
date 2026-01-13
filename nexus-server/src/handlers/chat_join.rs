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
use crate::i18n::t;

/// Error message for missing ChatCreate permission when creating a channel
fn err_permission_denied_chat_create(locale: &str) -> String {
    t(locale, "err-permission-denied-chat-create")
}

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

    // Check ChatJoin permission (required for both joining and creating)
    if !user.has_permission(Permission::ChatJoin) {
        eprintln!(
            "ChatJoin from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_message(&error_response(err_permission_denied(ctx.locale)))
            .await;
    }

    // Check if channel exists - if not, also require ChatCreate permission.
    // Note: There's a benign TOCTOU race here - the channel could be created by another
    // user between our exists() check and join() call. This is acceptable because if
    // another user creates it first, we just join the existing channel (which requires
    // only ChatJoin, not ChatCreate). No privilege escalation is possible.
    let channel_exists = ctx.channel_manager.exists(&channel).await;
    if !channel_exists && !user.has_permission(Permission::ChatCreate) {
        eprintln!(
            "ChatJoin from {} (user: {}) trying to create channel without ChatCreate permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_message(&error_response(err_permission_denied_chat_create(
                ctx.locale,
            )))
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

    // Build member list as unique nicknames (member counts are nicknames, not sessions).
    let member_nicknames = ctx
        .user_manager
        .get_unique_nicknames_for_sessions(&result.member_session_ids)
        .await;

    // Broadcast ChatUserJoined only when this nickname becomes present in the channel
    // (nickname-based membership; multiple sessions may map to the same nickname).
    let nickname_present_elsewhere = ctx
        .user_manager
        .sessions_contain_nickname(&result.member_session_ids, &user.nickname, Some(session_id))
        .await;

    if !nickname_present_elsewhere {
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
        TestContext, create_test_context, login_user, login_user_with_features, read_server_message,
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

        // Login user with ChatJoin and ChatCreate permissions and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
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

        // Login user with ChatJoin and ChatCreate permissions and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
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
    async fn test_chat_join_requires_chat_create_for_new_channel() {
        let mut test_ctx = create_test_context().await;

        // Login user with ChatJoin permission but WITHOUT ChatCreate permission
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin], // No ChatCreate
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Try to create a new channel - should fail without ChatCreate
        let result = handle_chat_join(
            "#newchannel".to_string(),
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
                // Should mention they can join but not create
                assert!(error.unwrap().contains("create"));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }

        // Verify channel was NOT created
        assert!(!test_ctx.channel_manager.exists("#newchannel").await);
    }

    #[tokio::test]
    async fn test_chat_join_existing_channel_without_chat_create() {
        let mut test_ctx = create_test_context().await;

        // First, create the channel using channel_manager directly
        test_ctx
            .channel_manager
            .join("#existing", DUMMY_SESSION_ID)
            .await
            .unwrap();

        // Login user with ChatJoin permission but WITHOUT ChatCreate permission
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin], // No ChatCreate - but channel already exists
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Try to join existing channel - should succeed even without ChatCreate
        let result = handle_chat_join(
            "#existing".to_string(),
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
                assert!(success);
                assert!(error.is_none());
                assert_eq!(channel, Some("#existing".to_string()));
                let members = members.unwrap();
                assert!(members.contains(&"alice".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_requires_feature() {
        let mut test_ctx = create_test_context().await;

        // Login user WITH ChatJoin and ChatCreate permissions but WITHOUT chat feature
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
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

        // Login user with ChatJoin and ChatCreate permissions and chat feature
        let session_id = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
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

    // =========================================================================
    // Multi-session join tests
    // =========================================================================

    /// Helper to add a second session for the same user to UserManager
    async fn add_second_session(
        test_ctx: &mut TestContext,
        username: &str,
        permissions: &[Permission],
        features: Vec<String>,
    ) -> u32 {
        use crate::users::user::NewSessionParams;
        use std::collections::HashSet;

        let perms: HashSet<Permission> = permissions.iter().copied().collect();

        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: 1,
                username: username.to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features,
                locale: "en".to_string(),
                avatar: None,
                nickname: username.to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add second session")
    }

    /// Helper to add a shared account session with custom nickname
    async fn add_shared_session(
        test_ctx: &mut TestContext,
        account_username: &str,
        nickname: &str,
        permissions: &[Permission],
        features: Vec<String>,
    ) -> u32 {
        use crate::users::user::NewSessionParams;
        use std::collections::HashSet;

        let perms: HashSet<Permission> = permissions.iter().copied().collect();

        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: 1,
                username: account_username.to_string(),
                is_admin: false,
                is_shared: true,
                permissions: perms,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features,
                locale: "en".to_string(),
                avatar: None,
                nickname: nickname.to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add shared session")
    }

    #[tokio::test]
    async fn test_chat_join_broadcasts_user_joined_to_other_members() {
        let mut test_ctx = create_test_context().await;

        // Login alice with chat permissions (including ChatCreate to create the channel)
        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice joins #general (creates it)
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Login bob with chat permissions (only ChatJoin needed to join existing channel)
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Bob joins #general - alice should receive ChatUserJoined
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Read bob's ChatJoinResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success, members, ..
            } => {
                assert!(success);
                let members = members.unwrap();
                assert_eq!(members.len(), 2);
                assert!(members.contains(&"alice".to_string()));
                assert!(members.contains(&"bob".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }

        // Check that alice received ChatUserJoined (via her tx channel)
        // Note: In real scenario, alice's session would receive this via her channel
        // Here we verify the message was sent by checking the rx channel
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserJoined {
                channel,
                nickname,
                is_admin,
                is_shared,
            } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "bob");
                assert!(!is_admin);
                assert!(!is_shared);
            }
            _ => panic!("Expected ChatUserJoined, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_join_no_broadcast_when_nickname_already_present() {
        let mut test_ctx = create_test_context().await;

        // Login alice session 1 with chat permissions (including ChatCreate to create the channel)
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice session 1 joins #general (creates it)
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Add second session for alice (only ChatJoin needed to join existing)
        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice session 2 joins #general - should NOT broadcast ChatUserJoined
        // because nickname "alice" is already present via session 1
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Read alice session 2's ChatJoinResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success, members, ..
            } => {
                assert!(success);
                // Members should still show only one "alice" (deduplicated)
                let members = members.unwrap();
                assert_eq!(members.len(), 1);
                assert!(members.contains(&"alice".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }

        // Verify no ChatUserJoined was sent (rx should be empty or timeout)
        // Use try_recv to check without blocking
        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive ChatUserJoined when nickname already present"
        );
    }

    #[tokio::test]
    async fn test_chat_join_shared_account_different_nicknames_broadcast() {
        let mut test_ctx = create_test_context().await;

        // Add shared session "Guest1" with chat permissions (including ChatCreate to create the channel)
        let guest1_session = add_shared_session(
            &mut test_ctx,
            "guest",
            "Guest1",
            &[Permission::ChatJoin, Permission::ChatCreate],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Guest1 joins #general (creates it)
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(guest1_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Add another shared session "Guest2" with chat permissions (only ChatJoin needed to join existing)
        let guest2_session = add_shared_session(
            &mut test_ctx,
            "guest",
            "Guest2",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Guest2 joins #general - should broadcast ChatUserJoined
        // because nickname "Guest2" is different from "Guest1"
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(guest2_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Read Guest2's ChatJoinResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success, members, ..
            } => {
                assert!(success);
                let members = members.unwrap();
                assert_eq!(members.len(), 2);
                assert!(members.contains(&"Guest1".to_string()));
                assert!(members.contains(&"Guest2".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }

        // Verify ChatUserJoined was sent for Guest2
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserJoined {
                channel,
                nickname,
                is_shared,
                ..
            } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "Guest2");
                assert!(is_shared);
            }
            _ => panic!("Expected ChatUserJoined, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_join_member_list_deduplicates_nicknames() {
        let mut test_ctx = create_test_context().await;

        // Login alice with 3 sessions
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let alice_session3 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // All three sessions join #general
        for session in [alice_session1, alice_session2, alice_session3] {
            let _ = test_ctx.channel_manager.join("#general", session).await;
        }

        // Login bob
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Bob joins - member list should show alice only once
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::ChatJoinResponse {
                success, members, ..
            } => {
                assert!(success);
                let members = members.unwrap();
                // Should be deduplicated: alice appears once, plus bob
                assert_eq!(members.len(), 2);
                assert!(members.contains(&"alice".to_string()));
                assert!(members.contains(&"bob".to_string()));
            }
            _ => panic!("Expected ChatJoinResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chat_join_first_session_triggers_broadcast_second_does_not() {
        let mut test_ctx = create_test_context().await;

        // Login bob first (will be in channel when alice joins, needs ChatCreate to create it)
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin, Permission::ChatCreate],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Bob joins #general (creates it)
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Login alice session 1 with chat permissions (only ChatJoin needed to join existing)
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice session 1 joins - should trigger ChatUserJoined
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Verify bob received ChatUserJoined for alice
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserJoined { nickname, .. } => {
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserJoined for alice, got {:?}", msg),
        }

        // Add alice session 2
        // Add second session for alice (only ChatJoin needed to join existing)
        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice session 2 joins - should NOT trigger ChatUserJoined
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Verify no additional ChatUserJoined was sent
        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive second ChatUserJoined for alice"
        );
    }
}
