use std::io;
use std::path::{Path, PathBuf};

use crate::constants::{
    ERR_PERSONAL_FILE_AREA_MIGRATION_FAILED, ERR_PERSONAL_FILE_AREA_PATH_BUSY,
    ERR_PERSONAL_FILE_AREA_TARGET_EXISTS, FILES_SHARED_DIR, FILES_USERS_DIR,
};

use super::activity::{FileActivityGuard, FileActivityMap};

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

#[derive(Debug)]
pub struct UserAreaMigration {
    migrated: bool,
    activity_guard: Option<FileActivityGuard>,
}

impl UserAreaMigration {
    pub fn not_needed() -> Self {
        Self {
            migrated: false,
            activity_guard: None,
        }
    }

    fn migrated(activity_guard: FileActivityGuard) -> Self {
        Self {
            migrated: true,
            activity_guard: Some(activity_guard),
        }
    }

    pub fn was_migrated(&self) -> bool {
        self.migrated
    }

    pub fn release_activity(&mut self) {
        self.activity_guard.take();
    }
}

#[derive(Debug)]
pub enum UserAreaMigrationError {
    TargetExists,
    Busy,
    Io(io::Error),
}

impl std::fmt::Display for UserAreaMigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetExists => write!(f, "{ERR_PERSONAL_FILE_AREA_TARGET_EXISTS}"),
            Self::Busy => write!(f, "{ERR_PERSONAL_FILE_AREA_PATH_BUSY}"),
            Self::Io(e) => write!(f, "{ERR_PERSONAL_FILE_AREA_MIGRATION_FAILED}: {e}"),
        }
    }
}

impl std::error::Error for UserAreaMigrationError {}

impl From<io::Error> for UserAreaMigrationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserAreaMigrationOutcome {
    NotNeeded,
    Migrated,
}

/// Rename `{root}/users/{old_username}` to `{root}/users/{new_username}`.
///
/// If the old user area is absent (or is not a directory), this is a no-op.
/// If the new path already exists as a distinct filesystem entry, the caller must
/// reject the account rename rather than merge or overwrite admin-managed data.
async fn rename_user_area_on_username_change(
    root: &Path,
    old_username: &str,
    new_username: &str,
) -> Result<UserAreaMigrationOutcome, UserAreaMigrationError> {
    if old_username == new_username {
        return Ok(UserAreaMigrationOutcome::NotNeeded);
    }

    let users_dir = root.join(FILES_USERS_DIR);
    let old_dir = users_dir.join(old_username);
    let new_dir = users_dir.join(new_username);

    let old_metadata = match tokio::fs::metadata(&old_dir).await {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(UserAreaMigrationOutcome::NotNeeded);
        }
        Err(e) => return Err(e.into()),
    };

    if !old_metadata.is_dir() {
        return Ok(UserAreaMigrationOutcome::NotNeeded);
    }

    match tokio::fs::metadata(&new_dir).await {
        Ok(_) => {
            if same_file::is_same_file(&old_dir, &new_dir).unwrap_or(false) {
                Ok(UserAreaMigrationOutcome::NotNeeded)
            } else {
                Err(UserAreaMigrationError::TargetExists)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            tokio::fs::rename(&old_dir, &new_dir).await?;
            Ok(UserAreaMigrationOutcome::Migrated)
        }
        Err(e) => Err(e.into()),
    }
}

