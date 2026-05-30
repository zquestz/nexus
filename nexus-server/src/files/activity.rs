//! Fail-fast activity tracking for filesystem operations.
//!
//! This is a Nexus-level reservation layer, not an OS lock. It gives handlers
//! a cheap way to reject sensitive operations when a related path is already
//! active, while leaving final correctness to atomic filesystem calls.
//!
//! # Concurrency invariant
//!
//! The `append_*_ops` builders form small store-before-load protocols across
//! `Exact`, `Below`, and `Read` counters. Each operation must publish its own
//! activity with `Increment`/`Reserve` before it checks the counter that would
//! make a conflicting operation unsafe. For example, a reader increments
//! `Read(path)` before checking `Exact(path)`, while a writer reserves
//! `Exact(path)` before checking `Read(path)`. Directory/descendant exclusion
//! uses the same pattern with `Exact(dir)` and `Below(dir)`.
//!
//! These pairs are intentionally `SeqCst`. A race may fail fast on both sides,
//! which is acceptable, but it must never let both conflicting operations enter.
//! Do not reorder builder ops or weaken the atomic ordering without updating the
//! proof and the concurrent stress tests below.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, Weak};

use crate::constants::{ERR_FILE_ACTIVITY_INVALID_PATH, FILES_SHARED_DIR, FILES_USERS_DIR};

const ACTIVITY_MAP_RETAIN_THRESHOLD: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum FileActivityKey {
    /// The path itself is being mutated or reserved.
    Exact(PathBuf),
    /// Some descendant under this directory is active.
    Below(PathBuf),
    /// The path is being read by one or more transfers.
    Read(PathBuf),
}

impl FileActivityKey {
    fn path(&self) -> &Path {
        match self {
            Self::Exact(path) | Self::Below(path) | Self::Read(path) => path,
        }
    }
}

#[derive(Debug)]
pub struct FileActivityBusy {
    path: PathBuf,
}

impl FileActivityBusy {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug)]
struct HeldCounter {
    counter: Arc<AtomicUsize>,
}

#[derive(Debug)]
pub struct FileActivityGuard {
    held: Vec<HeldCounter>,
}

