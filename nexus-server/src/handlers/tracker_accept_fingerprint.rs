//! TrackerAcceptFingerprint message handler — promotes a tracker's
//! `pending_fingerprint` (set after a Stage 1 TLS-cert mismatch) to its
//! active `fingerprint`, then respawns the registration task with the
//! new pin.

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

/// Handle a `TrackerAcceptFingerprint { id }` request. Requires the
/// `tracker_edit` permission.
///
/// Reads the running task's in-memory `pending_fingerprint` (set on
/// Stage 1 mismatch — Stage 2 mismatches must NOT populate it; see
/// `tracker/task.rs`), persists it as the new active `fingerprint`,
/// then replaces the running task so the next refresh cycle uses the
/// new pin. Rejects with `err-tracker-no-pending-fingerprint` if
/// nothing is pending — including the case where the task isn't
/// running at all.
///
/// Defense-in-depth: even if the upstream invariant breaks and a
/// Stage 2 mismatch leaves `pending_fingerprint` set, the handler
/// rejects when `last_error_kind == "tracker_fingerprint_intercepted"`.
/// Accepting in that case would let an admin one-click pin an
/// attacker's certificate, defeating the Stage 2 protection entirely.
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

    // Hold the lifecycle lock across the status_for snapshot, the
    // update_fingerprint write, the row re-fetch, and the manager
    // replace. The pending fingerprint we read out of status_for is
    // part of the decision being committed, so it must be in the same
    // critical section as the DB write. Drop the guard before the
    // network write — see TrackerManager::lock_lifecycle.
    let response: ServerMessage = 'lifecycle: {
        let _guard = ctx.tracker_manager.lock_lifecycle().await;

        // Pull the pending fingerprint out of the running task's status.
        // No status (id unknown or task disabled) → reject as "no pending"
        // since there's no observation to accept either way.
        //
        // Defense-in-depth: if `last_error_kind` indicates a Stage 2
        // mismatch (`tracker_fingerprint_intercepted`), reject regardless
        // of `pending_fingerprint`. The tracker task is supposed to leave
        // `pending_fingerprint` unset on Stage 2, but a regression there
        // would otherwise let an admin one-click pin an attacker's cert.
        // This guard catches that class of bug at the handler boundary.
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
                // Row vanished before we acquired the lifecycle lock
                // (concurrent `TrackerRemove`). The UPDATE affected 0
                // rows, so no state change to roll back.
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

        // Re-fetch the row so we can hand a fresh `TrackerRecord` to the
        // manager (it owns task lifecycle keyed by the full record).
        // Under the lifecycle lock the row can't be deleted out from
        // under us between the UPDATE and this read, but a DB-layer
        // error (I/O) still has to be handled.
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

        // Abort the existing task and respawn with the new pin so the next
        // refresh cycle uses the freshly-promoted fingerprint.
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

    /// The fingerprint a tracker was originally pinned with — the
    /// "old" cert before rotation.
    const OLD_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
         AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

    /// The fingerprint observed after a Stage 1 mismatch — the "new"
    /// cert the admin must accept to keep registering with the tracker.
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

    /// Unknown id: no running task → rejected as "no pending fingerprint".
    /// (We don't surface a separate "not found" here because the
    /// observable state from the admin's perspective is identical:
    /// nothing to accept either way.)
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

    /// Defense-in-depth: even if `pending_fingerprint` is set, the
    /// handler must reject when `last_error_kind` is the Stage 2
    /// `tracker_fingerprint_intercepted` kind. Stage 2 is an
    /// active-interception signal — accepting the TLS-observed cert
    /// would let an admin pin the attacker's certificate. The tracker
    /// task is supposed to leave `pending_fingerprint` unset on
    /// Stage 2 (see `tracker/task.rs`); this test simulates a
    /// regression where that invariant is broken upstream.
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
        // Simulate the regression: pending fingerprint set + Stage 2
        // error kind. Defense-in-depth must catch this.
        test_ctx
            .tracker_manager
            .set_pending_fingerprint_for_test(created.id, NEW_FINGERPRINT.to_string());
        test_ctx.tracker_manager.set_last_error_kind_for_test(
            created.id,
            ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED.to_string(),
        );

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

        // DB row's fingerprint must NOT have been changed — the OLD
        // pin still stands, the attacker's cert was not promoted.
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
        // Spawn a task so a status exists, but its pending_fingerprint
        // starts as None — exercising the "task running but nothing to
        // accept" branch.
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

    /// Success path — the cert-rotation scenario this flow is for:
    /// tracker row was already pinned with `OLD_FINGERPRINT`, the
    /// running task observed `NEW_FINGERPRINT` after a Stage 1 mismatch
    /// and stashed it in `pending_fingerprint`, admin accepts. Handler
    /// must replace the OLD pin with the NEW value (not just append),
    /// respawn the task, and return success with the row's id + name.
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
        // Sanity-check the starting state so a future regression that
        // makes `create` ignore `fingerprint` would surface here.
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

        // Manager should still have a task for this id (the replace
        // re-spawned with the new pin; pre-replace status is gone).
        assert!(test_ctx.tracker_manager.status_for(created.id).is_some());
    }

    /// Concurrent-lifecycle regression: TrackerAcceptFingerprint reads
    /// a pending fingerprint from manager status, runs
    /// `update_fingerprint`, re-fetches the row, then `replace()`s the
    /// task — and at any of those await points a concurrent
    /// TrackerRemove could delete the row and terminate the task.
    /// `TrackerManager::lock_lifecycle` keeps these two ops sequential;
    /// the invariant helper confirms it.
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
            // Simulate a Stage 1 mismatch: the running task observed a
            // new cert and stashed its fingerprint. Without the lock,
            // TrackerAcceptFingerprint and TrackerRemove can interleave
            // between the DB write and the manager replace.
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
