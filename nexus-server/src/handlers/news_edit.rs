//! NewsEdit message handler - Returns a news item for editing

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{NewsItem, ServerMessage};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_cannot_edit_admin_news, err_database, err_news_not_found,
    err_not_logged_in, err_permission_denied,
};
use crate::db::Permission;

/// Handle a news edit request (returns news item for editing)
pub async fn handle_news_edit<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("NewsEdit request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("NewsEdit"))
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
            let response = ServerMessage::NewsEditResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Fetch news item from database first to check authorship
    let news_record = match ctx.db.news.get_news_by_id(id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            let response = ServerMessage::NewsEditResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                news: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsEdit"))
                .await;
        }
    };

    // Check permission: user must be author OR have NewsEdit permission
    let is_author = news_record.author_id == requesting_user.db_user_id;
    let has_edit_permission = requesting_user.has_permission(Permission::NewsEdit);

    if !is_author && !has_edit_permission {
        eprintln!(
            "NewsEdit from {} (user: {}) without permission for news #{}",
            ctx.peer_addr, requesting_user.username, id
        );
        let response = ServerMessage::NewsEditResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check admin protection: non-admins cannot edit admin posts
    if news_record.author_is_admin && !requesting_user.is_admin {
        eprintln!(
            "NewsEdit from {} (user: {}) trying to edit admin news #{}",
            ctx.peer_addr, requesting_user.username, id
        );
        let response = ServerMessage::NewsEditResponse {
            success: false,
            error: Some(err_cannot_edit_admin_news(ctx.locale)),
            news: None,
        };
        return ctx.send_message(&response).await;
    }

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

    let response = ServerMessage::NewsEditResponse {
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
    async fn test_news_edit_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_edit(1, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_edit_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin (has all permissions)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_news_edit(99999, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_not_found(DEFAULT_TEST_LOCALE, 99999)));
            }
            _ => panic!("Expected NewsEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_edit_author_can_edit_own() {
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

        // Get the user's database ID
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        // Create a news item as this user
        let created = test_ctx
            .db
            .news
            .create_news(Some("My post"), None, user.id)
            .await
            .unwrap();

        let result = handle_news_edit(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.id, created.id);
                assert_eq!(news.body, Some("My post".to_string()));
            }
            _ => panic!("Expected NewsEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_edit_non_author_without_permission() {
        let mut test_ctx = create_test_context().await;

        // Create first user and their news
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

        let result = handle_news_edit(
            created.id,
            Some(other_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_edit_with_permission_can_edit_others() {
        let mut test_ctx = create_test_context().await;

        // Create first user and their news
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

        // Login as user with NewsEdit permission
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::NewsEdit],
            false,
        )
        .await;

        let result = handle_news_edit(
            created.id,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.author, "author");
            }
            _ => panic!("Expected NewsEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_edit_non_admin_cannot_edit_admin_post() {
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

        let result = handle_news_edit(
            created.id,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_cannot_edit_admin_news(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsEditResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_edit_admin_can_edit_admin_post() {
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

        let result = handle_news_edit(
            created.id,
            Some(admin2_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.author, "admin1");
                assert!(news.author_is_admin);
            }
            _ => panic!("Expected NewsEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_edit_returns_full_content() {
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

        // Create news with both body and image
        let created = test_ctx
            .db
            .news
            .create_news(
                Some("# News\n\nWith markdown!"),
                Some("data:image/png;base64,abc123"),
                admin.id,
            )
            .await
            .unwrap();

        // Update it to set updated_at
        test_ctx
            .db
            .news
            .update_news(
                created.id,
                Some("# Updated"),
                Some("data:image/png;base64,xyz"),
            )
            .await
            .unwrap();

        let result = handle_news_edit(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsEditResponse {
                success,
                error,
                news,
            } => {
                assert!(success);
                assert!(error.is_none());
                let news = news.unwrap();
                assert_eq!(news.body, Some("# Updated".to_string()));
                assert_eq!(news.image, Some("data:image/png;base64,xyz".to_string()));
                assert!(news.updated_at.is_some());
            }
            _ => panic!("Expected NewsEditResponse"),
        }
    }
}
