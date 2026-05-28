//! User area resolution and per-personal-area serialization
//!
//! Determines which file area root a user should access based on
//! whether they have a personal folder or should use the shared folder.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use nexus_common::names::fold_name;
use tokio::sync::{Mutex as AsyncMutex, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use crate::constants::{
    ERR_PERSONAL_FILE_AREA_MIGRATION_FAILED, ERR_PERSONAL_FILE_AREA_PATH_BUSY,
    ERR_PERSONAL_FILE_AREA_TARGET_EXISTS, FILES_SHARED_DIR, FILES_USERS_DIR,
};
use crate::files::path_lock::{PathLockMap, PathLockMode, lock_key};

/// Resolve the file area root for a specific user.
///
/// Returns `{root}/users/{username}/` if it exists as a directory, otherwise
/// `{root}/shared/`. The user sees their area as `/` (transparent to them).
/// For shared accounts, pass the account username (not the nickname). Does not
/// create directories — directory creation is the admin's responsibility.
///
/// The returned path is **not** canonicalized — the caller should canonicalize
/// it before passing to `resolve_path()` for security checks.
///
/// # Security Notes
///
/// - This function performs a TOCTOU-vulnerable `is_dir()` check. The caller
///   should use `resolve_path()` on any user-provided paths within the returned
///   area to enforce security at access time.
/// - Username validation (blocking path-sensitive characters like `/`, `..`)
///   is handled by the username validator, not this function.
/// - If an attacker somehow creates a file (not directory) named after a user,
///   that user falls back to the shared folder (safe behavior).
#[must_use]
#[allow(dead_code)]
pub async fn resolve_user_area(root: &Path, username: &str) -> PathBuf {
    let user_dir = root.join(FILES_USERS_DIR).join(username);

    let is_dir = tokio::fs::metadata(&user_dir)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false);

    if is_dir {
        user_dir
    } else {
        root.join(FILES_SHARED_DIR)
    }
}

#[derive(Default)]
pub struct PersonalAreaLockMap {
    inner: AsyncMutex<HashMap<String, Weak<RwLock<()>>>>,
}

pub type PersonalAreaReadGuard = OwnedRwLockReadGuard<()>;
pub type PersonalAreaWriteGuard = OwnedRwLockWriteGuard<()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonalAreaBusy;

impl PersonalAreaLockMap {
    pub fn new() -> Self {
        Self::default()
    }

    async fn entry_for(&self, key: String) -> Arc<RwLock<()>> {
        let mut map = self.inner.lock().await;
        match map.get(&key).and_then(Weak::upgrade) {
            Some(entry) => entry,
            None => {
                map.retain(|_, w| w.strong_count() > 0);
                let entry = Arc::new(RwLock::new(()));
                map.insert(key, Arc::downgrade(&entry));
                entry
            }
        }
    }

    pub async fn read(&self, username: &str) -> PersonalAreaReadGuard {
        self.entry_for(personal_area_key(username))
            .await
            .read_owned()
            .await
    }

    pub async fn read_many<I>(&self, usernames: I) -> Vec<PersonalAreaReadGuard>
    where
        I: IntoIterator<Item = String>,
    {
        let keys = normalized_area_keys(usernames);
        let mut guards = Vec::with_capacity(keys.len());
        for key in keys {
            guards.push(self.entry_for(key).await.read_owned().await);
        }
        guards
    }

    pub async fn try_write_many<I>(
        &self,
        usernames: I,
    ) -> Result<Vec<PersonalAreaWriteGuard>, PersonalAreaBusy>
    where
        I: IntoIterator<Item = String>,
    {
        let keys = normalized_area_keys(usernames);
        let mut guards = Vec::with_capacity(keys.len());
        for key in keys {
            let entry = self.entry_for(key).await;
            match entry.try_write_owned() {
                Ok(guard) => guards.push(guard),
                Err(_) => return Err(PersonalAreaBusy),
            }
        }
        Ok(guards)
    }
}

