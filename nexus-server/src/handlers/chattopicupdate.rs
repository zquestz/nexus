//! Handler for ChatTopicUpdate command

use std::io;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, ChatTopicError};

use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
    err_topic_contains_newlines, err_topic_invalid_characters, err_topic_too_long,
};
use crate::db::Permission;

/// Handle ChatTopicUpdate command
pub async fn handle_chattopicupdate(
    topic: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
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

    // Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error(&err_authentication(ctx.locale), Some("ChatTopicUpdate"))
                .await;
        }
    };

    // Check ChatTopicEdit permission (use is_admin from UserManager to avoid DB lookup for admins)
    let has_permission = if user.is_admin {
        true
    } else {
        match ctx
            .db
            .users
            .has_permission(user.db_user_id, Permission::ChatTopicEdit)
            .await
        {
            Ok(has_perm) => has_perm,
            Err(e) => {
                eprintln!("ChatTopicUpdate permission check error: {}", e);
                return ctx
                    .send_error(&err_database(ctx.locale), Some("ChatTopicUpdate"))
                    .await;
            }
        }
    };

    if !has_permission {
        eprintln!(
            "ChatTopicUpdate from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("ChatTopicUpdate"))
            .await;
    }

    // Save topic to database (with username who set it)
    if let Err(e) = ctx.db.config.set_topic(&topic, &user.username).await {
        eprintln!("Database error setting topic: {}", e);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ChatTopicUpdate"))
            .await;
    }

    // Broadcast ChatTopic to all users with ChatTopic permission
    ctx.user_manager
        .broadcast_to_permission(
            ServerMessage::ChatTopic {
                topic,
                username: user.username.clone(),
            },
            &ctx.db.users,
            Permission::ChatTopic,
        )
        .await;

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
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
    };

    #[tokio::test]
    async fn test_chattopic_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_chattopicupdate(
            "Test topic".to_string(),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, command } => {
                assert_eq!(message, err_not_logged_in(DEFAULT_TEST_LOCALE));
                assert_eq!(command, Some("ChatTopicUpdate".to_string()));
            }
            _ => panic!("Expected Error message, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_chattopic_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login user without ChatTopic permission
        let session_id = login_user(&mut test_ctx, "testuser", "password", &[], false).await;

        let result = handle_chattopicupdate(
            "Test topic".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
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

        // Login user with ChatTopicEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
        )
        .await;

        // Create topic that's too long (> 256 chars)
        let long_topic = "a".repeat(257);

        let result = handle_chattopicupdate(
            long_topic,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
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

        // Login user with ChatTopicEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "testuser",
            "password",
            &[Permission::ChatTopicEdit],
            false,
        )
        .await;

        // Create topic at exactly 256 chars
        let topic = "a".repeat(256);

        let result = handle_chattopicupdate(
            topic.clone(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }

        // Verify topic was saved
        let saved_topic = test_ctx.db.config.get_topic().await.unwrap();
        assert_eq!(saved_topic.topic, topic);
        assert_eq!(saved_topic.set_by, "testuser");
    }

    #[tokio::test]
    async fn test_chattopic_empty_allowed() {
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

        let result = handle_chattopicupdate(
            "".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }

        // Verify empty topic was saved
        let saved_topic = test_ctx.db.config.get_topic().await.unwrap();
        assert_eq!(saved_topic.topic, "");
        assert_eq!(saved_topic.set_by, "testuser");
    }

    #[tokio::test]
    async fn test_chattopic_newlines_rejected() {
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

        // Test with \n
        let result = handle_chattopicupdate(
            "Topic with\nnewline".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
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

        // Login admin user (admins automatically have all permissions)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_chattopicupdate(
            "Admin topic".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected ChatTopicUpdateResponse, got {:?}", response),
        }
    }
}
