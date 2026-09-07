//! Tracker authentication: on-disk hashes for the optional `registration` and `listing` passwords.
//!
//! Each is a single PHC-encoded Argon2id hash in its own `<data-dir>/<kind>.hash` file, written
//! atomically with mode `0o600` on Unix. File *presence* is the gating signal — absent = not required.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use argon2::password_hash::{PasswordHash, SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use nexus_common::secure_file;
use nexus_common::validators::MAX_PASSWORD_LENGTH;
use tokio::sync::Semaphore;

use crate::args::PasswordKind;
use crate::constants::{
    ERR_ARGON2_SEMAPHORE_CLOSED, ERR_DELETE_PASSWORD_FILE, ERR_HASH_PASSWORD,
    ERR_PARSE_PASSWORD_HASH, ERR_PASSWORD_EMPTY, ERR_PASSWORD_TOO_LONG, ERR_READ_PASSWORD_FILE,
    ERR_WRITE_PASSWORD_FILE, LISTING_HASH_FILENAME, REGISTRATION_HASH_FILENAME,
};

/// Path to the hash file for `kind` under `data_dir`.
#[must_use]
pub fn hash_path(data_dir: &Path, kind: PasswordKind) -> PathBuf {
    let filename = match kind {
        PasswordKind::Registration => REGISTRATION_HASH_FILENAME,
        PasswordKind::Listing => LISTING_HASH_FILENAME,
    };
    data_dir.join(filename)
}

/// Hash `plain` with Argon2id and atomically write the PHC result to the hash file.
///
/// # Errors
///
/// Errors if `plain` is empty, over `MAX_PASSWORD_LENGTH`, hashing fails, or the write fails.
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

/// Delete the hash file for `kind`. `Ok(true)` if removed, `Ok(false)` if already absent.
///
/// # Errors
///
/// Errors if the file exists but cannot be deleted.
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

/// Load the stored PHC hash for `kind`, `None` if no file (password not required).
///
/// # Errors
///
/// Errors if the file exists but can't be read or doesn't parse as PHC. Parsing at load lets
/// startup refuse a corrupt file and the SIGHUP reload keep prior state, rather than silently
/// failing every auth attempt.
pub fn load_password_hash(data_dir: &Path, kind: PasswordKind) -> Result<Option<String>, String> {
    let path = hash_path(data_dir, kind);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s.trim().to_string(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "{}{}: {}",
                ERR_READ_PASSWORD_FILE,
                path.display(),
                e
            ));
        }
    };
    // Reject a bad hash at load: otherwise `check_password` swallows the verify Err as `false`,
    // silently failing every auth.
    PasswordHash::new(&raw).map_err(|e| format!("{}{}", ERR_PARSE_PASSWORD_HASH, e))?;
    Ok(Some(raw))
}

/// Verify `plain` against a stored PHC hash: `Ok(true)` match, `Ok(false)` mismatch.
///
/// # Errors
///
/// Errors if `phc_hash` can't be parsed as a PHC string.
pub fn verify_password(plain: &str, phc_hash: &str) -> Result<bool, String> {
    let parsed =
        PasswordHash::new(phc_hash).map_err(|e| format!("{}{}", ERR_PARSE_PASSWORD_HASH, e))?;
    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}

/// Open / gated password check for the Register / List handlers. Spec semantics:
/// - `stored_hash = None` → open; any (or no) password passes.
/// - `Some(_)`, `provided = None` → fail (gated, missing).
/// - `Some(_)`, `Some(p)` → pass iff `p` verifies. A malformed stored hash also fails — never
///   accept unauthenticated requests.
///
/// Argon2id verification runs on the blocking pool, bounded by the daemon's shared
/// permits. Excess checks wait asynchronously without submitting blocking work.
#[must_use]
pub async fn check_password(
    provided: Option<&str>,
    stored_hash: Option<&str>,
    permits: &Arc<Semaphore>,
) -> bool {
    check_password_with_verifier(provided, stored_hash, permits, verify_password).await
}

