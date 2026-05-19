//! Per-target-path serialization for file mutation handlers.
//!
//! Each lock declares how other callers should behave on encounter:
//!
//! - `Wait` — short work (rename/move/copy/create_dir/delete). Others
//!   encountering a `Wait` lock queue and acquire it after release.
//! - `Fail` — long work (uploads, which can run for hours). Others
//!   encountering a `Fail` lock return `PathBusy` immediately.
//!
//! The lock's mode describes the lock itself, not the caller — callers
//! just specify which kind of lock they're claiming.
//!
//! State transitions go through a synchronous `std::sync::Mutex<Option<PathLockMode>>`
//! so that "claim the slot" and "publish the lock mode" happen in one
//! critical section. There is no window where a caller can observe a free
//! slot that another caller has already taken but not yet published.
//!
//! Scope is leaf-target only — recursive subtree races aren't covered.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::sync::{Arc, Weak};

use tokio::sync::{Mutex as AsyncMutex, Notify};

use crate::constants::ERR_FILE_LOCK_INVALID_PATH;

/// How a lock behaves when another caller encounters it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathLockMode {
    /// Others encountering this lock wait until it releases.
    Wait,
    /// Others encountering this lock return `PathBusy` immediately.
    Fail,
}

/// Returned when an acquisition encountered a `Fail` lock on the same path.
/// Carries the failed key so callers acquiring multiple locks can report
/// which side was busy.
#[derive(Debug)]
pub struct PathBusy {
    key: PathBuf,
}

impl PathBusy {
    pub fn key(&self) -> &Path {
        &self.key
    }
}

#[derive(Debug)]
struct LockEntry {
    /// `None` = free, `Some(mode)` = held with that mode.
    state: StdMutex<Option<PathLockMode>>,
    /// Woken when the held lock releases; waiters re-check `state` after waking.
    released: Notify,
}

/// Held lock. Drop clears the state to `None` and wakes any waiters.
#[derive(Debug)]
pub struct PathLockGuard {
    entry: Arc<LockEntry>,
}

impl Drop for PathLockGuard {
    fn drop(&mut self) {
        {
            // Poison-tolerant: writing `None` is always safe here.
            let mut state = self
                .entry
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *state = None;
        }
        self.entry.released.notify_waiters();
    }
}

#[derive(Default)]
pub struct PathLockMap {
    inner: AsyncMutex<HashMap<PathBuf, Weak<LockEntry>>>,
}

impl PathLockMap {
    pub fn new() -> Self {
        Self::default()
    }

    async fn entry_for(&self, key: PathBuf) -> Arc<LockEntry> {
        let mut map = self.inner.lock().await;
        match map.get(&key).and_then(Weak::upgrade) {
            Some(e) => e,
            None => {
                // Amortized GC: sweep dead `Weak`s on miss so one-shot
                // paths don't accumulate forever.
                map.retain(|_, w| w.strong_count() > 0);
                let e = Arc::new(LockEntry {
                    state: StdMutex::new(None),
                    released: Notify::new(),
                });
                map.insert(key, Arc::downgrade(&e));
                e
            }
        }
    }

    /// Acquire the per-path lock with the given mode.
    ///
    /// Returns `PathBusy` if the path is currently held by a `Fail` lock.
    /// Waits if currently held by a `Wait` lock; succeeds immediately if
    /// the path is free.
    pub async fn acquire(
        &self,
        key: PathBuf,
        mode: PathLockMode,
    ) -> Result<PathLockGuard, PathBusy> {
        let entry = self.entry_for(key.clone()).await;

        loop {
            // Construct `notified()` BEFORE inspecting state — `notify_waiters()`
            // racing the check-then-await would otherwise be lost. `enable()` is
            // defense against any future switch to `notify_one()`.
            let wait = entry.released.notified();
            tokio::pin!(wait);
            wait.as_mut().enable();

            {
                let mut state = entry
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                match *state {
                    Some(PathLockMode::Fail) => return Err(PathBusy { key }),
                    None => {
                        *state = Some(mode);
                        return Ok(PathLockGuard {
                            entry: Arc::clone(&entry),
                        });
                    }
                    Some(PathLockMode::Wait) => {
                        // Drop the state guard, then await the release.
                    }
                }
            }

            wait.as_mut().await;
        }
    }

