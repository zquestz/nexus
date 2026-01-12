//! NewsDelete message handler - Deletes a news item

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{NewsAction, ServerMessage};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_cannot_delete_admin_news, err_database, err_news_not_found,
    err_not_logged_in, err_permission_denied,
};
use crate::constants::FEATURE_NEWS;
use crate::db::Permission;

/// Handle a news delete request
pub async fn handle_news_delete<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(requesting_session_id) = session_id else {
        eprintln!("NewsDelete request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("NewsDelete"))
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
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Fetch existing news item to check authorship and admin status
    let existing_news = match ctx.db.news.get_news_by_id(id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsDelete"))
                .await;
        }
    };

    // Check permission: user must be author OR have NewsDelete permission
    let is_author = existing_news.author_id == requesting_user.db_user_id;
    let has_delete_permission = requesting_user.has_permission(Permission::NewsDelete);

    if !is_author && !has_delete_permission {
        eprintln!(
            "NewsDelete from {} (user: {}) without permission for news #{}",
            ctx.peer_addr, requesting_user.username, id
        );
        let response = ServerMessage::NewsDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check admin protection: non-admins cannot delete admin posts
    if existing_news.author_is_admin && !requesting_user.is_admin {
        eprintln!(
            "NewsDelete from {} (user: {}) trying to delete admin news #{}",
            ctx.peer_addr, requesting_user.username, id
        );
        let response = ServerMessage::NewsDeleteResponse {
            success: false,
            error: Some(err_cannot_delete_admin_news(ctx.locale)),
            id: None,
        };
        return ctx.send_message(&response).await;
    }

    // Delete news from database
    match ctx.db.news.delete_news(id).await {
        Ok(true) => {}
        Ok(false) => {
            // Race condition - news was already deleted
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error deleting news: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("NewsDelete"))
                .await;
        }
    };

    // Send success response
    let response = ServerMessage::NewsDeleteResponse {
        success: true,
        error: None,
        id: Some(id),
    };
    ctx.send_message(&response).await?;

    // Broadcast NewsUpdated to users with news feature and NewsList permission
    let broadcast = ServerMessage::NewsUpdated {
        action: NewsAction::Deleted,
        id,
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
    async fn test_news_delete_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_delete(1, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_delete_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_news_delete(99999, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_news_not_found(DEFAULT_TEST_LOCALE, 99999)));
            }
            _ => panic!("Expected NewsDeleteResponse with error"),
        }
    }

    #[tokio::test]
    async fn test_news_delete_author_can_delete_own() {
        let mut test_ctx = create_test_context().await;

        // Login as user without NewsDelete permission
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
            .create_news(Some("My post"), None, user.id)
            .await
            .unwrap();

        let result = handle_news_delete(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, id } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(created.id));
            }
            _ => panic!("Expected NewsDeleteResponse"),
        }

        // Verify it's actually deleted
        let fetched = test_ctx.db.news.get_news_by_id(created.id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_news_delete_non_author_without_permission() {
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

        // Login as another user without NewsDelete permission
        let other_session = login_user(&mut test_ctx, "other", "password", &[], false).await;

        let result = handle_news_delete(
            created.id,
            Some(other_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected NewsDeleteResponse with error"),
        }

        // Verify it's NOT deleted
        let fetched = test_ctx.db.news.get_news_by_id(created.id).await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_news_delete_with_permission_can_delete_others() {
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

        // Login as moderator with NewsDelete permission
        let mod_session = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[db::Permission::NewsDelete],
            false,
        )
        .await;

        let result = handle_news_delete(
            created.id,
            Some(mod_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, id } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(created.id));
            }
            _ => panic!("Expected NewsDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_delete_non_admin_cannot_delete_admin_post() {
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

        // Login as non-admin with NewsDelete permission
        let mod_session = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[db::Permission::NewsDelete],
            false,
        )
        .await;

        let result = handle_news_delete(
            created.id,
            Some(mod_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_cannot_delete_admin_news(DEFAULT_TEST_LOCALE))
                );
            }
            _ => panic!("Expected NewsDeleteResponse with error"),
        }

        // Verify it's NOT deleted
        let fetched = test_ctx.db.news.get_news_by_id(created.id).await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_news_delete_admin_can_delete_admin_post() {
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

        let result = handle_news_delete(
            created.id,
            Some(admin2_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, id } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(created.id));
            }
            _ => panic!("Expected NewsDeleteResponse"),
        }
    }

    #[tokio::test]
    async fn test_news_delete_admin_can_delete_any() {
        let mut test_ctx = create_test_context().await;

        // Create regular user and their news
        let _user_session = login_user(
            &mut test_ctx,
            "user",
            "password",
            &[db::Permission::NewsCreate],
            false,
        )
        .await;

        let user = test_ctx
            .db
            .users
            .get_user_by_username("user")
            .await
            .unwrap()
            .unwrap();

        let created = test_ctx
            .db
            .news
            .create_news(Some("User's post"), None, user.id)
            .await
            .unwrap();

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_news_delete(
            created.id,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::NewsDeleteResponse { success, error, id } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(id, Some(created.id));
            }
            _ => panic!("Expected NewsDeleteResponse"),
        }
    }
}
