//! TrackerCreate message handler — adds a new tracker to the
//! server's publisher list.

use std::io;

use nexus_common::framing::MAX_TRACKERS_PER_SERVER;
use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
    err_tracker_endpoint_duplicate, err_tracker_name_duplicate, err_tracker_too_many,
    validate_tracker_inputs,
};
use crate::constants::{
    LOG_TRACKER_CREATE_DB_ERROR, LOG_TRACKER_CREATE_LIMIT_REACHED,
    LOG_TRACKER_CREATE_NOT_LOGGED_IN, LOG_TRACKER_CREATE_PERMISSION_DENIED,
    LOG_TRACKER_CREATE_SUCCESS,
};
use crate::db::{CreateTrackerParams, Permission, TrackerDbError};

/// Fields for `handle_tracker_create`, mirroring the
/// `ClientMessage::TrackerCreate` variant. Bundled into a struct so
/// the handler signature stays under the workspace-wide
/// too-many-arguments limit.
pub struct TrackerCreateRequest {
    pub address: String,
    pub port: u16,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub name: String,
    pub enabled: bool,
}

/// Handle a `TrackerCreate { ... }` request. Requires the
/// `tracker_create` permission. Validates inputs, inserts the row,
/// and spawns a publisher task for it (or skips spawn if disabled).
pub async fn handle_tracker_create<W>(
    request: TrackerCreateRequest,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let TrackerCreateRequest {
        address,
        port,
        fingerprint,
        password,
        name,
        enabled,
    } = request;

    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrackerCreate"))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrackerCreate"))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerCreate) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_CREATE_PERMISSION_DENIED
        );
        let response = ServerMessage::TrackerCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // Normalize empty-string password to None at the protocol boundary.
    let password = password.filter(|s| !s.is_empty());

    // Validate inputs. First failure produces a typed response.
    if let Err(error) = validate_tracker_inputs(
        ctx.locale,
        &address,
        port,
        fingerprint.as_deref(),
        password.as_deref(),
        &name,
    ) {
        return ctx.send_message(&reject_create(error)).await;
    }

    // Cap on configured rows. Sized to match the
    // `TrackerListResponse` frame budget so the admin UI can always
    // serialize the full list — without this guard, an admin who
    // adds the 65th tracker breaks their own management view.
    match ctx.db.trackers.count().await {
        Ok(n) if n >= MAX_TRACKERS_PER_SERVER => {
            warn!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                count = n,
                max = MAX_TRACKERS_PER_SERVER,
                "{}", LOG_TRACKER_CREATE_LIMIT_REACHED
            );
            let response = ServerMessage::TrackerCreateResponse {
                success: false,
                error: Some(err_tracker_too_many(ctx.locale, MAX_TRACKERS_PER_SERVER)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Ok(_) => {}
        Err(e) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                err = %e,
                "{}", LOG_TRACKER_CREATE_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("TrackerCreate"))
                .await;
        }
    }

    // Insert into the DB.
    let result = ctx
        .db
        .trackers
        .create(CreateTrackerParams {
            address: &address,
            port,
            fingerprint: fingerprint.as_deref(),
            password: password.as_deref(),
            name: &name,
            enabled,
        })
        .await;

    let record = match result {
        Ok(r) => r,
        Err(TrackerDbError::EndpointDuplicate) => {
            let response = ServerMessage::TrackerCreateResponse {
                success: false,
                error: Some(err_tracker_endpoint_duplicate(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::NameDuplicate) => {
            let response = ServerMessage::TrackerCreateResponse {
                success: false,
                error: Some(err_tracker_name_duplicate(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::Other(e)) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                err = %e,
                "{}", LOG_TRACKER_CREATE_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("TrackerCreate"))
                .await;
        }
    };

    info!(
        user = %requesting_user.username,
        ip = %ctx.peer_addr,
        id = record.id,
        name = %record.name,
        "{}", LOG_TRACKER_CREATE_SUCCESS
    );

    // Spawn a publisher task for the new row. No-op if disabled.
    ctx.tracker_manager.spawn(record.clone());

    let response = ServerMessage::TrackerCreateResponse {
        success: true,
        error: None,
        id: Some(record.id),
        name: Some(record.name),
    };
    ctx.send_message(&response).await
}

fn reject_create(error: String) -> ServerMessage {
    ServerMessage::TrackerCreateResponse {
        success: false,
        error: Some(error),
        id: None,
        name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    fn valid_request() -> TrackerCreateRequest {
        TrackerCreateRequest {
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: None,
            password: None,
            name: "Public".to_string(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result =
            handle_tracker_create(valid_request(), None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn happy_path_inserts_and_returns_id() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let request = valid_request();
        let expected_name = request.name.clone();
        let result =
            handle_tracker_create(request, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert!(id.is_some());
                assert_eq!(name.as_deref(), Some(expected_name.as_str()));
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_bad_address() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        // Address with embedded port — rejected by validate_public_address.
        let result = handle_tracker_create(
            TrackerCreateRequest {
                address: "tracker.example.com:7500".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_zero_port() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let result = handle_tracker_create(
            TrackerCreateRequest {
                port: 0,
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_fingerprint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let result = handle_tracker_create(
            TrackerCreateRequest {
                fingerprint: Some("not-a-fingerprint".to_string()),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_empty_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let result = handle_tracker_create(
            TrackerCreateRequest {
                name: "   ".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_endpoint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        // First create.
        handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Second create with same (address, port) but different name.
        let result = handle_tracker_create(
            TrackerCreateRequest {
                name: "Different Name".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Second create with same name (case-insensitive) but different address.
        let result = handle_tracker_create(
            TrackerCreateRequest {
                address: "other.example.com".to_string(),
                name: "public".to_string(), // case-insensitive collision with "Public"
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_when_at_limit() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        // Bulk-seed up to the cap directly via the DB to keep the test
        // fast (the handler path would also work but is per-call slower).
        for i in 0..MAX_TRACKERS_PER_SERVER {
            test_ctx
                .db
                .trackers
                .create(crate::db::CreateTrackerParams {
                    address: &format!("tracker-{i}.example.com"),
                    port: 7510,
                    fingerprint: None,
                    password: None,
                    name: &format!("seed-{i}"),
                    enabled: true,
                })
                .await
                .expect("seed row");
        }

        let result = handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                let error = error.expect("error string");
                assert!(
                    error.contains(&MAX_TRACKERS_PER_SERVER.to_string()),
                    "error should mention the cap: {error}"
                );
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }
}
