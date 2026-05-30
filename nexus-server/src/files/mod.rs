//! File area module for browsing and managing files
//!
//! This module handles file area operations including:
//! - Path resolution with security checks
//! - User area determination (personal vs shared)
//! - Folder type parsing from naming conventions

use std::path::{Path, PathBuf};

use crate::constants::{FILES_DIR_NAME, FILES_SHARED_DIR, FILES_USERS_DIR};

pub mod activity;
pub mod area;
pub mod folder_type;
pub mod index;
pub mod operations;
pub mod path;

pub use activity::{FileActivityMap, activity_key};
pub use area::{
    UserAreaMigration, UserAreaMigrationError, migrate_user_area_on_username_change,
    resolve_user_area,
};
pub use folder_type::{FolderType, in_owned_dropbox, parse_folder_type};
pub use index::FileIndex;
pub use operations::{copy_path_recursive_async, is_subpath, remove_path_async, rename_path_async};
pub use path::{
    allows_upload, build_and_validate_candidate_path, is_hidden_name, normalize_client_path,
    resolve_new_path, resolve_path,
};

/// Get the default file root path under the given server data directory
/// (`<data_dir>/files/`).
#[must_use]
pub fn default_file_root(data_dir: &Path) -> PathBuf {
    data_dir.join(FILES_DIR_NAME)
}

/// Initialize file area directories, creating `{root}/`, `{root}/shared/`, and
/// `{root}/users/` if they don't exist. Idempotent via `create_dir_all()`.
pub fn init_file_area(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {}", root.display(), e))?;

    let shared_dir = root.join(FILES_SHARED_DIR);
    std::fs::create_dir_all(&shared_dir).map_err(|e| format!("{}: {}", shared_dir.display(), e))?;

    let users_dir = root.join(FILES_USERS_DIR);
    std::fs::create_dir_all(&users_dir).map_err(|e| format!("{}: {}", users_dir.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_default_file_root_under_data_dir() {
        let data = Path::new("/var/lib/nexusd");
        assert_eq!(default_file_root(data), Path::new("/var/lib/nexusd/files"));
    }

    #[test]
    fn test_init_file_area_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("files");

        assert!(!root.exists());

        init_file_area(&root).unwrap();

        assert!(root.exists());
        assert!(root.join(FILES_SHARED_DIR).exists());
        assert!(root.join(FILES_USERS_DIR).exists());
    }

    #[test]
    fn test_init_file_area_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("files");

        // Calling twice must not error (idempotent).
        init_file_area(&root).unwrap();
        init_file_area(&root).unwrap();

        assert!(root.exists());
    }
}
