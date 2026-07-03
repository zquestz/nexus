//! Per-tracker task manager. One `TrackerHandle` per active task,
//! indexed by tracker row `id`. Admin handlers spawn/replace/terminate;
//! status is exposed via shared `Arc<RwLock<TrackerStatus>>` for
//! `TrackerInfo` composition.
//!
//! Two locks with intentionally different runtime types:
//! - **`inner: std::sync::Mutex`** — the handle map. Critical sections
//!   are short and never cross an `await`; a sync `Mutex` is correct.
//! - **`lifecycle: tokio::sync::Mutex`** — must be tokio: handlers hold
//!   it across DB awaits, and a sync guard held across `.await` would
//!   block the worker and make the future `!Send`. Don't "fix" to std.
//!
//! No `derive(Clone)` — callers wrap in `Arc<TrackerManager>`.

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
#[cfg(test)]
use std::borrow::Cow;

/// Live handle to one tracker task. Held in the manager's HashMap
/// keyed by tracker `id`.
struct TrackerHandle {
    /// Aborting drops the task at its next await point (resources clean
    /// up via `Drop`); also `await`-able during graceful shutdown.
    join: JoinHandle<()>,
    /// Task is sole writer; manager/handlers read for `TrackerInfo`.
    status: Arc<RwLock<TrackerStatus>>,
    /// Display name snapshotted at spawn, for `terminate`/`replace` logs
    /// where the full record isn't in scope.
    name: String,
}

/// Per-tracker task supervisor.
///
/// API mirrors the data lifecycle:
/// - [`bootstrap`] at startup loads enabled rows and spawns one task each.
/// - [`spawn`] is called after `TrackerAdd` inserts a row.
/// - [`replace`] is called after `TrackerUpdate` (or
///   `TrackerAcceptFingerprint`) modifies a row.
/// - [`terminate`] is called after `TrackerRemove` removes a row.
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
    /// Serializes lifecycle-changing handlers across their DB write +
    /// follow-up `spawn`/`replace`/`terminate`. `inner` only guards one
    /// method, not the DB↔manager boundary; without this a `TrackerUpdate`
    /// can lose to a concurrent `TrackerRemove` and orphan a task. See
    /// [`Self::lock_lifecycle`].
    lifecycle: Arc<tokio::sync::Mutex<()>>,
    context: Arc<TrackerContext>,
}

impl TrackerManager {
    /// Starts with no tasks running; call [`Self::bootstrap`] to load
    /// existing enabled rows from the DB.
    #[must_use]
    pub fn new(context: Arc<TrackerContext>) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            lifecycle: Arc::new(tokio::sync::Mutex::new(())),
            context,
        }
    }

    /// Acquire the lifecycle coordination lock.
    ///
    /// Lifecycle-changing handlers **must** hold this across *both* their
    /// DB mutation and follow-up manager call — that pairing is the
    /// invariant. Otherwise a successful DB write can lose the runtime to
    /// a concurrent handler, orphaning or dropping a task.
    ///
    /// Read-only handlers (`TrackerList`, `TrackerEdit`) skip it; the brief
    /// display inconsistency is acceptable. Drop *before* the response —
    /// the lock covers the DB+manager section, not the network write.
    pub(crate) async fn lock_lifecycle(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.lifecycle.lock().await
    }

    /// Load all enabled tracker rows from the DB and spawn one
    /// tracker task per row. Called once at server startup.
    ///
    /// # Errors
    ///
    /// `sqlx::Error` if the DB read fails (catastrophic; startup should
    /// fail). Spawn never fails — a task that can't connect just retries.
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

    /// Abort the existing task (if any) and spawn a fresh one if enabled.
    ///
    /// Abort + respawn happen under one map lock so a concurrent
    /// `spawn`/`replace`/`terminate` for the same id can't interleave —
    /// preventing the old task's last DB write racing the new task's
    /// first read.
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
        // Disabled = admin paused this tracker; leave the slot empty.
        if record.enabled {
            self.spawn_locked(&mut map, record);
        }
    }

    /// Idempotent — calling on an unknown id is a no-op.
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

    /// Returns `None` if no task is running for this id (disabled
    /// tracker, or no row).
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

    /// Snapshot of runtime status for every running task.
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

    /// Test-only: set `pending_fingerprint` directly, bypassing the Stage 1
    /// mismatch flow so the `TrackerAcceptFingerprint`-success path can be
    /// tested without a rotated-cert `MockTracker`. No-op if no task for `id`.
    #[cfg(test)]
    pub(crate) fn set_pending_fingerprint_for_test(&self, id: i64, fingerprint: String) {
        let map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        if let Some(handle) = map.get(&id) {
            let mut status = handle
                .status
                .write()
                .expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
            status.pending_fingerprint = Some(fingerprint);
        }
    }

    /// Test-only: set `last_error_kind` directly to simulate specific error
    /// states (e.g. Stage 2 interception) without a `MockTracker`. Companion
    /// to [`Self::set_pending_fingerprint_for_test`]. No-op if no task for `id`.
    #[cfg(test)]
    pub(crate) fn set_last_error_kind_for_test(&self, id: i64, kind: impl Into<Cow<'static, str>>) {
        let map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        if let Some(handle) = map.get(&id) {
            let mut status = handle
                .status
                .write()
                .expect(EXPECT_TRACKER_STATUS_LOCK_POISONED);
            status.last_error_kind = Some(kind.into());
        }
    }

    /// Abort every task and await each handle so they've actually finished
    /// before returning (`abort()` itself doesn't wait). Take the joins out
    /// under the lock (so concurrent calls don't race), then await unlocked.
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

    /// Spawn helper that acquires the map lock itself, for callers
    /// ([`Self::spawn`], [`Self::bootstrap`]) that don't hold it.
    fn spawn_internal(&self, record: TrackerRecord) {
        let mut map = self
            .inner
            .lock()
            .expect(EXPECT_TRACKER_MANAGER_LOCK_POISONED);
        self.spawn_locked(&mut map, record);
    }

    /// Spawn-and-insert under an already-held map lock, so [`Self::replace`]
    /// can abort + respawn atomically. Any pre-existing handle for this id
    /// (shouldn't happen) is aborted defensively.
    fn spawn_locked(&self, map: &mut HashMap<i64, TrackerHandle>, record: TrackerRecord) {
        let id = record.id;
        let name = record.name.clone();
        let status = Arc::new(RwLock::new(TrackerStatus::default()));
        let task_status = Arc::clone(&status);
        let task_context = Arc::clone(&self.context);
        let task_lifecycle = Arc::clone(&self.lifecycle);

        let join = tokio::spawn(async move {
            task::run_with_lifecycle_lock(record, task_status, task_context, task_lifecycle).await;
        });

        if let Some(old) = map.insert(id, TrackerHandle { join, status, name }) {
            // Defensive: shouldn't happen given the call pattern; abort the
            // old task to avoid a leak.
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

    /// Minimal `TrackerContext` for manager tests. Tasks fail to connect
    /// (no tracker running), but assertions only verify the HashMap shape.
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

        // Replace (still enabled) should leave a handle present; the abort
        // isn't directly observable.
        manager.replace(record.clone());
        let second_status = manager
            .status_for(record.id)
            .expect("second status present");
        // Asserts a handle still exists, not that it's the same Arc.
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
        // Wedged tracker (accepts TLS, never replies): the task's 30s
        // handshake read timeout must yield to abort sooner.
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

        // Wait until the task is parked in the wedge: last_attempted_at is
        // set at the top of every cycle, so non-None means past idle.
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

        // Shutdown must return via abort, not the task's 30s timeout.
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
