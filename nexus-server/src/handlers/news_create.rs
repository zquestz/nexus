//! NewsCreate message handler - Creates a new news item

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{NewsAction, NewsItem, ServerMessage};
use nexus_common::validators::{self, NewsBodyError, NewsImageError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_database, err_news_body_invalid_characters, err_news_body_too_long,
    err_news_empty_content, err_news_image_invalid_format, err_news_image_too_large,
    err_news_image_unsupported_type, err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_NEWS;
use crate::db::Permission;

/// Handle a news create request
pub async fn handle_news_create<W>(
    body: Option<String>,
    image: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("NewsCreate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("NewsCreate"))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            // Session not found - likely a race condition, not a security event
            let response = ServerMessage::NewsCreateResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check NewsCreate permission
    if !requesting_user.has_permission(Permission::NewsCreate) {
        eprintln!(
            "NewsCreate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::NewsCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Normalize empty strings to None
    let body = body.filter(|s| !s.trim().is_empty());
    let image = image.filter(|s| !s.is_empty());

    // Validate that at least one of body or image is provided
    if body.is_none() && image.is_none() {
        let response = ServerMessage::NewsCreateResponse {
            success: false,
            error: Some(err_news_empty_content(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate body if provided
    if let Some(ref body_text) = body
        && let Err(e) = validators::validate_news_body(body_text)
    {
        let error_msg = match e {
            NewsBodyError::TooLong => {
                err_news_body_too_long(ctx.locale, validators::MAX_NEWS_BODY_LENGTH)
            }
            NewsBodyError::InvalidCharacters => err_news_body_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::NewsCreateResponse {
            success: false,
            error: Some(error_msg),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate image if provided
    if let Some(ref image_data) = image
        && let Err(e) = validators::validate_news_image(image_data)
    {
        let error_msg = match e {
            NewsImageError::TooLarge => err_news_image_too_large(ctx.locale),
            NewsImageError::InvalidFormat => err_news_image_invalid_format(ctx.locale),
            NewsImageError::UnsupportedType => err_news_image_unsupported_type(ctx.locale),
        };
        let response = ServerMessage::NewsCreateResponse {
            success: false,
            error: Some(error_msg),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Create news in database
    let news_record = match ctx
        .db
        .news
        .create_news(
            body.as_deref(),
            image.as_deref(),
            requesting_user.db_user_id,
        )
        .await
    {
        Ok(record) => record,
        Err(e) => {
            eprintln!("Database error creating news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsCreate"))
                .await;
        }
    };

    // Convert to protocol format
    let news = NewsItem {
        id: news_record.id,
        body: news_record.body,
        image: news_record.image,
        author: news_record.author_username,
        author_is_admin: news_record.author_is_admin,
        created_at: news_record.created_at,
        updated_at: news_record.updated_at,
    };

    // Send success response
    let response = ServerMessage::NewsCreateResponse {
        success: true,
        error: None,
        news: Some(news.clone()),
    };
    ctx.send_message(&response).await?;

    // Broadcast NewsUpdated to users with news feature and NewsList permission
    let broadcast = ServerMessage::NewsUpdated {
        action: NewsAction::Created,
        id: news.id,
    };
    ctx.user_manager
        .broadcast_to_feature(FEATURE_NEWS, broadcast, &ctx.db.users, Permission::NewsList)
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_news_create_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_create(
            Some("Test post".to_string()),
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_create_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without NewsCreate permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_news_create(
            Some("Test post".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsCreateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_create_empty_content() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let result = handle_news_create(
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_empty_content(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsCreateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_create_empty_strings_treated_as_none() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        // Empty strings should be treated as None
        let result = handle_news_create(
            Some("".to_string()),
            Some("".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_empty_content(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsCreateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_create_with_body() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let result = handle_news_create(
            Some("# Hello\n\nThis is news!".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("# Hello\n\nThis is news!".to_string()));
                assert!(news.image.is_none());
                assert_eq!(news.author, "alice");
                assert!(!news.author_is_admin);
                assert!(news.updated_at.is_none());
            }
            _ => panic!("Expected NewsCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_create_with_image() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let result = handle_news_create(
            None,
            Some("data:image/png;base64,iVBORw0KGgo=".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert!(news.body.is_none());
                assert_eq!(
                    news.image,
                    Some("data:image/png;base64,iVBORw0KGgo=".to_string())
                );
            }
            _ => panic!("Expected NewsCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_create_with_both() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let result = handle_news_create(
            Some("Check out this image!".to_string()),
            Some("data:image/png;base64,iVBORw0KGgo=".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("Check out this image!".to_string()));
                assert_eq!(
                    news.image,
                    Some("data:image/png;base64,iVBORw0KGgo=".to_string())
                );
            }
            _ => panic!("Expected NewsCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_create_body_too_long() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let long_body = "a".repeat(validators::MAX_NEWS_BODY_LENGTH + 1);
        let result = handle_news_create(
            Some(long_body),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_news_body_too_long(
                        DEFAULT_TEST_LOCALE,
                        validators::MAX_NEWS_BODY_LENGTH
                    ))
                );
            }
            _ => panic!("Expected NewsCreateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_create_invalid_image_format() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let result = handle_news_create(
            None,
            Some("not a data uri".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_news_image_invalid_format(DEFAULT_TEST_LOCALE))
                );
            }
            _ => panic!("Expected NewsCreateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_create_unsupported_image_type() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsCreate permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let result = handle_news_create(
            None,
            Some("data:image/gif;base64,R0lGODlh".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_news_image_unsupported_type(DEFAULT_TEST_LOCALE))
                );
            }
            _ => panic!("Expected NewsCreateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_create_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin (no explicit permissions needed)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_news_create(
            Some("Admin news".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsCreateResponse { success, news, .. } => {
                assert!(success);
                let news = news.unwrap();
                assert_eq!(news.author, "admin");
                assert!(news.author_is_admin);
            }
            _ => panic!("Expected NewsCreateResponse"),
        }
    }
}
