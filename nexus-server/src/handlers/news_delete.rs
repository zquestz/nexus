//! NewsDelete message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::protocol::{NewsAction, ServerMessage};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_cannot_delete_admin_news, err_database, err_news_not_found,
    err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    FEATURE_NEWS, HANDLER_NEWS_DELETE, LOG_NEWS_DELETE_ADMIN, LOG_NEWS_DELETE_DB_ERROR_DELETE,
    LOG_NEWS_DELETE_DB_ERROR_GET, LOG_NEWS_DELETE_NOT_LOGGED_IN, LOG_NEWS_DELETE_PERMISSION_DENIED,
    LOG_NEWS_DELETE_SUCCESS,
};
use crate::db::Permission;

pub async fn handle_news_delete<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_NEWS_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_NEWS_DELETE))
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
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_not_logged_in(ctx.locale)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Fetch existing item to check authorship and admin status.
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
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_NEWS_DELETE_DB_ERROR_GET);
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Author may delete own; otherwise NewsDelete required.
    let is_author = existing_news.author_id == Some(requesting_user.user_id);
    let has_delete_permission = requesting_user.has_permission(Permission::NewsDelete);

    if !is_author && !has_delete_permission {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_NEWS_DELETE_PERMISSION_DENIED);
        let response = ServerMessage::NewsDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
        };
        return ctx.send_message(&response).await;
    }

    // Non-admins cannot delete admin posts.
    if existing_news.author_is_admin && !requesting_user.is_admin {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, id = %id, "{}", LOG_NEWS_DELETE_ADMIN);
        let response = ServerMessage::NewsDeleteResponse {
            success: false,
            error: Some(err_cannot_delete_admin_news(ctx.locale)),
            id: None,
        };
        return ctx.send_message(&response).await;
    }

    match ctx.db.news.delete_news(id).await {
        Ok(true) => {}
        Ok(false) => {
            // Race: already deleted.
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_news_not_found(ctx.locale, id)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_NEWS_DELETE_DB_ERROR_DELETE);
            let response = ServerMessage::NewsDeleteResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                id: None,
            };
            return ctx.send_message(&response).await;
        }
    };

    // Broadcast to users with the news feature and NewsList permission.
    let broadcast = ServerMessage::NewsUpdated {
        action: NewsAction::Deleted,
        id,
    };
    ctx.user_manager
        .broadcast_to_feature(
            FEATURE_NEWS,
            broadcast,
            Permission::NewsList,
            Some(requesting_session_id),
        )
        .await;

    let response = ServerMessage::NewsDeleteResponse {
        success: true,
        error: None,
        id: Some(id),
    };
    info!(user = %requesting_user.username, ip = %ctx.peer_addr, id = %id, "{}", LOG_NEWS_DELETE_SUCCESS);
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

    #[tokio::test]
    async fn test_news_delete_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_news_delete(1, None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_news_delete_not_found() {
        let mut test_ctx = create_test_context().await;

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

    #[tokio::test]
    async fn test_news_delete_excludes_originator_from_broadcast() {
        let mut test_ctx = create_test_context().await;

        // Originator: alice with NewsCreate (lets her delete her own) + NewsList + "news"
        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::NewsCreate, db::Permission::NewsList],
            false,
            vec![FEATURE_NEWS.to_string()],
        )
        .await;

        // Pre-create a post authored by alice so she can delete it
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
            .create_news(Some("My post"), None, alice.id)
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

        let result = handle_news_delete(
            created.id,
            Some(alice_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        match read_server_message(&mut test_ctx).await {
            ServerMessage::NewsDeleteResponse { success, .. } => assert!(success),
            other => panic!("Expected NewsDeleteResponse, got {other:?}"),
        }

        assert!(
            test_ctx.rx.try_recv().is_err(),
            "Originator should not receive NewsUpdated for their own NewsDelete"
        );

        let broadcast = bob_rx
            .try_recv()
            .expect("Observer should receive NewsUpdated broadcast")
            .expect_message()
            .0;
        match broadcast {
            ServerMessage::NewsUpdated { action, id } => {
                assert_eq!(action, NewsAction::Deleted);
                assert_eq!(id, created.id);
            }
            other => panic!("Expected NewsUpdated, got {other:?}"),
        }
    }
}
