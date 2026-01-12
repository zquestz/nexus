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

    // Broadcast ChatUserLeft only when this nickname becomes absent from the channel
    // (nickname-based membership; multiple sessions may map to the same nickname).
    let nickname_present_elsewhere = ctx
        .user_manager
        .sessions_contain_nickname(&result.remaining_member_session_ids, &user.nickname, None)
        .await;

    if !nickname_present_elsewhere {
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
        TestContext, create_test_context, login_user_with_features, read_server_message,
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

    // =========================================================================
    // Multi-session leave tests
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
    async fn test_chat_leave_broadcasts_user_left_to_remaining_members() {
        let mut test_ctx = create_test_context().await;

        // Login alice with chat permissions
        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice joins #general
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Login bob with chat permissions
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Bob joins #general
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Drain the ChatUserJoined message from bob joining
        let _ = test_ctx.rx.recv().await;

        // Alice leaves #general - bob should receive ChatUserLeft
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify bob received ChatUserLeft for alice
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserLeft, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_no_broadcast_when_nickname_still_present() {
        let mut test_ctx = create_test_context().await;

        // Login alice session 1 with chat permissions
        let alice_session1 = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Alice session 1 joins #general
        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Add alice session 2 and join #general
        let alice_session2 = add_second_session(
            &mut test_ctx,
            "alice",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Login bob to observe
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Drain any pending messages (ChatUserJoined for bob)
        while test_ctx.rx.try_recv().is_ok() {}

        // Alice session 1 leaves - should NOT broadcast ChatUserLeft
        // because alice session 2 is still in the channel
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify no ChatUserLeft was sent
        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive ChatUserLeft when nickname still present via another session"
        );
    }

    #[tokio::test]
    async fn test_chat_leave_last_session_triggers_broadcast() {
        let mut test_ctx = create_test_context().await;

        // Login alice with two sessions
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

        // Both sessions join #general
        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session1)
            .await;
        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session2)
            .await;

        // Login bob to observe
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Drain any pending messages
        while test_ctx.rx.try_recv().is_ok() {}

        // Alice session 1 leaves - no broadcast (session 2 still there)
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify no ChatUserLeft yet
        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Should NOT broadcast when first session leaves"
        );

        // Alice session 2 leaves - NOW should broadcast ChatUserLeft
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify ChatUserLeft was sent
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserLeft, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_shared_account_different_nicknames() {
        let mut test_ctx = create_test_context().await;

        // Add two shared account sessions with different nicknames
        let guest1_session = add_shared_session(
            &mut test_ctx,
            "shared_acct",
            "Guest1",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let guest2_session = add_shared_session(
            &mut test_ctx,
            "shared_acct",
            "Guest2",
            &[Permission::ChatJoin],
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        // Both join #general
        let _ = test_ctx
            .channel_manager
            .join("#general", guest1_session)
            .await;
        let _ = test_ctx
            .channel_manager
            .join("#general", guest2_session)
            .await;

        // Login bob to observe
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Drain any pending messages
        while test_ctx.rx.try_recv().is_ok() {}

        // Guest1 leaves - should broadcast ChatUserLeft for "Guest1"
        // because Guest2 has a different nickname
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(guest1_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify ChatUserLeft was sent for Guest1
        // Note: Messages go to both remaining members (Guest2 and bob) via same tx channel
        // so we may receive multiple copies - just check we got the right nickname
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "Guest1");
            }
            _ => panic!("Expected ChatUserLeft for Guest1, got {:?}", msg),
        }

        // Drain any duplicate messages (same message sent to multiple members)
        while test_ctx.rx.try_recv().is_ok() {}

        // Guest2 leaves - should also broadcast ChatUserLeft for "Guest2"
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(guest2_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify ChatUserLeft was sent for Guest2
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "Guest2");
            }
            _ => panic!("Expected ChatUserLeft for Guest2, got {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_chat_leave_regular_user_multiple_sessions_no_broadcast_until_last() {
        let mut test_ctx = create_test_context().await;

        // Regular user with two sessions (same nickname)
        // This tests that no broadcast happens until the last session leaves
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

        // Both sessions join #general
        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session1)
            .await;
        let _ = test_ctx
            .channel_manager
            .join("#general", alice_session2)
            .await;

        // Login bob to observe
        let bob_session = login_user_with_features(
            &mut test_ctx,
            "bob",
            "password2",
            &[Permission::ChatJoin],
            false,
            vec![FEATURE_CHAT.to_string()],
        )
        .await;

        let _ = handle_chat_join(
            "#general".to_string(),
            Some(bob_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatJoinResponse

        // Drain any pending messages
        while test_ctx.rx.try_recv().is_ok() {}

        // Alice session 1 leaves - should NOT broadcast
        // because Alice session 2 has the same nickname
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session1),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify no ChatUserLeft was sent
        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Should NOT broadcast when another session has same nickname"
        );

        // Alice session 2 leaves - NOW should broadcast
        let _ = handle_chat_leave(
            "#general".to_string(),
            Some(alice_session2),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // ChatLeaveResponse

        // Verify ChatUserLeft was sent
        let (msg, _) = test_ctx.rx.recv().await.expect("Should receive message");
        match msg {
            ServerMessage::ChatUserLeft { channel, nickname } => {
                assert_eq!(channel, "#general");
                assert_eq!(nickname, "alice");
            }
            _ => panic!("Expected ChatUserLeft, got {:?}", msg),
        }
    }
}