async fn check_password_with_verifier<F>(
    provided: Option<&str>,
    stored_hash: Option<&str>,
    permits: &Arc<Semaphore>,
    verify: F,
) -> bool
where
    F: FnOnce(&str, &str) -> Result<bool, String> + Send + 'static,
{
    match (provided, stored_hash) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(plain), Some(hash)) => {
            let permit = Arc::clone(permits)
                .acquire_owned()
                .await
                .expect(ERR_ARGON2_SEMAPHORE_CLOSED);
            let plain = plain.to_owned();
            let hash = hash.to_owned();
            tokio::task::spawn_blocking(move || {
                // Cancelling the caller cannot stop a running blocking task. Keep
                // its capacity reserved until verification finishes or unwinds.
                let _permit = permit;
                verify(&plain, &hash).unwrap_or(false)
            })
            .await
            .unwrap_or(false)
        }
    }
}

/// Write the PHC string plus a trailing `\n` (keeps the file POSIX text for cat / grep).
fn write_hash_file(data_dir: &Path, kind: PasswordKind, hash: &str) -> Result<(), String> {
    let path = hash_path(data_dir, kind);
    let mut contents = Vec::with_capacity(hash.len() + 1);
    contents.extend_from_slice(hash.as_bytes());
    contents.push(b'\n');
    secure_file::write_atomic(&path, &contents)
        .map_err(|e| format!("{}{}: {}", ERR_WRITE_PASSWORD_FILE, path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Waker};
    use std::time::Duration;

    use tokio::sync::oneshot;
    use tokio::time::timeout;

    use crate::constants::MAX_CONCURRENT_ARGON2_OPS;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn assert_pending(future: Pin<&mut impl Future>) {
        assert!(
            future
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }

    #[tokio::test]
    async fn verification_concurrency_is_bounded_and_waiters_resume() {
        let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_ARGON2_OPS));
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut checks = Vec::new();
        let mut releases = Vec::new();
        let mut last_started = None;

        for index in 0..=MAX_CONCURRENT_ARGON2_OPS {
            let (started_tx, started_rx) = oneshot::channel();
            let (release_tx, release_rx) = oneshot::channel();
            let running = Arc::clone(&running);
            let peak = Arc::clone(&peak);
            let mut check = Box::pin(check_password_with_verifier(
                Some("secret"),
                Some("hash"),
                &permits,
                move |plain, hash| {
                    assert_eq!((plain, hash), ("secret", "hash"));
                    let active = running.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(active, Ordering::SeqCst);
                    let _ = started_tx.send(());
                    // Dropping the sender also unblocks cleanup after a failed assertion.
                    let _ = release_rx.blocking_recv();
                    running.fetch_sub(1, Ordering::SeqCst);
                    Ok(true)
                },
            ));
            assert_pending(check.as_mut());
            releases.push(release_tx);
            checks.push(check);

            if index < MAX_CONCURRENT_ARGON2_OPS {
                timeout(TEST_TIMEOUT, started_rx).await.unwrap().unwrap();
                assert_eq!(
                    permits.available_permits(),
                    MAX_CONCURRENT_ARGON2_OPS - index - 1
                );
            } else {
                last_started = Some(started_rx);
            }
        }

        assert_eq!(running.load(Ordering::SeqCst), MAX_CONCURRENT_ARGON2_OPS);
        releases.remove(0).send(()).unwrap();
        assert!(timeout(TEST_TIMEOUT, checks.remove(0)).await.unwrap());
        assert_pending(checks.last_mut().unwrap().as_mut());
        timeout(TEST_TIMEOUT, last_started.unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(running.load(Ordering::SeqCst), MAX_CONCURRENT_ARGON2_OPS);

        for release in releases {
            release.send(()).unwrap();
        }
        for check in checks {
            assert!(timeout(TEST_TIMEOUT, check).await.unwrap());
        }
        assert_eq!(running.load(Ordering::SeqCst), 0);
        assert_eq!(peak.load(Ordering::SeqCst), MAX_CONCURRENT_ARGON2_OPS);
        assert_eq!(permits.available_permits(), MAX_CONCURRENT_ARGON2_OPS);
    }

    #[tokio::test]
    async fn cancelling_caller_keeps_running_verification_reserved() {
        let permits = Arc::new(Semaphore::new(1));
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let mut check = Box::pin(check_password_with_verifier(
            Some("secret"),
            Some("hash"),
            &permits,
            move |_, _| {
                let _ = started_tx.send(());
                let _ = release_rx.blocking_recv();
                Ok(true)
            },
        ));
        assert_pending(check.as_mut());
        timeout(TEST_TIMEOUT, started_rx).await.unwrap().unwrap();

        drop(check);
        assert_eq!(permits.available_permits(), 0);
        let mut next = Box::pin(check_password_with_verifier(
            Some("secret"),
            Some("hash"),
            &permits,
            |_, _| Ok(true),
        ));
        assert_pending(next.as_mut());

        release_tx.send(()).unwrap();
        assert!(timeout(TEST_TIMEOUT, next).await.unwrap());
        assert_eq!(permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn cancelling_waiter_never_submits_verification() {
        let permits = Arc::new(Semaphore::new(1));
        let held = Arc::clone(&permits).acquire_owned().await.unwrap();
        let (started_tx, started_rx) = oneshot::channel();
        let mut check = Box::pin(check_password_with_verifier(
            Some("secret"),
            Some("hash"),
            &permits,
            move |_, _| {
                let _ = started_tx.send(());
                Ok(true)
            },
        ));
        assert_pending(check.as_mut());
        drop(check);
        assert!(timeout(TEST_TIMEOUT, started_rx).await.unwrap().is_err());
        assert_eq!(permits.available_permits(), 0);

        drop(held);
        let next =
            check_password_with_verifier(Some("secret"), Some("hash"), &permits, |_, _| Ok(true));
        assert!(timeout(TEST_TIMEOUT, next).await.unwrap());
        assert_eq!(permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn open_and_missing_password_checks_bypass_saturated_permits() {
        let permits = Arc::new(Semaphore::new(1));
        let _held = Arc::clone(&permits).acquire_owned().await.unwrap();
        for (provided, stored, expected) in [
            (None, None, true),
            (Some("ignored"), None, true),
            (None, Some("hash"), false),
        ] {
            assert_eq!(
                timeout(TEST_TIMEOUT, check_password(provided, stored, &permits))
                    .await
                    .unwrap(),
                expected,
            );
            assert_eq!(permits.available_permits(), 0);
        }
    }

    #[tokio::test]
    async fn verification_failures_release_permits_and_fail_closed() {
        let permits = Arc::new(Semaphore::new(1));
        let failed =
            check_password_with_verifier(Some("secret"), Some("hash"), &permits, |_, _| {
                Err("verify error".to_string())
            });
        assert!(!timeout(TEST_TIMEOUT, failed).await.unwrap());
        assert_eq!(permits.available_permits(), 1);

        let panicked =
            check_password_with_verifier(Some("secret"), Some("hash"), &permits, |_, _| {
                panic!("verify panic")
            });
        assert!(!timeout(TEST_TIMEOUT, panicked).await.unwrap());
        assert_eq!(permits.available_permits(), 1);

        let next =
            check_password_with_verifier(Some("secret"), Some("hash"), &permits, |_, _| Ok(true));
        assert!(timeout(TEST_TIMEOUT, next).await.unwrap());
        assert_eq!(permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn bounded_password_check_preserves_argon2_results() {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(b"secret", &salt)
            .unwrap()
            .to_string();
        let permits = Arc::new(Semaphore::new(1));

        for (plain, stored, expected) in [
            ("secret", hash.as_str(), true),
            ("wrong", hash.as_str(), false),
            ("", hash.as_str(), false),
            ("secret", "not a PHC hash", false),
        ] {
            assert_eq!(
                timeout(
                    TEST_TIMEOUT,
                    check_password(Some(plain), Some(stored), &permits)
                )
                .await
                .unwrap(),
                expected,
            );
            assert_eq!(permits.available_permits(), 1);
        }
    }

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

        // A single `$argon2id$` prefix confirms truncate-and-rewrite, not append.
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

        // Strip the trailing newline to simulate a hand-pasted file.
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
            secure_file::SECURE_FILE_MODE,
            "password file should be created with 0o600"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_password_file_perms_corrected_on_overwrite() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        set_password(tmp.path(), PasswordKind::Registration, "first").expect("set first");

        // Loosen perms, then confirm the atomic rename (new inode) restores 0o600 on re-set.
        let path = hash_path(tmp.path(), PasswordKind::Registration);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loosen");

        set_password(tmp.path(), PasswordKind::Registration, "second").expect("set second");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(
            mode & 0o777,
            secure_file::SECURE_FILE_MODE,
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
