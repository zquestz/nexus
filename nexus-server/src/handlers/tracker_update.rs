//! TrackerUpdate message handler — replaces a tracker's configuration
//! row and re-spawns its tracker task.

use std::io;

use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_database, err_not_logged_in, err_permission_denied,
    err_tracker_endpoint_duplicate, err_tracker_name_duplicate, err_tracker_not_found,
    validate_tracker_inputs,
};
use crate::constants::{
    HANDLER_TRACKER_UPDATE, LOG_TRACKER_UPDATE_DB_ERROR, LOG_TRACKER_UPDATE_NOT_LOGGED_IN,
    LOG_TRACKER_UPDATE_PERMISSION_DENIED, LOG_TRACKER_UPDATE_SUCCESS,
};
use crate::db::{Permission, TrackerDbError, UpdateTrackerParams};

/// Fields for `handle_tracker_update`, mirroring the
/// `ClientMessage::TrackerUpdate` variant. Bundled into a struct so
/// the handler signature stays under the workspace-wide
/// too-many-arguments limit.
pub struct TrackerUpdateRequest {
    pub id: i64,
    pub address: String,
    pub port: u16,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub name: String,
    pub enabled: bool,
}

/// Handle a `TrackerUpdate { ... }` request. Requires the
/// `tracker_edit` permission. Validates inputs, replaces the row, and
/// asks the manager to abort the existing task and spawn a fresh one
/// (or just abort, if the row was disabled).
pub async fn handle_tracker_update<W>(
    request: TrackerUpdateRequest,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let TrackerUpdateRequest {
        id,
        address,
        port,
        fingerprint,
        password,
        name,
        enabled,
    } = request;

    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRACKER_UPDATE))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRACKER_UPDATE),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerEdit) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_UPDATE_PERMISSION_DENIED
        );
        return ctx
            .send_message(&reject_update(err_permission_denied(ctx.locale)))
            .await;
    }

    // Normalize empty-string password / fingerprint to None at the
    // protocol boundary. An empty fingerprint means "clear the pin /
    // use TOFU on next connect" — same as omitted.
    let password = password.filter(|s| !s.is_empty());
    let fingerprint = fingerprint.filter(|s| !s.is_empty());

    if let Err(error) = validate_tracker_inputs(
        ctx.locale,
        &address,
        port,
        fingerprint.as_deref(),
        password.as_deref(),
        &name,
    ) {
        return ctx.send_message(&reject_update(error)).await;
    }

    let result = ctx
        .db
        .trackers
        .update(
            id,
            UpdateTrackerParams {
                address: &address,
                port,
                fingerprint: fingerprint.as_deref(),
                password: password.as_deref(),
                name: &name,
                enabled,
            },
        )
        .await;

    let record = match result {
        Ok(Some(r)) => r,
        Ok(None) => {
            return ctx
                .send_message(&reject_update(err_tracker_not_found(ctx.locale)))
                .await;
        }
        Err(TrackerDbError::EndpointDuplicate) => {
            return ctx
                .send_message(&reject_update(err_tracker_endpoint_duplicate(ctx.locale)))
                .await;
        }
        Err(TrackerDbError::NameDuplicate) => {
            return ctx
                .send_message(&reject_update(err_tracker_name_duplicate(ctx.locale)))
                .await;
        }
        Err(TrackerDbError::TooMany) => {
            // `update()` doesn't enforce the row cap — only `create()`
            // does. This arm is unreachable in practice; route it
            // through the generic DB-error path so an unexpected
            // future regression doesn't silently fall through.
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                id = id,
                "{}", LOG_TRACKER_UPDATE_DB_ERROR
            );
            let response = reject_update(err_database(ctx.locale));
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::Other(e)) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                id = id,
                err = %e,
                "{}", LOG_TRACKER_UPDATE_DB_ERROR
            );
            let response = reject_update(err_database(ctx.locale));
            return ctx.send_message(&response).await;
        }
    };

    info!(
        user = %requesting_user.username,
        ip = %ctx.peer_addr,
        id = record.id,
        name = %record.name,
        enabled = record.enabled,
        "{}", LOG_TRACKER_UPDATE_SUCCESS
    );

    // Abort the old task and spawn a fresh one (or just abort, if the
    // record is now disabled). The manager handles both cases.
    ctx.tracker_manager.replace(record.clone());

    let response = ServerMessage::TrackerUpdateResponse {
        success: true,
        error: None,
        id: Some(record.id),
        name: Some(record.name),
    };
    ctx.send_message(&response).await
}

fn reject_update(error: String) -> ServerMessage {
    ServerMessage::TrackerUpdateResponse {
        success: false,
        error: Some(error),
        id: None,
        name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, CreateTrackerParams};
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    async fn seed_tracker(test_ctx: &mut crate::handlers::testing::TestContext) -> i64 {
        test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "tracker.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Original",
                enabled: true,
            })
            .await
            .expect("create")
            .id
    }

    fn valid_request(id: i64) -> TrackerUpdateRequest {
        TrackerUpdateRequest {
            id,
            address: "tracker2.example.com".to_string(),
            port: 7520,
            fingerprint: None,
            password: None,
            name: "Renamed".to_string(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result =
            handle_tracker_update(valid_request(1), None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_tracker_update(
            valid_request(1),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn happy_path_replaces_row() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;
        let id = seed_tracker(&mut test_ctx).await;

        let request = valid_request(id);
        let expected_name = request.name.clone();
        let result =
            handle_tracker_update(request, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse {
                success,
                id: response_id,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert_eq!(response_id, Some(id));
                assert_eq!(name.as_deref(), Some(expected_name.as_str()));
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
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

        let result = handle_tracker_update(
            valid_request(99_999),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse {
                success,
                id: response_id,
                ..
            } => {
                assert!(!success);
                assert!(response_id.is_none());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_bad_address() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;
        let id = seed_tracker(&mut test_ctx).await;

        let result = handle_tracker_update(
            TrackerUpdateRequest {
                address: "tracker.example.com:7500".to_string(),
                ..valid_request(id)
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_endpoint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        // Seed two rows; try to update row #2 to take row #1's endpoint.
        let _first = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "first.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "First",
                enabled: true,
            })
            .await
            .expect("create first");
        let second = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "second.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Second",
                enabled: true,
            })
            .await
            .expect("create second");

        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: second.id,
                address: "first.example.com".to_string(),
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Second".to_string(),
                enabled: true,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let _first = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "first.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "First",
                enabled: true,
            })
            .await
            .expect("create first");
        let second = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "second.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Second",
                enabled: true,
            })
            .await
            .expect("create second");

        // Update second to take first's name (case-insensitive collision).
        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: second.id,
                address: "second.example.com".to_string(),
                port: 7510,
                fingerprint: None,
                password: None,
                name: "first".to_string(),
                enabled: true,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }
}
