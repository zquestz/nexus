//! Password hashing utilities using Argon2id.
//!
//! `fast: true` produces a `$FAST$<password>` plaintext hash for tests only,
//! detected automatically by `verify_password`. Never use fast mode in
//! production.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use nexus_common::validators::{self, PasswordStrength};
use std::fmt;

use crate::constants::ERR_PASSWORD_TASK_JOIN;

const FAST_HASH_PREFIX: &str = "$FAST$";

/// Password [`DUMMY_VERIFY_HASH`] was generated from. Not a real credential —
/// only the drift-guard test uses it, to re-derive the hash under the current
/// `Argon2::default()` parameters.
#[cfg(test)]
const DUMMY_VERIFY_PASSWORD: &str = "nexus-login-timing-equalizer";

/// A fixed Argon2id hash (current `Argon2::default()` params) used to equalize
/// login response timing for an unknown username. A login handler that finds no
/// account runs the real [`verify_password_async`] against THIS hash, so the
/// unknown-user path pays the same Argon2 cost as a wrong-password attempt on a
/// real account — closing the username-enumeration timing oracle. Because the
/// equalizer goes through `verify_password` (lenient `validate_password_input`,
/// not the strength check), a weak or empty password reaches Argon2 instead of
/// short-circuiting sub-millisecond. The supplied password never matches; the
/// result is discarded.
///
/// If `Argon2::default()` ever changes its parameters, the
/// `dummy_hash_matches_current_argon2_defaults` test fails — regenerate this
/// value from `DUMMY_VERIFY_PASSWORD` so the equalizer cost still matches the
/// cost of verifying accounts hashed by the new defaults.
const DUMMY_VERIFY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$7RgGov81OJ457eyqQUa7XQ$LFQL/pSW4kjwXefB0uoiSeE4qrWNIKON7v/AUn7eH/0";

#[derive(Debug)]
pub enum PasswordError {
    Validation(validators::PasswordError),
    Hash(argon2::password_hash::Error),
    /// The blocking task that ran the Argon2 work panicked or was cancelled.
    /// Should never happen in practice; surfaces here so the caller fails
    /// closed instead of treating the operation as a verify-success.
    TaskJoin,
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasswordError::Validation(e) => write!(f, "{:?}", e),
            PasswordError::Hash(e) => write!(f, "{}", e),
            PasswordError::TaskJoin => write!(f, "{}", ERR_PASSWORD_TASK_JOIN),
        }
    }
}

impl std::error::Error for PasswordError {}

impl From<argon2::password_hash::Error> for PasswordError {
    fn from(err: argon2::password_hash::Error) -> Self {
        PasswordError::Hash(err)
    }
}

/// Hash a password.
///
/// # Security
///
/// **Never use `fast: true` in production** - it stores the password in plaintext.
/// Fast mode exists solely to speed up test suites by avoiding Argon2's
/// intentionally slow computation.
pub fn hash_password(
    password: &str,
    min_strength: PasswordStrength,
    fast: bool,
) -> Result<String, PasswordError> {
    // Failsafe revalidation; handlers should also validate.
    if let Err(e) = validators::validate_password(password, min_strength, &[]) {
        return Err(PasswordError::Validation(e));
    }

    if fast {
        Ok(format!("{}{}", FAST_HASH_PREFIX, password))
    } else {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
        Ok(password_hash.to_string())
    }
}

/// Verify a password against a stored hash.
///
/// Automatically detects the hash type: `$FAST$` prefix uses direct string
/// comparison, all other hashes use Argon2 verification.
///
/// # Security
///
/// Argon2 verification uses constant-time comparison to prevent timing attacks.
/// Fast hash verification does not, but fast hashes should only exist in tests.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, PasswordError> {
    // Failsafe revalidation; validate_password_input allows the empty
    // passwords valid for guest accounts.
    if let Err(e) = validators::validate_password_input(password) {
        return Err(PasswordError::Validation(e));
    }

    if let Some(stored) = password_hash.strip_prefix(FAST_HASH_PREFIX) {
        return Ok(stored == password);
    }

    let parsed_hash = PasswordHash::new(password_hash)?;
    let argon2 = Argon2::default();

    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(PasswordError::Hash(e)),
    }
}

/// Maximum concurrent Argon2 operations across the whole server.
///
/// Each Argon2id op pins a blocking-pool thread and allocates its memory
/// parameter (~19 MiB at defaults). Without a cap, a distributed login
/// flood can run hundreds of ops at once — saturating every core and
/// gigabytes of memory — because the blocking pool admits far more
/// threads than Argon2 should ever occupy. Excess callers queue on the
/// semaphore instead: latency degrades under attack, the server does not.
const MAX_CONCURRENT_ARGON2_OPS: usize = 8;

static ARGON2_PERMITS: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_ARGON2_OPS);

/// Panic message: the static Argon2 semaphore is never closed.
const ERR_ARGON2_SEMAPHORE_CLOSED: &str = "Argon2 semaphore unexpectedly closed";

/// Async wrapper around [`hash_password`] that runs Argon2 on the blocking pool.
///
/// Argon2id with default params takes ~50–500 ms per call. Calling the sync
/// version from an async handler pins the worker thread for that duration; a
/// flood of password creates / changes from rotating IPs can pin every worker.
/// This wrapper offloads the Argon2 work via `spawn_blocking` so the runtime
/// stays responsive, and bounds global concurrency via [`ARGON2_PERMITS`].
///
/// The fast (test-only) path is sync-cheap (`format!`) and not offloaded.
pub async fn hash_password_async(
    password: String,
    min_strength: PasswordStrength,
    fast: bool,
) -> Result<String, PasswordError> {
    if fast {
        return hash_password(&password, min_strength, fast);
    }
    let _permit = ARGON2_PERMITS
        .acquire()
        .await
        .expect(ERR_ARGON2_SEMAPHORE_CLOSED);
    tokio::task::spawn_blocking(move || hash_password(&password, min_strength, fast))
        .await
        .unwrap_or(Err(PasswordError::TaskJoin))
}

