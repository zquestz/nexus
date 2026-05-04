//! TrackerEdit message handler — fetches a single tracker for the
//! admin's edit form (config + current runtime status).

use std::io;

use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, warn};

use super::{
    HandlerContext, compose_tracker_info, err_authentication, err_database, err_not_logged_in,
    err_permission_denied, err_tracker_not_found,
};
use crate::constants::{
    HANDLER_TRACKER_EDIT, LOG_TRACKER_EDIT_DB_ERROR, LOG_TRACKER_EDIT_NOT_LOGGED_IN,
    LOG_TRACKER_EDIT_PERMISSION_DENIED,
};
use crate::db::Permission;

/// Handle a `TrackerEdit { id }` request. Requires `tracker_edit`
/// permission. Returns the full `TrackerInfo` (config + runtime
/// status) on success.
pub async fn handle_tracker_edit<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_EDIT_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRACKER_EDIT))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_authentication(ctx.locale),
                    Some(HANDLER_TRACKER_EDIT),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerEdit) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_EDIT_PERMISSION_DENIED
        );
        let response = ServerMessage::TrackerEditResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            tracker: None,
        };
        return ctx.send_message(&response).await;
    }

    let row = match ctx.db.trackers.get_by_id(id).await {
        Ok(r) => r,
        Err(e) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                id = id,
                err = %e,
                "{}", LOG_TRACKER_EDIT_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_TRACKER_EDIT))
                .await;
        }
    };

    match row {
        None => {
            let response = ServerMessage::TrackerEditResponse {
                success: false,
                error: Some(err_tracker_not_found(ctx.locale)),
                tracker: None,
            };
            ctx.send_message(&response).await
        }
        Some(record) => {
            let status = ctx.tracker_manager.status_for(record.id);
            let response = ServerMessage::TrackerEditResponse {
                success: true,
                error: None,
                tracker: Some(compose_tracker_info(record, status, ctx.locale)),
            };
            ctx.send_message(&response).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, CreateTrackerParams};
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result = handle_tracker_edit(1, None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_tracker_edit(1, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerEditResponse {
                success, tracker, ..
            } => {
                assert!(!success);
                assert!(tracker.is_none());
            }
            other => panic!("Expected TrackerEditResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_id_returns_not_found() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let result =
            handle_tracker_edit(99_999, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerEditResponse {
                success, tracker, ..
            } => {
                assert!(!success);
                assert!(tracker.is_none());
            }
            other => panic!("Expected TrackerEditResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn returns_full_record_for_existing_tracker() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let created = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "tracker.example.com",
                port: 7510,
                fingerprint: None,
                password: Some("secret"),
                name: "Public",
                enabled: true,
            })
            .await
            .expect("create");

        let result = handle_tracker_edit(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerEditResponse {
                success, tracker, ..
            } => {
                assert!(success);
                let info = tracker.unwrap();
                assert_eq!(info.id, created.id);
                assert_eq!(info.address, "tracker.example.com");
                assert_eq!(info.port, 7510);
                assert_eq!(info.password.as_deref(), Some("secret"));
                assert_eq!(info.name, "Public");
                assert!(info.enabled);
            }
            other => panic!("Expected TrackerEditResponse, got {other:?}"),
        }
    }
}
