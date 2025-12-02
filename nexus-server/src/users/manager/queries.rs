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

    /// Get all session IDs for a given username (case-insensitive)
    pub async fn get_session_ids_for_user(&self, username: &str) -> Vec<u32> {
        let users = self.users.read().await;
        let username_lower = username.to_lowercase();
        users
            .iter()
            .filter(|(_, user)| user.username.to_lowercase() == username_lower)
            .map(|(session_id, _)| *session_id)
            .collect()
    }
}