    /// Acquire several per-path locks with the same mode. Sorts and dedupes
    /// `keys` so callers don't need to (any two callers locking the same
    /// set take them in the same order, and self-overlapping inputs like a
    /// rename to the same name collapse to a single lock). On any failure,
    /// previously-acquired guards drop with the returned Vec.
    pub async fn acquire_many(
        &self,
        mut keys: Vec<PathBuf>,
        mode: PathLockMode,
    ) -> Result<Vec<PathLockGuard>, PathBusy> {
        keys.sort();
        keys.dedup();
        let mut guards = Vec::with_capacity(keys.len());
        for key in keys {
            guards.push(self.acquire(key, mode).await?);
        }
        Ok(guards)
    }

    #[cfg(test)]
    pub async fn live_keys(&self) -> usize {
        let map = self.inner.lock().await;
        map.values().filter(|w| w.strong_count() > 0).count()
    }

    #[cfg(test)]
    pub async fn total_keys(&self) -> usize {
        self.inner.lock().await.len()
    }
}

/// Canonical lock key for a target path. Walks up to the deepest existing
/// ancestor, `canonicalize`s it, then re-appends the missing tail components.
///
/// Handles three cases with a single canonical key:
///   - All parents exist (rename/move/copy/delete/create_dir): the walk
///     stops at the parent and we get `canonicalize(parent).join(basename)`.
///   - Some parents don't exist yet (upload nested paths like
///     `dest/proj/src/main.rs`): we canonicalize the deepest existing
///     ancestor and append the to-be-created components.
///   - Aliases like `dir/./file`, `dir//file`, or paths through symlinked
///     intermediate dirs all collapse to the same on-disk key, so two
///     callers targeting the same final file always serialize.
///
/// For symlink deletion, pass the symlink path itself (not its target) —
/// the symlink's own basename is the resource being mutated.
///
/// `Path::exists()` follows symlinks, so a dangling intermediate symlink
/// passes through to the next existing ancestor — widens serialization,
/// never narrows it.
pub fn lock_key(target: &Path) -> io::Result<PathBuf> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, ERR_FILE_LOCK_INVALID_PATH);

    let parent = target.parent().ok_or_else(invalid)?;
    let basename = target.file_name().ok_or_else(invalid)?.to_os_string();

    // Walk up the parent chain to the deepest existing ancestor, collecting
    // the missing intermediate dir names. The basename itself is never fed
    // through canonicalize — that preserves the symlink-vs-target distinction
    // (delete locks on the symlink's own basename).
    let mut tail: Vec<OsString> = Vec::new();
    let mut existing: &Path = parent;
    while !existing.exists() {
        let name = existing.file_name().ok_or_else(invalid)?;
        tail.push(name.to_os_string());
        existing = existing.parent().ok_or_else(invalid)?;
    }

    let mut key = existing.canonicalize()?;
    for name in tail.into_iter().rev() {
        key.push(name);
    }
    key.push(basename);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tempfile::TempDir;

    // ---- lock_key tests ----

    #[tokio::test]
    async fn lock_key_canonicalizes_parent() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("a");
        std::fs::create_dir(&nested).unwrap();
        let target = nested.join("file.txt");
        let key = lock_key(&target).unwrap();
        assert_eq!(key.parent().unwrap(), nested.canonicalize().unwrap());
        assert_eq!(key.file_name().unwrap(), "file.txt");
    }

    #[tokio::test]
    async fn lock_key_target_need_not_exist() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("not-yet-created.txt");
        let key = lock_key(&target).unwrap();
        assert_eq!(
            key.parent().unwrap(),
            temp.path().canonicalize().unwrap().as_path()
        );
    }

    /// Nested upload pattern: `dest/proj/src/main.rs` where `proj` and `src`
    /// don't exist yet. The key should be `canonical(dest)/proj/src/main.rs`.
    #[tokio::test]
    async fn lock_key_walks_past_missing_intermediate_dirs() {
        let temp = TempDir::new().unwrap();
        let canonical = temp.path().canonicalize().unwrap();
        let target = temp.path().join("proj/src/main.rs");
        let key = lock_key(&target).unwrap();
        assert_eq!(key, canonical.join("proj/src/main.rs"));
    }

    /// Path aliases (`./`, `//`) must collapse to the same key as the plain
    /// form so two handlers locking the "same" path actually serialize.
    #[tokio::test]
    async fn lock_key_normalizes_aliases() {
        let temp = TempDir::new().unwrap();
        let sub = temp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        let plain = lock_key(&sub.join("file.txt")).unwrap();
        let dot = lock_key(&temp.path().join("sub/./file.txt")).unwrap();
        let double_slash =
            lock_key(&PathBuf::from(format!("{}//file.txt", sub.display()))).unwrap();

        assert_eq!(plain, dot);
        assert_eq!(plain, double_slash);
    }

    /// A path traversing a symlinked parent must lock the same key as the
    /// canonical-parent form, or BBS handlers and uploads will race despite
    /// "locking" the same final file.
    #[cfg(unix)]
    #[tokio::test]
    async fn lock_key_canonicalizes_through_symlinked_parent() {
        let temp = TempDir::new().unwrap();
        let real_dir = temp.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&real_dir, &link).unwrap();

        let via_link = lock_key(&link.join("file.txt")).unwrap();
        let via_real = lock_key(&real_dir.join("file.txt")).unwrap();
        assert_eq!(via_link, via_real);
    }

    /// Symlink delete: the symlink's own basename, not the symlink's target,
    /// is the resource being mutated — so its basename must be preserved
    /// verbatim in the key (no canonicalize on the leaf).
    #[cfg(unix)]
    #[tokio::test]
    async fn lock_key_preserves_symlink_basename() {
        let temp = TempDir::new().unwrap();
        let target_file = temp.path().join("target.txt");
        std::fs::write(&target_file, b"x").unwrap();
        let link = temp.path().join("link.txt");
        std::os::unix::fs::symlink(&target_file, &link).unwrap();

        let key = lock_key(&link).unwrap();
        assert_eq!(key.file_name().unwrap(), "link.txt");
    }

    #[tokio::test]
    async fn lock_key_rejects_root_path() {
        let target = PathBuf::from("/");
        assert!(lock_key(&target).is_err());
    }

    // ---- acquire / mode semantics tests ----

    fn k(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// Two `Wait` callers on the same key serialize.
    #[tokio::test]
    async fn wait_wait_same_key_serializes() {
        let locks = Arc::new(PathLockMap::new());
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let locks = locks.clone();
            let in_flight = in_flight.clone();
            let max_concurrent = max_concurrent.clone();
            handles.push(tokio::spawn(async move {
                let _g = locks.acquire(k("/x"), PathLockMode::Wait).await.unwrap();
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_concurrent.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(max_concurrent.load(Ordering::SeqCst), 1);
    }

    /// `Wait` callers on different keys run concurrently.
    #[tokio::test]
    async fn wait_wait_different_keys_run_concurrently() {
        let locks = Arc::new(PathLockMap::new());
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for i in 0..6 {
            let locks = locks.clone();
            let in_flight = in_flight.clone();
            let max_concurrent = max_concurrent.clone();
            handles.push(tokio::spawn(async move {
                let _g = locks
                    .acquire(k(&format!("/x{}", i)), PathLockMode::Wait)
                    .await
                    .unwrap();
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_concurrent.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(max_concurrent.load(Ordering::SeqCst) > 1);
    }

    /// A `Wait` caller hitting an existing `Fail` lock fails fast.
    #[tokio::test]
    async fn wait_fails_fast_behind_fail() {
        let locks = PathLockMap::new();
        let _g = locks.acquire(k("/x"), PathLockMode::Fail).await.unwrap();
        // A Fail lock would persist for the duration of a long operation;
        // we must not wait.
        let r = tokio::time::timeout(
            Duration::from_millis(50),
            locks.acquire(k("/x"), PathLockMode::Wait),
        )
        .await
        .expect("acquire returned within timeout");
        assert!(r.is_err());
    }

    /// A `Fail` caller hitting an existing `Fail` lock fails fast.
    #[tokio::test]
    async fn fail_fails_fast_behind_fail() {
        let locks = PathLockMap::new();
        let _g = locks.acquire(k("/x"), PathLockMode::Fail).await.unwrap();
        let r = locks.acquire(k("/x"), PathLockMode::Fail).await;
        assert!(r.is_err());
    }

    /// A `Fail` caller hitting an existing `Wait` lock waits and then
    /// succeeds — the encounter rule is set by the lock already in place,
    /// not by the caller's own mode.
    #[tokio::test]
    async fn fail_waits_behind_wait_then_succeeds() {
        let locks = Arc::new(PathLockMap::new());

        let holder = locks.acquire(k("/x"), PathLockMode::Wait).await.unwrap();

        let locks_w = locks.clone();
        let acquirer =
            tokio::spawn(async move { locks_w.acquire(k("/x"), PathLockMode::Fail).await });

        // Give it time to register as a waiter.
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(holder);

        let r = tokio::time::timeout(Duration::from_millis(200), acquirer)
            .await
            .expect("acquirer finished in time")
            .unwrap();
        assert!(r.is_ok(), "Fail caller should succeed after Wait releases");
    }

    /// After a `Fail` lock drops, subsequent acquires of either mode succeed.
    #[tokio::test]
    async fn lock_releases_after_fail_drops() {
        let locks = PathLockMap::new();
        {
            let _g = locks.acquire(k("/x"), PathLockMode::Fail).await.unwrap();
        }
        let g1 = locks.acquire(k("/x"), PathLockMode::Fail).await.unwrap();
        drop(g1);
        let g2 = locks.acquire(k("/x"), PathLockMode::Wait).await.unwrap();
        drop(g2);
    }

    /// Lock mode is per-key: a `Fail` on `/a` doesn't affect any acquire on `/b`.
    #[tokio::test]
    async fn lock_mode_is_per_key() {
        let locks = PathLockMap::new();
        let _g = locks.acquire(k("/a"), PathLockMode::Fail).await.unwrap();
        let r = locks.acquire(k("/b"), PathLockMode::Fail).await;
        assert!(r.is_ok());
        let r2 = locks.acquire(k("/c"), PathLockMode::Wait).await;
        assert!(r2.is_ok());
    }

    /// A queued `Wait` caller wakes promptly when the holding `Wait` drops.
    #[tokio::test]
    async fn queued_wait_wakes_after_holder_drops() {
        let locks = Arc::new(PathLockMap::new());
        let order = Arc::new(StdMutex::new(Vec::<&'static str>::new()));

        let holder = locks.acquire(k("/x"), PathLockMode::Wait).await.unwrap();
        order.lock().unwrap().push("holder_acquired");

        let locks_w = locks.clone();
        let order_w = order.clone();
        let waiter = tokio::spawn(async move {
            let _g = locks_w.acquire(k("/x"), PathLockMode::Wait).await.unwrap();
            order_w.lock().unwrap().push("waiter_acquired");
        });

        // Give the waiter time to register interest and park.
        tokio::time::sleep(Duration::from_millis(20)).await;
        order.lock().unwrap().push("releasing");
        drop(holder);

        tokio::time::timeout(Duration::from_millis(200), waiter)
            .await
            .expect("waiter completed in time")
            .unwrap();

        let seen = order.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec!["holder_acquired", "releasing", "waiter_acquired"]
        );
    }

    /// Regression for the publish-window race: a `Wait` caller must never
    /// be observed waiting behind a `Fail` lock. Drives 50 random
    /// interleavings on a multi-thread runtime and asserts the `Wait`
    /// caller always returns within a bounded budget (succeeds if the slot
    /// was free, `PathBusy` if `Fail` won the race).
    #[test]
    fn wait_does_not_block_during_fail_publish_window() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            for iter in 0..50 {
                let locks = Arc::new(PathLockMap::new());
                let key = PathBuf::from(format!("/race{}", iter));

                let l1 = locks.clone();
                let k1 = key.clone();
                let fail_call =
                    tokio::spawn(async move { l1.acquire(k1, PathLockMode::Fail).await });

                let l2 = locks.clone();
                let k2 = key.clone();
                let wait_call = tokio::spawn(async move {
                    tokio::time::timeout(
                        Duration::from_millis(100),
                        l2.acquire(k2, PathLockMode::Wait),
                    )
                    .await
                });

                // Drain `wait_call` first: if `Wait` won the race its
                // guard is sitting in the JoinHandle, and `fail_call`
                // would wait forever behind it. Drop the result (and
                // therefore the guard) before awaiting `fail_call`.
                let s = wait_call.await.unwrap();
                assert!(
                    s.is_ok(),
                    "Wait caller must not time out (publish-window race)"
                );
                drop(s);
                let _ = fail_call.await.unwrap();
            }
        });
    }

    /// `acquire_many` dedupes overlapping keys (so a rename-to-self
    /// doesn't deadlock waiting on its own held lock).
    #[tokio::test]
    async fn acquire_many_dedupes_same_key() {
        let locks = PathLockMap::new();
        let guards = locks
            .acquire_many(vec![k("/x"), k("/x")], PathLockMode::Wait)
            .await
            .unwrap();
        assert_eq!(guards.len(), 1);
    }

    /// `acquire_many` sorts before acquiring so any two callers locking
    /// the same set of keys take them in the same order.
    #[tokio::test]
    async fn acquire_many_acquires_in_lex_order() {
        // We can't directly observe the order, but we can verify the
        // operation succeeds when one of the keys would otherwise be a
        // different sort position. Trivial smoke test for the API.
        let locks = PathLockMap::new();
        let guards = locks
            .acquire_many(vec![k("/z"), k("/a"), k("/m")], PathLockMode::Wait)
            .await
            .unwrap();
        assert_eq!(guards.len(), 3);
    }

    /// On a mid-batch failure, previously-acquired guards drop with the
    /// returned `Err` — the held holder gets its slot back. The error also
    /// carries the failed key so callers acquiring multiple locks can
    /// report which side was busy.
    #[tokio::test]
    async fn acquire_many_rolls_back_on_failure() {
        let locks = PathLockMap::new();
        // Pre-acquire a Fail lock on /b so /b's acquire fails inside the batch.
        let _block = locks.acquire(k("/b"), PathLockMode::Fail).await.unwrap();

        let r = locks
            .acquire_many(vec![k("/a"), k("/b")], PathLockMode::Wait)
            .await;
        let err = r.expect_err("expected PathBusy");
        assert_eq!(err.key(), k("/b").as_path());

        // /a should be free again — partial Vec dropped on the Err return.
        let _g = locks.acquire(k("/a"), PathLockMode::Wait).await.unwrap();
    }

    /// `acquire` populates `PathBusy.key` with the key that was being attempted.
    #[tokio::test]
    async fn acquire_path_busy_carries_key() {
        let locks = PathLockMap::new();
        let _block = locks.acquire(k("/x"), PathLockMode::Fail).await.unwrap();
        let err = locks
            .acquire(k("/x"), PathLockMode::Wait)
            .await
            .expect_err("expected PathBusy");
        assert_eq!(err.key(), k("/x").as_path());
    }

    /// Dead `Weak` entries stick around in the map until the next miss
    /// triggers the retain sweep.
    #[tokio::test]
    async fn dropped_guard_collects_entry() {
        let locks = PathLockMap::new();
        {
            let _g = locks.acquire(k("/a"), PathLockMode::Wait).await.unwrap();
            assert_eq!(locks.live_keys().await, 1);
            assert_eq!(locks.total_keys().await, 1);
        }
        assert_eq!(locks.live_keys().await, 0);
        assert_eq!(locks.total_keys().await, 1);

        let _g = locks.acquire(k("/b"), PathLockMode::Wait).await.unwrap();
        assert_eq!(locks.live_keys().await, 1);
        assert_eq!(locks.total_keys().await, 1);
    }
}
