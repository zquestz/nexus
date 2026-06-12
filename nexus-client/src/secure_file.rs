//! Owner-only file and directory creation for sensitive client state.
//!
//! Config (`config.json`), transfer credentials (`transfers.json`), and the
//! encrypted chat history are written through here so they get restrictive
//! permissions *from creation* — there is never a window where a fresh file is
//! world-readable. On Unix (including macOS) files are created `0o600` and
//! directories `0o700`; Windows has no Unix mode and relies on the user-private
//! profile directory, the same convention the client uses for the rest of its
//! state.

use std::fs;
use std::io;
use std::path::Path;

/// Owner read/write only.
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;
/// Owner read/write/execute only.
#[cfg(unix)]
const DIR_MODE: u32 = 0o700;

/// Write `contents` to `path`, creating or truncating it with owner-only
/// permissions.
///
/// On Unix the mode is applied at creation, so the file is never world-readable
/// even momentarily, and re-enforced on the open handle (an `fchmod`, so no
/// TOCTOU) to lock down a pre-existing file whose permissions drifted — done
/// while the file is still empty, before any content is written.
pub fn write_owner_only(path: &Path, contents: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(path)?;
        file.set_permissions(fs::Permissions::from_mode(FILE_MODE))?;
        file.write_all(contents)
    }

    #[cfg(not(unix))]
    {
        fs::write(path, contents)
    }
}

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
