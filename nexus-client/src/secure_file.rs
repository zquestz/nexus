//! Owner-only directory creation for sensitive client state.
//!
//! Config (`config.json`), transfer credentials (`transfers.json`), and the
//! encrypted chat history use [`nexus_common::secure_file::write_atomic`] for
//! atomic owner-only file replacement. This module keeps the client-specific
//! owner-only directory creation helper.

use std::fs;
use std::io;
use std::path::Path;

/// Owner read/write/execute only.
#[cfg(unix)]
const DIR_MODE: u32 = 0o700;

/// Recursively create `path` and any missing parents, owner-only on Unix. On
/// Unix the target directory's mode is re-enforced to `0o700`, so an existing
/// directory left world-listable by an older install is tightened (the parents
/// it creates also get `0o700`; pre-existing ancestors are left untouched).
pub fn create_dir_owner_only(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

        fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(path)?;
        // `mode` applies only to newly-created dirs, so re-enforce on `path` to
        // tighten a directory left world-listable by an older install. Scoped to
        // the target dir only — never its shared XDG ancestors (`~/.config`,
        // `~/.local/share`), which belong to other apps too.
        fs::set_permissions(path, fs::Permissions::from_mode(DIR_MODE))
    }

    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}
