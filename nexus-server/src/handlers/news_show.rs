//! NewsShow message handler - Returns a single news item by ID

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{NewsItem, ServerMessage};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_database, err_news_not_found, err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;

/// Handle a news show request
pub async fn handle_news_show<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("NewsShow request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("NewsShow"))
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
            let response = ServerMessage::NewsShowResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check NewsList permission (required to view news)
    if !requesting_user.has_permission(Permission::NewsList) {
        eprintln!(
            "NewsShow from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::NewsShowResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch news item from database
    let news_record = match ctx.db.news.get_news_by_id(id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            let response = ServerMessage::NewsShowResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsShow"))
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

    let response = ServerMessage::NewsShowResponse {
        success: true,
        error: None,
        news: Some(news),
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_news_show_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_show(1, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_show_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without NewsList permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_news_show(1, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsShowResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsShowResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_show_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsList],
            false,
        )
        .await;

        let result =
            handle_news_show(99999, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsShowResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_not_found(DEFAULT_TEST_LOCALE, 99999)));
            }
            _ => panic!("Expected NewsShowResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_show_returns_item() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsList],
            false,
        )
        .await;

        // Get the user's database ID
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        // Create a news item
        let created = test_ctx
            .db
            .news
            .create_news(
                Some("Test news post"),
                Some("data:image/png;base64,abc"),
                user.id,
            )
            .await
            .unwrap();

        let result = handle_news_show(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsShowResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.id, created.id);
                assert_eq!(news.body, Some("Test news post".to_string()));
                assert_eq!(news.image, Some("data:image/png;base64,abc".to_string()));
                assert_eq!(news.author, "alice");
                assert!(!news.author_is_admin);
                assert!(news.updated_at.is_none());
            }
            _ => panic!("Expected NewsShowResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_show_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin (no explicit permissions needed)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Get the admin's database ID
        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        // Create a news item
        let created = test_ctx
            .db
            .news
            .create_news(Some("Admin news"), None, admin.id)
            .await
            .unwrap();

        let result = handle_news_show(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsShowResponse { success, news, .. } => {
                assert!(success);
                let news = news.unwrap();
                assert_eq!(news.author, "admin");
                assert!(news.author_is_admin);
            }
            _ => panic!("Expected NewsShowResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_show_includes_updated_at() {
        let mut test_ctx = create_test_context().await;

        // Login as user with NewsList permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsList],
            false,
        )
        .await;

        // Get the user's database ID
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        // Create a news item
        let created = test_ctx
            .db
            .news
            .create_news(Some("Original"), None, user.id)
            .await
            .unwrap();

        // Update it
        test_ctx
            .db
            .news
            .update_news(created.id, Some("Updated"), None)
            .await
            .unwrap();

        let result = handle_news_show(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsShowResponse { success, news, .. } => {
                assert!(success);
                let news = news.unwrap();
                assert_eq!(news.body, Some("Updated".to_string()));
                assert!(news.updated_at.is_some());
            }
            _ => panic!("Expected NewsShowResponse"),
        }
    }
}