fn personal_area_key(username: &str) -> String {
    fold_name(username)
}

fn normalized_area_keys<I>(usernames: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut keys: Vec<String> = usernames
        .into_iter()
        .map(|username| personal_area_key(&username))
        .filter(|key| !key.is_empty())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

pub async fn resolve_user_area_with_read_lock(
    root: &Path,
    locks: &PersonalAreaLockMap,
    username: &str,
) -> (PathBuf, Vec<PersonalAreaReadGuard>) {
    let guard = locks.read(username).await;
    let user_dir = root.join(FILES_USERS_DIR).join(username);

    let is_dir = tokio::fs::metadata(&user_dir)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false);

    if is_dir {
        (user_dir, vec![guard])
    } else {
        drop(guard);
        (root.join(FILES_SHARED_DIR), Vec::new())
    }
}

pub fn personal_area_names_for_root_path(client_path: &str) -> Vec<String> {
    let segments = normalized_client_segments(client_path);
    if segments.len() >= 2 && segments[0] == FILES_USERS_DIR {
        vec![segments[1].clone()]
    } else {
        Vec::new()
    }
}

pub fn personal_area_names_for_root_child(parent_path: &str, child_name: &str) -> Vec<String> {
    let mut names = personal_area_names_for_root_path(parent_path);
    let segments = normalized_client_segments(parent_path);
    if segments.len() == 1 && segments[0] == FILES_USERS_DIR {
        names.push(child_name.to_string());
    }
    names
}

pub fn personal_area_names_for_root_rename(source_path: &str, new_name: &str) -> Vec<String> {
    let mut names = personal_area_names_for_root_path(source_path);
    let mut segments = normalized_client_segments(source_path);
    if !segments.is_empty() {
        segments.pop();
    }
    if segments.len() == 1 && segments[0] == FILES_USERS_DIR {
        names.push(new_name.to_string());
    }
    names
}

pub fn personal_area_names_for_root_destination(
    destination_dir: &str,
    source_basename: &str,
) -> Vec<String> {
    let mut names = personal_area_names_for_root_path(destination_dir);
    let segments = normalized_client_segments(destination_dir);
    if segments.len() == 1 && segments[0] == FILES_USERS_DIR {
        names.push(source_basename.to_string());
    }
    names
}

pub fn personal_area_names_for_root_filesystem_path(root: &Path, path: &Path) -> Vec<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| personal_area_names_for_root_path(&relative.to_string_lossy()))
        .unwrap_or_default()
}

fn normalized_client_segments(client_path: &str) -> Vec<String> {
    client_path
        .trim_start_matches(['/', '\\'])
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalAreaMigration {
    NotNeeded,
    Migrated,
}

#[derive(Debug)]
pub enum PersonalAreaMigrationError {
    TargetExists,
    Busy,
    Io(io::Error),
}

impl std::fmt::Display for PersonalAreaMigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetExists => write!(f, "{ERR_PERSONAL_FILE_AREA_TARGET_EXISTS}"),
            Self::Busy => write!(f, "{ERR_PERSONAL_FILE_AREA_PATH_BUSY}"),
            Self::Io(e) => write!(f, "{ERR_PERSONAL_FILE_AREA_MIGRATION_FAILED}: {e}"),
        }
    }
}

impl std::error::Error for PersonalAreaMigrationError {}