impl Drop for FileActivityGuard {
    fn drop(&mut self) {
        release_all(&mut self.held);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ActivityOp {
    Increment(FileActivityKey),
    Reserve(FileActivityKey),
    CheckClear(FileActivityKey),
}

impl ActivityOp {
    fn key(&self) -> &FileActivityKey {
        match self {
            Self::Increment(key) | Self::Reserve(key) | Self::CheckClear(key) => key,
        }
    }
}

#[derive(Default)]
pub struct FileActivityMap {
    inner: StdMutex<HashMap<FileActivityKey, Weak<AtomicUsize>>>,
}

impl FileActivityMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter activity for a new or existing child path.
    ///
    /// Descendant counters are incremented for protected ancestors, then those
    /// ancestors' exact counters are checked. This order prevents a child
    /// operation and a parent directory mutation from both proceeding.
    pub async fn try_enter_child_path(
        &self,
        file_root: &Path,
        target: &Path,
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        self.try_enter_child_paths(file_root, &[target.to_path_buf()])
            .await
    }

    pub async fn try_enter_child_paths(
        &self,
        file_root: &Path,
        targets: &[PathBuf],
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let mut target_keys = Vec::with_capacity(targets.len());
        for target in targets {
            target_keys.push(activity_key(target).await?);
        }
        self.try_enter_child_keys(file_root, target_keys).await
    }

    pub async fn try_enter_child_keys(
        &self,
        file_root: &Path,
        target_keys: Vec<PathBuf>,
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let file_root = tokio::fs::canonicalize(file_root).await?;
        let mut ops = Vec::new();
        for target_key in target_keys {
            append_child_ops(&mut ops, &file_root, target_key);
        }
        Ok(self.try_enter_ops(ops))
    }

    /// Enter shared read activity for existing file paths.
    ///
    /// Readers can coexist with other readers of the same file, but they fail
    /// fast against active writers of the same file or protected ancestors.
    pub async fn try_enter_read_paths(
        &self,
        file_root: &Path,
        targets: &[PathBuf],
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let mut target_keys = Vec::with_capacity(targets.len());
        for target in targets {
            target_keys.push(activity_key(target).await?);
        }
        self.try_enter_read_keys(file_root, target_keys).await
    }

    pub async fn try_enter_read_keys(
        &self,
        file_root: &Path,
        target_keys: Vec<PathBuf>,
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let file_root = tokio::fs::canonicalize(file_root).await?;
        let mut ops = Vec::new();
        for target_key in target_keys {
            append_read_ops(&mut ops, &file_root, target_key);
        }
        Ok(self.try_enter_ops(ops))
    }

    /// Enter descendant activity under a directory without reserving the
    /// directory itself.
    ///
    /// This is for long operations that create or touch children over time,
    /// such as multi-file uploads. Other child activity can proceed, but an
    /// exact mutation of the directory or a protected ancestor fails fast.
    pub async fn try_enter_descendant_path(
        &self,
        file_root: &Path,
        directory: &Path,
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let file_root = tokio::fs::canonicalize(file_root).await?;
        let directory_key = activity_key(directory).await?;
        let mut ops = Vec::new();
        append_descendant_ops(&mut ops, &file_root, directory_key);
        Ok(self.try_enter_ops(ops))
    }

    /// Enter activity for mutating a directory itself.
    ///
    /// This protects parent ancestors like `try_enter_child_path`, reserves the
    /// directory's exact path, and rejects if any descendant is active below it.
    pub async fn try_enter_directory_path(
        &self,
        file_root: &Path,
        directory: &Path,
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        self.try_enter_directory_paths(file_root, &[directory.to_path_buf()])
            .await
    }

    pub async fn try_enter_directory_paths(
        &self,
        file_root: &Path,
        directories: &[PathBuf],
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let mut directory_keys = Vec::with_capacity(directories.len());
        for directory in directories {
            directory_keys.push(activity_key(directory).await?);
        }
        self.try_enter_directory_keys(file_root, directory_keys)
            .await
    }

    pub async fn try_enter_directory_keys(
        &self,
        file_root: &Path,
        directory_keys: Vec<PathBuf>,
    ) -> io::Result<Result<FileActivityGuard, FileActivityBusy>> {
        let file_root = tokio::fs::canonicalize(file_root).await?;
        let mut ops = Vec::new();
        for directory_key in directory_keys {
            append_directory_ops(&mut ops, &file_root, directory_key);
        }
        Ok(self.try_enter_ops(ops))
    }

    fn counter_for(&self, key: &FileActivityKey) -> Arc<AtomicUsize> {
        let mut map = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match map.get(key).and_then(Weak::upgrade) {
            Some(counter) => counter,
            None => {
                if map.len() >= ACTIVITY_MAP_RETAIN_THRESHOLD {
                    map.retain(|_, counter| counter.strong_count() > 0);
                }
                let counter = Arc::new(AtomicUsize::new(0));
                map.insert(key.clone(), Arc::downgrade(&counter));
                counter
            }
        }
    }

    fn try_enter_ops(&self, ops: Vec<ActivityOp>) -> Result<FileActivityGuard, FileActivityBusy> {
        let mut held = Vec::new();

        for op in dedupe_ops(ops) {
            let counter = self.counter_for(op.key());
            match op {
                ActivityOp::Increment(_) => {
                    // These enter/check pairs are Dekker-style store-before-load
                    // protocols. Keep them SeqCst: fail-fast races may reject both
                    // contenders, but must not let both proceed on weak-memory CPUs.
                    counter.fetch_add(1, Ordering::SeqCst);
                    held.push(HeldCounter { counter });
                }
                ActivityOp::Reserve(_) => {
                    let previous = counter.fetch_add(1, Ordering::SeqCst);
                    held.push(HeldCounter { counter });
                    if previous != 0 {
                        let path = op.key().path().to_path_buf();
                        release_all(&mut held);
                        return Err(FileActivityBusy { path });
                    }
                }
                ActivityOp::CheckClear(_) => {
                    if counter.load(Ordering::SeqCst) != 0 {
                        let path = op.key().path().to_path_buf();
                        release_all(&mut held);
                        return Err(FileActivityBusy { path });
                    }
                }
            }
        }

        Ok(FileActivityGuard { held })
    }
}

fn append_child_ops(ops: &mut Vec<ActivityOp>, file_root: &Path, target_key: PathBuf) {
    let ancestors = protected_ancestor_keys(file_root, &target_key);
    // Child vs ancestor directory: publish `Below(ancestor)` before checking
    // `Exact(ancestor)`. Then reserve the target before checking active readers.
    for ancestor in &ancestors {
        ops.push(ActivityOp::Increment(FileActivityKey::Below(
            ancestor.clone(),
        )));
    }
    for ancestor in &ancestors {
        ops.push(ActivityOp::CheckClear(FileActivityKey::Exact(
            ancestor.clone(),
        )));
    }
    ops.push(ActivityOp::Reserve(FileActivityKey::Exact(
        target_key.clone(),
    )));
    ops.push(ActivityOp::CheckClear(FileActivityKey::Read(target_key)));
}

fn append_read_ops(ops: &mut Vec<ActivityOp>, file_root: &Path, target_key: PathBuf) {
    let ancestors = protected_ancestor_keys(file_root, &target_key);
    // Read vs writer/directory: publish ancestor `Below` and target `Read`
    // before checking ancestor/target `Exact` counters.
    for ancestor in &ancestors {
        ops.push(ActivityOp::Increment(FileActivityKey::Below(
            ancestor.clone(),
        )));
    }
    ops.push(ActivityOp::Increment(FileActivityKey::Read(
        target_key.clone(),
    )));
    for ancestor in &ancestors {
        ops.push(ActivityOp::CheckClear(FileActivityKey::Exact(
            ancestor.clone(),
        )));
    }
    ops.push(ActivityOp::CheckClear(FileActivityKey::Exact(target_key)));
}

fn append_descendant_ops(ops: &mut Vec<ActivityOp>, file_root: &Path, directory_key: PathBuf) {
    let activity_keys = protected_descendant_keys(file_root, &directory_key);
    // Descendant activity is shared: publish all protected `Below` keys before
    // checking the matching directory `Exact` counters.
    for activity_key in &activity_keys {
        ops.push(ActivityOp::Increment(FileActivityKey::Below(
            activity_key.clone(),
        )));
    }
    for activity_key in &activity_keys {
        ops.push(ActivityOp::CheckClear(FileActivityKey::Exact(
            activity_key.clone(),
        )));
    }
}

fn append_directory_ops(ops: &mut Vec<ActivityOp>, file_root: &Path, directory_key: PathBuf) {
    let ancestors = protected_ancestor_keys(file_root, &directory_key);
    // Directory mutation first protects its own ancestors, then reserves the
    // directory `Exact` counter before checking readers and descendants below it.
    for ancestor in &ancestors {
        ops.push(ActivityOp::Increment(FileActivityKey::Below(
            ancestor.clone(),
        )));
    }
    for ancestor in &ancestors {
        ops.push(ActivityOp::CheckClear(FileActivityKey::Exact(
            ancestor.clone(),
        )));
    }
    ops.push(ActivityOp::Reserve(FileActivityKey::Exact(
        directory_key.clone(),
    )));
    ops.push(ActivityOp::CheckClear(FileActivityKey::Read(
        directory_key.clone(),
    )));
    ops.push(ActivityOp::CheckClear(FileActivityKey::Below(
        directory_key,
    )));
}

fn release_all(held: &mut Vec<HeldCounter>) {
    for held_counter in held.drain(..).rev() {
        held_counter.counter.fetch_sub(1, Ordering::Release);
    }
}

fn dedupe_ops(ops: Vec<ActivityOp>) -> Vec<ActivityOp> {
    let mut seen = HashSet::with_capacity(ops.len());
    let mut deduped = Vec::with_capacity(ops.len());
    for op in ops {
        if seen.insert(op.clone()) {
            deduped.push(op);
        }
    }
    deduped
}

/// Canonical activity key for a target path. Walks up to the deepest existing
/// ancestor, canonicalizes it, then re-appends missing tail components.
pub async fn activity_key(target: &Path) -> io::Result<PathBuf> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, ERR_FILE_ACTIVITY_INVALID_PATH);

    let parent = target.parent().ok_or_else(invalid)?;
    let basename = target.file_name().ok_or_else(invalid)?.to_os_string();

    let mut tail: Vec<OsString> = Vec::new();
    let mut existing: &Path = parent;
    while !tokio::fs::try_exists(existing).await.unwrap_or(false) {
        let name = existing.file_name().ok_or_else(invalid)?;
        tail.push(name.to_os_string());
        existing = existing.parent().ok_or_else(invalid)?;
    }

    let mut key = tokio::fs::canonicalize(existing).await?;
    for name in tail.into_iter().rev() {
        key.push(name);
    }
    key.push(basename);
    Ok(key)
}

