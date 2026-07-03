//! TrackerAcceptFingerprint handler: promotes a tracker's
//! `pending_fingerprint` (set on a Stage 1 cert mismatch) to its active
//! `fingerprint` and respawns the registration task with the new pin.

use std::io;

use nexus_common::ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED;
use nexus_common::protocol::ServerMessage;
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_database, err_not_logged_in, err_permission_denied,
    err_tracker_no_pending_fingerprint, err_tracker_not_found,
};
use crate::constants::{
    HANDLER_TRACKER_ACCEPT_FINGERPRINT, LOG_TRACKER_ACCEPT_FINGERPRINT_DB_ERROR,
    LOG_TRACKER_ACCEPT_FINGERPRINT_NO_PENDING, LOG_TRACKER_ACCEPT_FINGERPRINT_NOT_LOGGED_IN,
    LOG_TRACKER_ACCEPT_FINGERPRINT_PERMISSION_DENIED, LOG_TRACKER_ACCEPT_FINGERPRINT_SUCCESS,
};
use crate::db::Permission;

/// Requires `tracker_edit`. Persists the running task's in-memory
/// `pending_fingerprint` (Stage 1 only — Stage 2 must NOT populate it; see
/// `tracker/task.rs`) and respawns the task. Rejects with
/// `err-tracker-no-pending-fingerprint` when nothing is pending (including a
/// task that isn't running).
///
/// Defense-in-depth: even if a Stage 2 mismatch wrongly left
/// `pending_fingerprint` set, reject when
/// `last_error_kind == "tracker_fingerprint_intercepted"` — otherwise an
/// admin could one-click pin an attacker's cert, defeating Stage 2.
pub async fn handle_tracker_accept_fingerprint<W>(
    id: i64,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(
                &err_not_logged_in(ctx.locale),
                Some(HANDLER_TRACKER_ACCEPT_FINGERPRINT),
            )
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_not_logged_in(ctx.locale),
                    Some(HANDLER_TRACKER_ACCEPT_FINGERPRINT),
                )
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerEdit) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_PERMISSION_DENIED
        );
        return ctx
            .send_message(&reject_accept(err_permission_denied(ctx.locale)))
            .await;
    }

    // Hold the lifecycle lock across the status snapshot, the
    // update_fingerprint write, the row re-fetch, and the manager replace:
    // the pending fingerprint read here is part of the committed decision, so
    // it must share the critical section with the DB write. Drop before the
    // network write — see TrackerManager::lock_lifecycle.
    let response: ServerMessage = 'lifecycle: {
        let _guard = ctx.tracker_manager.lock_lifecycle().await;

        // No status (unknown/disabled id) → "no pending" (nothing to accept).
        // Stage 2 defense-in-depth: reject when `last_error_kind` is the
        // intercepted kind regardless of `pending_fingerprint` (see fn doc).
        let pending = match ctx.tracker_manager.status_for(id) {
            Some(status)
                if status.last_error_kind.as_deref()
                    == Some(ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED) =>
            {
                warn!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_NO_PENDING
                );
                break 'lifecycle reject_accept(err_tracker_no_pending_fingerprint(ctx.locale));
            }
            Some(status) => match status.pending_fingerprint {
                Some(fp) => fp,
                None => {
                    warn!(
                        user = %requesting_user.username,
                        ip = %ctx.peer_addr,
                        id = id,
                        "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_NO_PENDING
                    );
                    break 'lifecycle reject_accept(err_tracker_no_pending_fingerprint(ctx.locale));
                }
            },
            None => {
                warn!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_NO_PENDING
                );
                break 'lifecycle reject_accept(err_tracker_no_pending_fingerprint(ctx.locale));
            }
        };

        match ctx.db.trackers.update_fingerprint(id, &pending).await {
            Ok(true) => {}
            Ok(false) => {
                // Row removed concurrently; UPDATE hit 0 rows, nothing to roll back.
                break 'lifecycle reject_accept(err_tracker_not_found(ctx.locale));
            }
            Err(e) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    err = %e,
                    "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_DB_ERROR
                );
                break 'lifecycle reject_accept(err_database(ctx.locale));
            }
        }

        // Re-fetch for a fresh `TrackerRecord` to hand the manager. The
        // lifecycle lock blocks deletion between UPDATE and this read, but a
        // DB I/O error still has to be handled.
        let record = match ctx.db.trackers.get_by_id(id).await {
            Ok(Some(r)) => r,
            Ok(None) => break 'lifecycle reject_accept(err_tracker_not_found(ctx.locale)),
            Err(e) => {
                error!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    id = id,
                    err = %e,
                    "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_DB_ERROR
                );
                break 'lifecycle reject_accept(err_database(ctx.locale));
            }
        };

        info!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            id = record.id,
            name = %record.name,
            "{}", LOG_TRACKER_ACCEPT_FINGERPRINT_SUCCESS
        );

        // Respawn so the next refresh uses the promoted fingerprint.
        ctx.tracker_manager.replace(record.clone());

        ServerMessage::TrackerAcceptFingerprintResponse {
            success: true,
            error: None,
            id: Some(record.id),
            name: Some(record.name),
        }
    };
    ctx.send_message(&response).await
}

