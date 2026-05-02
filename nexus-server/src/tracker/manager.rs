//! Per-tracker task manager.
//!
//! Holds one `TrackerHandle` per active tracker task, indexed by the
//! tracker row's `id`. Admin handlers call into the manager to spawn /
//! replace / terminate tasks; the manager exposes runtime status via
//! shared `Arc<RwLock<TrackerStatus>>` handles for compose-into-
//! `TrackerInfo`-response paths.
//!
//! Locks held in this module are `std::sync::Mutex` (not
//! `tokio::sync`) — every critical section is short and never crosses
//! an `await`. The map gets locked, an entry mutated, the lock dropped
//! before any further async work.
//!
//! The manager itself is `Clone`-able via `Arc` from the caller side
//! (it holds shared inner state), but we don't `derive(Clone)` —
//! callers wrap the whole struct in `Arc<TrackerManager>` and clone
//! that.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use super::context::TrackerContext;
use super::status::TrackerStatus;
use super::task;
use crate::constants::{
    EXPECT_TRACKER_MANAGER_LOCK_POISONED, EXPECT_TRACKER_STATUS_LOCK_POISONED,
    LOG_TRACKER_REGISTRATION_BOOTSTRAP_DONE, LOG_TRACKER_REGISTRATION_HANDLE_REPLACED,
    LOG_TRACKER_REGISTRATION_SPAWN_SKIPPED, LOG_TRACKER_REGISTRATION_TASK_ABORTED,
};
use crate::db::TrackerRecord;

/// Live handle to one tracker task. Held in the manager's HashMap
/// keyed by tracker `id`.
struct TrackerHandle {
    /// Cancellation handle. Aborting drops the task at its next await
    /// point; the TLS stream and any other resources clean up via
    /// their `Drop` impls. The `JoinHandle` is also `await`-able
    /// during graceful shutdown to wait for the task to actually
    /// finish.
    join: JoinHandle<()>,
    /// Shared with the task itself. The task is the sole writer; the
    /// manager / handlers read for `TrackerInfo` composition.
    status: Arc<RwLock<TrackerStatus>>,
    /// Snapshot of the tracker's display name at spawn time, for logs
    /// in `terminate` / `replace` where the full record isn't in scope.
    name: String,
}

/// Per-tracker task supervisor.
///
/// API mirrors the data lifecycle:
/// - [`bootstrap`] at startup loads enabled rows and spawns one task each.
/// - [`spawn`] is called after `TrackerCreate` inserts a row.
/// - [`replace`] is called after `TrackerUpdate` modifies a row.
/// - [`terminate`] is called after `TrackerDelete` removes a row.
/// - [`status_for`] / [`status_all`] feed admin response composition.
/// - [`shutdown`] aborts every task during server shutdown.
///
/// [`bootstrap`]: TrackerManager::bootstrap
/// [`spawn`]: TrackerManager::spawn
/// [`replace`]: TrackerManager::replace
/// [`terminate`]: TrackerManager::terminate
/// [`status_for`]: TrackerManager::status_for
/// [`status_all`]: TrackerManager::status_all
/// [`shutdown`]: TrackerManager::shutdown
pub struct TrackerManager {
    inner: Mutex<HashMap<i64, TrackerHandle>>,
    context: Arc<TrackerContext>,
}

