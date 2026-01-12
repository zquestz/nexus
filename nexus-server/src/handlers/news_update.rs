//! NewsUpdate message handler - Updates an existing news item

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{NewsAction, NewsItem, ServerMessage};
use nexus_common::validators::{self, NewsBodyError, NewsImageError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_cannot_edit_admin_news, err_database, err_news_body_invalid_characters,
    err_news_body_too_long, err_news_empty_content, err_news_image_invalid_format,
    err_news_image_too_large, err_news_image_unsupported_type, err_news_not_found,
    err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_NEWS;
use crate::db::Permission;

/// Handle a news update request
pub async fn handle_news_update<W>(
    id: i64,
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
        eprintln!("NewsUpdate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("NewsUpdate"))
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
            let response = ServerMessage::NewsUpdateResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Normalize empty strings to None
    let body = body.filter(|s| !s.trim().is_empty());
    let image = image.filter(|s| !s.is_empty());

    // Validate that at least one of body or image is provided
    if body.is_none() && image.is_none() {
        let response = ServerMessage::NewsUpdateResponse {
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
        let response = ServerMessage::NewsUpdateResponse {
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
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(error_msg),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch existing news item to check authorship and admin status
    let existing_news = match ctx.db.news.get_news_by_id(id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            let response = ServerMessage::NewsUpdateResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsUpdate"))
                .await;
        }
    };

    // Check permission: user must be author OR have NewsEdit permission
    let is_author = existing_news.author_id == requesting_user.db_user_id;
    let has_edit_permission = requesting_user.has_permission(Permission::NewsEdit);

    if !is_author && !has_edit_permission {
        eprintln!(
            "NewsUpdate from {} (user: {}) without permission for news #{}",
            ctx.peer_addr, requesting_user.username, id
        );
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check admin protection: non-admins cannot edit admin posts
    if existing_news.author_is_admin && !requesting_user.is_admin {
        eprintln!(
            "NewsUpdate from {} (user: {}) trying to edit admin news #{}",
            ctx.peer_addr, requesting_user.username, id
        );
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(err_cannot_edit_admin_news(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Update news in database
    let news_record = match ctx
        .db
        .news
        .update_news(id, body.as_deref(), image.as_deref())
        .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            // Race condition - news was deleted between our check and update
            let response = ServerMessage::NewsUpdateResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error updating news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsUpdate"))
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
    let response = ServerMessage::NewsUpdateResponse {
        success: true,
        error: None,
        news: Some(news.clone()),
    };
    ctx.send_message(&response).await?;

    // Broadcast NewsUpdated to users with news feature and NewsList permission
    let broadcast = ServerMessage::NewsUpdated {
        action: NewsAction::Updated,
        id: news.id,
    };
    ctx.user_manager
        .broadcast_to_feature(FEATURE_NEWS, broadcast, Permission::NewsList)
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_news_update_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_update(
            1,
            Some("Updated".to_string()),
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_update_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_news_update(
            99999,
            Some("Updated".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_not_found(DEFAULT_TEST_LOCALE, 99999)));
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_empty_content() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_empty_content(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_author_can_update_own() {
        let mut test_ctx = create_test_context().await;

        // Login as user without NewsEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, user.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            Some("Updated by author".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("Updated by author".to_string()));
                assert!(news.updated_at.is_some());
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_non_author_without_permission() {
        let mut test_ctx = create_test_context().await;

        // Create author and their news
        let _author_session = login_user(
            &mut test_ctx,
            "author",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let author = test_ctx
            .db
            .users
            .get_user_by_username("author")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Author's post"), None, author.id)
            .await
            .unwrap();

        // Login as another user without NewsEdit permission
        let other_session = login_user(&mut test_ctx, "other", "password", &[], false).await;

        let result = handle_news_update(
            created.id,
            Some("Hacked!".to_string()),
            None,
            Some(other_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_with_permission_can_update_others() {
        let mut test_ctx = create_test_context().await;

        // Create author and their news
        let _author_session = login_user(
            &mut test_ctx,
            "author",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let author = test_ctx
            .db
            .users
            .get_user_by_username("author")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Author's post"), None, author.id)
            .await
            .unwrap();

        // Login as editor with NewsEdit permission
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::NewsEdit],
            false,
        )
        .await;

        let result = handle_news_update(
            created.id,
            Some("Edited by editor".to_string()),
            None,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("Edited by editor".to_string()));
                // Author should still be the original author
                assert_eq!(news.author, "author");
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_non_admin_cannot_edit_admin_post() {
        let mut test_ctx = create_test_context().await;

        // Create admin and their news
        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Admin's post"), None, admin.id)
            .await
            .unwrap();

        // Login as non-admin with NewsEdit permission
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::NewsEdit],
            false,
        )
        .await;

        let result = handle_news_update(
            created.id,
            Some("Trying to edit admin post".to_string()),
            None,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_cannot_edit_admin_news(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_admin_can_edit_admin_post() {
        let mut test_ctx = create_test_context().await;

        // Create first admin and their news
        let _admin1_session = login_user(&mut test_ctx, "admin1", "password", &[], true).await;

        let admin1 = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Admin1's post"), None, admin1.id)
            .await
            .unwrap();

        // Login as another admin
        let admin2_session = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        let result = handle_news_update(
            created.id,
            Some("Edited by admin2".to_string()),
            None,
            Some(admin2_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("Edited by admin2".to_string()));
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_body_too_long() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, admin.id)
            .await
            .unwrap();

        let long_body = "a".repeat(validators::MAX_NEWS_BODY_LENGTH + 1);
        let result = handle_news_update(
            created.id,
            Some(long_body),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_news_body_too_long(
                        DEFAULT_TEST_LOCALE,
                        validators::MAX_NEWS_BODY_LENGTH
                    ))
                );
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_invalid_image() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            None,
            Some("not a data uri".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_news_image_invalid_format(DEFAULT_TEST_LOCALE))
                );
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_with_image() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            Some("Updated with image".to_string()),
            Some("data:image/png;base64,iVBORw0KGgo=".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsUpdateResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("Updated with image".to_string()));
                assert_eq!(
                    news.image,
                    Some("data:image/png;base64,iVBORw0KGgo=".to_string())
                );
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }
}