fn reject_accept(error: String) -> ServerMessage {
    ServerMessage::TrackerAcceptFingerprintResponse {
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

    /// The originally-pinned ("old") cert, before rotation.
    const OLD_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
         AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

    /// The ("new") cert observed after a Stage 1 mismatch, awaiting accept.
    const NEW_FINGERPRINT: &str = "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:\
         11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00";

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result =
            handle_tracker_accept_fingerprint(1, None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_tracker_accept_fingerprint(1, Some(session_id), &mut test_ctx.handler_context())
                .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAcceptFingerprintResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAcceptFingerprintResponse, got {other:?}"),
        }
    }

    /// Unknown id → "no pending fingerprint" (no separate "not found": from
    /// the admin's view there's nothing to accept either way).
    #[tokio::test]
    async fn unknown_id_returns_no_pending() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let result = handle_tracker_accept_fingerprint(
            99_999,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAcceptFingerprintResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAcceptFingerprintResponse, got {other:?}"),
        }
    }

    /// Defense-in-depth: with `pending_fingerprint` set, still reject when
    /// `last_error_kind` is the Stage 2 `tracker_fingerprint_intercepted`
    /// kind — accepting would pin the attacker's cert. Simulates a regression
    /// where the task wrongly leaves `pending_fingerprint` set on Stage 2.
    #[tokio::test]
    async fn rejects_stage2_intercepted_even_with_pending_set() {
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
                fingerprint: Some(OLD_FINGERPRINT),
                password: None,
                name: "Public",
                enabled: true,
            })
            .await
            .expect("create");
        test_ctx.tracker_manager.spawn(created.clone());
        // Regression: pending set + Stage 2 kind; defense-in-depth must catch it.
        test_ctx
            .tracker_manager
            .set_pending_fingerprint_for_test(created.id, NEW_FINGERPRINT.to_string());
        test_ctx
            .tracker_manager
            .set_last_error_kind_for_test(created.id, ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED);

        let result = handle_tracker_accept_fingerprint(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAcceptFingerprintResponse { success, error, .. } => {
                assert!(!success, "Stage 2 must be rejected");
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAcceptFingerprintResponse, got {other:?}"),
        }

        // OLD pin must still stand — the attacker's cert was not promoted.
        let row = test_ctx
            .db
            .trackers
            .get_by_id(created.id)
            .await
            .expect("get_by_id")
            .expect("row should still exist");
        assert_eq!(row.fingerprint.as_deref(), Some(OLD_FINGERPRINT));
    }

    /// Existing tracker row but no pending fingerprint set on the task:
    /// rejected with the "no pending" error.
    #[tokio::test]
    async fn existing_row_without_pending_returns_no_pending() {
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
                password: None,
                name: "Public",
                enabled: true,
            })
            .await
            .expect("create");
        // Status exists but pending_fingerprint is None: the "running but
        // nothing to accept" branch.
        test_ctx.tracker_manager.spawn(created.clone());

        let result = handle_tracker_accept_fingerprint(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAcceptFingerprintResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerAcceptFingerprintResponse, got {other:?}"),
        }
    }

    /// Success / cert-rotation path: row pinned with OLD, task stashed NEW in
    /// `pending_fingerprint` after a Stage 1 mismatch, admin accepts. Handler
    /// must replace (not append) the pin, respawn, and return id + name.
    #[tokio::test]
    async fn happy_path_replaces_old_fingerprint_with_pending() {
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
                fingerprint: Some(OLD_FINGERPRINT),
                password: None,
                name: "Public",
                enabled: true,
            })
            .await
            .expect("create");
        // Guards against a regression where `create` ignores `fingerprint`.
        assert_eq!(created.fingerprint.as_deref(), Some(OLD_FINGERPRINT));

        test_ctx.tracker_manager.spawn(created.clone());
        test_ctx
            .tracker_manager
            .set_pending_fingerprint_for_test(created.id, NEW_FINGERPRINT.to_string());

        let result = handle_tracker_accept_fingerprint(
            created.id,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerAcceptFingerprintResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert_eq!(id, Some(created.id));
                assert_eq!(name.as_deref(), Some("Public"));
            }
            other => panic!("Expected TrackerAcceptFingerprintResponse, got {other:?}"),
        }

        // Old pin must be gone, replaced by the previously-pending one.
        let row = test_ctx
            .db
            .trackers
            .get_by_id(created.id)
            .await
            .expect("get_by_id")
            .expect("row should still exist");
        assert_eq!(row.fingerprint.as_deref(), Some(NEW_FINGERPRINT));
        assert_ne!(row.fingerprint.as_deref(), Some(OLD_FINGERPRINT));

        // Task still present (the replace respawned it).
        assert!(test_ctx.tracker_manager.status_for(created.id).is_some());
    }

    /// Concurrent-lifecycle regression: accept (read pending → update → refetch
    /// → replace) races a TrackerRemove that could delete the row at any await.
    /// `lock_lifecycle` serializes them; the invariant helper confirms it.
    #[tokio::test]
    async fn concurrent_accept_fingerprint_and_remove_preserve_invariant() {
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
            let address = format!("rotate{i}.example.com");
            let name = format!("Rotate {i}");
            let record = test_ctx
                .db
                .trackers
                .create(CreateTrackerParams {
                    address: &address,
                    port: 7510,
                    fingerprint: Some(OLD_FINGERPRINT),
                    password: None,
                    name: &name,
                    enabled: true,
                })
                .await
                .expect("create");
            test_ctx.tracker_manager.spawn(record.clone());
            // Stage 1 mismatch: task stashed a new cert. Without the lock,
            // accept and remove interleave between the DB write and the replace.
            test_ctx
                .tracker_manager
                .set_pending_fingerprint_for_test(record.id, NEW_FINGERPRINT.to_string());
            let id = record.id;

            {
                let mut fw_accept = FrameWriter::new(sink());
                let mut fw_remove = FrameWriter::new(sink());
                let mut ctx_accept = concurrent_handler_context(&test_ctx, &mut fw_accept);
                let mut ctx_remove = concurrent_handler_context(&test_ctx, &mut fw_remove);

                let _ = tokio::join!(
                    handle_tracker_accept_fingerprint(id, Some(session_id), &mut ctx_accept),
                    handle_tracker_remove(id, Some(session_id), &mut ctx_remove),
                );
            }

            assert_tracker_db_and_manager_consistent(&test_ctx).await;

            let _ = test_ctx.db.trackers.delete(id).await;
            test_ctx.tracker_manager.terminate(id);
        }
    }
}
