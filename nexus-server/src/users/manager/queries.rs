//! Query methods for UserManager

use super::UserManager;
use crate::users::user::User;

impl UserManager {
    /// Get all connected users
    pub async fn get_all_users(&self) -> Vec<User> {
        let users = self.users.read().await;
        users.values().cloned().collect()
    }

    /// Get a user by session ID
    pub async fn get_user_by_session_id(&self, session_id: u32) -> Option<User> {
        let users = self.users.read().await;
        users.get(&session_id).cloned()
    }
}