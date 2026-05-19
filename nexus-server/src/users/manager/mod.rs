//! User manager for tracking connected users

pub mod broadcasts;
mod helpers;
mod mutations;
mod queries;

pub use mutations::{AddUserError, DisconnectedSession};

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::{Mutex, MutexGuard, RwLock};

use crate::users::user::UserSession;

/// Manages all connected users
#[derive(Debug, Clone)]
pub struct UserManager {
    pub(super) users: Arc<RwLock<HashMap<u32, UserSession>>>,
    pub(super) next_id: Arc<AtomicU32>,
    /// Serializes admin user-state mutations (user/group create,
    /// update, delete) across the span where they make
    /// requester-authority decisions and reconcile authority-related
    /// session cache (is_admin, permissions, group_id,
    /// bandwidth_weight + override). Held across the DB write itself
    /// (the central SQL UPDATE/DELETE plus any authority-relevant DB
    /// reads like `get_user_permissions` / `get_group_by_id`) so
    /// concurrent same-target handlers serialize on the write. Held
    /// across in-memory channel sends (`tx.send`, `broadcast_to_*`,
    /// voice-leave notifications) that enqueue to writer tasks.
    /// Always dropped before any direct writer-socket I/O
    /// (`ctx.send_message`, `ctx.send_error_and_disconnect`).
    user_state_lock: Arc<Mutex<()>>,
}

impl UserManager {
    /// Create a new user manager
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU32::new(1)),
            user_state_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Generate the next session ID
    pub(super) fn next_session_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn lock_user_state(&self) -> MutexGuard<'_, ()> {
        self.user_state_lock.lock().await
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
