//! TrackerAdd message handler — adds a new tracker to the
//! server's tracker list.

use std::io;

use nexus_common::framing::MAX_TRACKERS_PER_SERVER;
use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_database, err_not_logged_in, err_permission_denied,
    err_tracker_endpoint_duplicate, err_tracker_name_duplicate, err_tracker_too_many,
    validate_tracker_inputs,
};
use crate::constants::{
    HANDLER_TRACKER_ADD, LOG_TRACKER_ADD_DB_ERROR, LOG_TRACKER_ADD_LIMIT_REACHED,
    LOG_TRACKER_ADD_NOT_LOGGED_IN, LOG_TRACKER_ADD_PERMISSION_DENIED, LOG_TRACKER_ADD_SUCCESS,
};
use crate::db::{CreateTrackerParams, Permission, TrackerDbError};

/// Fields for `handle_tracker_add`, mirroring the
/// `ClientMessage::TrackerAdd` variant. Bundled into a struct so
/// the handler signature stays under the workspace-wide
/// too-many-arguments limit.
pub struct TrackerAddRequest {
    pub address: String,
    pub port: u16,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub name: String,
    pub enabled: bool,
}

/// Handle a `TrackerAdd { ... }` request. Requires the
/// `tracker_add` permission. Validates inputs, inserts the row,
/// and spawns a tracker task for it (or skips spawn if disabled).
pub async fn handle_tracker_add<W>(
    request: TrackerAddRequest,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let TrackerAddRequest {
        address,
        port,
        fingerprint,
        password,
        name,
        enabled,
    } = request;

    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_ADD_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_TRACKER_ADD))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRACKER_ADD),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerAdd) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_ADD_PERMISSION_DENIED
        );
        return ctx
            .send_message(&reject_add(err_permission_denied(ctx.locale)))
            .await;
    }

    // Normalize empty-string password / fingerprint to None at the
    // protocol boundary. An empty fingerprint means "clear the pin /
    // use TOFU on next connect" — same as omitted.
    let password = password.filter(|s| !s.is_empty());
    let fingerprint = fingerprint.filter(|s| !s.is_empty());

    // Validate inputs. First failure produces a typed response.
    if let Err(error) = validate_tracker_inputs(
        ctx.locale,
        &address,
        port,
        fingerprint.as_deref(),
        password.as_deref(),
        &name,
    ) {
        return ctx.send_message(&reject_add(error)).await;
    }

    // Hold the lifecycle lock across the DB insert and the manager
    // spawn so a concurrent TrackerRemove targeting the newly-assigned
    // id can't terminate the slot before we've populated it (which
    // would leave the row in the DB with no task in the manager).
    // Drop the guard before the network write — see
    // TrackerManager::lock_lifecycle. The `MAX_TRACKERS_PER_SERVER`
    // cap is enforced atomically inside the insert (see
    // `SQL_INSERT_TRACKER`); the lock doesn't replace that.
    let response: ServerMessage = 'lifecycle: {
        let _guard = ctx.tracker_manager.lock_lifecycle().await;

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
            Err(TrackerDbError::TooMany) => {
                warn!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    max = MAX_TRACKERS_PER_SERVER,
                    "{}", LOG_TRACKER_ADD_LIMIT_REACHED
                );
                break 'lifecycle reject_add(err_tracker_too_many(
                    ctx.locale,
                    MAX_TRACKERS_PER_SERVER,
                ));
            }
            Err(TrackerDbError::EndpointDuplicate) => {
                break 'lifecycle reject_add(err_tracker_endpoint_duplicate(ctx.locale));
            }
            Err(TrackerDbError::NameDuplicate) => {
                break 'lifecycle reject_add(err_tracker_name_duplicate(ctx.locale));
            }
            Err(TrackerDbError::Other(e)) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    err = %e,
                    "{}", LOG_TRACKER_ADD_DB_ERROR
                );
                break 'lifecycle reject_add(err_database(ctx.locale));
            }
        };

        info!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            id = record.id,
            name = %record.name,
            "{}", LOG_TRACKER_ADD_SUCCESS
        );

        // Spawn a tracker task for the new row. No-op if disabled.
        ctx.tracker_manager.spawn(record.clone());

        ServerMessage::TrackerAddResponse {
            success: true,
            error: None,
            id: Some(record.id),
            name: Some(record.name),
        }
    };
    ctx.send_message(&response).await
}

