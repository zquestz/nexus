//! TrackerUpdate message handler — patches a tracker's configuration
//! row and re-spawns its tracker task.

use std::io;

use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_database, err_no_fields_to_update, err_not_logged_in,
    err_permission_denied, err_tracker_endpoint_duplicate, err_tracker_name_duplicate,
    err_tracker_not_found, validate_tracker_inputs,
};
use crate::constants::{
    HANDLER_TRACKER_UPDATE, LOG_TRACKER_UPDATE_DB_ERROR, LOG_TRACKER_UPDATE_NOT_LOGGED_IN,
    LOG_TRACKER_UPDATE_PERMISSION_DENIED, LOG_TRACKER_UPDATE_SUCCESS,
};
use crate::db::{Permission, TrackerDbError, UpdateTrackerParams};

/// Bundled into a struct to keep the handler signature under the
/// too-many-arguments limit.
pub struct TrackerUpdateRequest {
    pub id: i64,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub name: Option<String>,
    pub enabled: Option<bool>,
}

/// Requires `tracker_edit`. Patches the row, then asks the manager to
/// respawn the task (or just abort, if the row was disabled).
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

    if address.is_none()
        && port.is_none()
        && fingerprint.is_none()
        && password.is_none()
        && name.is_none()
        && enabled.is_none()
    {
        return ctx
            .send_message(&reject_update(err_no_fields_to_update(ctx.locale)))
            .await;
    }

    // Empty fingerprint means "clear the pin / use TOFU on next connect".
    // Empty password means "open tracker".
    // Omitted fields are merged from the current row below.
    let fingerprint = fingerprint.map(|value| if value.is_empty() { None } else { Some(value) });
    let password = password.map(|value| if value.is_empty() { None } else { Some(value) });

    // Lifecycle lock spans the read/merge + DB update + manager replace so a
    // concurrent TrackerRemove can't delete the row between them (orphan task).
    // Guard drops before the network write.
    let response: ServerMessage = 'lifecycle: {
        let _guard = ctx.tracker_manager.lock_lifecycle().await;

        let existing = match ctx.db.trackers.get_by_id(id).await {
            Ok(Some(record)) => record,
            Ok(None) => break 'lifecycle reject_update(err_tracker_not_found(ctx.locale)),
            Err(e) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    err = %e,
                    "{}",
                    LOG_TRACKER_UPDATE_DB_ERROR
                );
                break 'lifecycle reject_update(err_database(ctx.locale));
            }
        };

        let address = address.unwrap_or(existing.address);
        let port = port.unwrap_or(existing.port);
        let fingerprint = fingerprint.unwrap_or(existing.fingerprint);
        let password = password.unwrap_or(existing.password);
        let name = name.unwrap_or(existing.name);
        let enabled = enabled.unwrap_or(existing.enabled);

        if let Err(error) = validate_tracker_inputs(
            ctx.locale,
            &address,
            port,
            fingerprint.as_deref(),
            password.as_deref(),
            &name,
        ) {
            break 'lifecycle reject_update(error);
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
            Ok(None) => break 'lifecycle reject_update(err_tracker_not_found(ctx.locale)),
            Err(TrackerDbError::EndpointDuplicate) => {
                break 'lifecycle reject_update(err_tracker_endpoint_duplicate(ctx.locale));
            }
            Err(TrackerDbError::NameDuplicate) => {
                break 'lifecycle reject_update(err_tracker_name_duplicate(ctx.locale));
            }
            Err(TrackerDbError::TooMany) => {
                // `update()` doesn't enforce the row cap; unreachable in practice.
                // Routed through the generic DB-error path as a safety net.
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    "{}", LOG_TRACKER_UPDATE_DB_ERROR
                );
                break 'lifecycle reject_update(err_database(ctx.locale));
            }
            Err(TrackerDbError::Other(e)) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    err = %e,
                    "{}", LOG_TRACKER_UPDATE_DB_ERROR
                );
                break 'lifecycle reject_update(err_database(ctx.locale));
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

        ctx.tracker_manager.replace(record.clone());

        ServerMessage::TrackerUpdateResponse {
            success: true,
            error: None,
            id: Some(record.id),
            name: Some(record.name),
        }
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
    use crate::handlers::testing::{
        CONCURRENT_LIFECYCLE_ITERATIONS, assert_tracker_db_and_manager_consistent,
        concurrent_handler_context, create_test_context, login_user, read_server_message,
    };

    const TEST_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
         AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

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
            address: Some("tracker2.example.com".to_string()),
            port: Some(7520),
            fingerprint: None,
            password: None,
            name: Some("Renamed".to_string()),
            enabled: Some(true),
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
        let expected_name = request.name.clone().unwrap();
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
    async fn partial_update_preserves_omitted_fields() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let tracker = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "tracker.example.com",
                port: 7510,
                fingerprint: Some(TEST_FINGERPRINT),
                password: Some("secret"),
                name: "Original",
                enabled: true,
            })
            .await
            .expect("create");

        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: tracker.id,
                address: None,
                port: None,
                fingerprint: None,
                password: None,
                name: Some("Renamed".to_string()),
                enabled: None,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }

        let updated = test_ctx
            .db
            .trackers
            .get_by_id(tracker.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.address, "tracker.example.com");
        assert_eq!(updated.port, 7510);
        assert_eq!(updated.fingerprint.as_deref(), Some(TEST_FINGERPRINT));
        assert_eq!(updated.password.as_deref(), Some("secret"));
        assert_eq!(updated.name, "Renamed");
        assert!(updated.enabled);
    }

    #[tokio::test]
    async fn partial_update_empty_fingerprint_and_password_clear() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let tracker = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "tracker.example.com",
                port: 7510,
                fingerprint: Some(TEST_FINGERPRINT),
                password: Some("secret"),
                name: "Original",
                enabled: true,
            })
            .await
            .expect("create");

        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: tracker.id,
                address: None,
                port: None,
                fingerprint: Some(String::new()),
                password: Some(String::new()),
                name: None,
                enabled: None,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }

        let updated = test_ctx
            .db
            .trackers
            .get_by_id(tracker.id)
            .await
            .unwrap()
            .unwrap();
        assert!(updated.fingerprint.is_none());
        assert!(updated.password.is_none());
        assert_eq!(updated.address, "tracker.example.com");
        assert_eq!(updated.name, "Original");
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
    async fn rejects_no_fields() {
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
                id,
                address: None,
                port: None,
                fingerprint: None,
                password: None,
                name: None,
                enabled: None,
            },
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
                address: Some("tracker.example.com:7500".to_string()),
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
                address: Some("first.example.com".to_string()),
                port: Some(7510),
                fingerprint: None,
                password: None,
                name: Some("Second".to_string()),
                enabled: Some(true),
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
                address: Some("second.example.com".to_string()),
                port: Some(7510),
                fingerprint: None,
                password: None,
                name: Some("first".to_string()),
                enabled: Some(true),
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
    async fn rejects_duplicate_name_unicode() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "first.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Équipe",
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
                name: "Other",
                enabled: true,
            })
            .await
            .expect("create second");

        // Rename `second` to a Unicode-case variant of "Équipe" — collides via
        // the folded name_lower, which ASCII NOCASE would miss.
        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: second.id,
                address: Some("second.example.com".to_string()),
                port: Some(7510),
                fingerprint: None,
                password: None,
                name: Some("équipe".to_string()),
                enabled: Some(true),
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

    /// Concurrent TrackerUpdate + TrackerRemove must not leave an orphan
    /// task (respawned for a deleted row) or a missing task. The lifecycle
    /// lock prevents both; the invariant helper catches either failure.
    #[tokio::test]
    async fn concurrent_update_and_remove_preserve_invariant() {
        use nexus_common::framing::FrameWriter;
        use tokio::io::sink;

        use crate::handlers::handle_tracker_remove;

        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit, db::Permission::TrackerRemove],
            false,
        )
        .await;

        for i in 0..CONCURRENT_LIFECYCLE_ITERATIONS {
            let address = format!("seed{i}.example.com");
            let name = format!("Seed {i}");
            let record = test_ctx
                .db
                .trackers
                .create(CreateTrackerParams {
                    address: &address,
                    port: 7510,
                    fingerprint: None,
                    password: None,
                    name: &name,
                    enabled: true,
                })
                .await
                .expect("seed tracker create");
            test_ctx.tracker_manager.spawn(record.clone());
            let id = record.id;

            let update_request = TrackerUpdateRequest {
                id,
                address: Some(format!("renamed{i}.example.com")),
                port: Some(7511),
                fingerprint: None,
                password: None,
                name: Some(format!("Renamed {i}")),
                enabled: Some(true),
            };

            {
                let mut fw_update = FrameWriter::new(sink());
                let mut fw_remove = FrameWriter::new(sink());
                let mut ctx_update = concurrent_handler_context(&test_ctx, &mut fw_update);
                let mut ctx_remove = concurrent_handler_context(&test_ctx, &mut fw_remove);

                let _ = tokio::join!(
                    handle_tracker_update(update_request, Some(session_id), &mut ctx_update),
                    handle_tracker_remove(id, Some(session_id), &mut ctx_remove),
                );
            }

            assert_tracker_db_and_manager_consistent(&test_ctx).await;

            // Best-effort cleanup so the next iteration starts clean.
            let _ = test_ctx.db.trackers.delete(id).await;
            test_ctx.tracker_manager.terminate(id);
        }
    }
}
