//! Tracker authentication (password hash storage)
//!
//! On-disk hashes for the optional `registration` and `listing` passwords.
//! Each password lives in its own file under the data directory:
//!
//! - `<data-dir>/registration.hash`
//! - `<data-dir>/listing.hash`
//!
//! Each file holds a single PHC-encoded Argon2id hash. The file's *presence*
//! is the gating signal — absent file means that password is not required.
//! Files are written atomically (temp file + rename) with mode `0o600` on
//! Unix.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use argon2::password_hash::{PasswordHash, SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use nexus_common::validators::MAX_PASSWORD_LENGTH;

use crate::args::PasswordKind;
#[cfg(unix)]
use crate::constants::PASSWORD_FILE_MODE;
use crate::constants::{
    ERR_DELETE_PASSWORD_FILE, ERR_HASH_PASSWORD, ERR_PARSE_PASSWORD_HASH, ERR_PASSWORD_EMPTY,
    ERR_PASSWORD_TOO_LONG, ERR_READ_PASSWORD_FILE, ERR_RENAME_FILE, ERR_WRITE_PASSWORD_FILE,
    LISTING_HASH_FILENAME, REGISTRATION_HASH_FILENAME,
};

/// Path to the password hash file for the given kind under `data_dir`.
#[must_use]
pub fn hash_path(data_dir: &Path, kind: PasswordKind) -> PathBuf {
    let filename = match kind {
        PasswordKind::Registration => REGISTRATION_HASH_FILENAME,
        PasswordKind::Listing => LISTING_HASH_FILENAME,
    };
    data_dir.join(filename)
}

/// Hash `plain` with Argon2id and atomically write the PHC-encoded result
/// to the hash file (writes to `<file>.tmp` then renames).
///
/// # Errors
///
/// Returns an error string (already prefixed with the relevant operator-
/// facing constant) if `plain` is empty, exceeds `MAX_PASSWORD_LENGTH`,
/// hashing fails, or the file cannot be written.
pub fn set_password(data_dir: &Path, kind: PasswordKind, plain: &str) -> Result<(), String> {
    if plain.is_empty() {
        return Err(ERR_PASSWORD_EMPTY.to_string());
    }
    if plain.len() > MAX_PASSWORD_LENGTH {
        return Err(format!(
            "{}{} bytes",
            ERR_PASSWORD_TOO_LONG, MAX_PASSWORD_LENGTH
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| format!("{}{}", ERR_HASH_PASSWORD, e))?
        .to_string();
    write_hash_file(data_dir, kind, &hash)
}

/// Delete the hash file for `kind`. Returns `Ok(true)` if a file was
/// actually removed and `Ok(false)` if no file was present (already cleared).
///
/// # Errors
///
/// Returns an error string if the file exists but cannot be deleted.
pub fn clear_password(data_dir: &Path, kind: PasswordKind) -> Result<bool, String> {
    let path = hash_path(data_dir, kind);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!(
            "{}{}: {}",
            ERR_DELETE_PASSWORD_FILE,
            path.display(),
            e
        )),
    }
}

/// Load the stored PHC hash for `kind`, returning `None` if no file is
/// present (which means the corresponding password is not required).
///
/// # Errors
///
/// Returns an error string if the file exists but cannot be read.
pub fn load_password_hash(data_dir: &Path, kind: PasswordKind) -> Result<Option<String>, String> {
    let path = hash_path(data_dir, kind);
    match fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "{}{}: {}",
            ERR_READ_PASSWORD_FILE,
            path.display(),
            e
        )),
    }
}

/// Verify `plain` against a previously-stored PHC hash. Returns `Ok(true)`
/// for a match, `Ok(false)` for a mismatch, and an error if the stored
/// hash is malformed.
///
/// # Errors
///
/// Returns an error string if `phc_hash` cannot be parsed as a PHC string.
#[allow(dead_code)] // used by TrackerRegister/TrackerList handlers (next step)
pub fn verify_password(plain: &str, phc_hash: &str) -> Result<bool, String> {
    let parsed =
        PasswordHash::new(phc_hash).map_err(|e| format!("{}{}", ERR_PARSE_PASSWORD_HASH, e))?;
    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}

/// Append `.tmp` to a file path, preserving the rest. Used as the
/// destination for the write step of the atomic write+rename pattern.
fn temp_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

