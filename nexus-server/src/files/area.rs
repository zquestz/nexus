//! User area resolution
//!
//! Determines which file area root a user should access based on
//! whether they have a personal folder or should use the shared folder.

use std::path::{Path, PathBuf};

use crate::constants::{FILES_SHARED_DIR, FILES_USERS_DIR};

/// Resolve the file area root for a specific user.
///
/// Returns the path to the user's file area:
/// - If `{root}/users/{username}/` exists as a directory, returns that path
/// - Otherwise, returns `{root}/shared/`
///
/// The user sees their area as `/` (transparent to them).
///
/// # Arguments
///
/// * `root` - The file area root directory (e.g., `~/.local/share/nexusd/files/`).
///   Should be an absolute path for consistent behavior.
/// * `username` - The username to resolve the area for. For shared accounts,
///   this should be the account username (not the nickname).
///
/// # Returns
///
/// The path to the user's file area root directory. The returned path is
/// **not** canonicalized - the caller should canonicalize it before passing
/// to `resolve_path()` for security checks.
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
///
/// # Note
///
/// This function does not create directories - it only checks if
/// the user's personal folder exists. Directory creation is the
/// admin's responsibility.
#[must_use]
pub fn resolve_user_area(root: &Path, username: &str) -> PathBuf {
    let user_dir = root.join(FILES_USERS_DIR).join(username);

    if user_dir.is_dir() {
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

    /// Create a test file area structure
    fn setup_test_root() -> TempDir {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let root = temp.path();

        // Create shared directory
        fs::create_dir_all(root.join(FILES_SHARED_DIR)).expect("Failed to create shared dir");

        // Create users directory (but no user folders yet)
        fs::create_dir_all(root.join(FILES_USERS_DIR)).expect("Failed to create users dir");

        temp
    }

    #[test]
    fn test_user_without_personal_folder_gets_shared() {
        let temp = setup_test_root();
        let root = temp.path();

        let area = resolve_user_area(root, "alice");

        assert_eq!(area, root.join(FILES_SHARED_DIR));
    }

    #[test]
    fn test_user_with_personal_folder_gets_personal() {
        let temp = setup_test_root();
        let root = temp.path();

        // Create alice's personal folder
        let alice_dir = root.join(FILES_USERS_DIR).join("alice");
        fs::create_dir(&alice_dir).expect("Failed to create alice dir");

        let area = resolve_user_area(root, "alice");

        assert_eq!(area, alice_dir);
    }

    #[test]
    fn test_file_not_directory_falls_back_to_shared() {
        let temp = setup_test_root();
        let root = temp.path();

        // Create a file (not directory) named "bob" in users/
        let bob_file = root.join(FILES_USERS_DIR).join("bob");
        fs::write(&bob_file, "not a directory").expect("Failed to create bob file");

        // Should fall back to shared since it's not a directory
        let area = resolve_user_area(root, "bob");

        assert_eq!(area, root.join(FILES_SHARED_DIR));
    }

    #[test]
    fn test_shared_account_uses_account_name() {
        let temp = setup_test_root();
        let root = temp.path();

        // Create guest folder for shared account
        let guest_dir = root.join(FILES_USERS_DIR).join("guest");
        fs::create_dir(&guest_dir).expect("Failed to create guest dir");

        // Shared account users use their account username, not nickname
        let area = resolve_user_area(root, "guest");

        assert_eq!(area, guest_dir);
    }

    #[test]
    fn test_unicode_username() {
        let temp = setup_test_root();
        let root = temp.path();

        // Create folder with unicode username
        let unicode_dir = root.join(FILES_USERS_DIR).join("用户");
        fs::create_dir(&unicode_dir).expect("Failed to create unicode dir");

        let area = resolve_user_area(root, "用户");

        assert_eq!(area, unicode_dir);
    }
}