/// Reserve old and new user-area paths, then migrate under that directory activity.
pub async fn migrate_user_area_on_username_change(
    root: &Path,
    activity: &FileActivityMap,
    old_username: &str,
    new_username: &str,
) -> Result<UserAreaMigration, UserAreaMigrationError> {
    if old_username == new_username {
        return Ok(UserAreaMigration::not_needed());
    }

    let users_dir = root.join(FILES_USERS_DIR);
    let old_dir = users_dir.join(old_username);
    let new_dir = users_dir.join(new_username);

    {
        let old_metadata = match tokio::fs::metadata(&old_dir).await {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // No source directory means there is no filesystem rename to
                // protect; target activity should not block a DB-only rename.
                return Ok(UserAreaMigration::not_needed());
            }
            Err(e) => return Err(e.into()),
        };

        if !old_metadata.is_dir() {
            return Ok(UserAreaMigration::not_needed());
        }
    }

    let activity_guard = activity
        .try_enter_directory_paths(root, &[old_dir.clone(), new_dir.clone()])
        .await
        .map_err(UserAreaMigrationError::Io)?
        .map_err(|_| UserAreaMigrationError::Busy)?;

    match rename_user_area_on_username_change(root, old_username, new_username).await? {
        UserAreaMigrationOutcome::Migrated => Ok(UserAreaMigration::migrated(activity_guard)),
        UserAreaMigrationOutcome::NotNeeded => Ok(UserAreaMigration::not_needed()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    async fn resolve_test_user_area(root: &Path, username: &str) -> PathBuf {
        resolve_user_area(root, username).await
    }

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

        let area = resolve_test_user_area(root, "alice").await;

        assert_eq!(area, root.join(FILES_SHARED_DIR));
    }

    #[tokio::test]
    async fn test_user_with_personal_folder_gets_personal() {
        let temp = setup_test_root();
        let root = temp.path();

        let alice_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&alice_dir).expect("Failed to create alice dir");

        let area = resolve_test_user_area(root, "alice").await;

        assert_eq!(area, alice_dir);
    }

    #[tokio::test]
    async fn test_file_not_directory_falls_back_to_shared() {
        let temp = setup_test_root();
        let root = temp.path();

        // A file (not directory) named after a user falls back to shared.
        let bob_file = root.join(FILES_USERS_DIR).join("bob");
        fs::write(&bob_file, "not a directory").expect("Failed to create bob file");

        let area = resolve_test_user_area(root, "bob").await;

        assert_eq!(area, root.join(FILES_SHARED_DIR));
    }

    #[tokio::test]
    async fn test_shared_account_uses_account_name() {
        let temp = setup_test_root();
        let root = temp.path();

        let shared_dir = root.join(FILES_USERS_DIR).join("shared_acct");
        fs::create_dir(&shared_dir).expect("Failed to create shared_acct dir");

        // Shared account users resolve by account username, not nickname.
        let area = resolve_test_user_area(root, "shared_acct").await;

        assert_eq!(area, shared_dir);
    }

    #[tokio::test]
    async fn test_unicode_username() {
        let temp = setup_test_root();
        let root = temp.path();

        let unicode_dir = root.join(FILES_USERS_DIR).join("用户");
        fs::create_dir(&unicode_dir).expect("Failed to create unicode dir");

        let area = resolve_test_user_area(root, "用户").await;

        assert_eq!(area, unicode_dir);
    }

    #[tokio::test]
    async fn test_rename_user_area_renames_existing_directory() {
        let temp = setup_test_root();
        let root = temp.path();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&old_dir).expect("Failed to create alice dir");
        fs::write(old_dir.join("note.txt"), "personal").expect("Failed to create file");

        let result = rename_user_area_on_username_change(root, "alice", "alicia")
            .await
            .unwrap();

        assert_eq!(result, UserAreaMigrationOutcome::Migrated);
        assert!(!old_dir.exists());
        assert!(
            root.join(FILES_USERS_DIR)
                .join("alicia")
                .join("note.txt")
                .exists()
        );
    }

    #[tokio::test]
    async fn test_rename_user_area_missing_old_allows_preprovisioned_new() {
        let temp = setup_test_root();
        let root = temp.path();

        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&new_dir).expect("Failed to create preprovisioned dir");

        let result = rename_user_area_on_username_change(root, "alice", "alicia")
            .await
            .unwrap();

        assert_eq!(result, UserAreaMigrationOutcome::NotNeeded);
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_rename_user_area_rejects_distinct_target() {
        let temp = setup_test_root();
        let root = temp.path();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&old_dir).expect("Failed to create old dir");
        fs::create_dir(&new_dir).expect("Failed to create new dir");

        let result = rename_user_area_on_username_change(root, "alice", "alicia").await;

        assert!(matches!(result, Err(UserAreaMigrationError::TargetExists)));
        assert!(old_dir.exists());
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_rename_user_area_case_only_keeps_access() {
        let temp = setup_test_root();
        let root = temp.path();

        let old_dir = root.join(FILES_USERS_DIR).join("Alice");
        fs::create_dir(&old_dir).expect("Failed to create old dir");
        fs::write(old_dir.join("note.txt"), "personal").expect("Failed to create file");

        rename_user_area_on_username_change(root, "Alice", "alice")
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
    async fn test_migrate_user_area_with_activity_respects_busy_target() {
        let temp = setup_test_root();
        let root = temp.path();
        let activity = FileActivityMap::new();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&old_dir).expect("Failed to create old dir");

        let _target_guard = activity
            .try_enter_directory_path(root, &new_dir)
            .await
            .unwrap()
            .unwrap();

        let result = migrate_user_area_on_username_change(root, &activity, "alice", "alicia").await;

        assert!(matches!(result, Err(UserAreaMigrationError::Busy)));
        assert!(old_dir.exists());
        assert!(!new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_user_area_missing_old_ignores_busy_target() {
        let temp = setup_test_root();
        let root = temp.path();
        let activity = FileActivityMap::new();

        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&new_dir).expect("Failed to create preprovisioned dir");

        let _target_guard = activity
            .try_enter_directory_path(root, &new_dir)
            .await
            .unwrap()
            .unwrap();

        let result = migrate_user_area_on_username_change(root, &activity, "alice", "alicia")
            .await
            .unwrap();

        assert!(!result.was_migrated());
        assert!(new_dir.exists());
    }

    #[tokio::test]
    async fn test_migrate_user_area_fails_when_source_descendant_busy() {
        let temp = setup_test_root();
        let root = temp.path();
        let activity = FileActivityMap::new();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&old_dir).expect("Failed to create old dir");

        let _source_guard = activity
            .try_enter_descendant_path(root, &old_dir)
            .await
            .unwrap()
            .unwrap();
        let result = migrate_user_area_on_username_change(root, &activity, "alice", "alicia").await;

        assert!(matches!(result, Err(UserAreaMigrationError::Busy)));
        assert!(old_dir.exists());
        assert!(!root.join(FILES_USERS_DIR).join("alicia").exists());
    }

    #[tokio::test]
    async fn test_migrate_user_area_guard_stays_active_until_released() {
        let temp = setup_test_root();
        let root = temp.path();
        let activity = FileActivityMap::new();

        let old_dir = root.join(FILES_USERS_DIR).join("alice");
        let new_dir = root.join(FILES_USERS_DIR).join("alicia");
        fs::create_dir(&old_dir).expect("Failed to create old dir");
        fs::write(old_dir.join("note.txt"), "personal").expect("Failed to create file");

        let mut migration =
            migrate_user_area_on_username_change(root, &activity, "alice", "alicia")
                .await
                .unwrap();

        assert!(migration.was_migrated());
        assert!(
            activity
                .try_enter_child_path(root, &new_dir.join("other.txt"))
                .await
                .unwrap()
                .is_err()
        );

        migration.release_activity();

        assert!(
            activity
                .try_enter_child_path(root, &new_dir.join("other.txt"))
                .await
                .unwrap()
                .is_ok()
        );
    }
}
