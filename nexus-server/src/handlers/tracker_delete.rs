//! TrackerDelete message handler — removes a tracker from the
//! server's publisher list and aborts its task.

use std::io;

use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
    err_tracker_not_found,
};
use crate::constants::{
    LOG_TRACKER_DELETE_DB_ERROR, LOG_TRACKER_DELETE_NOT_LOGGED_IN,
    LOG_TRACKER_DELETE_PERMISSION_DENIED, LOG_TRACKER_DELETE_SUCCESS,
};
use crate::db::Permission;

/// Handle a `TrackerDelete { id }` request. Requires the
/// `tracker_delete` permission. Removes the row, aborts the publisher
/// task, and reports the deleted name back to the client for the
/// confirmation toast.
pub async fn handle_tracker_delete<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_DELETE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrackerDelete"))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrackerDelete"))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerDelete) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_DELETE_PERMISSION_DENIED
        );
        let response = ServerMessage::TrackerDeleteResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch the existing row first so we can echo its name back for
    // the confirmation toast. Returns "not found" if the id is unknown.
    let existing = match ctx.db.trackers.get_by_id(id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            let response = ServerMessage::TrackerDeleteResponse {
                success: false,
                error: Some(err_tracker_not_found(ctx.locale)),
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                id = id,
                err = %e,
                "{}", LOG_TRACKER_DELETE_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("TrackerDelete"))
                .await;
        }
    };

    match ctx.db.trackers.delete(id).await {
        Ok(true) => {}
        Ok(false) => {
            // Race: row vanished between the get and the delete. Treat
            // as not-found rather than success (nothing was actually
            // deleted on this call).
            let response = ServerMessage::TrackerDeleteResponse {
                success: false,
                error: Some(err_tracker_not_found(ctx.locale)),
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                id = id,
                name = %existing.name,
                err = %e,
                "{}", LOG_TRACKER_DELETE_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("TrackerDelete"))
                .await;
        }
    }

    info!(
        user = %requesting_user.username,
        ip = %ctx.peer_addr,
        id = id,
        name = %existing.name,
        "{}", LOG_TRACKER_DELETE_SUCCESS
    );

    // Abort the publisher task (idempotent; no-op if disabled or never spawned).
    ctx.tracker_manager.terminate(id);

    let response = ServerMessage::TrackerDeleteResponse {
        success: true,
        error: None,
        name: Some(existing.name),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, CreateTrackerParams};
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result = handle_tracker_delete(1, None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_tracker_delete(1, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerDeleteResponse { success, name, .. } => {
                assert!(!success);
                assert!(name.is_none());
            }
            other => panic!("Expected TrackerDeleteResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_id_returns_not_found() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerDelete],
            false,
        )
        .await;

        let result =
            handle_tracker_delete(99_999, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerDeleteResponse { success, name, .. } => {
                assert!(!success);
                assert!(name.is_none());
            }
            other => panic!("Expected TrackerDeleteResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn deletes_existing_row_and_returns_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerDelete],
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
                password: None,
                name: "ToDelete",
                enabled: true,
            })
            .await
            .expect("create");

        let result = handle_tracker_delete(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerDeleteResponse {
                success,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert_eq!(name.as_deref(), Some("ToDelete"));
            }
            other => panic!("Expected TrackerDeleteResponse, got {other:?}"),
        }

        // Row should be gone.
        assert!(
            test_ctx
                .db
                .trackers
                .get_by_id(created.id)
                .await
                .expect("get")
                .is_none()
        );
    }
}
