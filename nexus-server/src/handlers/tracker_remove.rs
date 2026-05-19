//! TrackerRemove message handler — removes a tracker from the
//! server's tracker list and aborts its task.

use std::io;

use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_database, err_not_logged_in, err_permission_denied, err_tracker_not_found,
};
use crate::constants::{
    HANDLER_TRACKER_REMOVE, LOG_TRACKER_REMOVE_DB_ERROR, LOG_TRACKER_REMOVE_NOT_LOGGED_IN,
    LOG_TRACKER_REMOVE_PERMISSION_DENIED, LOG_TRACKER_REMOVE_SUCCESS,
};
use crate::db::Permission;

/// Handle a `TrackerRemove { id }` request. Requires the
/// `tracker_remove` permission. Removes the row, aborts the tracker
/// task, and reports the removed name back to the client for the
/// confirmation toast.
pub async fn handle_tracker_remove<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_REMOVE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRACKER_REMOVE))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRACKER_REMOVE),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerRemove) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_REMOVE_PERMISSION_DENIED
        );
        return ctx
            .send_message(&reject_remove(err_permission_denied(ctx.locale)))
            .await;
    }

    // Hold the lifecycle lock across the DB read, the DB delete, and the
    // manager terminate call so a concurrent TrackerUpdate /
    // TrackerAcceptFingerprint can't slip a `replace()` in after we've
    // already deleted the row (which would leave an orphan task running
    // against a missing row). Drop the guard before the network write —
    // see TrackerManager::lock_lifecycle.
    let response: ServerMessage = 'lifecycle: {
        let _guard = ctx.tracker_manager.lock_lifecycle().await;

        // Fetch the existing row first so we can echo its name back for
        // the confirmation toast. Returns "not found" if the id is unknown.
        let existing = match ctx.db.trackers.get_by_id(id).await {
            Ok(Some(r)) => r,
            Ok(None) => break 'lifecycle reject_remove(err_tracker_not_found(ctx.locale)),
            Err(e) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    err = %e,
                    "{}", LOG_TRACKER_REMOVE_DB_ERROR
                );
                break 'lifecycle reject_remove(err_database(ctx.locale));
            }
        };

        match ctx.db.trackers.delete(id).await {
            Ok(true) => {}
            Ok(false) => {
                // Race: row vanished between the get and the delete. Treat
                // as not-found rather than success (nothing was actually
                // deleted on this call).
                break 'lifecycle reject_remove(err_tracker_not_found(ctx.locale));
            }
            Err(e) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    name = %existing.name,
                    err = %e,
                    "{}", LOG_TRACKER_REMOVE_DB_ERROR
                );
                break 'lifecycle reject_remove(err_database(ctx.locale));
            }
        }

        info!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            id = id,
            name = %existing.name,
            "{}", LOG_TRACKER_REMOVE_SUCCESS
        );

        // Abort the tracker task (idempotent; no-op if disabled or never spawned).
        ctx.tracker_manager.terminate(id);

        ServerMessage::TrackerRemoveResponse {
            success: true,
            error: None,
            name: Some(existing.name),
        }
    };
    ctx.send_message(&response).await
}

fn reject_remove(error: String) -> ServerMessage {
    ServerMessage::TrackerRemoveResponse {
        success: false,
        error: Some(error),
        name: None,
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
        let result = handle_tracker_remove(1, None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_tracker_remove(1, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerRemoveResponse { success, name, .. } => {
                assert!(!success);
                assert!(name.is_none());
            }
            other => panic!("Expected TrackerRemoveResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_id_returns_not_found() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerRemove],
            false,
        )
        .await;

        let result =
            handle_tracker_remove(99_999, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerRemoveResponse { success, name, .. } => {
                assert!(!success);
                assert!(name.is_none());
            }
            other => panic!("Expected TrackerRemoveResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn removes_existing_row_and_returns_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerRemove],
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
                name: "ToRemove",
                enabled: true,
            })
            .await
            .expect("create");

        let result = handle_tracker_remove(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerRemoveResponse {
                success,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert_eq!(name.as_deref(), Some("ToRemove"));
            }
            other => panic!("Expected TrackerRemoveResponse, got {other:?}"),
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
