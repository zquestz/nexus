//! User manager for tracking connected users

mod broadcasts;
mod helpers;
mod mutations;
mod queries;

use crate::users::user::UserSession;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages all connected users
#[derive(Debug, Clone)]
pub struct UserManager {
    pub(super) users: Arc<RwLock<HashMap<u32, UserSession>>>,
    pub(super) next_id: Arc<RwLock<u32>>,
}

impl UserManager {
    /// Create a new user manager
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
