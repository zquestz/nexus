//! User area resolution
//!
//! Determines which file area root a user should access based on
//! whether they have a personal folder or should use the shared folder.

use std::path::{Path, PathBuf};

use crate::constants::{FILES_SHARED_DIR, FILES_USERS_DIR};

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

#[cfg(test)]
mod tests {
    use std::fs;

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
}
