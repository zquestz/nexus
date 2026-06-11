//! NewsUpdate message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use crate::constants::{
    HANDLER_NEWS_UPDATE, LOG_NEWS_UPDATE_ADMIN, LOG_NEWS_UPDATE_DB_ERROR,
    LOG_NEWS_UPDATE_DB_ERROR_GET, LOG_NEWS_UPDATE_IMAGE_VALIDATE_ERROR,
    LOG_NEWS_UPDATE_NOT_LOGGED_IN, LOG_NEWS_UPDATE_PERMISSION_DENIED, LOG_NEWS_UPDATE_SUCCESS,
};

use nexus_common::protocol::{NewsAction, NewsItem, ServerMessage};
use nexus_common::validators::{self, NewsBodyError};

#[cfg(test)]
use super::err_news_image_invalid_format;
#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_cannot_edit_admin_news, err_database, err_news_body_invalid_characters,
    err_news_body_too_long, err_news_empty_content, err_news_not_found, err_no_fields_to_update,
    err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_NEWS;
use crate::db::Permission;

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
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_NEWS_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_NEWS_UPDATE))
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
            let response = ServerMessage::NewsUpdateResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Fetch existing item to check authorship and admin status.
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
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_NEWS_UPDATE_DB_ERROR_GET);
            let response = ServerMessage::NewsUpdateResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Author may edit own; otherwise NewsEdit required.
    let is_author = existing_news.author_id == Some(requesting_user.user_id);
    let has_edit_permission = requesting_user.has_permission(Permission::NewsEdit);

    if !is_author && !has_edit_permission {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_NEWS_UPDATE_PERMISSION_DENIED);
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Non-admins cannot edit admin posts.
    if existing_news.author_is_admin && !requesting_user.is_admin {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, id = %id, "{}", LOG_NEWS_UPDATE_ADMIN);
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(err_cannot_edit_admin_news(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    if body.is_none() && image.is_none() {
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(err_no_fields_to_update(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    if let Some(ref body_text) = body
        && !body_text.trim().is_empty()
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

    if let Some(ref image_data) = image
        && !image_data.is_empty()
        && let Err(error_msg) = super::validate_news_image_blocking(
            image_data,
            ctx.locale,
            ctx.peer_addr,
            LOG_NEWS_UPDATE_IMAGE_VALIDATE_ERROR,
        )
        .await
    {
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(error_msg),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    let body = match body {
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(value),
        None => existing_news.body,
    };
    let image = match image {
        Some(value) if value.is_empty() => None,
        Some(value) => Some(value),
        None => existing_news.image,
    };

    if body.is_none() && image.is_none() {
        let response = ServerMessage::NewsUpdateResponse {
            success: false,
            error: Some(err_news_empty_content(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    let news_record = match ctx
        .db
        .news
        .update_news(id, body.as_deref(), image.as_deref())
        .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            // Race: deleted between our check and the update.
            let response = ServerMessage::NewsUpdateResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_NEWS_UPDATE_DB_ERROR);
            let response = ServerMessage::NewsUpdateResponse {
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
        action: NewsAction::Updated,
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

    let response = ServerMessage::NewsUpdateResponse {
        success: true,
        error: None,
        news: Some(news),
    };
    info!(user = %requesting_user.username, ip = %ctx.peer_addr, id = %id, "{}", LOG_NEWS_UPDATE_SUCCESS);
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::FEATURE_NEWS;
    use crate::db;
    use crate::handlers::testing::{
        create_test_context, login_observer_user, login_user, login_user_with_features,
        read_server_message,
    };

    const VALID_PNG_DATA_URI: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

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
                assert_eq!(error, Some(err_no_fields_to_update(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_update_author_can_update_own() {
        let mut test_ctx = create_test_context().await;

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
                assert!(news.updated_at >= news.created_at);
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_omitted_fields_are_unchanged() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let image = VALID_PNG_DATA_URI;
        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), Some(image), admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            Some("Updated body".to_string()),
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
                assert_eq!(news.body, Some("Updated body".to_string()));
                assert_eq!(news.image, Some(image.to_string()));
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_empty_body_clears_body_preserves_image() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let image = VALID_PNG_DATA_URI;
        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), Some(image), admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            Some(String::new()),
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
                assert!(news.body.is_none());
                assert_eq!(news.image, Some(image.to_string()));
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_empty_fields_clear_existing_values() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        let image = VALID_PNG_DATA_URI;
        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), Some(image), admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            None,
            Some(String::new()),
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
                assert_eq!(news.body, Some("Original".to_string()));
                assert!(news.image.is_none());
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }

        let result = handle_news_update(
            created.id,
            Some("   ".to_string()),
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
                assert!(!success);
                assert_eq!(error, Some(err_news_empty_content(DEFAULT_TEST_LOCALE)));
                assert!(news.is_none());
            }
            _ => panic!("Expected NewsUpdateResponse with error"),
        }

        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), Some(image), admin.id)
            .await
            .unwrap();

        let result = handle_news_update(
            created.id,
            Some("   ".to_string()),
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
                assert!(news.body.is_none());
                assert_eq!(news.image, Some(image.to_string()));
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_non_author_without_permission() {
        let mut test_ctx = create_test_context().await;

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
                assert_eq!(news.author.as_deref(), Some("author"));
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_non_admin_cannot_edit_admin_post() {
        let mut test_ctx = create_test_context().await;

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
            Some(VALID_PNG_DATA_URI.to_string()),
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
                assert_eq!(news.image, Some(VALID_PNG_DATA_URI.to_string()));
            }
            _ => panic!("Expected NewsUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_update_excludes_originator_from_broadcast() {
        let mut test_ctx = create_test_context().await;

        // Originator: alice with NewsCreate (lets her edit her own) + NewsList + "news"
        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate, db::Permission::NewsList],
            false,
            vec![FEATURE_NEWS.to_string()],
        )
        .await;

        // Pre-create a post authored by alice so she can update it
        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, alice.id)
            .await
            .unwrap();

        // Observer: bob with NewsList + "news" feature, on a separate broadcast channel
        let (_bob_session, mut bob_rx) = login_observer_user(
            &mut test_ctx,
            "bob",
            "password",
            &[db::Permission::NewsList],
            vec![FEATURE_NEWS.to_string()],
        )
        .await;

        let result = handle_news_update(
            created.id,
            Some("Updated".to_string()),
            None,
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        match read_server_message(&mut test_ctx).await {
            ServerMessage::NewsUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected NewsUpdateResponse, got {other:?}"),
        }

        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Originator should not receive NewsUpdated for their own NewsUpdate"
        );

        let broadcast = bob_rx
            .try_recv()
            .expect("Observer should receive NewsUpdated broadcast")
            .expect_message()
            .0;
        match broadcast {
            ServerMessage::NewsUpdated { action, id } => {
                assert_eq!(action, NewsAction::Updated);
                assert_eq!(id, created.id);
            }
            other => panic!("Expected NewsUpdated, got {other:?}"),
        }
    }
}
