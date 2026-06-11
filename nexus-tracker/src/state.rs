//! Shared daemon state (`Arc<TrackerState>`), built once at startup and
//! handed to every spawned connection task.

use std::path::Path;
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use tracing::{error, info};

use crate::args::PasswordKind;
use crate::auth;
use crate::constants::{
    ERR_LISTING_HASH_LOCK_POISONED, ERR_PASSWORD_HASH_LOCK_POISONED,
    ERR_REGISTRATION_HASH_LOCK_POISONED, LOG_PASSWORD_RELOAD_FAILED, LOG_PASSWORD_RELOADED,
};
use crate::registry::Registry;
use crate::resolver::{Resolver, TokioResolver};
use nexus_common::rate_limiter::RateLimiter;

/// Daemon-level state shared across connection tasks. The password
/// hashes are `RwLock` because SIGHUP may swap them at runtime; handlers
/// snapshot under a brief read lock, then verify (slow Argon2) outside
/// it so a verify can't block a reload.
pub struct TrackerState {
    pub registry: Mutex<Registry>,

    /// Argon2id PHC of the registration password, `None` if open.
    /// Reloaded by [`reload_passwords`](Self::reload_passwords) on SIGHUP.
    pub registration_password_hash: RwLock<Option<String>>,

    /// Argon2id PHC of the listing password, `None` if open.
    /// Reloaded by [`reload_passwords`](Self::reload_passwords) on SIGHUP.
    pub listing_password_hash: RwLock<Option<String>>,

    /// Seconds between refreshes, advertised in the register response.
    pub refresh_interval: u32,

    /// Debited per TCP accept; over-limit peers are dropped silently.
    pub connection_rate_limiter: RateLimiter,

    /// Debited only on failed password verify; over-limit attempts get
    /// `rate_limited`.
    pub auth_failure_rate_limiter: RateLimiter,

    /// Per-entry minimum between accepted refreshes
    /// (`Duration::ZERO` disables it; tests use this).
    pub refresh_floor: Duration,

    /// DNS resolver for register-time address validation. `Send + Sync`
    /// (via [`Resolver`]) is required so `TrackerState` can be shared as
    /// `Arc`. Tests swap a mock via [`with_resolver`](Self::with_resolver).
    pub resolver: Box<dyn Resolver>,
}

impl TrackerState {
    /// Build a new state. `connection_rate` / `auth_failure_rate` are
    /// per-IP events/minute (0 = unlimited); `refresh_floor` is the
    /// per-entry minimum between refreshes (`Duration::ZERO` disables).
    #[must_use]
    pub fn new(
        registry: Registry,
        registration_password_hash: Option<String>,
        listing_password_hash: Option<String>,
        refresh_interval: u32,
        connection_rate: u32,
        auth_failure_rate: u32,
        refresh_floor: Duration,
    ) -> Self {
        Self {
            registry: Mutex::new(registry),
            registration_password_hash: RwLock::new(registration_password_hash),
            listing_password_hash: RwLock::new(listing_password_hash),
            refresh_interval,
            connection_rate_limiter: RateLimiter::per_minute(connection_rate),
            auth_failure_rate_limiter: RateLimiter::per_minute(auth_failure_rate),
            refresh_floor,
            resolver: Box::new(TokioResolver),
        }
    }

    /// Replace the DNS resolver (tests, for deterministic responses).
    #[must_use]
    pub fn with_resolver(mut self, resolver: Box<dyn Resolver>) -> Self {
        self.resolver = resolver;
        self
    }

    #[must_use]
    pub fn registration_gated(&self) -> bool {
        self.registration_password_hash
            .read()
            .expect(ERR_REGISTRATION_HASH_LOCK_POISONED)
            .is_some()
    }

    #[must_use]
    pub fn listing_gated(&self) -> bool {
        self.listing_password_hash
            .read()
            .expect(ERR_LISTING_HASH_LOCK_POISONED)
            .is_some()
    }

    /// Clone the current registration hash so the caller can verify
    /// outside the lock (keeps slow Argon2 off the SIGHUP path).
    #[must_use]
    pub fn registration_password_snapshot(&self) -> Option<String> {
        self.registration_password_hash
            .read()
            .expect(ERR_REGISTRATION_HASH_LOCK_POISONED)
            .clone()
    }

    /// Clone the current listing hash. See
    /// [`registration_password_snapshot`](Self::registration_password_snapshot).
    #[must_use]
    pub fn listing_password_snapshot(&self) -> Option<String> {
        self.listing_password_hash
            .read()
            .expect(ERR_LISTING_HASH_LOCK_POISONED)
            .clone()
    }

