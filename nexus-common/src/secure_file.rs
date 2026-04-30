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
/// other platforms) set atomically at open time, then renames into
/// place. A crash between write and rename leaves the `.tmp` file
/// behind but the previous contents at `path` (if any) are untouched.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] from `open`, `write`, or
/// `rename`. Callers typically wrap with an operator-facing prefix and
/// the path; see `nexus-tracker/src/tls.rs` for the convention.
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
    }

    fs::rename(&tmp, path)?;
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
