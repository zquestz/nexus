//! Shared context for per-tracker registration tasks.
//!
//! Every spawned task needs read access to the database, the user
//! manager (for live `user_count`), and a few startup-computed values
//! (the server's own TLS fingerprint, the BBS port, the optional
//! WebSocket port). Bundling those references into one struct keeps
//! task spawn signatures terse and centralizes the dependency surface
//! for testing.

use crate::db::Database;
use crate::users::UserManager;

/// Shared infrastructure handed to every per-tracker registration task.
///
/// Cheap to clone (everything inside is `Arc`-backed or `Copy`). The
/// struct is wrapped in `Arc<TrackerContext>` once at construction and
/// shared across tasks; `Database` and `UserManager` are themselves
/// `Arc`-cloneable, so wrapping them in another `Arc` here would only
/// add indirection.
#[derive(Clone)]
pub struct TrackerContext {
    /// Database access — used to fetch per-refresh `ServerInfo` /
    /// guest-enabled fields and to TOFU-write the observed fingerprint
    /// on first connect.
    pub db: Database,
    /// User manager — used to compute the live `user_count` field on
    /// each `TrackerServerRegister`.
    pub user_manager: UserManager,
    /// The BBS server's own TLS certificate fingerprint, in canonical
    /// form (32 uppercase hex bytes separated by colons, 95 bytes).
    /// Computed once at startup from `server.crt` and injected here.
    /// `&'static str` because the value is process-lifetime constant
    /// and the same `Box::leak`'d slice is shared with the handler
    /// dispatch path in `main.rs`.
    pub server_fingerprint: &'static str,
    /// The BBS server's main TCP port (typically 7500).
    pub server_port: u16,
    /// The BBS server's WebSocket port, if `--websocket` is enabled.
    /// `None` otherwise.
    pub server_websocket_port: Option<u16>,
}