impl TrackerManager {
    /// Construct a fresh manager with no tasks running. Call
    /// [`Self::bootstrap`] to load existing enabled rows from the DB.
    #[must_use]
    pub fn new(context: Arc<TrackerContext>) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            context,
        }
    }

    /// Load all enabled tracker rows from the DB and spawn one
    /// tracker task per row. Called once at server startup.
    ///
    /// # Errors
    ///
    /// Returns `sqlx::Error` if the DB read fails (catastrophic;
    /// startup should fail). Task spawn itself never fails — a task
    /// that fails to connect just transitions to backoff/retry.
    pub async fn bootstrap(&self) -> Result<(), sqlx::Error> {
        let rows = self.context.db.trackers.list_all().await?;
        let total = rows.len();
        let mut spawned = 0usize;
        for record in rows {
            if record.enabled {
                self.spawn_internal(record);
                spawned += 1;
            }
        }
        info!(
            spawned = spawned,
            skipped = total - spawned,
            "{}",
            LOG_TRACKER_REGISTRATION_BOOTSTRAP_DONE
        );
        Ok(())
    }

    /// Spawn a tracker task for a freshly-created tracker. Called
    /// by the `TrackerCreate` handler after the DB insert succeeds.
    /// No-op if the record is disabled.
    pub fn spawn(&self, record: TrackerRecord) {
        if !record.enabled {
            debug!(
                id = record.id,
                name = %record.name,
                "{}", LOG_TRACKER_REGISTRATION_SPAWN_SKIPPED
            );
            return;
        }
        self.spawn_internal(record);
    }

    /// Abort the existing task (if any) and spawn a fresh one if the
    /// new record is enabled. Called by the `TrackerUpdate` handler
    /// after the DB update succeeds.
    ///
    /// The abort and the optional respawn happen under a single map
    /// lock so a concurrent `spawn` / `replace` / `terminate` for the
    /// same id cannot interleave between the two halves.
    pub fn replace(&self, record: TrackerRecord) {
        let mut map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        if let Some(old) = map.remove(&record.id) {
            old.join.abort();
            debug!(
                id = record.id,
                name = %old.name,
                "{}", LOG_TRACKER_REGISTRATION_TASK_ABORTED
            );
        }
        // Spawn fresh only if still enabled. Disabled record means the
        // admin paused this tracker; leave the slot empty.
        if record.enabled {
            self.spawn_locked(&mut map, record);
        }
    }

    /// Abort the tracker task for this tracker and remove its
    /// handle from the map. Idempotent — calling on an unknown id is
    /// a no-op.
    pub fn terminate(&self, id: i64) {
        let mut map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        if let Some(handle) = map.remove(&id) {
            handle.join.abort();
            debug!(
                id = id,
                name = %handle.name,
                "{}", LOG_TRACKER_REGISTRATION_TASK_ABORTED
            );
        }
    }

    /// Read the runtime status of one tracker. Returns `None` if no
    /// task is running for this id (disabled tracker, or no row).
    #[must_use]
    pub fn status_for(&self, id: i64) -> Option<TrackerStatus> {
        let map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        map.get(&id).map(|h| {
            h.status
                .read()
                .expect(EXPECT_TRACKER_STATUS_LOCK_POISONED)
                .clone()
        })
    }

    /// Snapshot of runtime status for every running task. Used by the
    /// `TrackerList` admin handler to compose `TrackerInfo` rows.
    #[must_use]
    pub fn status_all(&self) -> HashMap<i64, TrackerStatus> {
        let map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        map.iter()
            .map(|(id, h)| {
                let status = h
                    .status
                    .read()
                    .expect(EXPECT_TRACKER_STATUS_LOCK_POISONED)
                    .clone();
                (*id, status)
            })
            .collect()
    }

    /// Abort every running task and await each `JoinHandle` to ensure
    /// the tasks have actually finished before returning. Called from
    /// `main.rs` shutdown branch.
    ///
    /// `JoinHandle::abort()` returns immediately (doesn't wait); we
    /// await afterwards to give the runtime a chance to actually drop
    /// each task at its next poll. Both lock and await are required:
    /// take the joins out under the lock (so concurrent calls don't
    /// race), drop the lock, then await each handle.
    pub async fn shutdown(&self) {
        let joins: Vec<JoinHandle<()>> = {
            let mut map = self
                .inner
                .lock()
                .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
            map.drain().map(|(_, h)| h.join).collect()
        };
        for j in &joins {
            j.abort();
        }
        for j in joins {
            // `Err(JoinError::Cancelled)` is the expected outcome.
            let _ = j.await;
        }
    }

    /// Internal spawn helper that acquires the map lock itself. Used
    /// by [`Self::spawn`] and [`Self::bootstrap`] where the caller
    /// doesn't already hold the lock.
    fn spawn_internal(&self, record: TrackerRecord) {
        let mut map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        self.spawn_locked(&mut map, record);
    }

    /// Spawn-and-insert under an already-held map lock. Used by
    /// [`Self::replace`] to perform abort + respawn atomically.
    /// If a handle already existed for this id (which shouldn't
    /// happen — callers either terminate first or hold the lock
    /// across a remove), the old one is aborted defensively.
    fn spawn_locked(&self, map: &mut HashMap<i64, TrackerHandle>, record: TrackerRecord) {
        let id = record.id;
        let name = record.name.clone();
        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task_status = Arc::clone(&status);
        let task_context = Arc::clone(&self.context);

        let join = tokio::spawn(async move {
            task::run(record, task_status, task_context).await;
        });

        if let Some(old) = map.insert(id, TrackerHandle { join, status, name }) {
            // Defensive: shouldn't happen given the documented call
            // pattern, but if it does, the old task gets aborted to
            // avoid a leak.
            warn!(
                id = id,
                name = %old.name,
                "{}", LOG_TRACKER_REGISTRATION_HANDLE_REPLACED
            );
            old.join.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::testing::{MockBehavior, MockTracker};
    use super::*;
    use crate::db::testing::create_test_db;
    use crate::db::{CreateTrackerParams, Database};
    use crate::users::UserManager;

    /// Build a minimal `TrackerContext` for manager tests. The
    /// tracker task itself will fail to connect (no tracker is
    /// running) but the manager-level assertions don't depend on
    /// task progress — they just verify the HashMap shape.
    async fn test_context() -> Arc<TrackerContext> {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        Arc::new(TrackerContext {
            db,
            user_manager,
            server_fingerprint: "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
                 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
            server_port: 7500,
            server_websocket_port: None,
        })
    }

    fn create_params<'a>(address: &'a str, name: &'a str) -> CreateTrackerParams<'a> {
        CreateTrackerParams {
            address,
            port: 7510,
            fingerprint: None,
            password: None,
            name,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn spawn_creates_handle_in_map() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        let record = context
            .db
            .trackers
            .create(create_params("a.example.com", "A"))
            .await
            .expect("create");

        manager.spawn(record.clone());
        assert!(manager.status_for(record.id).is_some());
    }

    #[tokio::test]
    async fn spawn_no_op_for_disabled_record() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        let record = context
            .db
            .trackers
            .create(CreateTrackerParams {
                enabled: false,
                ..create_params("a.example.com", "A")
            })
            .await
            .expect("create");

        manager.spawn(record);
        // No status entry — manager only tracks running tasks.
        assert!(manager.status_for(1).is_none());
        assert!(manager.status_all().is_empty());
    }

    #[tokio::test]
    async fn terminate_removes_handle() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        let record = context
            .db
            .trackers
            .create(create_params("a.example.com", "A"))
            .await
            .expect("create");

        manager.spawn(record.clone());
        assert!(manager.status_for(record.id).is_some());

        manager.terminate(record.id);
        assert!(manager.status_for(record.id).is_none());
    }

    #[tokio::test]
    async fn terminate_unknown_id_is_noop() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));
        // Should not panic.
        manager.terminate(99_999);
    }

    #[tokio::test]
    async fn replace_aborts_and_respawns() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        let record = context
            .db
            .trackers
            .create(create_params("a.example.com", "A"))
            .await
            .expect("create");

        manager.spawn(record.clone());
        let first_status = manager.status_for(record.id).expect("first status present");

        // Replace with the same record but enabled still — should
        // produce a fresh handle (we can't directly observe the
        // abort, but the status entry should still be present).
        manager.replace(record.clone());
        let second_status = manager
            .status_for(record.id)
            .expect("second status present");
        // Both should be at default (no progress yet); the test asserts
        // the structural fact that a handle still exists, not that
        // they're the same Arc (they shouldn't be).
        assert!(!first_status.connected);
        assert!(!second_status.connected);
    }

    #[tokio::test]
    async fn replace_with_disabled_aborts_without_respawn() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        let record = context
            .db
            .trackers
            .create(create_params("a.example.com", "A"))
            .await
            .expect("create");

        manager.spawn(record.clone());
        assert!(manager.status_for(record.id).is_some());

        let disabled = TrackerRecord {
            enabled: false,
            ..record
        };
        manager.replace(disabled.clone());
        // Old task aborted, no new task spawned (disabled).
        assert!(manager.status_for(disabled.id).is_none());
    }

    #[tokio::test]
    async fn bootstrap_skips_disabled_rows() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        let enabled = context
            .db
            .trackers
            .create(create_params("a.example.com", "Enabled"))
            .await
            .expect("create enabled");
        let _disabled = context
            .db
            .trackers
            .create(CreateTrackerParams {
                enabled: false,
                ..create_params("b.example.com", "Disabled")
            })
            .await
            .expect("create disabled");

        manager.bootstrap().await.expect("bootstrap");

        // Only the enabled row has a handle.
        assert!(manager.status_for(enabled.id).is_some());
        assert_eq!(manager.status_all().len(), 1);
    }

    #[tokio::test]
    async fn shutdown_clears_all_handles() {
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));

        for i in 0..3 {
            let record = context
                .db
                .trackers
                .create(create_params(&format!("{i}.example.com"), &format!("T{i}")))
                .await
                .expect("create");
            manager.spawn(record);
        }
        assert_eq!(manager.status_all().len(), 3);

        manager.shutdown().await;
        assert!(manager.status_all().is_empty());
    }

    #[tokio::test]
    async fn shutdown_aborts_mid_handshake() {
        // Wedged tracker: accepts TLS, never sends HandshakeResponse.
        // The tracker task's H-1 timeout would otherwise hold this for 30s
        // on its own — abort must propagate through that await sooner.
        let mock = MockTracker::start(MockBehavior {
            wedge_after_tls: true,
            ..Default::default()
        })
        .await;
        let context = test_context().await;
        let manager = TrackerManager::new(Arc::clone(&context));
        let address = mock.addr.ip().to_string();
        let record = context
            .db
            .trackers
            .create(CreateTrackerParams {
                address: &address,
                port: mock.addr.port(),
                fingerprint: None,
                password: None,
                name: "Wedge",
                enabled: true,
            })
            .await
            .expect("create");
        manager.spawn(record);

        // Wait until the task has begun a cycle (i.e. is parked in the
        // handshake-await wedge). last_attempted_at is set at the top
        // of every cycle, so any non-None value means we're past idle.
        let started = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if manager
                    .status_all()
                    .values()
                    .any(|s| s.last_attempted_at.is_some())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(started.is_ok(), "task should have started a cycle");

        // Shutdown must return promptly via abort, not block on the
        // tracker task's 30s response timeout.
        tokio::time::timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown should complete despite parked task");
        assert!(manager.status_all().is_empty());

        mock.stop().await;
    }

    #[tokio::test]
    async fn status_for_unknown_id_returns_none() {
        let context = test_context().await;
        let manager = TrackerManager::new(context);
        assert!(manager.status_for(99_999).is_none());
    }
}