fn reject_add(error: String) -> ServerMessage {
    ServerMessage::TrackerAddResponse {
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
    use crate::handlers::testing::{
        CONCURRENT_LIFECYCLE_ITERATIONS, assert_tracker_db_and_manager_consistent,
        concurrent_handler_context, create_test_context, login_user, read_server_message,
    };
    use crate::handlers::tracker_list::handle_tracker_list;
    use crate::handlers::tracker_remove::handle_tracker_remove;
    use crate::handlers::tracker_update::{TrackerUpdateRequest, handle_tracker_update};

    fn valid_request() -> TrackerAddRequest {
        TrackerAddRequest {
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
            handle_tracker_add(valid_request(), None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_tracker_add(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn happy_path_inserts_and_returns_id() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        let request = valid_request();
        let expected_name = request.name.clone();
        let result =
            handle_tracker_add(request, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert!(id.is_some());
                assert_eq!(name.as_deref(), Some(expected_name.as_str()));
            }
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_bad_address() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        // Address with embedded port — rejected by validate_public_address.
        let result = handle_tracker_add(
            TrackerAddRequest {
                address: "tracker.example.com:7500".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_zero_port() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        let result = handle_tracker_add(
            TrackerAddRequest {
                port: 0,
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_fingerprint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        let result = handle_tracker_add(
            TrackerAddRequest {
                fingerprint: Some("not-a-fingerprint".to_string()),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_empty_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        let result = handle_tracker_add(
            TrackerAddRequest {
                name: "   ".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_endpoint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        // First add.
        handle_tracker_add(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Second add with same (address, port) but different name.
        let result = handle_tracker_add(
            TrackerAddRequest {
                name: "Different Name".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
            false,
        )
        .await;

        handle_tracker_add(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Second add with same name (case-insensitive) but different address.
        let result = handle_tracker_add(
            TrackerAddRequest {
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
            ServerMessage::TrackerAddResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_when_at_limit() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd],
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

        let result = handle_tracker_add(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse { success, error, .. } => {
                assert!(!success);
                let error = error.expect("error string");
                assert!(
                    error.contains(&MAX_TRACKERS_PER_SERVER.to_string()),
                    "error should mention the cap: {error}"
                );
            }
            other => panic!("Expected TrackerAddResponse, got {other:?}"),
        }
    }

    /// Walk a full tracker admin lifecycle through the protocol layer:
    /// add → list → update (rename) → list → remove → list. Catches
    /// regressions in the handler-to-manager wiring (e.g. a future
    /// refactor that forgets to call `manager.spawn`/`replace`/`terminate`)
    /// that the per-handler unit tests would miss.
    #[tokio::test]
    async fn lifecycle_add_list_update_remove() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                db::Permission::TrackerAdd,
                db::Permission::TrackerList,
                db::Permission::TrackerEdit,
                db::Permission::TrackerRemove,
            ],
            false,
        )
        .await;

        // ---- Add ----
        handle_tracker_add(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .expect("add call");
        let id = match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAddResponse {
                success: true,
                id: Some(id),
                ..
            } => id,
            other => panic!("expected add success, got {other:?}"),
        };
        // Manager spawned a task for the new row.
        assert!(
            test_ctx.tracker_manager.status_for(id).is_some(),
            "manager should have spawned a task on add"
        );

        // ---- List shows 1 entry with original name ----
        handle_tracker_list(Some(session_id), &mut test_ctx.handler_context())
            .await
            .expect("list call");
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerListResponse {
                success: true,
                trackers,
                ..
            } => {
                assert_eq!(trackers.len(), 1);
                assert_eq!(trackers[0].id, id);
                assert_eq!(trackers[0].name, "Public");
            }
            other => panic!("expected list success, got {other:?}"),
        }

        // ---- Update (rename) ----
        handle_tracker_update(
            TrackerUpdateRequest {
                id,
                address: "tracker.example.com".to_string(),
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Renamed".to_string(),
                enabled: true,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .expect("update call");
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success: true, .. } => {}
            other => panic!("expected update success, got {other:?}"),
        }
        // Manager replaced the task — still tracking this id.
        assert!(
            test_ctx.tracker_manager.status_for(id).is_some(),
            "manager should still have a task after update"
        );

        // ---- List shows 1 entry with new name ----
        handle_tracker_list(Some(session_id), &mut test_ctx.handler_context())
            .await
            .expect("list call after update");
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerListResponse {
                success: true,
                trackers,
                ..
            } => {
                assert_eq!(trackers.len(), 1);
                assert_eq!(trackers[0].id, id);
                assert_eq!(trackers[0].name, "Renamed");
            }
            other => panic!("expected list success after update, got {other:?}"),
        }

        // ---- Remove ----
        handle_tracker_remove(id, Some(session_id), &mut test_ctx.handler_context())
            .await
            .expect("remove call");
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerRemoveResponse { success: true, .. } => {}
            other => panic!("expected remove success, got {other:?}"),
        }
        // Manager dropped the task.
        assert!(
            test_ctx.tracker_manager.status_for(id).is_none(),
            "manager should have terminated the task on remove"
        );

        // ---- List shows empty ----
        handle_tracker_list(Some(session_id), &mut test_ctx.handler_context())
            .await
            .expect("list call after remove");
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerListResponse {
                success: true,
                trackers,
                ..
            } => {
                assert!(trackers.is_empty(), "list should be empty after remove");
            }
            other => panic!("expected empty list, got {other:?}"),
        }
    }

    /// Concurrent-lifecycle regression: TrackerAdd inserts a new row
    /// and then `spawn()`s its task; a concurrent TrackerRemove
    /// targeting the just-assigned id can delete the row and call
    /// `terminate()` (a no-op while the slot is still empty), letting
    /// TrackerAdd's later `spawn()` land against a missing row.
    ///
    /// Setup is intentionally contrived to make the race targetable:
    /// SQLite `INTEGER PRIMARY KEY` advances by 1 per successful
    /// INSERT, so we can predict the id `TrackerAdd` will assign and
    /// pre-target it from the concurrent TrackerRemove. This simulates
    /// a client that guessed or observed the id immediately after
    /// creation (rather than racing two independent admin requests
    /// "naturally"). It's lower-stakes than the update / accept races
    /// but worth covering since the handler shape is identical.
    #[tokio::test]
    async fn concurrent_add_and_remove_preserve_invariant() {
        use nexus_common::framing::FrameWriter;
        use tokio::io::sink;

        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerAdd, db::Permission::TrackerRemove],
            false,
        )
        .await;

        // SQLite rowid starts at 1 on a fresh DB and advances on every
        // successful INSERT. `TrackerAdd` is the only inserter in this
        // test, and its name/address values are unique per iteration so
        // every INSERT succeeds — iteration `i` predicts id `i + 1`.
        for i in 0..CONCURRENT_LIFECYCLE_ITERATIONS {
            let predicted_id = (i as i64) + 1;
            let add_request = TrackerAddRequest {
                address: format!("added{i}.example.com"),
                port: 7510,
                fingerprint: None,
                password: None,
                name: format!("Added {i}"),
                enabled: true,
            };

            {
                let mut fw_add = FrameWriter::new(sink());
                let mut fw_remove = FrameWriter::new(sink());
                let mut ctx_add = concurrent_handler_context(&test_ctx, &mut fw_add);
                let mut ctx_remove = concurrent_handler_context(&test_ctx, &mut fw_remove);

                let _ = tokio::join!(
                    handle_tracker_add(add_request, Some(session_id), &mut ctx_add),
                    handle_tracker_remove(predicted_id, Some(session_id), &mut ctx_remove),
                );
            }

            assert_tracker_db_and_manager_consistent(&test_ctx).await;

            let _ = test_ctx.db.trackers.delete(predicted_id).await;
            test_ctx.tracker_manager.terminate(predicted_id);
        }
    }
}