impl From<io::Error> for PersonalAreaMigrationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Rename `{root}/users/{old_username}` to `{root}/users/{new_username}`.
///
/// If the old personal area is absent (or is not a directory), this is a no-op.
/// If the new path already exists as a distinct filesystem entry, the caller must
/// reject the account rename rather than merge or overwrite admin-managed data.
async fn migrate_personal_area_on_rename(
    root: &Path,
    old_username: &str,
    new_username: &str,
) -> Result<PersonalAreaMigration, PersonalAreaMigrationError> {
    if old_username == new_username {
        return Ok(PersonalAreaMigration::NotNeeded);
    }

    let users_dir = root.join(FILES_USERS_DIR);
    let old_dir = users_dir.join(old_username);
    let new_dir = users_dir.join(new_username);

    let old_metadata = match tokio::fs::metadata(&old_dir).await {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(PersonalAreaMigration::NotNeeded);
        }
        Err(e) => return Err(e.into()),
    };

    if !old_metadata.is_dir() {
        return Ok(PersonalAreaMigration::NotNeeded);
    }

    match tokio::fs::metadata(&new_dir).await {
        Ok(_) => {
            if same_file::is_same_file(&old_dir, &new_dir).unwrap_or(false) {
                Ok(PersonalAreaMigration::NotNeeded)
            } else {
                Err(PersonalAreaMigrationError::TargetExists)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            tokio::fs::rename(&old_dir, &new_dir).await?;
            Ok(PersonalAreaMigration::Migrated)
        }
        Err(e) => Err(e.into()),
    }
}

