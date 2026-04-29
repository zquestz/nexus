//! Shared daemon state passed to per-connection tasks and handlers.
//!
//! This is the small bundle of "things every handler needs": the
//! [`Registry`] (behind a `Mutex` for cross-task access), the optional
//! password hashes for the gated flows, and the operator-configured
//! refresh interval. The state is constructed once at startup in
//! `main.rs` and shared as `Arc<TrackerState>` to every spawned
//! connection task.

use std::sync::Mutex;

use crate::registry::Registry;

/// Daemon-level state shared across connection tasks.
///
/// Cross-task mutation goes through the `Mutex<Registry>`; the password
/// hashes and `refresh_interval` are immutable for the lifetime of the
/// process (no SIGHUP reload in v0.1.0 — see TODO.md).
pub struct TrackerState {
    /// In-memory entry store. Locked for register / refresh / list /
    /// evict_stale operations.
    pub registry: Mutex<Registry>,

    /// Argon2id PHC hash of the registration password, or `None` if
    /// registration is open. Loaded once at startup from
    /// `<data-dir>/registration.hash` via [`crate::auth::load_password_hash`].
    pub registration_password_hash: Option<String>,

    /// Argon2id PHC hash of the listing password, or `None` if listing
    /// is open. Loaded once at startup from `<data-dir>/listing.hash`.
    pub listing_password_hash: Option<String>,

    /// `refresh_interval` value advertised in `TrackerRegisterResponse`.
    /// Servers wait this many seconds between refreshes.
    pub refresh_interval: u32,
}

impl TrackerState {
    /// Build a new state. Take the `Mutex` ownership of `registry` so
    /// no other path can hand the registry to a different mutex.
    #[must_use]
    pub fn new(
        registry: Registry,
        registration_password_hash: Option<String>,
        listing_password_hash: Option<String>,
        refresh_interval: u32,
    ) -> Self {
        Self {
            registry: Mutex::new(registry),
            registration_password_hash,
            listing_password_hash,
            refresh_interval,
        }
    }

    /// Whether registration requires a password (true when a hash is loaded).
    #[must_use]
    pub fn registration_gated(&self) -> bool {
        self.registration_password_hash.is_some()
    }

    /// Whether listing requires a password (true when a hash is loaded).
    #[must_use]
    pub fn listing_gated(&self) -> bool {
        self.listing_password_hash.is_some()
    }
}