fn protected_descendant_keys(file_root: &Path, directory_key: &Path) -> Vec<PathBuf> {
    let Ok(relative) = directory_key.strip_prefix(file_root) else {
        // Defensive fallback for already-canonical keys outside file_root, such
        // as admin-created symlink targets. In normal in-area paths this branch
        // is not reached.
        return vec![directory_key.to_path_buf()];
    };

    let components: Vec<OsString> = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_os_string()),
            _ => None,
        })
        .collect();

    if components.is_empty() {
        return Vec::new();
    }

    // `/shared` is intentionally not a global activity key, and `/users` is
    // Nexus infrastructure rather than a mutable user file area. The protected
    // top level for personal areas starts at `/users/<name>`.
    let start_len = match components[0].to_str() {
        Some(FILES_SHARED_DIR | FILES_USERS_DIR) => 2,
        _ => 1,
    };

    if components.len() < start_len {
        return Vec::new();
    }

    let mut keys = Vec::with_capacity(components.len() - start_len + 1);
    for len in start_len..=components.len() {
        let mut path = file_root.to_path_buf();
        for component in &components[..len] {
            path.push(component);
        }
        keys.push(path);
    }
    keys
}

fn protected_ancestor_keys(file_root: &Path, target_key: &Path) -> Vec<PathBuf> {
    let Ok(relative) = target_key.strip_prefix(file_root) else {
        return Vec::new();
    };

    let components: Vec<OsString> = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_os_string()),
            _ => None,
        })
        .collect();

    if components.len() < 2 {
        return Vec::new();
    }

    let parent_len = components.len() - 1;
    // `/shared` is intentionally not a global activity key, and `/users` is
    // Nexus infrastructure rather than a mutable user file area. The protected
    // top level for personal areas starts at `/users/<name>`.
    let start_len = match components[0].to_str() {
        Some(FILES_SHARED_DIR | FILES_USERS_DIR) => 2,
        _ => 1,
    };

    if parent_len < start_len {
        return Vec::new();
    }

    let mut ancestors = Vec::with_capacity(parent_len - start_len + 1);
    for len in start_len..=parent_len {
        let mut path = file_root.to_path_buf();
        for component in &components[..len] {
            path.push(component);
        }
        ancestors.push(path);
    }
    ancestors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use tempfile::TempDir;

    fn setup_root() -> TempDir {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(FILES_SHARED_DIR)).unwrap();
        std::fs::create_dir(temp.path().join(FILES_USERS_DIR)).unwrap();
        std::fs::create_dir(temp.path().join(FILES_SHARED_DIR).join("music")).unwrap();
        std::fs::create_dir(temp.path().join(FILES_USERS_DIR).join("alice")).unwrap();
        temp
    }

    #[tokio::test]
    async fn sibling_children_do_not_conflict() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let first = temp.path().join("shared/music/a.mp3");
        let second = temp.path().join("shared/music/b.mp3");

        let _first_guard = activity
            .try_enter_child_path(temp.path(), &first)
            .await
            .unwrap()
            .unwrap();
        let second_guard = activity
            .try_enter_child_path(temp.path(), &second)
            .await
            .unwrap();

        assert!(second_guard.is_ok());
    }

    #[tokio::test]
    async fn same_exact_path_conflicts() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let target = temp.path().join("shared/music/a.mp3");

        let _guard = activity
            .try_enter_child_path(temp.path(), &target)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_child_path(temp.path(), &target)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn same_read_path_does_not_conflict() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let target = temp.path().join("shared/music/a.mp3");

        let _first_guard = activity
            .try_enter_read_paths(temp.path(), std::slice::from_ref(&target))
            .await
            .unwrap()
            .unwrap();
        let second_guard = activity
            .try_enter_read_paths(temp.path(), &[target])
            .await
            .unwrap();

        assert!(second_guard.is_ok());
    }

    #[tokio::test]
    async fn active_read_blocks_same_path_mutation() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let target = temp.path().join("shared/music/a.mp3");

        let _read_guard = activity
            .try_enter_read_paths(temp.path(), std::slice::from_ref(&target))
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_child_path(temp.path(), &target)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn active_mutation_blocks_same_path_read() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let target = temp.path().join("shared/music/a.mp3");

        let _write_guard = activity
            .try_enter_child_path(temp.path(), &target)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_read_paths(temp.path(), &[target])
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn active_parent_directory_blocks_read() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let child = temp.path().join("shared/music/a.mp3");
        let parent = temp.path().join("shared/music");

        let _parent_guard = activity
            .try_enter_directory_path(temp.path(), &parent)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_read_paths(temp.path(), &[child])
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn active_read_blocks_parent_directory_mutation() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let child = temp.path().join("shared/music/a.mp3");
        let parent = temp.path().join("shared/music");

        let _read_guard = activity
            .try_enter_read_paths(temp.path(), &[child])
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_directory_path(temp.path(), &parent)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn active_child_blocks_parent_directory_mutation() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let child = temp.path().join("shared/music/a.mp3");
        let parent = temp.path().join("shared/music");

        let _child_guard = activity
            .try_enter_child_path(temp.path(), &child)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_directory_path(temp.path(), &parent)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[test]
    fn concurrent_child_and_parent_directory_do_not_both_enter() {
        let temp = setup_root();
        let file_root = temp.path().to_path_buf();
        let parent = file_root.join("shared/music");
        let child = parent.join("a.mp3");

        for _ in 0..1024 {
            let activity = Arc::new(FileActivityMap::new());
            let start = Arc::new(Barrier::new(3));

            let child_activity = Arc::clone(&activity);
            let child_start = Arc::clone(&start);
            let child_root = file_root.clone();
            let child_path = child.clone();
            let child_thread = std::thread::spawn(move || {
                let mut ops = Vec::new();
                append_child_ops(&mut ops, &child_root, child_path);
                child_start.wait();
                child_activity.try_enter_ops(ops)
            });

            let directory_activity = Arc::clone(&activity);
            let directory_start = Arc::clone(&start);
            let directory_root = file_root.clone();
            let directory_path = parent.clone();
            let directory_thread = std::thread::spawn(move || {
                let mut ops = Vec::new();
                append_directory_ops(&mut ops, &directory_root, directory_path);
                directory_start.wait();
                directory_activity.try_enter_ops(ops)
            });

            start.wait();
            let child_result = child_thread.join().unwrap();
            let directory_result = directory_thread.join().unwrap();

            assert!(
                !(child_result.is_ok() && directory_result.is_ok()),
                "child activity and parent directory activity both entered"
            );
        }
    }

    #[test]
    fn concurrent_read_and_same_path_write_do_not_both_enter() {
        let temp = setup_root();
        let file_root = temp.path().to_path_buf();
        let target = file_root.join("shared/music/a.mp3");

        for _ in 0..1024 {
            let activity = Arc::new(FileActivityMap::new());
            let start = Arc::new(Barrier::new(3));

            let read_activity = Arc::clone(&activity);
            let read_start = Arc::clone(&start);
            let read_root = file_root.clone();
            let read_path = target.clone();
            let read_thread = std::thread::spawn(move || {
                let mut ops = Vec::new();
                append_read_ops(&mut ops, &read_root, read_path);
                read_start.wait();
                read_activity.try_enter_ops(ops)
            });

            let write_activity = Arc::clone(&activity);
            let write_start = Arc::clone(&start);
            let write_root = file_root.clone();
            let write_path = target.clone();
            let write_thread = std::thread::spawn(move || {
                let mut ops = Vec::new();
                append_child_ops(&mut ops, &write_root, write_path);
                write_start.wait();
                write_activity.try_enter_ops(ops)
            });

            start.wait();
            let read_result = read_thread.join().unwrap();
            let write_result = write_thread.join().unwrap();

            assert!(
                !(read_result.is_ok() && write_result.is_ok()),
                "read activity and same-path write activity both entered"
            );
        }
    }

    #[test]
    fn concurrent_read_and_parent_directory_do_not_both_enter() {
        let temp = setup_root();
        let file_root = temp.path().to_path_buf();
        let parent = file_root.join("shared/music");
        let child = parent.join("a.mp3");

        for _ in 0..1024 {
            let activity = Arc::new(FileActivityMap::new());
            let start = Arc::new(Barrier::new(3));

            let read_activity = Arc::clone(&activity);
            let read_start = Arc::clone(&start);
            let read_root = file_root.clone();
            let read_path = child.clone();
            let read_thread = std::thread::spawn(move || {
                let mut ops = Vec::new();
                append_read_ops(&mut ops, &read_root, read_path);
                read_start.wait();
                read_activity.try_enter_ops(ops)
            });

            let directory_activity = Arc::clone(&activity);
            let directory_start = Arc::clone(&start);
            let directory_root = file_root.clone();
            let directory_path = parent.clone();
            let directory_thread = std::thread::spawn(move || {
                let mut ops = Vec::new();
                append_directory_ops(&mut ops, &directory_root, directory_path);
                directory_start.wait();
                directory_activity.try_enter_ops(ops)
            });

            start.wait();
            let read_result = read_thread.join().unwrap();
            let directory_result = directory_thread.join().unwrap();

            assert!(
                !(read_result.is_ok() && directory_result.is_ok()),
                "read activity and parent directory activity both entered"
            );
        }
    }

    #[tokio::test]
    async fn active_parent_directory_blocks_child() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let child = temp.path().join("shared/music/a.mp3");
        let parent = temp.path().join("shared/music");

        let _parent_guard = activity
            .try_enter_directory_path(temp.path(), &parent)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_child_path(temp.path(), &child)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn personal_area_root_is_protected_ancestor() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let child = temp.path().join("users/alice/file.txt");
        let personal_root = temp.path().join("users/alice");

        let _child_guard = activity
            .try_enter_child_path(temp.path(), &child)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_directory_path(temp.path(), &personal_root)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn descendant_activity_blocks_directory_mutation() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let parent = temp.path().join("shared/music");

        let _descendant_guard = activity
            .try_enter_descendant_path(temp.path(), &parent)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_directory_path(temp.path(), &parent)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn descendant_activity_allows_child_activity() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let parent = temp.path().join("shared/music");
        let child = parent.join("a.mp3");

        let _descendant_guard = activity
            .try_enter_descendant_path(temp.path(), &parent)
            .await
            .unwrap()
            .unwrap();
        let child_guard = activity
            .try_enter_child_path(temp.path(), &child)
            .await
            .unwrap();

        assert!(child_guard.is_ok());
    }

    #[tokio::test]
    async fn descendant_activity_allows_same_directory_descendant_activity() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let parent = temp.path().join("shared/music");

        let _first_guard = activity
            .try_enter_descendant_path(temp.path(), &parent)
            .await
            .unwrap()
            .unwrap();
        let second_guard = activity
            .try_enter_descendant_path(temp.path(), &parent)
            .await
            .unwrap();

        assert!(second_guard.is_ok());
    }

    #[tokio::test]
    async fn active_directory_blocks_descendant_activity() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let parent = temp.path().join("shared/music");

        let _directory_guard = activity
            .try_enter_directory_path(temp.path(), &parent)
            .await
            .unwrap()
            .unwrap();
        let blocked = activity
            .try_enter_descendant_path(temp.path(), &parent)
            .await
            .unwrap();

        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn shared_root_descendant_activity_is_not_global() {
        let temp = setup_root();
        let activity = FileActivityMap::new();
        let shared_root = temp.path().join("shared");

        let _descendant_guard = activity
            .try_enter_descendant_path(temp.path(), &shared_root)
            .await
            .unwrap()
            .unwrap();
        let directory_guard = activity
            .try_enter_directory_path(temp.path(), &shared_root)
            .await
            .unwrap();

        assert!(directory_guard.is_ok());
    }
}