#[cfg(unix)]
fn write_hash_file(data_dir: &Path, kind: PasswordKind, hash: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;

    let path = hash_path(data_dir, kind);
    let tmp = temp_path_for(&path);

    // Write to `<file>.tmp` with 0o600 atomically on create; rename swaps
    // it into place. Crash between write and rename leaves a `.tmp` file
    // but the previous hash file is untouched.
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(PASSWORD_FILE_MODE)
            .open(&tmp)
            .map_err(|e| format!("{}{}: {}", ERR_WRITE_PASSWORD_FILE, tmp.display(), e))?;
        file.write_all(hash.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .map_err(|e| format!("{}{}: {}", ERR_WRITE_PASSWORD_FILE, tmp.display(), e))?;
    }

    fs::rename(&tmp, &path).map_err(|e| format!("{}{}: {}", ERR_RENAME_FILE, path.display(), e))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_hash_file(data_dir: &Path, kind: PasswordKind, hash: &str) -> Result<(), String> {
    let path = hash_path(data_dir, kind);
    let tmp = temp_path_for(&path);

    {
        let mut file = fs::File::create(&tmp)
            .map_err(|e| format!("{}{}: {}", ERR_WRITE_PASSWORD_FILE, tmp.display(), e))?;
        file.write_all(hash.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .map_err(|e| format!("{}{}: {}", ERR_WRITE_PASSWORD_FILE, tmp.display(), e))?;
    }

    fs::rename(&tmp, &path).map_err(|e| format!("{}{}: {}", ERR_RENAME_FILE, path.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_path_uses_kind_specific_filename() {
        let dir = Path::new("/data");
        assert_eq!(
            hash_path(dir, PasswordKind::Registration),
            Path::new("/data/registration.hash")
        );
        assert_eq!(
            hash_path(dir, PasswordKind::Listing),
            Path::new("/data/listing.hash")
        );
    }

    #[test]
    fn test_set_then_load_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set");

        let stored = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("file present");
        assert!(stored.starts_with("$argon2id$"), "should be PHC-encoded");
    }

    #[test]
    fn test_verify_correct_and_incorrect_plaintexts() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Listing, "hunter2").expect("set");
        let stored = load_password_hash(tmp.path(), PasswordKind::Listing)
            .expect("load")
            .expect("file present");

        assert!(verify_password("hunter2", &stored).expect("verify"));
        assert!(!verify_password("hunter3", &stored).expect("verify"));
        assert!(!verify_password("", &stored).expect("verify"));
    }

    #[test]
    fn test_set_overwrites_existing_hash() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "first").expect("set first");
        let first = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("present");

        set_password(tmp.path(), PasswordKind::Registration, "second").expect("set second");
        let second = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("present");

        assert_ne!(first, second, "hash should change after overwrite");
        assert!(verify_password("second", &second).expect("verify"));
        assert!(!verify_password("first", &second).expect("verify"));

        // Confirm the file was *truncated* and rewritten, not appended:
        // a single PHC string means a single `$argon2id$` prefix.
        let raw = fs::read_to_string(hash_path(tmp.path(), PasswordKind::Registration))
            .expect("read raw");
        assert_eq!(
            raw.matches("$argon2id$").count(),
            1,
            "overwrite should leave exactly one hash, not append a second"
        );
    }

    #[test]
    fn test_set_with_same_plaintext_produces_different_hashes() {
        let tmp = tempfile::tempdir().expect("tempdir");

        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set 1");
        let first = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("present");

        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set 2");
        let second = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("present");

        assert_ne!(
            first, second,
            "fresh salt should produce different hashes for the same plaintext"
        );
        assert!(verify_password("hunter2", &first).expect("verify 1"));
        assert!(verify_password("hunter2", &second).expect("verify 2"));
    }

    #[test]
    fn test_load_handles_hash_without_trailing_newline() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set");

        // Simulate a hand-pasted or non-Unix-line-ending file by stripping
        // the trailing newline `set_password` writes.
        let path = hash_path(tmp.path(), PasswordKind::Registration);
        let raw = fs::read_to_string(&path).expect("read");
        let trimmed = raw.trim_end_matches('\n');
        fs::write(&path, trimmed).expect("rewrite without newline");

        let stored = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("present");
        assert!(verify_password("hunter2", &stored).expect("verify"));
    }

    #[test]
    fn test_clear_when_present_removes_file_and_returns_true() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set");

        let cleared = clear_password(tmp.path(), PasswordKind::Registration).expect("clear");
        assert!(cleared, "should report a file was removed");
        assert!(
            load_password_hash(tmp.path(), PasswordKind::Registration)
                .expect("load")
                .is_none(),
            "no file should remain"
        );
    }

    #[test]
    fn test_clear_when_absent_returns_false_without_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cleared = clear_password(tmp.path(), PasswordKind::Registration).expect("clear absent");
        assert!(!cleared, "should report no file was present");
    }

    #[test]
    fn test_load_when_absent_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(
            load_password_hash(tmp.path(), PasswordKind::Listing)
                .expect("load")
                .is_none()
        );
    }

    #[test]
    fn test_set_empty_password_is_rejected() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = set_password(tmp.path(), PasswordKind::Registration, "")
            .expect_err("empty password must error");
        assert_eq!(err, ERR_PASSWORD_EMPTY);
    }

    #[test]
    fn test_set_too_long_password_is_rejected() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let too_long = "a".repeat(MAX_PASSWORD_LENGTH + 1);
        let err = set_password(tmp.path(), PasswordKind::Registration, &too_long)
            .expect_err("over-length password must error");
        assert!(
            err.starts_with(ERR_PASSWORD_TOO_LONG),
            "error should start with ERR_PASSWORD_TOO_LONG, got: {err}"
        );
        // And nothing was written.
        assert!(
            load_password_hash(tmp.path(), PasswordKind::Registration)
                .expect("load")
                .is_none()
        );
    }

    #[test]
    fn test_set_at_max_length_is_accepted() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plain = "a".repeat(MAX_PASSWORD_LENGTH);
        set_password(tmp.path(), PasswordKind::Registration, &plain).expect("set");
        let stored = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load")
            .expect("present");
        assert!(verify_password(&plain, &stored).expect("verify"));
    }

    #[test]
    fn test_set_leaves_no_tmp_file_behind() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set");

        let final_path = hash_path(tmp.path(), PasswordKind::Registration);
        let tmp_path = temp_path_for(&final_path);
        assert!(final_path.exists(), "final hash file should exist");
        assert!(
            !tmp_path.exists(),
            "tmp file should have been renamed away, not left behind: {}",
            tmp_path.display()
        );
    }

    #[test]
    fn test_kinds_are_independent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "regpass").expect("set reg");

        // Listing should still be absent.
        assert!(
            load_password_hash(tmp.path(), PasswordKind::Listing)
                .expect("load")
                .is_none()
        );

        set_password(tmp.path(), PasswordKind::Listing, "listpass").expect("set list");
        let reg = load_password_hash(tmp.path(), PasswordKind::Registration)
            .expect("load reg")
            .expect("present");
        let list = load_password_hash(tmp.path(), PasswordKind::Listing)
            .expect("load list")
            .expect("present");
        assert!(verify_password("regpass", &reg).expect("verify"));
        assert!(verify_password("listpass", &list).expect("verify"));
    }

    #[cfg(unix)]
    #[test]
    fn test_password_file_is_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "hunter2").expect("set");

        let path = hash_path(tmp.path(), PasswordKind::Registration);
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(
            mode & 0o777,
            PASSWORD_FILE_MODE,
            "password file should be created with 0o600"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_password_file_perms_corrected_on_overwrite() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "first").expect("set first");

        // Loosen the existing file's perms to simulate tampering or a
        // botched manual setup, then re-set and confirm the atomic
        // rename naturally restores 0o600 (rename swaps the inode).
        let path = hash_path(tmp.path(), PasswordKind::Registration);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loosen");

        set_password(tmp.path(), PasswordKind::Registration, "second").expect("set second");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(
            mode & 0o777,
            PASSWORD_FILE_MODE,
            "atomic rename should restore 0o600 from the freshly-created tmp file"
        );
    }

    #[test]
    fn test_verify_with_malformed_hash_errors() {
        let err = verify_password("anything", "not-a-phc-string")
            .expect_err("malformed hash should error");
        assert!(err.starts_with(ERR_PARSE_PASSWORD_HASH));
    }
}
