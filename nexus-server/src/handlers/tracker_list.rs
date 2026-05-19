//! TrackerList message handler — returns all configured trackers
//! with their current runtime status.

use std::io;

use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, warn};

use super::{
    HandlerContext, compose_tracker_info, err_database, err_not_logged_in, err_permission_denied,
};
use crate::constants::{
    HANDLER_TRACKER_LIST, LOG_TRACKER_LIST_DB_ERROR, LOG_TRACKER_LIST_NOT_LOGGED_IN,
    LOG_TRACKER_LIST_PERMISSION_DENIED,
};
use crate::db::Permission;

/// Handle a `TrackerList` request. Requires the `tracker_list`
/// permission (admins inherit it).
pub async fn handle_tracker_list<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_LIST_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRACKER_LIST))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRACKER_LIST),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerList) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_LIST_PERMISSION_DENIED
        );
        let response = ServerMessage::TrackerListResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            trackers: Vec::new(),
        };
        return ctx.send_message(&response).await;
    }

    let rows = match ctx.db.trackers.list_all().await {
        Ok(r) => r,
        Err(e) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                err = %e,
                "{}", LOG_TRACKER_LIST_DB_ERROR
            );
            let response = ServerMessage::TrackerListResponse {
                success: false,
                error: Some(err_database(ctx.locale)),
                trackers: Vec::new(),
            };
            return ctx.send_message(&response).await;
        }
    };

    let mut statuses = ctx.tracker_manager.status_all();
    let trackers: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let id = row.id;
            compose_tracker_info(row, statuses.remove(&id), ctx.locale)
        })
        .collect();

    let response = ServerMessage::TrackerListResponse {
        success: true,
        error: None,
        trackers,
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
        let result = handle_tracker_list(None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_tracker_list(Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerListResponse {
                success, trackers, ..
            } => {
                assert!(!success);
                assert!(trackers.is_empty());
            }
            other => panic!("Expected TrackerListResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn empty_list_for_admin() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_tracker_list(Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerListResponse {
                success, trackers, ..
            } => {
                assert!(success);
                assert_eq!(trackers.len(), 0);
            }
            other => panic!("Expected TrackerListResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn returns_configured_trackers_sorted_by_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerList],
            false,
        )
        .await;

        // Insert in non-alphabetical order to verify sort.
        test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "z.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "zeta",
                enabled: true,
            })
            .await
            .expect("create zeta");
        test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "a.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Alpha",
                enabled: true,
            })
            .await
            .expect("create Alpha");

        let result = handle_tracker_list(Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerListResponse {
                success, trackers, ..
            } => {
                assert!(success);
                assert_eq!(trackers.len(), 2);
                assert_eq!(trackers[0].name, "Alpha");
                assert_eq!(trackers[1].name, "zeta");
                // No tracker task spawned in tests → all runtime fields default.
                assert!(!trackers[0].connected);
                assert!(trackers[0].last_connected_at.is_none());
            }
            other => panic!("Expected TrackerListResponse, got {other:?}"),
        }
    }
}