    /// Re-read both hash files. Each flow loads independently: a read /
    /// parse error preserves that flow's prior state (and logs); a clean
    /// `None` (file removed) opens the flow. Takes effect on next refresh.
    pub fn reload_passwords(&self, data_dir: &Path) {
        self.reload_one(data_dir, PasswordKind::Registration);
        self.reload_one(data_dir, PasswordKind::Listing);
    }

    fn reload_one(&self, data_dir: &Path, kind: PasswordKind) {
        let lock = match kind {
            PasswordKind::Registration => &self.registration_password_hash,
            PasswordKind::Listing => &self.listing_password_hash,
        };
        match auth::load_password_hash(data_dir, kind) {
            Ok(new) => {
                let gated = new.is_some();
                *lock.write().expect(ERR_PASSWORD_HASH_LOCK_POISONED) = new;
                info!(kind = %kind, gated = gated, "{}", LOG_PASSWORD_RELOADED);
            }
            Err(e) => {
                error!(kind = %kind, err = %e, "{}", LOG_PASSWORD_RELOAD_FAILED);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fresh_state() -> TrackerState {
        TrackerState::new(Registry::new(0, 0), None, None, 300, 0, 0, Duration::ZERO)
    }

    /// Argon2id PHC for "secret", suitable for round-trip tests.
    fn hash_secret() -> String {
        use argon2::password_hash::{SaltString, rand_core::OsRng};
        use argon2::{Argon2, PasswordHasher};
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"secret", &salt)
            .expect("hash")
            .to_string()
    }

    #[test]
    fn reload_picks_up_new_hash() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let state = fresh_state();
        assert!(!state.registration_gated());

        // After reload of a fresh hash, the flow is gated.
        let hash = hash_secret();
        fs::write(
            auth::hash_path(tmp.path(), PasswordKind::Registration),
            &hash,
        )
        .expect("write hash");

        state.reload_passwords(tmp.path());
        assert!(state.registration_gated());
        assert_eq!(
            state.registration_password_snapshot().as_deref(),
            Some(hash.as_str())
        );
        // Listing was untouched.
        assert!(!state.listing_gated());
    }

    #[test]
    fn reload_handles_file_removed_as_open_flow() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let hash = hash_secret();
        fs::write(
            auth::hash_path(tmp.path(), PasswordKind::Registration),
            &hash,
        )
        .expect("write hash");

        let state = TrackerState::new(
            Registry::new(0, 0),
            Some(hash.clone()),
            None,
            300,
            0,
            0,
            Duration::ZERO,
        );
        assert!(state.registration_gated());

        // Operator runs `clear-password registration` (file deleted).
        fs::remove_file(auth::hash_path(tmp.path(), PasswordKind::Registration))
            .expect("remove hash");

        state.reload_passwords(tmp.path());
        assert!(!state.registration_gated());
    }

    #[test]
    fn reload_preserves_state_on_malformed_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let original = hash_secret();
        fs::write(
            auth::hash_path(tmp.path(), PasswordKind::Registration),
            &original,
        )
        .expect("write hash");

        let state = TrackerState::new(
            Registry::new(0, 0),
            Some(original.clone()),
            None,
            300,
            0,
            0,
            Duration::ZERO,
        );

        // Corrupt the file (operator typo, partial write, etc.).
        fs::write(
            auth::hash_path(tmp.path(), PasswordKind::Registration),
            b"this is not a valid argon2id PHC string",
        )
        .expect("write garbage");

        state.reload_passwords(tmp.path());
        // Previous in-memory state survives the bad reload.
        assert_eq!(
            state.registration_password_snapshot().as_deref(),
            Some(original.as_str()),
            "malformed file must NOT clobber previous state",
        );
        assert!(state.registration_gated());
    }

    #[test]
    fn reload_updates_each_flow_independently() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Registration: write a valid hash.
        let reg_hash = hash_secret();
        fs::write(
            auth::hash_path(tmp.path(), PasswordKind::Registration),
            &reg_hash,
        )
        .expect("write reg");
        // Listing: corrupt.
        fs::write(
            auth::hash_path(tmp.path(), PasswordKind::Listing),
            b"garbage",
        )
        .expect("write listing garbage");

        let state = fresh_state();
        state.reload_passwords(tmp.path());
        // Registration loaded; listing unchanged (still None).
        assert!(state.registration_gated());
        assert!(!state.listing_gated());
    }
}
