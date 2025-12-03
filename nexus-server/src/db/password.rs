//! Password hashing utilities using Argon2id

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use nexus_common::validators;
use std::fmt;

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

/// Hash a password using Argon2id with a random salt
///
/// Uses the Argon2id algorithm (recommended by OWASP) to securely hash passwords.
/// Each hash includes a randomly generated salt, so the same password will produce
/// different hashes each time this function is called.
///
/// # Arguments
///
/// * `password` - The plaintext password to hash
///
/// # Returns
///
/// * `Ok(String)` - The password hash in PHC string format (includes algorithm, salt, and hash)
///   - Format looks like: `$argon2id$v=19$m=19456,t=2,p=1$...`
/// * `Err` - If the hashing operation fails (rare, typically memory allocation issues)
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    // Validate password format (failsafe - handlers should also validate)
    // If this fails, it indicates a bug or attack bypassing handler validation
    if let Err(e) = validators::validate_password(password) {
        return Err(PasswordError::Validation(e));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(password_hash.to_string())
}

/// Verify a password against a stored hash
///
/// Compares a plaintext password against a previously hashed password using
/// constant-time comparison to prevent timing attacks.
///
/// # Arguments
///
/// * `password` - The plaintext password to verify
/// * `password_hash` - The stored hash (in PHC string format from `hash_password`)
///
/// # Returns
///
/// * `Ok(true)` - Password matches the hash
/// * `Ok(false)` - Password does not match the hash
/// * `Err` - If the hash is malformed or verification fails for technical reasons
///
/// # Security
///
/// This function uses constant-time comparison to prevent timing-based attacks
/// that could reveal information about the password or hash.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, PasswordError> {
    // Validate password format (failsafe - handlers should also validate)
    // If this fails, it indicates a bug or attack bypassing handler validation
    if let Err(e) = validators::validate_password(password) {
        return Err(PasswordError::Validation(e));
    }

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
    fn test_hash_and_verify() {
        let password = "my_secure_password";
        let hash = hash_password(password).unwrap();

        // Verify correct password
        assert!(verify_password(password, &hash).unwrap());

        // Verify incorrect password
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_salts() {
        let password = "same_password";
        let hash1 = hash_password(password).unwrap();
        let hash2 = hash_password(password).unwrap();

        // Hashes should be different due to different salts
        assert_ne!(hash1, hash2);

        // But both should verify correctly
        assert!(verify_password(password, &hash1).unwrap());
        assert!(verify_password(password, &hash2).unwrap());
    }
}
