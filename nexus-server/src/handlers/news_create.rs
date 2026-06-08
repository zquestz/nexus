//! NewsCreate message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::{NewsAction, NewsItem, ServerMessage};
use nexus_common::validators::{self, NewsBodyError, NewsImageError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_database, err_news_body_invalid_characters, err_news_body_too_long,
    err_news_empty_content, err_news_image_invalid_format, err_news_image_too_large,
    err_news_image_unsupported_type, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    FEATURE_NEWS, HANDLER_NEWS_CREATE, LOG_NEWS_CREATE_DB_ERROR, LOG_NEWS_CREATE_NOT_LOGGED_IN,
    LOG_NEWS_CREATE_PERMISSION_DENIED, LOG_NEWS_CREATE_SUCCESS,
};
use crate::db::Permission;

pub async fn handle_news_create<W>(
    body: Option<String>,
    image: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_NEWS_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_NEWS_CREATE))
            .await;
    };

    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            // Session not found — likely a race, not a security event.
            let response = ServerMessage::NewsCreateResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    if !requesting_user.has_permission(Permission::NewsCreate) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_NEWS_CREATE_PERMISSION_DENIED);
        let response = ServerMessage::NewsCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    let body = body.filter(|s| !s.trim().is_empty());
    let image = image.filter(|s| !s.is_empty());

    // At least one of body or image must be present.
    if body.is_none() && image.is_none() {
        let response = ServerMessage::NewsCreateResponse {
            success: false,
            error: Some(err_news_empty_content(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

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

    let news_record = match ctx
        .db
        .news
        .create_news(body.as_deref(), image.as_deref(), requesting_user.user_id)
        .await
    {
        Ok(record) => record,
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_NEWS_CREATE_DB_ERROR);
            let response = ServerMessage::NewsCreateResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    let news = NewsItem {
        id: news_record.id,
        body: news_record.body,
        image: news_record.image,
        author: news_record.author_username,
        author_is_admin: news_record.author_is_admin,
        created_at: news_record.created_at,
        updated_at: news_record.updated_at,
    };

    // Broadcast to users with the news feature and NewsList permission.
    let broadcast = ServerMessage::NewsUpdated {
        action: NewsAction::Created,
        id: news.id,
    };
    ctx.user_manager
        .broadcast_to_feature(
            FEATURE_NEWS,
            broadcast,
            Permission::NewsList,
            Some(requesting_session_id),
        )
        .await;

    let response = ServerMessage::NewsCreateResponse {
        success: true,
        error: None,
        news: Some(news),
    };
    info!(user = %requesting_user.username, ip = %ctx.peer_addr, id = %news_record.id, "{}", LOG_NEWS_CREATE_SUCCESS);
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{
        create_test_context, login_observer_user, login_user, login_user_with_features,
        read_server_message,
    };

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

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_news_create(
            Some("Test post".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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
                assert_eq!(news.updated_at, news.created_at);
            }
            _ => panic!("Expected NewsCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_create_with_image() {
        let mut test_ctx = create_test_context().await;

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
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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
        let response = read_server_message(&mut test_ctx).await;
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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_news_create(
            Some("Admin news".to_string()),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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

    #[tokio::test]
    async fn test_news_create_excludes_originator_from_broadcast() {
        let mut test_ctx = create_test_context().await;

        // Originator: alice with NewsCreate + NewsList + "news" feature
        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate, db::Permission::NewsList],
            false,
            vec![FEATURE_NEWS.to_string()],
        )
        .await;

        // Observer: bob with NewsList + "news" feature, on a separate broadcast channel
        let (_bob_session, mut bob_rx) = login_observer_user(
            &mut test_ctx,
            "bob",
            "password",
            &[db::Permission::NewsList],
            vec![FEATURE_NEWS.to_string()],
        )
        .await;

        let result = handle_news_create(
            Some("Test post".to_string()),
            None,
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        // Originator gets the typed response on TCP
        match read_server_message(&mut test_ctx).await {
            ServerMessage::NewsCreateResponse { success, .. } => assert!(success),
            other => panic!("Expected NewsCreateResponse, got {other:?}"),
        }

        // Originator's broadcast channel must be empty (excluded)
        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Originator should not receive NewsUpdated for their own NewsCreate"
        );

        // Observer should receive NewsUpdated{Created}
        let broadcast = bob_rx
            .try_recv()
            .expect("Observer should receive NewsUpdated broadcast")
            .expect_message()
            .0;
        match broadcast {
            ServerMessage::NewsUpdated { action, .. } => {
                assert_eq!(action, NewsAction::Created);
            }
            other => panic!("Expected NewsUpdated, got {other:?}"),
        }
    }
}
