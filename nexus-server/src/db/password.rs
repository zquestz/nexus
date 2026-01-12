//! Password hashing utilities using Argon2id
//!
//! Provides secure password hashing for production use, with an optional fast
//! mode for testing that avoids Argon2's intentional slowness.
//!
//! # Fast Mode
//!
//! When `fast: true` is passed to `hash_password`, it produces a simple hash
//! with the format `$FAST$<password>`. This is detected automatically by
//! `verify_password` for instant verification.
//!
//! **Never use fast mode in production** - it stores passwords in plaintext.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use nexus_common::validators;
use std::fmt;

/// Prefix for fast (test-only) password hashes
const FAST_HASH_PREFIX: &str = "$FAST$";

/// Error type for password operations
#[derive(Debug)]
pub enum PasswordError {
    /// Password validation failed
    Validation(validators::PasswordError),
    /// Hashing or verification operation failed
    Hash(argon2::password_hash::Error),
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasswordError::Validation(e) => write!(f, "{:?}", e),
            PasswordError::Hash(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for PasswordError {}

impl From<argon2::password_hash::Error> for PasswordError {
    fn from(err: argon2::password_hash::Error) -> Self {
        PasswordError::Hash(err)
    }
}

/// Hash a password
///
/// # Arguments
///
/// * `password` - The plaintext password to hash
/// * `fast` - If true, use simple hash for testing. If false, use Argon2id.
///
/// # Returns
///
/// * `Ok(String)` - The password hash
///   - Fast mode: `$FAST$<password>` (plaintext, for testing only)
///   - Normal mode: Argon2id hash in PHC string format
/// * `Err` - If validation or hashing fails
///
/// # Security
///
/// **Never use `fast: true` in production** - it stores the password in plaintext.
/// Fast mode exists solely to speed up test suites by avoiding Argon2's
/// intentionally slow computation.
pub fn hash_password(password: &str, fast: bool) -> Result<String, PasswordError> {
    // Validate password format (failsafe - handlers should also validate)
    // If this fails, it indicates a bug or attack bypassing handler validation
    if let Err(e) = validators::validate_password(password) {
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

/// Verify a password against a stored hash
///
/// Automatically detects the hash type:
/// - Hashes starting with `$FAST$` use direct string comparison
/// - All other hashes use Argon2 verification
///
/// # Arguments
///
/// * `password` - The plaintext password to verify
/// * `password_hash` - The stored hash (from `hash_password`)
///
/// # Returns
///
/// * `Ok(true)` - Password matches the hash
/// * `Ok(false)` - Password does not match the hash
/// * `Err` - If the hash is malformed or verification fails for technical reasons
///
/// # Security
///
/// Argon2 verification uses constant-time comparison to prevent timing attacks.
/// Fast hash verification does not, but fast hashes should only exist in tests.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, PasswordError> {
    // Validate password format (failsafe - handlers should also validate)
    // Use validate_password_input since empty passwords are valid for guest accounts
    if let Err(e) = validators::validate_password_input(password) {
        return Err(PasswordError::Validation(e));
    }

    // Fast hash - direct comparison (test mode only)
    if let Some(stored) = password_hash.strip_prefix(FAST_HASH_PREFIX) {
        return Ok(stored == password);
    }

    // Argon2 hash - full verification
    let parsed_hash = PasswordHash::new(password_hash)?;
    let argon2 = Argon2::default();

    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(PasswordError::Hash(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2_hash_and_verify() {
        let password = "my_secure_password";
        let hash = hash_password(password, false).unwrap();

        // Should be Argon2 format
        assert!(hash.starts_with("$argon2"), "Should be Argon2 hash");

        // Verify correct password
        assert!(verify_password(password, &hash).unwrap());

        // Verify incorrect password
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_argon2_different_salts() {
        let password = "same_password";
        let hash1 = hash_password(password, false).unwrap();
        let hash2 = hash_password(password, false).unwrap();

        // Hashes should be different due to different salts
        assert_ne!(hash1, hash2);

        // But both should verify correctly
        assert!(verify_password(password, &hash1).unwrap());
        assert!(verify_password(password, &hash2).unwrap());
    }

    #[test]
    fn test_fast_hash_and_verify() {
        let password = "test_password";
        let hash = hash_password(password, true).unwrap();

        // Should be fast format
        assert_eq!(hash, "$FAST$test_password");

        // Verify correct password
        assert!(verify_password(password, &hash).unwrap());

        // Verify incorrect password
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_fast_hash_same_every_time() {
        let password = "same_password";
        let hash1 = hash_password(password, true).unwrap();
        let hash2 = hash_password(password, true).unwrap();

        // Fast hashes should be identical (no salt)
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_verify_auto_detects_hash_type() {
        let password = "test_password";

        // Create both hash types
        let fast_hash = hash_password(password, true).unwrap();
        let argon2_hash = hash_password(password, false).unwrap();

        // verify_password should handle both
        assert!(verify_password(password, &fast_hash).unwrap());
        assert!(verify_password(password, &argon2_hash).unwrap());

        // And reject wrong passwords for both
        assert!(!verify_password("wrong", &fast_hash).unwrap());
        assert!(!verify_password("wrong", &argon2_hash).unwrap());
    }
}
