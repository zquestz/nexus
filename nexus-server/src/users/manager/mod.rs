//! User manager for tracking connected users

pub mod broadcasts;
mod helpers;
mod mutations;
mod queries;

pub use mutations::AddUserError;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, mpsc};

use crate::db::Permission;
use crate::flood::{FloodCheck, FloodConfig, FloodKey, FloodState};
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
    /// Sender for session ids found dead (closed `ConnectionWriter`) during a
    /// broadcast. The reaper task drains these and runs the canonical disconnect
    /// teardown, so cleanup never re-enters a broadcast helper or nests a
    /// `read_user_state` acquisition. Unbounded: volume is capped by live
    /// sessions and the reaper drains immediately.
    dead_session_tx: mpsc::UnboundedSender<u32>,
    /// Handed to the reaper once at startup via `take_dead_session_rx`.
    dead_session_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<u32>>>>,
    /// Shared chat/DM flood buckets keyed by visible user identity. Lock order:
    /// users map first, then flood state. Never acquire users while holding a
    /// flood lock.
    pub(super) flood_state: FloodState,
}

impl UserManager {
    pub fn new() -> Self {
        let (dead_session_tx, dead_session_rx) = mpsc::unbounded_channel();
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU32::new(1)),
            user_state_lock: Arc::new(RwLock::new(())),
            server_info_lock: Arc::new(RwLock::new(())),
            dead_session_tx,
            dead_session_rx: Arc::new(Mutex::new(Some(dead_session_rx))),
            flood_state: FloodState::new(),
        }
    }

    pub(super) fn next_session_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Queue session ids found dead during a broadcast for the reaper to tear
    /// down via the canonical disconnect path. Unbounded and reaper-drained, so
    /// `send` only fails once the reaper is gone (shutdown), which we ignore.
    pub(super) fn enqueue_dead_sessions(&self, session_ids: &[u32]) {
        for &session_id in session_ids {
            let _ = self.dead_session_tx.send(session_id);
        }
    }

    /// Take the dead-session receiver for the reaper task. `None` if already
    /// taken (one reaper only) or the lock is poisoned — the caller logs and
    /// continues without the safety-net sweep.
    pub fn take_dead_session_rx(&self) -> Option<mpsc::UnboundedReceiver<u32>> {
        self.dead_session_rx.lock().ok()?.take()
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

    /// Check chat/DM flood protection for a live session.
    ///
    /// The session lookup and key derivation happen under the users read lock,
    /// then the flood map is locked second. This prevents a removed session from
    /// recreating an orphan bucket after disconnect pruning.
    pub async fn check_message_flood(
        &self,
        session_id: u32,
        config: &FloodConfig,
        now: Instant,
    ) -> Option<FloodCheck> {
        let users = self.users.read().await;
        let session = users.get(&session_id)?;
        let key = FloodKey::for_session(session);
        let has_unlimited = session.has_permission(Permission::ChatUnlimited);
        Some(self.flood_state.check(key, config, has_unlimited, now))
    }

    #[cfg(test)]
    pub(crate) fn flood_state_for_test(&self) -> &FloodState {
        &self.flood_state
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
