//! Shared context for per-tracker publisher tasks.
//!
//! Every spawned task needs read access to the database, the user
//! manager (for live `user_count`), and a few startup-computed values
//! (the server's own TLS fingerprint, the BBS port, the optional
//! WebSocket port). Bundling those references into one struct keeps
//! task spawn signatures terse and centralizes the dependency surface
//! for testing.

use std::sync::Arc;

use crate::db::Database;
use crate::users::UserManager;

/// Shared infrastructure handed to every per-tracker publisher task.
///
/// Cheap to clone (everything inside is `Arc` or `Copy`).
#[derive(Clone)]
pub struct PublisherContext {
    /// Database access — used to fetch per-refresh `ServerInfo` /
    /// guest-enabled fields and to TOFU-write the observed fingerprint
    /// on first connect.
    pub db: Arc<Database>,
    /// User manager — used to compute the live `user_count` field on
    /// each `TrackerServerRegister`.
    pub user_manager: Arc<UserManager>,
    /// The BBS server's own TLS certificate fingerprint, in canonical
    /// form (32 uppercase hex bytes separated by colons, 95 bytes).
    /// Computed once at startup from `server.crt` and injected here.
    pub server_fingerprint: String,
    /// The BBS server's main TCP port (typically 7500).
    pub server_port: u16,
    /// The BBS server's WebSocket port, if `--websocket` is enabled.
    /// `None` otherwise.
    pub server_websocket_port: Option<u16>,
}
