//! Atomic owner-only file writes for sensitive daemon state.
//!
//! [`write_atomic`] is the workspace's single implementation of the
//! "write a sensitive file without a permissions race" pattern: it
//! writes contents to `<path>.tmp` first (with mode `0o600` set
//! atomically at create time on Unix), then renames into place. A crash
//! between write and rename leaves a stray `.tmp` file but never a
//! partially-written or world-readable target file.
//!
//! Used by `nexus-server` for TLS cert + key writes, and by
//! `nexus-tracker` for TLS cert + key + password-hash writes.
//!
//! On non-Unix platforms the file is created with platform defaults —
//! Windows relies on NTFS ACLs to restrict access to the daemon's user.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Owner-only file mode for sensitive on-disk state on Unix
/// (`0o600` — read/write for owner only). Set atomically at file
/// creation by [`write_atomic`].
#[cfg(unix)]
pub const SECURE_FILE_MODE: u32 = 0o600;

/// Atomically write `contents` to `path` with owner-only permissions.
///
/// Writes to `<path>.tmp` first with mode `0o600` (Unix; no-op on
/// other platforms) set atomically at open time, fsyncs the contents
/// to disk, then renames into place. On Unix the parent directory is
/// also fsynced after the rename so the directory entry is durable.
///
/// # Durability guarantee
///
/// On clean return, the contents at `path` are durable on stable
/// storage (modulo a buggy filesystem layer). Specifically:
/// - A crash *before* return may leave `<path>.tmp` behind; `path`
///   itself is untouched (still has its previous contents, if any).
/// - A crash *after* return reads back the new contents on next boot.
///
/// The fsync calls are intentional — without them, a crash between
/// `write` and the kernel's writeback to disk could leave the file
/// metadata pointing at unwritten blocks (post-reboot reads return
/// garbage). For the use cases (TLS cert/key, tracker password hash)
/// the perf cost is invisible since these files are written rarely
/// (cert generation, `set-password`).
///
/// # Errors
///
/// Returns the underlying [`io::Error`] from `open`, `write`, `sync`,
/// or `rename`. Callers typically wrap with an operator-facing prefix
/// and the path; see `nexus-tracker/src/tls.rs` for the convention.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let tmp = temp_path_for(path);

    {
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(SECURE_FILE_MODE)
                .open(&tmp)?
        };
        #[cfg(not(unix))]
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        // Fsync the contents to disk before close — without this, a
        // crash between this point and writeback could leave the file
        // metadata claiming bytes that aren't actually on disk.
        file.sync_all()?;
    }

    fs::rename(&tmp, path)?;

    // Fsync the parent directory so the rename's directory-entry
    // update is durable. Unix-only — Windows/NTFS uses a different
    // durability model and `FlushFileBuffers` on a directory handle
    // is not portable. The fallback to `.` covers the case where
    // `path` has no parent component (a bare filename); both our
    // production callers pass absolute paths under the daemon's
    // data-dir so this branch is defensive only.
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::File::open(parent)?.sync_all()?;
    }

    Ok(())
}

/// Append `.tmp` to a file path, preserving the rest. Used as the
/// destination for the write step of the atomic write+rename pattern.
fn temp_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_contents_exactly() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data");
        write_atomic(&path, b"hello world").expect("write");
        assert_eq!(fs::read(&path).expect("read"), b"hello world");
    }

    #[test]
    fn overwrites_existing_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data");
        fs::write(&path, b"old contents").expect("seed");
        write_atomic(&path, b"new contents").expect("write");
        assert_eq!(fs::read(&path).expect("read"), b"new contents");
    }

    #[cfg(unix)]
    #[test]
    fn fresh_file_is_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data");
        write_atomic(&path, b"x").expect("write");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(
            mode & 0o777,
            SECURE_FILE_MODE,
            "fresh file must be created with 0o600"
        );
    }

    #[cfg(unix)]
    #[test]
    fn loose_pre_existing_mode_is_corrected_via_rename() {
        // The atomic-write pattern's load-bearing property: even when a
        // pre-existing target file has a looser mode, the rename swaps
        // in the freshly-created `.tmp` (which is `0o600`), leaving the
        // path with the strict mode.
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data");
        fs::write(&path, b"old").expect("seed");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loosen");

        write_atomic(&path, b"new").expect("write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(
            mode & 0o777,
            SECURE_FILE_MODE,
            "atomic rename must restore 0o600 from the freshly-created tmp file"
        );
    }

    #[test]
    fn no_tmp_file_left_behind_on_success() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("data");
        write_atomic(&path, b"x").expect("write");
        let tmp_path = temp_path_for(&path);
        assert!(
            !tmp_path.exists(),
            "tmp file should have been renamed away: {}",
            tmp_path.display()
        );
    }
}