/// Async wrapper around [`verify_password`] that runs Argon2 on the blocking pool.
///
/// See [`hash_password_async`] for rationale and the global concurrency
/// bound. Fast hashes short-circuit inline (string compare); only the
/// Argon2 path is offloaded.
pub async fn verify_password_async(
    password: String,
    password_hash: String,
) -> Result<bool, PasswordError> {
    if password_hash.starts_with(FAST_HASH_PREFIX) {
        return verify_password(&password, &password_hash);
    }
    let _permit = ARGON2_PERMITS
        .acquire()
        .await
        .expect(ERR_ARGON2_SEMAPHORE_CLOSED);
    tokio::task::spawn_blocking(move || verify_password(&password, &password_hash))
        .await
        .unwrap_or(Err(PasswordError::TaskJoin))
}

/// Verify against the fixed dummy Argon2 hash used for unknown-user login timing.
///
/// This deliberately routes through [`verify_password_async`], not
/// [`hash_password_async`], so weak or empty passwords use the same lenient
/// validation and Argon2 work as a wrong password for a real account.
pub async fn verify_unknown_user_password_for_timing(
    password: String,
) -> Result<bool, PasswordError> {
    verify_password_async(password, DUMMY_VERIFY_HASH.to_string()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_hash_matches_current_argon2_defaults() {
        // Re-derive the dummy hash from its known password and embedded salt
        // under TODAY's Argon2::default(). If the params drift, the rehash won't
        // match and this fails — the const must be regenerated so the timing
        // equalizer's cost still tracks real-account verifies.
        let parsed = PasswordHash::new(DUMMY_VERIFY_HASH).expect("DUMMY_VERIFY_HASH must parse");
        let salt = parsed.salt.expect("DUMMY_VERIFY_HASH must carry a salt");
        let rehash = Argon2::default()
            .hash_password(DUMMY_VERIFY_PASSWORD.as_bytes(), salt)
            .expect("re-hash must succeed")
            .to_string();
        assert_eq!(
            rehash, DUMMY_VERIFY_HASH,
            "Argon2::default() params drifted — regenerate DUMMY_VERIFY_HASH from DUMMY_VERIFY_PASSWORD"
        );
    }

    #[test]
    fn dummy_hash_runs_argon2_for_weak_and_empty_passwords() {
        // It is a real Argon2id hash, not a fast/plaintext one.
        assert!(DUMMY_VERIFY_HASH.starts_with("$argon2id$"));
        assert!(!DUMMY_VERIFY_HASH.starts_with(FAST_HASH_PREFIX));

        // The equalizer path uses verify_password (lenient input validation), so
        // weak and empty passwords reach Argon2 and return false rather than
        // short-circuiting on strength — which is what closes the timing oracle.
        assert!(matches!(verify_password("a", DUMMY_VERIFY_HASH), Ok(false)));
        assert!(matches!(verify_password("", DUMMY_VERIFY_HASH), Ok(false)));
        // And the known password still verifies, confirming the const is intact.
        assert!(matches!(
            verify_password(DUMMY_VERIFY_PASSWORD, DUMMY_VERIFY_HASH),
            Ok(true)
        ));
    }

    #[tokio::test]
    async fn unknown_user_timing_helper_accepts_weak_password_input() {
        assert!(matches!(
            verify_unknown_user_password_for_timing("a".to_string()).await,
            Ok(false)
        ));
    }

    #[test]
    fn test_argon2_hash_and_verify() {
        let password = "my_secure_password";
        let hash = hash_password(password, PasswordStrength::Weak, false).unwrap();

        assert!(hash.starts_with("$argon2"), "Should be Argon2 hash");

        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_argon2_different_salts() {
        let password = "same_password";
        let hash1 = hash_password(password, PasswordStrength::Weak, false).unwrap();
        let hash2 = hash_password(password, PasswordStrength::Weak, false).unwrap();

        // Different salts produce different hashes that both still verify.
        assert_ne!(hash1, hash2);
        assert!(verify_password(password, &hash1).unwrap());
        assert!(verify_password(password, &hash2).unwrap());
    }

    #[test]
    fn test_fast_hash_and_verify() {
        let password = "test_password";
        let hash = hash_password(password, PasswordStrength::Weak, true).unwrap();

        assert_eq!(hash, "$FAST$test_password");

        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_fast_hash_same_every_time() {
        let password = "same_password";
        let hash1 = hash_password(password, PasswordStrength::Weak, true).unwrap();
        let hash2 = hash_password(password, PasswordStrength::Weak, true).unwrap();

        // Fast hashes are identical (no salt).
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_verify_auto_detects_hash_type() {
        let password = "test_password";

        let fast_hash = hash_password(password, PasswordStrength::Weak, true).unwrap();
        let argon2_hash = hash_password(password, PasswordStrength::Weak, false).unwrap();

        assert!(verify_password(password, &fast_hash).unwrap());
        assert!(verify_password(password, &argon2_hash).unwrap());

        assert!(!verify_password("wrong", &fast_hash).unwrap());
        assert!(!verify_password("wrong", &argon2_hash).unwrap());
    }
}