/// Lock the old and new personal-area paths, then migrate under that lock.
///
/// This serializes the rename with server file mutations that target the same
/// exact directory names, so the target-exists check and filesystem rename see a
/// stable in-server view.
pub async fn migrate_personal_area_on_rename_with_locks(
    root: &Path,
    area_locks: &PersonalAreaLockMap,
    locks: &PathLockMap,
    old_username: &str,
    new_username: &str,
) -> Result<PersonalAreaMigration, PersonalAreaMigrationError> {
    if old_username == new_username {
        return Ok(PersonalAreaMigration::NotNeeded);
    }

    let users_dir = root.join(FILES_USERS_DIR);
    let old_dir = users_dir.join(old_username);
    let new_dir = users_dir.join(new_username);
    let mut area_guards = area_locks
        .try_write_many([old_username.to_string()])
        .await
        .map_err(|_| PersonalAreaMigrationError::Busy)?;
    let old_key = lock_key(&old_dir).await?;

    {
        // Waiting here is only for short exact-path mutations. Long-lived file
        // operations and transfers serialize via the fail-fast personal-area
        // write lock above.
        let _old_guard = locks
            .acquire(old_key.clone(), PathLockMode::Wait)
            .await
            .map_err(|_| {
                PersonalAreaMigrationError::Io(io::Error::other(ERR_PERSONAL_FILE_AREA_PATH_BUSY))
            })?;

        let old_metadata = match tokio::fs::metadata(&old_dir).await {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(PersonalAreaMigration::NotNeeded);
            }
            Err(e) => return Err(e.into()),
        };

        if !old_metadata.is_dir() {
            return Ok(PersonalAreaMigration::NotNeeded);
        }
    }

    if fold_name(old_username) != fold_name(new_username) {
        area_guards.extend(
            area_locks
                .try_write_many([new_username.to_string()])
                .await
                .map_err(|_| PersonalAreaMigrationError::Busy)?,
        );
    }

    let new_key = lock_key(&new_dir).await?;

    let _guards = locks
        .acquire_many(vec![old_key, new_key], PathLockMode::Wait)
        .await
        .map_err(|_| {
            PersonalAreaMigrationError::Io(io::Error::other(ERR_PERSONAL_FILE_AREA_PATH_BUSY))
        })?;

    migrate_personal_area_on_rename(root, old_username, new_username).await
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, time::Duration};

    use tempfile::TempDir;

    use super::*;

    fn setup_test_root() -> TempDir {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let root = temp.path();

        fs::create_dir_all(root.join(FILES_SHARED_DIR)).expect("Failed to create shared dir");
        fs::create_dir_all(root.join(FILES_USERS_DIR)).expect("Failed to create users dir");

        temp
    }

    #[tokio::test]
    async fn test_user_without_personal_folder_gets_shared() {
        let temp = setup_test_root();
        let root = temp.path();

        let area = resolve_user_area(root, "alice").await;

        assert_eq!(area, root.join(FILES_SHARED_DIR));
    }

    #[tokio::test]
    async fn test_user_with_personal_folder_gets_personal() {
        let temp = setup_test_root();
        let root = temp.path();

        let alice_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&alice_dir).expect("Failed to create alice dir");

        let area = resolve_user_area(root, "alice").await;

        assert_eq!(area, alice_dir);
    }

    #[tokio::test]
    async fn test_file_not_directory_falls_back_to_shared() {
        let temp = setup_test_root();
        let root = temp.path();

        // A file (not directory) named after a user falls back to shared.
        let bob_file = root.join(FILES_USERS_DIR).join("bob");
        fs::write(&bob_file, "not a directory").expect("Failed to create bob file");

        let area = resolve_user_area(root, "bob").await;

        assert_eq!(area, root.join(FILES_SHARED_DIR));
    }

    #[tokio::test]
    async fn test_shared_account_uses_account_name() {
        let temp = setup_test_root();
        let root = temp.path();

        let guest_dir = root.join(FILES_USERS_DIR).join("guest");
        fs::create_dir(&guest_dir).expect("Failed to create guest dir");

        // Shared account users resolve by account username, not nickname.
        let area = resolve_user_area(root, "guest").await;

        assert_eq!(area, guest_dir);
    }

    #[tokio::test]
    async fn test_unicode_username() {
        let temp = setup_test_root();
        let root = temp.path();

        let unicode_dir = root.join(FILES_USERS_DIR).join("用户");
        fs::create_dir(&unicode_dir).expect("Failed to create unicode dir");

        let area = resolve_user_area(root, "用户").await;

        assert_eq!(area, unicode_dir);
    }

    #[tokio::test]
    async fn test_migrate_personal_area_renames_existing_directory() {
        let temp = setup_test_root();
        let root = temp.path();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&old_dir).expect("Failed to create alice dir");
        fs::write(old_dir.join("note.txt"), "personal").expect("Failed to create file");

        let result = migrate_personal_area_on_rename(root, "alice", "alicia")
            .await
            .unwrap();

        assert_eq!(result, PersonalAreaMigration::Migrated);
        assert!(!old_dir.exists());
        assert!(
            root.join(FILES_USERS_DIR)
                .join("alicia")
                .join("note.txt")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_migrate_personal_area_missing_old_allows_preprovisioned_new() {
        let temp = setup_test_root();
        let root = temp.path();

        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&new_dir).expect("Failed to create preprovisioned dir");

        let result = migrate_personal_area_on_rename(root, "alice", "alicia")
            .await
            .unwrap();

        assert_eq!(result, PersonalAreaMigration::NotNeeded);
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_personal_area_rejects_distinct_target() {
        let temp = setup_test_root();
        let root = temp.path();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&old_dir).expect("Failed to create old dir");
        fs::create_dir(&new_dir).expect("Failed to create new dir");

        let result = migrate_personal_area_on_rename(root, "alice", "alicia").await;

        assert!(matches!(
            result,
            Err(PersonalAreaMigrationError::TargetExists)
        ));
        assert!(old_dir.exists());
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_personal_area_case_only_keeps_access() {
        let temp = setup_test_root();
        let root = temp.path();

        let old_dir = root.join(FILES_USERS_DIR).join("Alice");
        fs::create_dir(&old_dir).expect("Failed to create old dir");
        fs::write(old_dir.join("note.txt"), "personal").expect("Failed to create file");

        migrate_personal_area_on_rename(root, "Alice", "alice")
            .await
            .unwrap();

        assert!(
            root.join(FILES_USERS_DIR)
                .join("alice")
                .join("note.txt")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_migrate_personal_area_with_locks_respects_busy_target() {
        let temp = setup_test_root();
        let root = temp.path();
        let locks = PathLockMap::new();
        let area_locks = PersonalAreaLockMap::new();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&old_dir).expect("Failed to create old dir");

        let target_key = lock_key(&new_dir).await.unwrap();
        let _target_guard = locks.acquire(target_key, PathLockMode::Fail).await.unwrap();

        let result = migrate_personal_area_on_rename_with_locks(
            root,
            &area_locks,
            &locks,
            "alice",
            "alicia",
        )
        .await;

        assert!(matches!(result, Err(PersonalAreaMigrationError::Io(_))));
        assert!(old_dir.exists());
        assert!(!new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_personal_area_with_locks_missing_old_ignores_busy_target() {
        let temp = setup_test_root();
        let root = temp.path();
        let locks = PathLockMap::new();
        let area_locks = PersonalAreaLockMap::new();

        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&new_dir).expect("Failed to create preprovisioned dir");

        let target_key = lock_key(&new_dir).await.unwrap();
        let _target_guard = locks.acquire(target_key, PathLockMode::Fail).await.unwrap();

        let result = migrate_personal_area_on_rename_with_locks(
            root,
            &area_locks,
            &locks,
            "alice",
            "alicia",
        )
        .await
        .unwrap();

        assert_eq!(result, PersonalAreaMigration::NotNeeded);
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_personal_area_with_locks_missing_old_ignores_busy_area_target() {
        let temp = setup_test_root();
        let root = temp.path();
        let locks = PathLockMap::new();
        let area_locks = PersonalAreaLockMap::new();

        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&new_dir).expect("Failed to create preprovisioned dir");

        let _target_guard = area_locks.read("alicia").await;
        let result = migrate_personal_area_on_rename_with_locks(
            root,
            &area_locks,
            &locks,
            "alice",
            "alicia",
        )
        .await
        .unwrap();

        assert_eq!(result, PersonalAreaMigration::NotNeeded);
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_personal_area_with_locks_fails_when_source_area_busy() {
        let temp = setup_test_root();
        let root = temp.path();
        let locks = PathLockMap::new();
        let area_locks = PersonalAreaLockMap::new();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&old_dir).expect("Failed to create old dir");

        let _source_guard = area_locks.read("alice").await;
        let result = migrate_personal_area_on_rename_with_locks(
            root,
            &area_locks,
            &locks,
            "alice",
            "alicia",
        )
        .await;

        assert!(matches!(result, Err(PersonalAreaMigrationError::Busy)));
        assert!(old_dir.exists());
        assert!(!root.join(FILES_USERS_DIR).join("alicia").exists());
    }

    #[tokio::test]
    async fn test_migrate_personal_area_with_locks_rechecks_target_after_wait() {
        let temp = setup_test_root();
        let root = temp.path();
        let locks = Arc::new(PathLockMap::new());
        let area_locks = Arc::new(PersonalAreaLockMap::new());

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&old_dir).expect("Failed to create old dir");

        let target_key = lock_key(&new_dir).await.unwrap();
        let target_guard = locks.acquire(target_key, PathLockMode::Wait).await.unwrap();

        let root_path = root.to_path_buf();
        let locks_for_task = Arc::clone(&locks);
        let area_locks_for_task = Arc::clone(&area_locks);
        let mut task = tokio::spawn(async move {
            migrate_personal_area_on_rename_with_locks(
                &root_path,
                &area_locks_for_task,
                &locks_for_task,
                "alice",
                "alicia",
            )
            .await
        });

        tokio::task::yield_now().await;
        tokio::select! {
            result = &mut task => panic!("migration completed while target lock was held: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(20)) => {}
        }

        fs::create_dir(&new_dir).expect("Failed to create target dir");
        drop(target_guard);

        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("migration task should finish")
            .expect("migration task should not panic");

        assert!(matches!(
            result,
            Err(PersonalAreaMigrationError::TargetExists)
        ));
        assert!(old_dir.exists());
        assert!(new_dir.exists());
    }
}
