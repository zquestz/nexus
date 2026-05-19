//! Shared context for per-tracker registration tasks.
//!
//! Bundles the DB, user manager (live `user_count`), and startup-computed
//! values (server fingerprint, BBS port, optional WS port) into one struct
//! to keep task spawn signatures terse and centralize the test surface.

use crate::db::Database;
use crate::users::UserManager;

/// Shared infrastructure handed to every per-tracker registration task.
///
/// Cheap to clone (all fields `Arc`-backed or `Copy`). `Database`/`UserManager`
/// are already `Arc`-cloneable, so an extra `Arc` wrapper would only add
/// indirection.
#[derive(Clone)]
pub struct TrackerContext {
    pub db: Database,
    pub user_manager: UserManager,
    /// The server's own TLS fingerprint in canonical form (95 bytes).
    /// `&'static str`: process-lifetime constant, the same `Box::leak`'d
    /// slice shared with the handler dispatch path in `main.rs`.
    pub server_fingerprint: &'static str,
    pub server_port: u16,
    pub server_websocket_port: Option<u16>,
}
