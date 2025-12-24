//! File area module for browsing and managing files
//!
//! This module handles file area operations including:
//! - Path resolution with security checks
//! - User area determination (personal vs shared)
//! - Folder type parsing from naming conventions

// Allow dead code and unused imports during Phase 0 - these will be used in later phases
// when file browsing and transfer handlers are implemented
#![allow(dead_code)]
#![allow(unused_imports)]

use std::path::{Path, PathBuf};

use crate::constants::{
    DATA_DIR_NAME, ERR_CREATE_FILE_DIR, ERR_NO_FILE_ROOT, FILES_DIR_NAME, FILES_SHARED_DIR,
    FILES_USERS_DIR,
};

pub mod area;
pub mod folder_type;
pub mod path;

pub use area::resolve_user_area;
pub use folder_type::{FolderType, parse_folder_type};
pub use path::{
    FileError, allows_upload, build_and_validate_candidate_path, build_candidate_path,
    resolve_new_path, resolve_path,
};

/// Get the default file root path for the platform
///
/// Returns the platform-specific path where the file area should be stored:
/// - **Linux**: `~/.local/share/nexusd/files/`
/// - **macOS**: `~/Library/Application Support/nexusd/files/`
/// - **Windows**: `%APPDATA%\nexusd\files\`
///
/// # Errors
///
/// Returns an error if the platform's data directory cannot be determined.
#[must_use = "file root result should be used"]
pub fn default_file_root() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir().ok_or_else(|| ERR_NO_FILE_ROOT.to_string())?;
    Ok(data_dir.join(DATA_DIR_NAME).join(FILES_DIR_NAME))
}

/// Initialize file area directories
///
/// Creates the following directories if they don't exist:
/// - `{root}/`
/// - `{root}/shared/`
/// - `{root}/users/`
///
/// Uses `create_dir_all()` for idempotent creation.
///
/// # Errors
///
/// Returns an error if directory creation fails.
pub fn init_file_area(root: &Path) -> Result<(), String> {
    // Create root directory
    std::fs::create_dir_all(root)
        .map_err(|e| format!("{}{}: {}", ERR_CREATE_FILE_DIR, root.display(), e))?;

    // Create shared directory
    let shared_dir = root.join(FILES_SHARED_DIR);
    std::fs::create_dir_all(&shared_dir)
        .map_err(|e| format!("{}{}: {}", ERR_CREATE_FILE_DIR, shared_dir.display(), e))?;

    // Create users directory
    let users_dir = root.join(FILES_USERS_DIR);
    std::fs::create_dir_all(&users_dir)
        .map_err(|e| format!("{}{}: {}", ERR_CREATE_FILE_DIR, users_dir.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_init_file_area_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("files");

        // Directories don't exist yet
        assert!(!root.exists());

        // Initialize
        init_file_area(&root).unwrap();

        // All directories should exist
        assert!(root.exists());
        assert!(root.join(FILES_SHARED_DIR).exists());
        assert!(root.join(FILES_USERS_DIR).exists());
    }

    #[test]
    fn test_init_file_area_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("files");

        // Initialize twice - should not error
        init_file_area(&root).unwrap();
        init_file_area(&root).unwrap();

        assert!(root.exists());
    }
}
