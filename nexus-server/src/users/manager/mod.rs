//! User manager for tracking connected users

mod broadcasts;
mod helpers;
mod mutations;
mod queries;

pub use mutations::AddUserError;

use crate::users::user::UserSession;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::RwLock;

/// Manages all connected users
#[derive(Debug, Clone)]
pub struct UserManager {
    pub(super) users: Arc<RwLock<HashMap<u32, UserSession>>>,
    pub(super) next_id: Arc<AtomicU32>,
}

impl UserManager {
    /// Create a new user manager
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU32::new(1)),
        }
    }

    /// Generate the next session ID
    pub(super) fn next_session_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
