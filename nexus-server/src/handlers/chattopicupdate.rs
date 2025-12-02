//! Handler for ChatTopicUpdate command

use super::{
    HandlerContext, err_database, err_not_logged_in, err_permission_denied,
    err_topic_contains_newlines, err_topic_too_long,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Maximum topic length in characters
const MAX_TOPIC_LENGTH: usize = 256;

/// Handle ChatTopicUpdate command
pub async fn handle_chattopicupdate(
    topic: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // 1. Check if user is logged in (check authentication first)
    let id = match session_id {
        Some(id) => id,
        None => {
            return ctx
                .send_error(&err_not_logged_in(ctx.locale), Some("ChatTopicUpdate"))
                .await;
        }
    };

    // 2. Validate topic length (before expensive user lookup)
    if topic.len() > MAX_TOPIC_LENGTH {
        return ctx
            .send_error(
                &err_topic_too_long(ctx.locale, MAX_TOPIC_LENGTH),
                Some("ChatTopicUpdate"),
            )
            .await;
    }

    // 3. Validate topic does not contain newlines
    if topic.contains('\n') || topic.contains('\r') {
        return ctx
            .send_error(
                &err_topic_contains_newlines(ctx.locale),
                Some("ChatTopicUpdate"),
            )
            .await;
    }

    // 4. Get user from session
    let user = match ctx.user_manager.get_user_by_session_id(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error(&err_not_logged_in(ctx.locale), Some("ChatTopicUpdate"))
                .await;
        }
    };

    // 5. Check if user has ChatTopicEdit permission
    let has_permission = match ctx
        .db
        .users
        .has_permission(user.db_user_id, Permission::ChatTopicEdit)
        .await
    {
        Ok(has_perm) => has_perm,
        Err(e) => {
            eprintln!("Database error checking ChatTopicUpdate permission: {}", e);
            return ctx
                .send_error(&err_database(ctx.locale), Some("ChatTopicUpdate"))
                .await;
        }
    };

    if !has_permission {
        eprintln!(
            "Permission denied: User '{}' (IP: {}) attempted ChatTopicEdit without permission",
            user.username, ctx.peer_addr
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("ChatTopicUpdate"))
            .await;
    }

    // 6. Save topic to database (with username who set it)
    if let Err(e) = ctx.db.config.set_topic(&topic, &user.username).await {
        eprintln!("Database error setting topic: {}", e);
        return ctx
            .send_error(&err_database(ctx.locale), Some("ChatTopicUpdate"))
            .await;
    }

    // 7. Broadcast ChatTopic to all users with ChatTopic permission
    ctx.user_manager
        .broadcast_to_permission(
            ServerMessage::ChatTopic {
                topic: topic.clone(),
                username: user.username.clone(),
            },
            &ctx.db.users,
            Permission::ChatTopic,
        )
        .await;

    // 8. Send success response to updater
    ctx.send_message(&ServerMessage::ChatTopicUpdateResponse {
        success: true,
        error: None,
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permission;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, login_user, read_server_message,
    };
    use nexus_common::protocol::ServerMessage;

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
