//! Shared daemon state passed to per-connection tasks and handlers.
//!
//! This is the small bundle of "things every handler needs": the
//! [`Registry`] (behind a `Mutex` for cross-task access), the optional
//! password hashes for the gated flows (behind `RwLock` so SIGHUP can
//! swap them), the operator-configured refresh interval, and the
//! per-IP rate limiters. The state is constructed once at startup in
//! `main.rs` and shared as `Arc<TrackerState>` to every spawned
//! connection task.

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
use crate::rate_limiter::RateLimiter;
use crate::registry::Registry;
use crate::resolver::{Resolver, TokioResolver};

/// Daemon-level state shared across connection tasks.
///
/// Cross-task mutation goes through the `Mutex<Registry>` and the
/// `RwLock`s on the password hashes; `refresh_interval` and
/// `refresh_floor` are immutable for the lifetime of the process. The
/// rate limiters carry their own internal mutexes.
///
/// The password hashes are wrapped in `RwLock` because `SIGHUP`
/// re-reads both files and may swap their contents at runtime. Handlers
/// take a brief read lock to clone the current hash out, then release
/// the lock before running Argon2 verification (so a slow verify
/// doesn't block a SIGHUP reload).
pub struct TrackerState {
    /// In-memory entry store. Locked for register / refresh / list /
    /// evict_stale operations.
    pub registry: Mutex<Registry>,

    /// Argon2id PHC hash of the registration password, or `None` if
    /// registration is open. Loaded at startup from
    /// `<data-dir>/registration.hash` via [`crate::auth::load_password_hash`];
    /// reloaded by [`TrackerState::reload_passwords`] on SIGHUP (Unix).
    pub registration_password_hash: RwLock<Option<String>>,

    /// Argon2id PHC hash of the listing password, or `None` if listing
    /// is open. Loaded at startup from `<data-dir>/listing.hash`;
    /// reloaded by [`TrackerState::reload_passwords`] on SIGHUP (Unix).
    pub listing_password_hash: RwLock<Option<String>>,

    /// `refresh_interval` value advertised in `TrackerServerRegisterResponse`.
    /// Servers wait this many seconds between refreshes.
    pub refresh_interval: u32,

    /// Per-IP connection-rate limiter. Decremented at TCP accept time;
    /// over-limit peers get their connection dropped silently.
    pub connection_rate_limiter: RateLimiter,

    /// Per-IP failed-auth limiter. Successful auths don't debit; only
    /// failed password verifications do. Once empty, further attempts
    /// from the offending IP are rejected as `rate_limited`.
    pub auth_failure_rate_limiter: RateLimiter,

    /// Per-entry minimum elapsed time between accepted refreshes.
    /// Production initializes this from
    /// [`crate::constants::REFRESH_FLOOR_INTERVAL`]; tests may pass
    /// `Duration::ZERO` to disable the floor and exercise the refresh
    /// path without waiting out the window.
    pub refresh_floor: Duration,

    /// DNS resolver used by the address-validation step in
    /// `TrackerServerRegister`. Defaults to [`TokioResolver`] in
    /// [`Self::new`]; tests substitute a mock via
    /// [`Self::with_resolver`].
    pub resolver: Box<dyn Resolver>,
}

impl TrackerState {
    /// Build a new state. Take the `Mutex` ownership of `registry` so
    /// no other path can hand the registry to a different mutex.
    ///
    /// `connection_rate` and `auth_failure_rate` are events per minute
    /// per source IP (0 = unlimited). `refresh_floor` is the minimum
    /// elapsed time between accepted refreshes per entry
    /// (`Duration::ZERO` disables the floor).
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

    /// Replace the DNS resolver used by address validation. Intended
    /// for tests that need deterministic DNS responses (NXDOMAIN /
    /// resolved-but-no-match / transient failure). Production code
    /// should rely on the [`TokioResolver`] default installed by
    /// [`Self::new`].
    #[must_use]
    pub fn with_resolver(mut self, resolver: Box<dyn Resolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// Whether registration requires a password (true when a hash is loaded).
    #[must_use]
    pub fn registration_gated(&self) -> bool {
        self.registration_password_hash
            .read()
            .expect(ERR_REGISTRATION_HASH_LOCK_POISONED)
            .is_some()
    }

    /// Whether listing requires a password (true when a hash is loaded).
    #[must_use]
    pub fn listing_gated(&self) -> bool {
        self.listing_password_hash
            .read()
            .expect(ERR_LISTING_HASH_LOCK_POISONED)
            .is_some()
    }

    /// Snapshot of the current registration hash. Handlers call this
    /// and then verify outside the lock so Argon2 doesn't block a
    /// concurrent SIGHUP reload.
    #[must_use]
    pub fn registration_password_snapshot(&self) -> Option<String> {
        self.registration_password_hash
            .read()
            .expect(ERR_REGISTRATION_HASH_LOCK_POISONED)
            .clone()
    }

    /// Snapshot of the current listing hash. See
    /// [`registration_password_snapshot`](Self::registration_password_snapshot).
    #[must_use]
    pub fn listing_password_snapshot(&self) -> Option<String> {
        self.listing_password_hash
            .read()
            .expect(ERR_LISTING_HASH_LOCK_POISONED)
            .clone()
    }

    /// Re-read both password hash files from disk and update the
    /// in-memory state. Each flow is loaded independently: a parse
    /// error or read error on one file preserves *that flow's*
    /// previous state and logs loudly; the other flow's update may
    /// still succeed. A successfully-loaded `None` (file removed)
    /// transitions the flow to open.
    ///
    /// Existing connections continue uninterrupted; the new hash takes
    /// effect on the next refresh / new connection.
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

        // Write a fresh registration hash. After reload, the flow is gated.
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
