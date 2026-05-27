//! User manager for tracking connected users

pub mod broadcasts;
mod helpers;
mod mutations;
mod queries;

pub use mutations::AddUserError;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::users::user::UserSession;

#[derive(Debug, Clone)]
pub struct NicknameIpSnapshot {
    pub representative: UserSession,
    pub ips: Vec<String>,
}

/// Manages all connected users
#[derive(Debug, Clone)]
pub struct UserManager {
    pub(super) users: Arc<RwLock<HashMap<u32, UserSession>>>,
    pub(super) next_id: Arc<AtomicU32>,
    /// Serializes admin user-state mutations (user/group create/update/delete)
    /// across requester-authority decisions and the authority session-cache
    /// reconciliation (is_admin, permissions, group_id, bandwidth_weight +
    /// override). The write lock (`lock_user_state`) is held across the DB write
    /// and any authority-relevant reads (`get_user_permissions` /
    /// `get_group_by_id`) so same-target handlers serialize, and across in-memory
    /// channel sends (`tx.send`, `broadcast_to_*`, voice-leave). The read lock
    /// (`read_user_state`) is held by login and by handlers that snapshot a
    /// session then emit a nickname-keyed broadcast (kick/trust create, chat
    /// join/leave, user message, target resolution) across the snapshot→action
    /// window, so a concurrent rename can't make the captured nickname stale
    /// before a ChatUserJoined/ChatUserLeft goes out (those have no client-side id
    /// reconciliation — a stale nickname is a permanent ghost). Ban creation uses
    /// the write lock because it mutates live user state through forced teardown.
    /// Always dropped before direct writer-socket I/O (`ctx.send_message`,
    /// `ctx.send_error_and_disconnect`).
    user_state_lock: Arc<RwLock<()>>,
    /// Serializes server config reads (login) against writes (ServerInfoUpdate).
    /// Login holds the read lock during snapshot + session creation so concurrent
    /// logins proceed freely. ServerInfoUpdate holds the write lock across the DB
    /// transaction and in-memory side-effects so login never sees a partial update.
    /// Always dropped before direct socket I/O.
    server_info_lock: Arc<RwLock<()>>,
}

impl UserManager {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU32::new(1)),
            user_state_lock: Arc::new(RwLock::new(())),
            server_info_lock: Arc::new(RwLock::new(())),
        }
    }

    pub(super) fn next_session_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Exclusive lock for user-state mutations (rename, create, delete, group
    /// update). Held across the DB write and any authority-relevant reads so
    /// same-target handlers serialize. Always dropped before direct socket I/O.
    pub async fn lock_user_state(&self) -> RwLockWriteGuard<'_, ()> {
        self.user_state_lock.write().await
    }

    /// Shared lock for login — multiple logins can proceed concurrently while
    /// serializing against user-state mutations that hold the write lock.
    pub async fn read_user_state(&self) -> RwLockReadGuard<'_, ()> {
        self.user_state_lock.read().await
    }

    /// Exclusive lock for `server_info_update` — blocks concurrent logins from
    /// reading config while a mutation is in flight.
    pub async fn lock_server_info_state(&self) -> RwLockWriteGuard<'_, ()> {
        self.server_info_lock.write().await
    }

    /// Shared lock for login — multiple logins can read config concurrently;
    /// all are paused while `ServerInfoUpdate` holds the write lock.
    pub async fn read_server_info_state(&self) -> RwLockReadGuard<'_, ()> {
        self.server_info_lock.read().await
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
