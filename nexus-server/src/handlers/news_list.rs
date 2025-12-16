//! NewsList message handler - Returns all news items

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{NewsItem, ServerMessage};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{HandlerContext, err_database, err_not_logged_in, err_permission_denied};
use crate::db::Permission;

/// Handle a news list request
pub async fn handle_news_list<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("NewsList request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("NewsList"))
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
            let response = ServerMessage::NewsListResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                items: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Check NewsList permission
    if !requesting_user.has_permission(Permission::NewsList) {
        eprintln!(
            "NewsList from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::NewsListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            items: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch all news from database
    let news_records = match ctx.db.news.get_all_news().await {
        Ok(records) => records,
        Err(e) => {
            eprintln!("Database error getting news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsList"))
                .await;
        }
    };

    // Convert to protocol format
    let items: Vec<NewsItem> = news_records
        .into_iter()
        .map(|record| NewsItem {
            id: record.id,
            body: record.body,
            image: record.image,
            author: record.author_username,
            author_is_admin: record.author_is_admin,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
        .collect();

    let response = ServerMessage::NewsListResponse {
        success: true,
        error: None,
        items: Some(items),
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_news_list_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_list(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_list_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without NewsList permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_news_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsListResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsListResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_list_empty() {
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

        let result = handle_news_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsListResponse {
                success,
                error,
                items,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(items.unwrap().len(), 0);
            }
            _ => panic!("Expected NewsListResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_list_returns_items_oldest_first() {
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

        // Create some news items
        test_ctx
            .db
            .news
            .create_news(Some("First post"), None, user.id)
            .await
            .unwrap();
        test_ctx
            .db
            .news
            .create_news(Some("Second post"), None, user.id)
            .await
            .unwrap();
        test_ctx
            .db
            .news
            .create_news(Some("Third post"), None, user.id)
            .await
            .unwrap();

        let result = handle_news_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsListResponse {
                success,
                error,
                items,
            } => {
                assert!(success);
                assert!(error.is_none());
                let items = items.unwrap();
                assert_eq!(items.len(), 3);
                // Should be ordered oldest first
                assert_eq!(items[0].body, Some("First post".to_string()));
                assert_eq!(items[1].body, Some("Second post".to_string()));
                assert_eq!(items[2].body, Some("Third post".to_string()));
            }
            _ => panic!("Expected NewsListResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_list_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin (no explicit permissions needed)
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_news_list(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsListResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected NewsListResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_list_includes_author_info() {
        let mut test_ctx = create_test_context().await;

        // Create an admin user
        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a regular user with news permissions
        let _user_session_id = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[db::Permission::NewsList, db::Permission::NewsCreate],
            false,
        )
        .await;

        // Get database IDs
        let admin = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let user = test_ctx
            .db
            .users
            .get_user_by_username("user")
            .await
            .unwrap()
            .unwrap();

        // Create news from both users
        test_ctx
            .db
            .news
            .create_news(Some("Admin post"), None, admin.id)
            .await
            .unwrap();
        test_ctx
            .db
            .news
            .create_news(Some("User post"), None, user.id)
            .await
            .unwrap();

        let result =
            handle_news_list(Some(admin_session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::NewsListResponse { success, items, .. } => {
                assert!(success);
                let items = items.unwrap();
                assert_eq!(items.len(), 2);

                // First post from admin
                assert_eq!(items[0].author, "admin");
                assert!(items[0].author_is_admin);

                // Second post from user
                assert_eq!(items[1].author, "user");
                assert!(!items[1].author_is_admin);
            }
            _ => panic!("Expected NewsListResponse"),
        }
    }
}
